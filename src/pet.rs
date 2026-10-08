//! The persistent pet: who it is, its needs and stats, its care record, and the save file.

use crate::monster::*;
use crate::now;

pub const MAX_POOPS: usize = 3;

// Needs drain rates, in points per second.
pub const FULL_RATE: f32 = 100.0 / 5400.0; // empty in 90 min
pub const HAPPY_RATE: f32 = 100.0 / 7200.0; // empty in 2 h (faster with poop / hunger)
pub const TIRE_RATE: f32 = 100.0 / 10800.0; // tired after 3 h awake
pub const REST_RATE: f32 = 100.0 / 1800.0; // rested after 30 min of sleep
pub const DOZE_BELOW: f32 = 10.0; // falls asleep on its own under this much energy

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sleep {
    Unchanged,
    FellAsleep,
    Woke,
}

pub struct Pet {
    pub born: u64,
    pub species: Species,
    pub stage_since: u64,
    pub stats: Stats,
    pub full: f32,
    pub happy: f32,
    pub energy: f32,
    pub poops: Vec<(i32, i32)>, // screen positions of dropped poops
    pub asleep: bool,
    pub next_poop: u64, // 0 = none pending
    pub mistakes: u32,
    pub starving: bool, // already counted a care mistake for this empty stomach
    pub sulking: bool,  // ...or for this bout of misery
    pub wins: u32,
    pub losses: u32,
    pub rank: u8,
    pub rank_wins: u8,
    pub seen: u32, // bitmask of species ever raised; survives starting over
    pub generation: u32,
    pub life_mod: i64,      // seconds added to (or taken from) the base lifespan by how it's been raised
    pub care_day: u64,      // last day of age that was checked for a mistake-free bonus
    pub mistakes_mark: u32, // mistakes count at the start of that day
}

const DAY: u64 = 86_400;
const BASE_LIFESPAN: u64 = 14 * DAY;
const MIN_LIFESPAN: u64 = 4 * DAY;
pub const MISTAKE_COST: i64 = 12 * 3600;
pub const OVERWORK_COST: i64 = 3 * 3600;
const GOOD_DAY_BONUS: i64 = 12 * 3600;

impl Pet {
    pub fn new(seen: u32) -> Pet {
        let t = now();
        Pet {
            born: t,
            species: Species::Egg,
            stage_since: t,
            stats: [10; 5],
            full: 80.0,
            happy: 80.0,
            energy: 100.0,
            poops: Vec::new(),
            asleep: false,
            next_poop: 0,
            mistakes: 0,
            starving: false,
            sulking: false,
            wins: 0,
            losses: 0,
            rank: 0,
            rank_wins: 0,
            seen: seen | Species::Egg.bit(),
            generation: 1,
            life_mod: 0,
            care_day: 0,
            mistakes_mark: 0,
        }
    }

    /// A fresh egg for the next generation, inheriting a tenth of the retiree's stats (up to +50 each).
    pub fn successor(&self) -> Pet {
        let mut p = Pet::new(self.seen);
        p.generation = self.generation + 1;
        for (s, parent) in p.stats.iter_mut().zip(self.stats) {
            *s += (parent / 10).min(50);
        }
        p
    }

    pub fn stage(&self) -> Stage {
        self.species.info().stage
    }

    pub fn age(&self) -> u64 {
        now().saturating_sub(self.born)
    }

    pub fn mistake(&mut self) {
        self.mistakes += 1;
        self.life_mod -= MISTAKE_COST;
    }

    /// Total lifespan in seconds: a base that good care stretches and neglect or overwork shortens.
    pub fn lifespan(&self) -> u64 {
        let base = if self.species == Species::Grumbloo { BASE_LIFESPAN * 3 / 4 } else { BASE_LIFESPAN };
        (base as i64 + self.life_mod).max(MIN_LIFESPAN as i64) as u64
    }

    pub fn life_left(&self) -> u64 {
        self.lifespan().saturating_sub(self.age())
    }

    /// In its twilight years: the last 15% of its life.
    pub fn elderly(&self) -> bool {
        self.age() * 100 >= self.lifespan() * 85
    }

    /// Called every second. Each full day of age earns extra lifespan, whether or not the app was
    /// running, except that every care mistake made since the last review spoils one day's bonus.
    pub fn review_day(&mut self) {
        let day = self.age() / DAY;
        if day > self.care_day {
            let spoiled = self.mistakes.saturating_sub(self.mistakes_mark) as u64;
            let good_days = (day - self.care_day).saturating_sub(spoiled);
            self.life_mod += GOOD_DAY_BONUS * good_days as i64;
            self.care_day = day;
            self.mistakes_mark = self.mistakes;
        }
    }

    /// One second of life: hunger, happiness and energy drift, sleep, and care mistakes.
    /// `may_doze` is false while it's busy (battling, training) so it finishes before nodding off.
    pub fn live_second(&mut self, may_doze: bool) -> Sleep {
        if self.stage() == Stage::Egg {
            return Sleep::Unchanged;
        }
        let slow = if self.asleep { 0.5 } else { 1.0 };
        self.full = (self.full - FULL_RATE * slow).max(0.0);
        let mut sad = HAPPY_RATE * (1.0 + self.poops.len() as f32);
        if self.full < 20.0 {
            sad *= 2.0;
        }
        self.happy = (self.happy - sad * slow).max(0.0);

        let mut change = Sleep::Unchanged;
        if self.asleep {
            self.energy = (self.energy + REST_RATE).min(100.0);
            if self.energy >= 100.0 {
                self.asleep = false;
                change = Sleep::Woke;
            }
        } else {
            let tire = if self.elderly() { 1.5 } else { 1.0 };
            self.energy = (self.energy - TIRE_RATE * tire).max(0.0);
            if self.energy < DOZE_BELOW && may_doze {
                self.asleep = true;
                change = Sleep::FellAsleep;
            }
        }

        // Care mistakes: letting it starve or sulk counts once per episode.
        if self.full <= 0.0 && !self.starving {
            self.starving = true;
            self.mistake();
        } else if self.full > 25.0 {
            self.starving = false;
        }
        if self.happy <= 0.0 && !self.sulking {
            self.sulking = true;
            self.mistake();
        } else if self.happy > 25.0 {
            self.sulking = false;
        }
        change
    }

    /// Books a battle result. Returns the new rank if this win earned a promotion.
    pub fn record_battle(&mut self, won: bool) -> Option<u8> {
        if !won {
            self.losses += 1;
            self.happy = (self.happy - 8.0).max(0.0);
            return None;
        }
        self.wins += 1;
        self.happy = (self.happy + 10.0).min(100.0);
        if self.rank as usize >= RANKS.len() - 1 {
            return None; // already at the top: nothing left to count towards
        }
        self.rank_wins += 1;
        if self.rank_wins < WINS_TO_RANK_UP {
            return None;
        }
        self.rank += 1;
        self.rank_wins = 0;
        Some(self.rank)
    }

    /// Needs drift for time the pet wasn't being simulated (app closed, PC asleep): half-speed drain
    /// that never starves it below 15, and a long enough gap counts as a full night's sleep.
    pub fn catch_up(&mut self, secs: u64) {
        if self.stage() == Stage::Egg {
            return;
        }
        let away = secs as f32;
        self.full = (self.full - away * FULL_RATE * 0.5).max(self.full.min(15.0));
        self.happy = (self.happy - away * HAPPY_RATE * 0.5).max(self.happy.min(15.0));
        if secs > 1800 {
            self.energy = 100.0;
            self.asleep = false;
        }
    }

    fn dir() -> Option<std::path::PathBuf> {
        let appdata = std::path::PathBuf::from(std::env::var_os("APPDATA")?);
        let dir = appdata.join("desklings");
        let old = appdata.join("digidesktop"); // the project's original name
        if !dir.exists() && old.exists() && std::fs::rename(&old, &dir).is_err() {
            return Some(old); // couldn't move it: keep using it rather than orphaning the save
        }
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn path() -> Option<std::path::PathBuf> {
        Some(Pet::dir()?.join("state.txt"))
    }

    fn hall_path() -> Option<std::path::PathBuf> {
        Some(Pet::dir()?.join("halloffame.txt"))
    }

    /// Adds this monster to the Hall of Fame file.
    pub fn enshrine(&self) {
        use std::io::Write;
        let Some(path) = Pet::hall_path() else { return };
        let info = self.species.info();
        let line = format!(
            "Gen {} · {} ({}) · lived {}d · Rank {} · {}W {}L · stats {} (total {})\n",
            self.generation,
            info.name,
            info.stage.name(),
            self.age() / DAY,
            RANKS[self.rank as usize],
            self.wins,
            self.losses,
            self.stats.map(|v| v.to_string()).join("/"),
            total(&self.stats),
        );
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    /// The most recent Hall of Fame entries, newest first.
    pub fn hall_of_fame(limit: usize) -> Vec<String> {
        let text = Pet::hall_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        text.lines().rev().take(limit).map(str::to_string).collect()
    }

    /// Writes the save to a temp file, flushes it to disk, then swaps it in, so a crash or power
    /// cut leaves either the old save or the new one. Returns false if it couldn't save.
    pub fn save(&self, x: f32) -> bool {
        use std::io::Write;
        let Some(path) = Pet::path() else { return false };
        let tmp = path.with_extension("tmp");
        let written = std::fs::File::create(&tmp).and_then(|mut f| {
            f.write_all(self.serialize(x, now()).as_bytes())?;
            f.sync_all()
        });
        written.is_ok() && std::fs::rename(&tmp, &path).is_ok()
    }

    pub fn serialize(&self, x: f32, now: u64) -> String {
        let poops: Vec<String> = self.poops.iter().map(|(x, y)| format!("{x}:{y}")).collect();
        let stats: Vec<String> = self.stats.iter().map(|v| v.to_string()).collect();
        let flags = self.starving as u8 | (self.sulking as u8) << 1;
        format!(
            "born={}\nspecies={}\nstage_since={}\nstats={}\nfull={}\nhappy={}\nenergy={}\npoops={}\nasleep={}\n\
             next_poop={}\nmistakes={}\nflags={}\nwins={}\nlosses={}\nrank={}\nrank_wins={}\nseen={}\n\
             generation={}\nlife_mod={}\ncare_day={}\nmistakes_mark={}\nx={}\nsaved={}\n",
            self.born,
            self.species.info().name,
            self.stage_since,
            stats.join(","),
            self.full,
            self.happy,
            self.energy,
            poops.join(","),
            self.asleep as u8,
            self.next_poop,
            self.mistakes,
            flags,
            self.wins,
            self.losses,
            self.rank,
            self.rank_wins,
            self.seen,
            self.generation,
            self.life_mod,
            self.care_day,
            self.mistakes_mark,
            x,
            now
        )
    }

    /// Parses a save file. Missing keys fall back to defaults so older saves keep loading, but a
    /// file that looks damaged (cut off, impossible birth time, unknown species) is an error.
    pub fn parse(text: &str, now: u64) -> Result<(Pet, Option<f32>), String> {
        let mut p = Pet::new(0);
        let (mut born, mut species, mut seen, mut x, mut saved) = (None, None, None, None, None);
        let num = |v: &str, default: f32| v.parse::<f32>().ok().filter(|f| f.is_finite()).unwrap_or(default).clamp(0.0, 100.0);
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "born" => born = v.parse::<u64>().ok(),
                "species" => species = Some(Species::from_name(v).ok_or_else(|| format!("unknown species {v:?}"))?),
                "stage_since" => p.stage_since = v.parse().unwrap_or(p.stage_since),
                "stats" => {
                    let s: Vec<u16> = v.split(',').filter_map(|n| n.parse().ok()).collect();
                    if s.len() == 5 {
                        for (dst, src) in p.stats.iter_mut().zip(s) {
                            *dst = src.min(STAT_MAX);
                        }
                    }
                }
                "full" => p.full = num(v, p.full),
                "happy" => p.happy = num(v, p.happy),
                "energy" => p.energy = num(v, p.energy),
                "poops" => {
                    p.poops = v
                        .split(',')
                        .filter_map(|xy| xy.split_once(':'))
                        .filter_map(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))
                        .take(MAX_POOPS)
                        .collect()
                }
                "asleep" => p.asleep = v == "1",
                "next_poop" => p.next_poop = v.parse().unwrap_or(0),
                "mistakes" => p.mistakes = v.parse().unwrap_or(0),
                "flags" => {
                    let f: u8 = v.parse().unwrap_or(0);
                    p.starving = f & 1 != 0;
                    p.sulking = f & 2 != 0;
                }
                "wins" => p.wins = v.parse().unwrap_or(0),
                "losses" => p.losses = v.parse().unwrap_or(0),
                "rank" => p.rank = v.parse::<u8>().unwrap_or(0).min(RANKS.len() as u8 - 1),
                "rank_wins" => p.rank_wins = v.parse().unwrap_or(0),
                "seen" => seen = v.parse().ok(),
                "generation" => p.generation = v.parse().unwrap_or(1).max(1),
                "life_mod" => p.life_mod = v.parse().unwrap_or(0),
                "care_day" => p.care_day = v.parse().unwrap_or(0),
                "mistakes_mark" => p.mistakes_mark = v.parse().unwrap_or(0),
                "x" => x = v.parse::<f32>().ok().filter(|f| f.is_finite()),
                "saved" => saved = v.parse::<u64>().ok(),
                _ => {}
            }
        }

        // Every save ends with `saved=...` and a newline; anything else was cut off mid-write.
        if !text.ends_with('\n') {
            return Err("file ends mid-line (truncated?)".into());
        }
        let saved = saved.ok_or("no saved= line (file truncated?)")?;
        p.born = born.filter(|&b| b <= now).ok_or("missing or impossible born=")?;
        p.species = match species {
            Some(s) => s,
            // Saves from before evolution paths: carry on as whatever its age implies.
            None => {
                p.stage_since = now;
                match now - p.born {
                    0..60 => Species::Egg,
                    60..660 => Species::Blip,
                    660..4260 => Species::Blop,
                    _ => Species::Raptin,
                }
            }
        };
        p.seen = seen.unwrap_or_else(|| {
            let line = [Species::Egg, Species::Blip, Species::Blop];
            line.iter().filter(|s| s.info().stage <= p.stage()).fold(p.species.bit(), |m, s| m | s.bit())
        });
        p.catch_up(now.saturating_sub(saved));
        Ok((p, x))
    }

    /// Loads the saved pet. A damaged save is kept aside as `state.bad.<time>.txt`
    /// and a new egg starts instead, keeping the collection if it can be read. The message says why.
    pub fn load() -> (Pet, Option<f32>, Option<String>) {
        let Some(path) = Pet::path() else { return (Pet::new(0), None, None) };
        let Ok(text) = std::fs::read_to_string(&path) else { return (Pet::new(0), None, None) };
        match Pet::parse(&text, now()) {
            Ok((p, x)) => (p, x, None),
            Err(why) => {
                // Keep every damaged save under its own name; copying works even if moving doesn't.
                let bad = path.with_file_name(format!("state.bad.{}.txt", now()));
                let kept = std::fs::rename(&path, &bad).is_ok() || std::fs::copy(&path, &bad).is_ok();
                let seen = text.lines().find_map(|l| l.strip_prefix("seen=")?.parse().ok()).unwrap_or(0);
                let note = if kept {
                    format!("Save file was damaged ({why}); kept a copy as {}.", bad.file_name().unwrap_or_default().to_string_lossy())
                } else {
                    format!("Save file was damaged ({why}) and couldn't be backed up.")
                };
                (Pet::new(seen), None, Some(note))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successor_inherits_a_tenth_capped() {
        let mut p = Pet::new(0);
        p.stats = [999, 300, 45, 0, 120];
        p.generation = 3;
        let kid = p.successor();
        assert_eq!(kid.stats, [60, 40, 14, 10, 22]);
        assert_eq!(kid.generation, 4);
        assert_eq!(kid.species, Species::Egg);
    }

    #[test]
    fn care_changes_lifespan() {
        let mut p = Pet::new(0);
        let base = p.lifespan();
        p.mistake();
        p.mistake();
        assert_eq!(p.lifespan(), base - 2 * MISTAKE_COST as u64);
        for _ in 0..100 {
            p.mistake();
        }
        assert_eq!(p.lifespan(), MIN_LIFESPAN);
    }

    const NOW: u64 = 1_800_000_000;

    fn sample() -> Pet {
        let mut p = Pet::new(0b1011);
        p.born = NOW - 3 * DAY;
        p.species = Species::Mystifur;
        p.stage_since = NOW - DAY;
        p.stats = [70, 50, 60, 90, 140];
        p.full = 42.5;
        p.happy = 77.0;
        p.energy = 31.0;
        p.poops = vec![(100, 200), (-50, 1300)];
        p.asleep = true;
        p.next_poop = NOW + 99;
        p.mistakes = 2;
        p.starving = true;
        p.wins = 7;
        p.losses = 3;
        p.rank = 2;
        p.rank_wins = 1;
        p.generation = 4;
        p.life_mod = -3600;
        p.care_day = 3;
        p.mistakes_mark = 2;
        p
    }

    #[test]
    fn save_round_trips() {
        let a = sample();
        let (b, x) = Pet::parse(&a.serialize(123.0, NOW), NOW).unwrap();
        assert_eq!(x, Some(123.0));
        assert_eq!(a.serialize(123.0, NOW), b.serialize(123.0, NOW));
        // Spot-check fields directly too, in case serialize and parse both forget one.
        assert_eq!((b.species, b.stats, b.wins, b.rank, b.generation), (Species::Mystifur, [70, 50, 60, 90, 140], 7, 2, 4));
        assert_eq!((b.poops.clone(), b.asleep, b.starving, b.life_mod, b.seen), (vec![(100, 200), (-50, 1300)], true, true, -3600, 0b1011));
    }

    #[test]
    fn cut_inside_the_last_line_is_rejected() {
        let text = sample().serialize(0.0, NOW);
        let mid = text.find("saved=").unwrap() + 9;
        assert!(Pet::parse(&text[..mid], NOW).is_err(), "saved= cut mid-number must not load");
        assert!(Pet::parse(text.trim_end(), NOW).is_err(), "missing final newline");
    }

    #[test]
    fn busy_pet_stays_awake_until_done() {
        let mut p = sample();
        p.asleep = false;
        p.energy = DOZE_BELOW - 1.0;
        assert_eq!(p.live_second(false), Sleep::Unchanged);
        assert!(!p.asleep);
        assert_eq!(p.live_second(true), Sleep::FellAsleep);
        assert!(p.asleep);
        p.energy = 99.99;
        assert_eq!(p.live_second(true), Sleep::Woke);
        assert_eq!(p.energy, 100.0);
    }

    #[test]
    fn starving_is_one_mistake_per_episode() {
        let mut p = sample();
        p.asleep = false;
        p.full = 0.001;
        p.starving = false;
        let before = p.mistakes;
        for _ in 0..100 {
            p.live_second(true);
        }
        assert_eq!(p.mistakes, before + 1);
        p.full = 50.0;
        p.live_second(true);
        assert!(!p.starving);
    }

    #[test]
    fn rank_stops_counting_at_the_top() {
        let mut p = sample();
        p.rank = 0;
        p.rank_wins = 0;
        assert_eq!(p.record_battle(true), None);
        assert_eq!(p.record_battle(true), None);
        assert_eq!(p.record_battle(true), Some(1));
        assert_eq!(p.rank_wins, 0);
        p.rank = (RANKS.len() - 1) as u8;
        for _ in 0..300 {
            assert_eq!(p.record_battle(true), None);
        }
        assert_eq!(p.rank_wins, 0, "no progress counter at max rank");
        let losses = p.losses;
        p.record_battle(false);
        assert_eq!(p.losses, losses + 1);
    }

    #[test]
    fn truncated_or_garbage_saves_are_rejected() {
        let text = sample().serialize(0.0, NOW);
        let cut = &text[..text.find("saved=").unwrap()];
        assert!(Pet::parse(cut, NOW).is_err(), "cut-off file must not load");
        assert!(Pet::parse("", NOW).is_err());
        assert!(Pet::parse("born=17", NOW).is_err());
        let future = text.replace(&format!("born={}", NOW - 3 * DAY), &format!("born={}", NOW + 10));
        assert!(Pet::parse(&future, NOW).is_err());
        let renamed = text.replace("species=Mystifur", "species=Mystimon");
        assert!(Pet::parse(&renamed, NOW).is_err(), "unknown species must not be treated as a legacy save");
    }

    #[test]
    fn legacy_save_still_loads() {
        // The very first save format: no species, stats or record.
        let born = NOW - 2 * 3600;
        let text = format!("born={born}
full=50
happy=60
energy=70
poops=10:20
asleep=0
next_poop=0
x=5
saved={NOW}
");
        let (p, x) = Pet::parse(&text, NOW).unwrap();
        assert_eq!(p.species, Species::Raptin);
        assert_eq!(p.stats, [10; 5]);
        assert_eq!(p.poops, vec![(10, 20)]);
        assert_eq!(x, Some(5.0));
        assert_ne!(p.seen & Species::Blop.bit(), 0);
    }

    #[test]
    fn loaded_values_are_clamped() {
        let text = sample().serialize(0.0, NOW).replace("stats=70,50,60,90,140", "stats=1200,5,65535,0,999").replace("full=42.5", "full=NaN").replace("happy=77", "happy=-5");
        let (p, _) = Pet::parse(&text, NOW).unwrap();
        assert_eq!(p.stats, [999, 5, 999, 0, 999]);
        assert_eq!(p.full, 80.0);
        assert_eq!(p.happy, 0.0);
    }

    #[test]
    fn time_away_drains_gently() {
        let mut p = sample();
        p.full = 90.0;
        p.catch_up(10 * DAY);
        assert_eq!(p.full, 15.0);
        assert_eq!(p.energy, 100.0);
        assert!(!p.asleep);
        let mut low = sample();
        low.happy = 5.0;
        low.catch_up(DAY);
        assert_eq!(low.happy, 5.0, "already below the floor: left alone");
    }

    #[test]
    fn mistakes_spoil_good_days() {
        let mut p = Pet::new(0);
        p.born -= DAY + 5;
        let base = p.lifespan();
        p.mistake();
        p.review_day();
        assert_eq!(p.lifespan(), base - MISTAKE_COST as u64, "a day with a mistake earns nothing");

        let mut gap = Pet::new(0);
        gap.born -= 4 * DAY + 5;
        let base = gap.lifespan();
        gap.mistake();
        gap.review_day();
        assert_eq!(gap.lifespan(), base - MISTAKE_COST as u64 + 3 * GOOD_DAY_BONUS as u64, "4 days, 1 spoiled");
    }

    #[test]
    fn mistake_free_day_adds_life() {
        let mut p = Pet::new(0);
        p.born -= DAY + 5;
        let base = p.lifespan();
        p.review_day();
        assert_eq!(p.lifespan(), base + GOOD_DAY_BONUS as u64);
        p.review_day(); // same day: no double bonus
        assert_eq!(p.lifespan(), base + GOOD_DAY_BONUS as u64);
    }

    #[test]
    fn elderly_in_last_fifteen_percent() {
        let mut p = Pet::new(0);
        p.born -= p.lifespan() * 80 / 100;
        assert!(!p.elderly());
        p.born -= p.lifespan() * 10 / 100;
        assert!(p.elderly());
    }
}
