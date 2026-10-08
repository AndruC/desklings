//! The game's rules and state machine: what the monster is doing, what the player may do, and
//! how training, battles, evolution and retirement play out. No Windows code: the shell in
//! main.rs feeds it the time and the player's input, and carries out the `Event`s it returns.

use crate::monster::*;
use crate::pet::*;
use crate::sprites::*;

pub const CMD_FEED: i32 = 1;
pub const CMD_PLAY: i32 = 2;
pub const CMD_CLEAN: i32 = 3;
pub const CMD_SLEEP: i32 = 4;
pub const CMD_QUIT: i32 = 5;
pub const CMD_RESET: i32 = 6;
pub const CMD_BATTLE: i32 = 7;
pub const CMD_GUIDE: i32 = 8;
pub const CMD_HALL: i32 = 9;
pub const CMD_RETIRE: i32 = 10;
pub const CMD_TRAIN: i32 = 20; // + drill index
pub const CMD_LAST: i32 = CMD_TRAIN + Drill::ALL.len() as i32 - 1;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    Idle,
    Walk,
    Eat,
    Joy,
    Train(Drill),
    Show(Sprite), // react with an icon for a moment
    Battle,
}

/// Things the shell has to do in the world on the game's behalf.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Event {
    /// Jump with this velocity (in 100%-DPI pixels per tick), if standing.
    Hop(f32),
    /// Drop a poop where it stands; the shell records where it landed in `pet.poops`.
    Poop,
    /// Every poop is gone; remove their windows.
    PoopsCleared,
    /// A battle began: bring on the opponent.
    BattleStarted,
    /// The battle is over: send the opponent away.
    BattleOver,
    /// Its lifespan is up: say goodbye, enshrine it, then call `Game::retire`.
    Retire,
    Save,
}

pub struct Battle {
    pub me: Fighter,
    pub foe: Fighter,
    pub my_turn: bool,
    step_t: u32,
    pub turns: u32,
    pub result: Option<bool>, // Some(won) once decided; shown for a moment before it's booked
    end_t: u32,
    pub lunge: (u32, u32), // ticks left for (me, foe)
    pub flash: (u32, u32),
}

pub struct Game {
    pub pet: Pet,
    pub act: Act,
    pub act_t: u32,
    pub dir: i32,
    pub battle: Option<Battle>,
    pub news: String, // last training / battle result, shown in the menu
    pub retiring: bool,
    pub evo_flash: u32,
    rng: Rng,
}

impl Game {
    pub fn new(pet: Pet, seed: u64) -> Game {
        Game {
            pet,
            act: Act::Idle,
            act_t: 20,
            dir: -1,
            battle: None,
            news: String::new(),
            retiring: false,
            evo_flash: 0,
            rng: Rng(seed | 1),
        }
    }

    fn set_act(&mut self, act: Act, ticks: u32) {
        self.act = act;
        self.act_t = ticks;
    }

    fn training(&self) -> bool {
        matches!(self.act, Act::Train(_))
    }

    /// In the middle of something that has to finish first.
    pub fn in_scene(&self) -> bool {
        self.battle.is_some() || self.training() || self.retiring
    }

    /// The single source of truth for what the player may do right now. The menu greys out
    /// whatever this rejects, and `command` and `clean_one` refuse it.
    pub fn allowed(&self, cmd: i32) -> bool {
        let p = &self.pet;
        let scene = self.in_scene();
        let free = !p.asleep && p.stage() != Stage::Egg && !scene;
        match cmd {
            CMD_FEED | CMD_PLAY => free,
            CMD_TRAIN..=CMD_LAST => free,
            CMD_BATTLE => free && p.stage() >= Stage::InTraining,
            CMD_CLEAN => !p.poops.is_empty() && !scene,
            CMD_SLEEP => p.stage() != Stage::Egg && !scene,
            CMD_RETIRE => !scene && p.stage() >= Stage::Rookie,
            CMD_RESET => !scene,
            CMD_GUIDE | CMD_HALL | CMD_QUIT => true,
            _ => false,
        }
    }

    // ------------------------------------------------------------------ time

    /// Once per second of clock time. `standing` says it's on the ground and not being held,
    /// which evolving, retiring and pooping wait for.
    pub fn second(&mut self, now: u64, standing: bool) -> Vec<Event> {
        let mut ev = Vec::new();
        if let Some(next) = self.due_evolution(now, standing) {
            self.evolve(next, now, &mut ev);
        }
        self.pet.review_day(now);
        if self.pet.life_left_at(now) == 0 && !self.retiring && self.battle.is_none() && standing {
            self.retiring = true;
            ev.push(Event::Retire);
        }
        match self.pet.live_second(now, !(self.battle.is_some() || self.training())) {
            Sleep::Woke => self.set_act(Act::Joy, 15),
            // Reactions don't tick while asleep, so drop any pending one or it sticks.
            Sleep::FellAsleep => self.set_act(Act::Idle, 20),
            Sleep::Unchanged => {}
        }
        if self.pet.next_poop != 0 && now >= self.pet.next_poop && standing && self.battle.is_none() {
            self.pet.next_poop = 0;
            if self.pet.poops.len() >= MAX_POOPS {
                // Nowhere left to go: that's on you.
                self.pet.mistake();
                self.pet.happy = (self.pet.happy - 10.0).max(0.0);
            } else {
                ev.push(Event::Poop);
                if !self.pet.asleep && !self.training() {
                    self.set_act(Act::Walk, 30); // wander away from it
                }
            }
        }
        ev
    }

    fn due_evolution(&self, now: u64, standing: bool) -> Option<Species> {
        let dur = self.pet.stage().duration()?;
        let busy = self.battle.is_some() || self.training() || !standing;
        if now < self.pet.stage_since + dur || self.pet.life_left_at(now) == 0 || busy {
            return None;
        }
        evolution(self.pet.species, &self.pet.stats, self.pet.mistakes, self.pet.wins)
    }

    fn evolve(&mut self, next: Species, now: u64, ev: &mut Vec<Event>) {
        let from = self.pet.species.info().name;
        self.pet.species = next;
        self.pet.stage_since = now;
        self.pet.seen |= next.bit();
        self.pet.asleep = false;
        evolve_bonus(next, &mut self.pet.stats);
        self.news = format!("{from} evolved into {}!", next.info().name);
        self.evo_flash = 30;
        self.set_act(Act::Show(STAR), 30);
        ev.push(Event::Hop(14.0));
        ev.push(Event::Save);
    }

    /// Every animation frame (10 a second) while it isn't being held: counts down whatever it's
    /// doing and decides what's next.
    pub fn tick(&mut self, now: u64) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.pet.asleep || self.act == Act::Battle {
            return ev;
        }
        if self.act == Act::Train(Drill::Run) && self.act_t % 8 == 0 {
            self.dir = -self.dir; // laps
        }
        self.act_t = self.act_t.saturating_sub(1);
        if self.act_t == 0 {
            match self.act {
                Act::Train(d) => self.finish_training(d, now, &mut ev),
                _ => self.choose_next(now),
            }
        }
        ev
    }

    fn choose_next(&mut self, now: u64) {
        if self.pet.stage() == Stage::Egg {
            return self.set_act(Act::Idle, 50);
        }
        let p = &self.pet;
        let lazy = p.full < 20.0 || p.happy < 20.0 || p.energy < 25.0 || p.elderly_at(now);
        let idle_odds = if lazy { 75 } else { 40 };
        if self.rng.below(100) < idle_odds {
            let t = 20 + self.rng.below(40);
            self.set_act(Act::Idle, t);
        } else {
            self.dir = if self.rng.below(2) == 0 { -1 } else { 1 };
            let t = 20 + self.rng.below(60);
            self.set_act(Act::Walk, t);
        }
    }

    // ------------------------------------------------------------------ player actions

    pub fn command(&mut self, cmd: i32, now: u64) -> Vec<Event> {
        let mut ev = Vec::new();
        if !self.allowed(cmd) {
            return ev;
        }
        if let CMD_TRAIN..=CMD_LAST = cmd {
            self.start_training(Drill::ALL[(cmd - CMD_TRAIN) as usize]);
            return ev;
        }
        match cmd {
            CMD_FEED => {
                let p = &mut self.pet;
                if p.full > 90.0 {
                    p.happy = (p.happy - 5.0).max(0.0); // overfed
                }
                p.full = (p.full + 30.0).min(100.0);
                if p.next_poop == 0 {
                    p.next_poop = now + 180 + self.rng.below(420) as u64;
                }
                self.set_act(Act::Eat, 20);
            }
            CMD_PLAY => {
                let p = &mut self.pet;
                p.happy = (p.happy + 20.0).min(100.0);
                p.energy = (p.energy - 8.0).max(0.0);
                p.full = (p.full - 5.0).max(0.0);
                self.set_act(Act::Joy, 25);
                ev.push(Event::Hop(14.0));
            }
            CMD_BATTLE => {
                self.start_battle(&mut ev);
                return ev;
            }
            CMD_CLEAN => {
                self.pet.poops.clear();
                ev.push(Event::PoopsCleared);
                self.pet.happy = (self.pet.happy + 5.0).min(100.0);
                self.set_act(Act::Joy, 10);
            }
            CMD_SLEEP => {
                self.pet.asleep = !self.pet.asleep;
                self.set_act(Act::Idle, 20);
            }
            CMD_RESET => {
                self.pet.poops.clear();
                ev.push(Event::PoopsCleared);
                self.pet = Pet::new(self.pet.seen);
                self.news.clear();
                self.set_act(Act::Idle, 20);
            }
            _ => return ev, // Quit, Guide, Hall and Retire are the shell's (they need dialogs)
        }
        ev.push(Event::Save);
        ev
    }

    pub fn poke(&mut self) -> Vec<Event> {
        if self.pet.asleep || self.in_scene() || self.pet.stage() == Stage::Egg {
            return Vec::new();
        }
        self.pet.happy = (self.pet.happy + 3.0).min(100.0);
        self.set_act(Act::Joy, 12);
        vec![Event::Hop(6.0)]
    }

    /// Cleans the poop at `index` in `pet.poops`, if cleaning is allowed right now.
    pub fn clean_one(&mut self, index: usize) -> bool {
        if !self.allowed(CMD_CLEAN) || index >= self.pet.poops.len() {
            return false;
        }
        self.pet.poops.remove(index);
        self.pet.happy = (self.pet.happy + 2.0).min(100.0);
        true
    }

    // ------------------------------------------------------------------ training

    fn start_training(&mut self, d: Drill) {
        if self.pet.energy < 15.0 {
            self.news = "Too tired to train. Let it rest.".into();
            return self.set_act(Act::Show(SWEAT), 20);
        }
        self.overwork_check();
        self.pet.energy -= 12.0;
        self.pet.full = (self.pet.full - 6.0).max(0.0);
        self.set_act(Act::Train(d), 40);
    }

    /// Pushing a tired monster costs it some lifespan.
    fn overwork_check(&mut self) {
        if self.pet.energy < 30.0 {
            self.pet.life_mod -= OVERWORK_COST;
        }
    }

    fn finish_training(&mut self, d: Drill, now: u64, ev: &mut Vec<Event>) {
        let elderly = self.pet.elderly_at(now);
        let (out, gm, gs) = train(d, self.pet.species, &mut self.pet.stats, self.pet.happy, elderly, &mut self.rng);
        let (main, side) = d.stats();
        let icon = match out {
            Outcome::Fail => {
                self.pet.happy = (self.pet.happy - 3.0).max(0.0);
                self.news = format!("{}: slacked off. (Happier monsters train better.)", d.name());
                SWEAT
            }
            Outcome::Good | Outcome::Great => {
                let great = if out == Outcome::Great { "Great! " } else { "" };
                let side_s = if gs > 0 { format!(", {} +{gs}", STAT_NAMES[side]) } else { String::new() };
                self.news = format!("{}: {great}{} +{gm}{side_s}", d.name(), STAT_NAMES[main]);
                if out == Outcome::Great {
                    ev.push(Event::Hop(10.0));
                    STAR
                } else {
                    UP
                }
            }
        };
        self.set_act(Act::Show(icon), 20);
        ev.push(Event::Save);
    }

    // ------------------------------------------------------------------ battles

    fn start_battle(&mut self, ev: &mut Vec<Event>) {
        if self.pet.energy < 20.0 {
            self.news = "Too tired to battle. Let it rest.".into();
            return self.set_act(Act::Show(SWEAT), 20);
        }
        self.overwork_check();
        self.pet.energy -= 20.0;
        self.pet.full = (self.pet.full - 8.0).max(0.0);
        let foe = opponent(self.pet.rank as usize, &mut self.rng);
        let me = Fighter::new(self.pet.species, self.pet.stats);
        self.battle = Some(Battle {
            my_turn: strikes_first(&me, &foe),
            me,
            foe,
            step_t: 6,
            turns: 0,
            result: None,
            end_t: 0,
            lunge: (0, 0),
            flash: (0, 0),
        });
        self.set_act(Act::Battle, 0);
        ev.push(Event::BattleStarted);
    }

    /// Every animation frame during a battle. Blows only start once the opponent has walked up.
    pub fn battle_tick(&mut self, foe_arrived: bool) -> Vec<Event> {
        let mut ev = Vec::new();
        let Some(b) = self.battle.as_mut() else { return ev };
        b.lunge = (b.lunge.0.saturating_sub(1), b.lunge.1.saturating_sub(1));
        b.flash = (b.flash.0.saturating_sub(1), b.flash.1.saturating_sub(1));
        if !foe_arrived {
            return ev;
        }
        if let Some(won) = b.result {
            b.end_t = b.end_t.saturating_sub(1);
            if b.end_t == 0 {
                self.finish_battle(won, &mut ev);
            }
            return ev;
        }
        if b.step_t > 0 {
            b.step_t -= 1;
            return ev;
        }
        b.step_t = 8;
        b.turns += 1;
        let hit = if b.my_turn {
            b.lunge.0 = 3;
            strike(&b.me, &mut b.foe, &mut self.rng)
        } else {
            b.lunge.1 = 3;
            strike(&b.foe, &mut b.me, &mut self.rng)
        };
        if let Hit::Hit(_) | Hit::Crit(_) = hit {
            let flash = if b.my_turn { &mut b.flash.1 } else { &mut b.flash.0 };
            *flash = if matches!(hit, Hit::Crit(_)) { 6 } else { 3 };
        }
        b.my_turn = !b.my_turn;
        if let Some(won) = outcome(&b.me, &b.foe, b.turns) {
            b.result = Some(won);
            b.end_t = 25;
        }
        ev
    }

    fn finish_battle(&mut self, won: bool, ev: &mut Vec<Event>) {
        let Some(b) = self.battle.take() else { return };
        ev.push(Event::BattleOver);
        let foe = b.foe.sp.info().name;
        let rank = RANKS[self.pet.rank as usize];
        let promoted = self.pet.record_battle(won);
        if won {
            self.news = format!("Beat a wild {foe} (Rank {rank})!");
            // A small bonus to a stat that still has room to grow.
            let open: Vec<usize> = (0..5).filter(|&i| self.pet.stats[i] < STAT_MAX).collect();
            if !open.is_empty() {
                let i = open[self.rng.below(open.len() as u32) as usize];
                let gain = add(&mut self.pet.stats, i, 2);
                self.news += &format!(" {} +{gain}", STAT_NAMES[i]);
            }
            if let Some(r) = promoted {
                self.news += &format!(" Promoted to Rank {}!", RANKS[r as usize]);
            }
            self.set_act(Act::Show(STAR), 25);
            ev.push(Event::Hop(12.0));
        } else {
            self.news = format!("Lost to a wild {foe} (Rank {rank}). Train up and try again!");
            self.set_act(Act::Show(SWEAT), 25);
        }
        ev.push(Event::Save);
    }

    // ------------------------------------------------------------------ endings

    /// The app is closing: settle anything in progress so it's saved fairly. A battle that's
    /// already decided is booked as decided; one still going counts as running away; a drill in
    /// progress finishes early (its cost is already paid).
    pub fn settle(&mut self, now: u64) -> Vec<Event> {
        let mut ev = Vec::new();
        match self.battle.as_ref().map(|b| b.result) {
            Some(Some(won)) => self.finish_battle(won, &mut ev),
            Some(None) => {
                self.battle = None;
                self.pet.record_battle(false);
                self.news = "Ran away from a battle.".into();
                ev.push(Event::BattleOver);
            }
            None => {}
        }
        if let Act::Train(d) = self.act {
            self.finish_training(d, now, &mut ev);
        }
        ev
    }

    /// Replaces the pet with its successor. The shell has already written the Hall of Fame entry
    /// (`enshrined` says whether that worked).
    pub fn retire(&mut self, enshrined: bool) -> Vec<Event> {
        let mut ev = vec![Event::PoopsCleared];
        if self.battle.take().is_some() {
            ev.push(Event::BattleOver);
        }
        self.pet.poops.clear();
        let name = self.pet.species.info().name;
        self.pet = self.pet.successor();
        self.news = format!("{name} retired. Generation {} begins!", self.pet.generation);
        if !enshrined {
            self.news += " (Couldn't write the Hall of Fame file.)";
        }
        self.retiring = false;
        self.set_act(Act::Idle, 20);
        ev.push(Event::Save);
        ev
    }

    // ------------------------------------------------------------------ text

    pub fn farewell(&self, now: u64) -> String {
        let p = &self.pet;
        let name = p.species.info().name;
        format!(
            "{name} has lived a full life of {} and retires to the Hall of Fame.\n\n\
             A new egg arrives, carrying on a little of {name}'s strength (Generation {}).",
            duration(p.age_at(now)),
            p.generation + 1
        )
    }

    /// The read-only status lines at the top of the menu. Empty strings are separators.
    pub fn status_lines(&self, now: u64, save_failed: bool) -> Vec<String> {
        let p = &self.pet;
        let info = p.species.info();
        let mut lines = vec![format!("{} · {}\t{}", info.name, info.stage.name(), duration(p.age_at(now)))];
        if p.stage() != Stage::Egg {
            let progress = if p.rank as usize == RANKS.len() - 1 {
                "top rank".to_string()
            } else {
                format!("{}/{} to next", p.rank_wins, WINS_TO_RANK_UP)
            };
            lines.push(format!("Rank {}  ({progress})\t{}W {}L", RANKS[p.rank as usize], p.wins, p.losses));
            lines.push(String::new());
            for i in 0..5 {
                lines.push(format!("{}\t{}", STAT_NAMES[i], p.stats[i]));
            }
            lines.push(String::new());
            lines.push(format!("Hunger\t{}", hearts(p.full)));
            lines.push(format!("Happy\t{}", hearts(p.happy)));
            lines.push(format!("Energy\t{}", hearts(p.energy)));
            lines.push(format!("Care mistakes\t{}", p.mistakes));
            let life = if p.elderly_at(now) { "Twilight years" } else { "Lifespan" };
            lines.push(format!("Gen {} · {life}\t~{} left", p.generation, duration(p.life_left_at(now))));
        }
        if save_failed {
            lines.push(String::new());
            lines.push("⚠ Couldn't save. Check that %APPDATA%\\desklings is writable.".into());
        }
        if !self.news.is_empty() {
            lines.push(String::new());
            lines.push(self.news.clone());
        }
        lines
    }

    pub fn guide(&self, now: u64) -> String {
        let p = &self.pet;
        let info = p.species.info();
        let next = match p.stage().duration() {
            None => "Fully evolved.".to_string(),
            Some(_) if !can_evolve(p.species) => "This is its final form.".into(),
            Some(d) if now < p.stage_since + d => format!("Next evolution in {}.", duration(p.stage_since + d - now)),
            Some(_) if evolution(p.species, &p.stats, p.mistakes, p.wins).is_none() => {
                "Old enough to evolve — still missing a requirement.".into()
            }
            Some(_) => "Evolving any moment now!".into(),
        };
        let stats: Vec<String> = (0..5).map(|i| format!("{} {}", STAT_NAMES[i], p.stats[i])).collect();
        let raised: Vec<&str> = ALL_SPECIES[1..].iter().filter(|s| p.seen & s.bit() != 0).map(|s| s.info().name).collect();
        format!(
            "{} — {} · Generation {}\n\"{}\"\n\n{}\n{}\n\n{}\n\nStats: {} (total {})\nCare mistakes: {} · Battles: {} won, {} lost · Rank {}\n\n\
             Monsters raised ({}/{}): {}",
            info.name,
            info.stage.name(),
            p.generation,
            info.blurb,
            next,
            paths(p.species),
            lifespan_text(p, now),
            stats.join(" · "),
            total(&p.stats),
            p.mistakes,
            p.wins,
            p.losses,
            RANKS[p.rank as usize],
            raised.len(),
            ALL_SPECIES.len() - 1,
            if raised.is_empty() { "none yet".to_string() } else { raised.join(", ") },
        )
    }
}

pub fn duration(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
        _ => format!("{}d {}h", secs / 86400, secs % 86400 / 3600),
    }
}

fn lifespan_text(p: &Pet, now: u64) -> String {
    let left = duration(p.life_left_at(now));
    let tip = "Each mistake-free day adds half a day (days with the app closed count too); care mistakes and \
               training while tired take time away. A Grumbloo's life is a quarter shorter. Time away of more \
               than half an hour counts as a full night's sleep.";
    if p.elderly_at(now) {
        format!("In its twilight years: about {left} left before it retires. It moves slower and gains less from training.\n{tip}")
    } else {
        format!("Lifespan: about {left} left. {tip}")
    }
}

fn hearts(v: f32) -> String {
    let n = ((v / 20.0).ceil() as usize).min(5);
    "♥".repeat(n) + &"♡".repeat(5 - n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn game(species: Species) -> Game {
        let mut p = Pet::new(0);
        p.born = NOW - 2 * 86_400;
        p.stage_since = NOW;
        p.species = species;
        p.stats = [60; 5];
        p.energy = 100.0;
        p.happy = 100.0;
        p.full = 100.0;
        Game::new(p, 7)
    }

    /// Runs the game like the shell does (10 frames a second) until `done` or `secs` pass.
    fn run(g: &mut Game, secs: u64, mut done: impl FnMut(&Game) -> bool) -> (u64, Vec<Event>) {
        let mut seen = Vec::new();
        for s in 0..secs {
            seen.extend(g.second(NOW + s, true));
            for _ in 0..10 {
                if g.battle.is_some() {
                    seen.extend(g.battle_tick(true));
                }
                seen.extend(g.tick(NOW + s));
            }
            if done(g) {
                return (s, seen);
            }
        }
        (secs, seen)
    }

    #[test]
    fn rules_for_what_is_allowed() {
        let mut egg = game(Species::Egg);
        egg.pet.poops = vec![(0, 0)];
        assert!(!egg.allowed(CMD_FEED) && !egg.allowed(CMD_TRAIN) && !egg.allowed(CMD_SLEEP));
        assert!(egg.allowed(CMD_CLEAN) && egg.allowed(CMD_RESET) && egg.allowed(CMD_QUIT));

        let mut g = game(Species::Raptin);
        g.pet.poops = vec![(0, 0)];
        assert!((CMD_FEED..=CMD_RETIRE).all(|c| g.allowed(c)));
        assert!((CMD_TRAIN..=CMD_LAST).all(|c| g.allowed(c)));
        assert!(!g.allowed(CMD_TRAIN - 1) && !g.allowed(CMD_LAST + 1) && !g.allowed(0));

        g.pet.asleep = true;
        assert!(!g.allowed(CMD_FEED) && !g.allowed(CMD_BATTLE) && g.allowed(CMD_SLEEP));
        g.pet.asleep = false;

        g.command(CMD_BATTLE, NOW);
        assert!(g.battle.is_some());
        for c in [CMD_FEED, CMD_TRAIN, CMD_BATTLE, CMD_CLEAN, CMD_SLEEP, CMD_RESET, CMD_RETIRE] {
            assert!(!g.allowed(c), "cmd {c} allowed mid-battle");
        }
        assert!(g.allowed(CMD_QUIT) && g.allowed(CMD_GUIDE));
    }

    #[test]
    fn refused_commands_change_nothing() {
        let mut g = game(Species::Raptin);
        g.command(CMD_BATTLE, NOW);
        let born = g.pet.born;
        assert!(g.command(CMD_RESET, NOW).is_empty());
        assert_eq!(g.pet.born, born, "Start over mid-battle must be refused");
        g.pet.poops = vec![(0, 0)];
        assert!(!g.clean_one(0), "clicking a poop mid-battle must be refused");
        assert_eq!(g.pet.poops.len(), 1);
    }

    #[test]
    fn training_at_low_energy_still_pays_out_then_sleeps() {
        let mut g = game(Species::Raptin);
        g.pet.energy = 20.0;
        let before = total(&g.pet.stats);
        g.command(CMD_TRAIN, NOW);
        assert!(matches!(g.act, Act::Train(_)));
        let (_, ev) = run(&mut g, 30, |g| g.pet.asleep);
        assert!(g.pet.asleep, "should doze off afterwards");
        assert!(ev.contains(&Event::Save), "the session was finished and saved");
        assert!(total(&g.pet.stats) > before || g.news.contains("slacked"), "news: {}", g.news);
        assert_eq!(g.act, Act::Idle, "no reaction left stuck while asleep");
    }

    #[test]
    fn a_whole_battle_is_booked_once() {
        let mut g = game(Species::Pyrorex);
        g.pet.stats = [300; 5];
        g.command(CMD_BATTLE, NOW);
        let (_, ev) = run(&mut g, 120, |g| g.battle.is_none());
        assert!(g.battle.is_none());
        assert_eq!(ev.iter().filter(|e| **e == Event::BattleOver).count(), 1);
        assert_eq!(g.pet.wins + g.pet.losses, 1);
        assert_eq!(g.pet.wins, 1, "a 1500-stat Pyrorex should beat Rank E");
    }

    #[test]
    fn quitting_settles_fairly() {
        // Decided win, still showing the banner: booked as a win.
        let mut won = game(Species::Pyrorex);
        won.pet.stats = [300; 5];
        won.command(CMD_BATTLE, NOW);
        run(&mut won, 120, |g| g.battle.as_ref().is_some_and(|b| b.result.is_some()));
        assert_eq!(won.battle.as_ref().unwrap().result, Some(true));
        won.settle(NOW);
        assert_eq!((won.pet.wins, won.pet.losses), (1, 0));

        // Still fighting: running away is a loss.
        let mut fled = game(Species::Raptin);
        fled.command(CMD_BATTLE, NOW);
        let ev = fled.settle(NOW);
        assert!(ev.contains(&Event::BattleOver));
        assert_eq!((fled.pet.wins, fled.pet.losses), (0, 1));

        // Mid-drill: the paid-for session still counts.
        let mut drill = game(Species::Raptin);
        drill.command(CMD_TRAIN, NOW);
        let before = total(&drill.pet.stats);
        drill.settle(NOW);
        assert!(!matches!(drill.act, Act::Train(_)));
        assert!(total(&drill.pet.stats) > before || drill.news.contains("slacked"));
    }

    #[test]
    fn battle_bonus_reports_what_it_gave() {
        let mut g = game(Species::Infernax);
        g.pet.stats = [STAT_MAX; 5];
        g.command(CMD_BATTLE, NOW);
        run(&mut g, 120, |g| g.battle.is_none());
        assert_eq!(g.pet.wins, 1);
        assert!(!g.news.contains('+'), "no stat can grow, so no bonus claimed: {}", g.news);

        // One point of room left: the bonus goes there and says +1, not +2.
        let mut g = game(Species::Infernax);
        g.pet.stats = [STAT_MAX, STAT_MAX, STAT_MAX - 1, STAT_MAX, STAT_MAX];
        g.command(CMD_BATTLE, NOW);
        run(&mut g, 120, |g| g.battle.is_none());
        assert_eq!(g.pet.stats[DEF], STAT_MAX);
        assert!(g.news.contains("Defense +1"), "{}", g.news);
    }

    #[test]
    fn evolution_waits_for_training_and_battles() {
        let mut g = game(Species::Blop);
        g.pet.stage_since = NOW - 10 * 3600; // long overdue
        g.command(CMD_TRAIN + 1, NOW);
        g.second(NOW, true);
        assert_eq!(g.pet.species, Species::Blop, "mustn't evolve mid-drill");
        run(&mut g, 10, |g| g.pet.species != Species::Blop);
        assert_ne!(g.pet.species, Species::Blop, "evolves once the drill is done");
    }

    #[test]
    fn retirement_is_announced_once_and_not_mid_battle() {
        let mut g = game(Species::Raptin);
        g.pet.born = NOW - 60 * 86_400;
        g.command(CMD_BATTLE, NOW);
        assert!(!g.second(NOW, true).contains(&Event::Retire), "not mid-battle");
        g.battle = None;
        g.act = Act::Idle;
        assert!(g.second(NOW, true).contains(&Event::Retire));
        assert!(!g.second(NOW + 1, true).contains(&Event::Retire), "only announced once");
        let old_gen = g.pet.generation;
        g.retire(true);
        assert_eq!(g.pet.generation, old_gen + 1);
        assert!(!g.retiring);
    }

    #[test]
    fn cleaning_one_poop_cheers_it_up() {
        let mut g = game(Species::Raptin);
        g.pet.happy = 50.0;
        g.pet.poops = vec![(1, 1), (2, 2)];
        assert!(g.clean_one(1));
        assert_eq!(g.pet.poops, vec![(1, 1)]);
        assert_eq!(g.pet.happy, 52.0);
        assert!(!g.clean_one(5), "no such poop");
    }

    #[test]
    fn full_poop_pile_is_a_care_mistake() {
        let mut g = game(Species::Raptin);
        g.pet.poops = vec![(0, 0); MAX_POOPS];
        g.pet.next_poop = NOW;
        let ev = g.second(NOW, true);
        assert!(!ev.contains(&Event::Poop));
        assert_eq!(g.pet.mistakes, 1);
    }
}
