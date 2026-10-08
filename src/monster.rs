//! Species, stats, evolution rules, training and battle maths. Pure game logic — no Windows code.

use crate::sprites::*;

pub const LIFE: usize = 0;
pub const POW: usize = 1;
pub const DEF: usize = 2;
pub const SPD: usize = 3;
pub const WIS: usize = 4;
pub const STAT_NAMES: [&str; 5] = ["Life", "Power", "Defense", "Speed", "Wisdom"];
pub const STAT_MAX: u16 = 999;
pub type Stats = [u16; 5];

pub fn total(s: &Stats) -> u32 {
    s.iter().map(|&v| v as u32).sum()
}

/// Raises a stat (capped at STAT_MAX) and returns how much it actually went up.
pub fn add(stats: &mut Stats, i: usize, n: u16) -> u16 {
    let before = stats[i];
    stats[i] = stats[i].saturating_add(n).min(STAT_MAX);
    stats[i].saturating_sub(before)
}

/// Small xorshift PRNG — plenty for wandering, training rolls and battles.
pub struct Rng(pub u64);

impl Rng {
    pub fn below(&mut self, n: u32) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % n.max(1) as u64) as u32
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Stage {
    Egg,
    Fresh,
    InTraining,
    Rookie,
    Champion,
    Ultimate,
}

impl Stage {
    pub fn name(self) -> &'static str {
        match self {
            Stage::Egg => "Egg",
            Stage::Fresh => "Fresh",
            Stage::InTraining => "In-Training",
            Stage::Rookie => "Rookie",
            Stage::Champion => "Champion",
            Stage::Ultimate => "Ultimate",
        }
    }

    /// Minimum time (seconds) spent in this stage before it can evolve.
    pub fn duration(self) -> Option<u64> {
        match self {
            Stage::Egg => Some(60),
            Stage::Fresh => Some(600),
            Stage::InTraining => Some(3600),
            Stage::Rookie => Some(86_400),
            Stage::Champion => Some(172_800),
            Stage::Ultimate => None,
        }
    }

    fn train_mult(self) -> f32 {
        match self {
            Stage::Egg => 0.0,
            Stage::Fresh => 0.5,
            Stage::InTraining => 0.75,
            Stage::Rookie => 1.0,
            Stage::Champion => 1.4,
            Stage::Ultimate => 1.8,
        }
    }

    /// One-off boost (times species growth) to every stat on reaching this stage.
    fn evolve_bonus(self) -> f32 {
        match self {
            Stage::Egg | Stage::Fresh => 0.0,
            Stage::InTraining => 3.0,
            Stage::Rookie => 10.0,
            Stage::Champion => 30.0,
            Stage::Ultimate => 60.0,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Pal {
    pub body: u32,
    pub belly: u32,
    pub accent: u32,
}

impl Pal {
    /// Darker variant so wild opponents read as "not yours".
    pub fn wild(self) -> Pal {
        fn dim(c: u32) -> u32 {
            let ch = |shift: u32| (((c >> shift) & 0xFF) * 3 / 4) << shift;
            0xFF00_0000 | ch(16) | ch(8) | ch(0)
        }
        Pal { body: dim(self.body), belly: dim(self.belly), accent: dim(self.accent) }
    }
}

const fn pal(body: u32, belly: u32, accent: u32) -> Pal {
    Pal { body, belly, accent }
}

pub struct Info {
    pub name: &'static str,
    pub stage: Stage,
    pub frames: &'static [Sprite],
    pub pal: Pal,
    /// Training/evolution multipliers for Life, Power, Defense, Speed, Wisdom.
    pub growth: [f32; 5],
    pub blurb: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Species {
    Egg,
    Blip,
    Blop,
    Raptin,
    Fluffin,
    Shellby,
    Pyrorex,
    Cragdon,
    Galewing,
    Mystifur,
    Bulwark,
    Tidecrest,
    Grumbloo,
    Infernax,
    Seraphox,
    Titanshell,
}

pub const ALL_SPECIES: [Species; 16] = [
    Species::Egg,
    Species::Blip,
    Species::Blop,
    Species::Raptin,
    Species::Fluffin,
    Species::Shellby,
    Species::Pyrorex,
    Species::Cragdon,
    Species::Galewing,
    Species::Mystifur,
    Species::Bulwark,
    Species::Tidecrest,
    Species::Grumbloo,
    Species::Infernax,
    Species::Seraphox,
    Species::Titanshell,
];

// Indexed by `Species as usize`, so keep it in enum order.
static INFO: [Info; 16] = [
    Info {
        name: "Egg",
        stage: Stage::Egg,
        frames: &[EGG],
        pal: pal(0xFF6BCB77, 0xFF6BCB77, 0xFF6BCB77),
        growth: [1.0; 5],
        blurb: "Something is wiggling inside.",
    },
    Info {
        name: "Blip",
        stage: Stage::Fresh,
        frames: &[BLIP_A, BLIP_B],
        pal: pal(0xFF7AC8FF, 0xFFC8E8FF, 0xFF7AC8FF),
        growth: [1.0; 5],
        blurb: "A bouncy blob of pure data.",
    },
    Info {
        name: "Blop",
        stage: Stage::InTraining,
        frames: &[BLOP],
        pal: pal(0xFFB58CFF, 0xFFE4D4FF, 0xFFB58CFF),
        growth: [1.0; 5],
        blurb: "Has ears now. Very proud of them.",
    },
    Info {
        name: "Raptin",
        stage: Stage::Rookie,
        frames: &[RAPTIN],
        pal: pal(0xFFFF9442, 0xFFFFE0A8, 0xFFFF9442),
        growth: [1.0, 1.4, 0.9, 1.0, 0.7],
        blurb: "A scrappy little dino who bites first.",
    },
    Info {
        name: "Fluffin",
        stage: Stage::Rookie,
        frames: &[FLUFFIN],
        pal: pal(0xFFFFE08A, 0xFFFFFFFF, 0xFFFF9EC4),
        growth: [0.8, 0.8, 0.8, 1.4, 1.2],
        blurb: "Quick, curious, and impossible to pet just once.",
    },
    Info {
        name: "Shellby",
        stage: Stage::Rookie,
        frames: &[SHELLBY],
        pal: pal(0xFF7BD389, 0xFFF4E3A1, 0xFF3E8E7E),
        growth: [1.3, 0.8, 1.4, 0.6, 0.9],
        blurb: "Slow and steady. Mostly slow.",
    },
    Info {
        name: "Pyrorex",
        stage: Stage::Champion,
        frames: &[PYROREX],
        pal: pal(0xFFE8503A, 0xFFFFD08A, 0xFFFFB627),
        growth: [1.0, 1.6, 0.9, 1.0, 0.7],
        blurb: "Its tail flame burns hotter with every win.",
    },
    Info {
        name: "Cragdon",
        stage: Stage::Champion,
        frames: &[CRAGDON],
        pal: pal(0xFF9A98A8, 0xFFD9C7A3, 0xFF5E5C6E),
        growth: [1.2, 1.2, 1.5, 0.6, 0.7],
        blurb: "Skin like bedrock, temper like a landslide.",
    },
    Info {
        name: "Galewing",
        stage: Stage::Champion,
        frames: &[GALEWING],
        pal: pal(0xFF4FA3F7, 0xFFE6F4FF, 0xFFFFC234),
        growth: [0.8, 1.0, 0.7, 1.7, 1.0],
        blurb: "Strikes before you see it move.",
    },
    Info {
        name: "Mystifur",
        stage: Stage::Champion,
        frames: &[MYSTIFUR],
        pal: pal(0xFFC9A0FF, 0xFFFFFFFF, 0xFF3B3B98),
        growth: [0.8, 0.7, 0.8, 1.1, 1.7],
        blurb: "Reads spellbooks for fun. The hat is load-bearing.",
    },
    Info {
        name: "Bulwark",
        stage: Stage::Champion,
        frames: &[BULWARK],
        pal: pal(0xFF5DAE5B, 0xFFF1D98A, 0xFF8A5A2B),
        growth: [1.3, 0.8, 1.7, 0.5, 0.9],
        blurb: "A walking fortress with spikes for manners.",
    },
    Info {
        name: "Tidecrest",
        stage: Stage::Champion,
        frames: &[TIDECREST],
        pal: pal(0xFF2EC4B6, 0xFFDFF7F3, 0xFF1B6CA8),
        growth: [1.6, 1.0, 1.1, 0.8, 0.9],
        blurb: "Rides the tide and never runs out of breath.",
    },
    Info {
        name: "Grumbloo",
        stage: Stage::Champion,
        frames: &[GRUMBLOO],
        pal: pal(0xFF9BC53D, 0xFFD4E79E, 0xFF6A4C93),
        growth: [0.8, 0.6, 0.8, 0.6, 0.6],
        blurb: "Neglected and gooey. Still loves you, somehow.",
    },
    Info {
        name: "Infernax",
        stage: Stage::Ultimate,
        frames: &[INFERNAX],
        pal: pal(0xFFB22C2C, 0xFFFFC56B, 0xFF4A1942),
        growth: [1.2, 1.8, 1.2, 1.1, 0.9],
        blurb: "Wings of ash, heart of a volcano.",
    },
    Info {
        name: "Seraphox",
        stage: Stage::Ultimate,
        frames: &[SERAPHOX],
        pal: pal(0xFFF7F3FF, 0xFFFFE9A8, 0xFFFFD34D),
        growth: [1.0, 1.1, 0.9, 1.8, 1.8],
        blurb: "A guardian fox said to grant swift wisdom.",
    },
    Info {
        name: "Titanshell",
        stage: Stage::Ultimate,
        frames: &[TITANSHELL],
        pal: pal(0xFF6E8B3D, 0xFFE8D8A8, 0xFF8C8C9C),
        growth: [1.8, 1.1, 1.8, 0.7, 1.0],
        blurb: "An island that decided to walk.",
    },
];

impl Species {
    pub fn info(self) -> &'static Info {
        &INFO[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Species> {
        ALL_SPECIES.iter().copied().find(|s| s.info().name == name)
    }

    pub fn bit(self) -> u32 {
        1 << self as u32
    }
}

const CHAMPION_MIN_TOTAL: u32 = 150;
const CHAMPION_DUD_MISTAKES: u32 = 5;
const ULTIMATE_MIN_TOTAL: u32 = 500;
const ULTIMATE_MIN_WINS: u32 = 5;
const ULTIMATE_MAX_MISTAKES: u32 = 3;

/// What `sp` becomes once it has spent long enough in its stage, or None if it can't evolve (yet).
pub fn evolution(sp: Species, s: &Stats, mistakes: u32, wins: u32) -> Option<Species> {
    use Species::*;
    let t = total(s);
    Some(match sp {
        Egg => Blip,
        Blip => Blop,
        // The Rookie follows whatever it was trained in most as a baby. Ties go to Raptin.
        Blop => {
            let power = s[POW];
            let agile = s[SPD].max(s[WIS]);
            let sturdy = s[DEF].max(s[LIFE]);
            if power >= agile && power >= sturdy {
                Raptin
            } else if agile >= sturdy {
                Fluffin
            } else {
                Shellby
            }
        }
        Raptin | Fluffin | Shellby if mistakes >= CHAMPION_DUD_MISTAKES || t < CHAMPION_MIN_TOTAL => Grumbloo,
        Raptin => if s[POW] >= s[DEF] { Pyrorex } else { Cragdon },
        Fluffin => if s[SPD] >= s[WIS] { Galewing } else { Mystifur },
        Shellby => if s[DEF] >= s[LIFE] { Bulwark } else { Tidecrest },
        Pyrorex | Cragdon | Galewing | Mystifur | Bulwark | Tidecrest => {
            if mistakes > ULTIMATE_MAX_MISTAKES || wins < ULTIMATE_MIN_WINS || t < ULTIMATE_MIN_TOTAL {
                return None;
            }
            match sp {
                Pyrorex | Cragdon => Infernax,
                Galewing | Mystifur => Seraphox,
                _ => Titanshell,
            }
        }
        Grumbloo | Infernax | Seraphox | Titanshell => return None,
    })
}

/// Whether `sp` has any evolution at all, given perfect stats and care.
pub fn can_evolve(sp: Species) -> bool {
    evolution(sp, &[STAT_MAX; 5], 0, u32::MAX).is_some()
}

/// "1 day", "2 days", "1 hour"...: how long a stage lasts, for the guide.
fn stage_time(stage: Stage) -> String {
    let secs = stage.duration().unwrap_or(0);
    let (n, unit) = if secs % 86_400 == 0 { (secs / 86_400, "day") } else { (secs / 3600, "hour") };
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

/// Player-facing description of where `sp` can go next. The numbers and durations come from
/// the same constants `evolution` uses; tests check the tie-break wording matches too.
pub fn paths(sp: Species) -> String {
    use Species::*;
    let dud = format!("Grumbloo if {CHAMPION_DUD_MISTAKES}+ care mistakes or total stats under {CHAMPION_MIN_TOTAL}");
    let (as_rookie, as_champion) = (stage_time(Stage::Rookie), stage_time(Stage::Champion));
    let rookie = |a: &str, b: &str, hi: &str, lo: &str| {
        format!("After {as_rookie} as a Rookie:\n  • {a} if {hi} ≥ {lo}\n  • {b} if {lo} > {hi}\n  • {dud}")
    };
    let champion = |into: &str| {
        format!(
            "After {as_champion} as a Champion, becomes {into} with {ULTIMATE_MIN_TOTAL}+ total stats, \
             {ULTIMATE_MIN_WINS}+ battle wins and no more than {ULTIMATE_MAX_MISTAKES} care mistakes."
        )
    };
    match sp {
        Egg => "Hatches into Blip.".into(),
        Blip => "Grows into Blop.".into(),
        Blop => "Becomes a Rookie based on its best stat:\n  • Power → Raptin\n  • Speed or Wisdom → Fluffin\n  • Defense or Life → Shellby".into(),
        Raptin => rookie("Pyrorex", "Cragdon", "Power", "Defense"),
        Fluffin => rookie("Galewing", "Mystifur", "Speed", "Wisdom"),
        Shellby => rookie("Bulwark", "Tidecrest", "Defense", "Life"),
        Pyrorex | Cragdon => champion("Infernax"),
        Galewing | Mystifur => champion("Seraphox"),
        Bulwark | Tidecrest => champion("Titanshell"),
        Grumbloo => "A dead end... but a happy one. Start over to try another path.".into(),
        Infernax | Seraphox | Titanshell => "Fully evolved. Keep training and climb to Rank S!".into(),
    }
}

/// Applies the stat boost for arriving at `sp`'s stage.
pub fn evolve_bonus(sp: Species, stats: &mut Stats) {
    let info = sp.info();
    let b = info.stage.evolve_bonus();
    for i in 0..5 {
        add(stats, i, (b * info.growth[i]).round() as u16);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drill {
    Run,
    Lift,
    Endure,
    Study,
    Swim,
}

impl Drill {
    pub const ALL: [Drill; 5] = [Drill::Run, Drill::Lift, Drill::Endure, Drill::Study, Drill::Swim];

    pub fn label(self) -> &'static str {
        match self {
            Drill::Run => "Run\tSpeed, Life",
            Drill::Lift => "Lift\tPower, Defense",
            Drill::Endure => "Endure\tDefense, Life",
            Drill::Study => "Study\tWisdom, Speed",
            Drill::Swim => "Swim\tLife, Power",
        }
    }

    pub fn name(self) -> &'static str {
        self.label().split('\t').next().unwrap_or("")
    }

    /// (main stat, side stat)
    pub fn stats(self) -> (usize, usize) {
        match self {
            Drill::Run => (SPD, LIFE),
            Drill::Lift => (POW, DEF),
            Drill::Endure => (DEF, LIFE),
            Drill::Study => (WIS, SPD),
            Drill::Swim => (LIFE, POW),
        }
    }

    pub fn icon(self) -> Sprite {
        match self {
            Drill::Run => SHOE,
            Drill::Lift => DUMBBELL,
            Drill::Endure => SHIELD,
            Drill::Study => BOOK,
            Drill::Swim => WAVE,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Fail,
    Good,
    Great,
}

/// Resolves a training session. Unhappy monsters slack off; happy ones sometimes nail it;
/// elderly ones only gain a little.
/// Returns the outcome and the (main, side) stat gains.
pub fn train(drill: Drill, sp: Species, stats: &mut Stats, happy: f32, elderly: bool, rng: &mut Rng) -> (Outcome, u16, u16) {
    let fail_pct = ((100.0 - happy) / 4.0).clamp(0.0, 25.0) as u32;
    let great_pct = if happy >= 80.0 { 15 } else { 10 };
    let r = rng.below(100);
    let outcome = if r < fail_pct {
        Outcome::Fail
    } else if r >= 100 - great_pct {
        Outcome::Great
    } else {
        Outcome::Good
    };
    if outcome == Outcome::Fail {
        return (outcome, 0, 0);
    }
    let info = sp.info();
    let (main, side) = drill.stats();
    let great = if outcome == Outcome::Great { 2.0 } else { 1.0 };
    let age = if elderly { 0.5 } else { 1.0 };
    let base = (3 + rng.below(4)) as f32 * info.stage.train_mult() * great * age;
    let gm = add(stats, main, (base * info.growth[main]).round().max(1.0) as u16);
    let gs = add(stats, side, (base * 0.4 * info.growth[side]).round() as u16);
    (outcome, gm, gs)
}

pub const RANKS: [&str; 6] = ["E", "D", "C", "B", "A", "S"];
pub const WINS_TO_RANK_UP: u8 = 3;
/// Total stats of a typical opponent at each rank.
const RANK_BUDGET: [u32; 6] = [60, 130, 230, 360, 520, 750];

pub struct Fighter {
    pub sp: Species,
    pub stats: Stats,
    pub hp: i32,
    pub max_hp: i32,
}

impl Fighter {
    pub fn new(sp: Species, stats: Stats) -> Fighter {
        let max_hp = 20 + stats[LIFE] as i32 * 2;
        Fighter { sp, stats, hp: max_hp, max_hp }
    }
}

/// A wild monster for a battle at `rank`, with stats shaped by its species.
pub fn opponent(rank: usize, rng: &mut Rng) -> Fighter {
    use Species::*;
    let rank = rank.min(RANKS.len() - 1);
    let pool: &[Species] = match rank {
        0 => &[Blop, Raptin, Fluffin, Shellby],
        1 => &[Raptin, Fluffin, Shellby, Grumbloo],
        2 => &[Raptin, Fluffin, Shellby, Pyrorex, Galewing, Bulwark, Grumbloo],
        3 => &[Pyrorex, Cragdon, Galewing, Mystifur, Bulwark, Tidecrest],
        4 => &[Pyrorex, Cragdon, Galewing, Mystifur, Bulwark, Tidecrest, Infernax, Seraphox, Titanshell],
        _ => &[Infernax, Seraphox, Titanshell],
    };
    let sp = pool[rng.below(pool.len() as u32) as usize];
    let g = sp.info().growth;
    let weights: [f32; 5] = std::array::from_fn(|i| g[i] * (0.8 + rng.below(40) as f32 / 100.0));
    let sum: f32 = weights.iter().sum();
    let budget = RANK_BUDGET[rank] as f32;
    let stats = weights.map(|w| ((budget * w / sum).round() as u16).clamp(1, STAT_MAX));
    Fighter::new(sp, stats)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Miss,
    Hit(i32),
    Crit(i32),
}

/// One attack: Speed decides hits and dodges, Wisdom finds weak spots (crits),
/// Power drives damage and Defense soaks it.
pub fn attack(a: &Fighter, d: &Fighter, rng: &mut Rng) -> Hit {
    let st = |f: &Fighter, i: usize| f.stats[i] as f32;
    let hit = (0.8 + 0.4 * (st(a, SPD) - st(d, SPD)) / (st(a, SPD) + st(d, SPD) + 40.0)).clamp(0.5, 0.97);
    if rng.below(1000) as f32 >= hit * 1000.0 {
        return Hit::Miss;
    }
    let crit_chance = 0.05 + 0.25 * st(a, WIS) / (st(a, WIS) + 150.0);
    let crit = (rng.below(1000) as f32) < crit_chance * 1000.0;
    let roll = (85 + rng.below(31)) as f32 / 100.0;
    let dmg = (st(a, POW) + 6.0) * roll * 60.0 / (60.0 + st(d, DEF) * 0.8);
    let dmg = if crit { dmg * 1.75 } else { dmg }.round().max(1.0) as i32;
    if crit { Hit::Crit(dmg) } else { Hit::Hit(dmg) }
}

pub const MAX_TURNS: u32 = 40;

/// The faster monster strikes first; ties go to `me`.
pub fn strikes_first(me: &Fighter, foe: &Fighter) -> bool {
    me.stats[SPD] >= foe.stats[SPD]
}

/// One turn: `atk` attacks `def`, whose HP drops on a hit.
pub fn strike(atk: &Fighter, def: &mut Fighter, rng: &mut Rng) -> Hit {
    let hit = attack(atk, def, rng);
    if let Hit::Hit(d) | Hit::Crit(d) = hit {
        def.hp -= d;
    }
    hit
}

/// `Some(won)` once the battle is over: a knockout, or after MAX_TURNS whoever has the larger
/// share of their HP left (ties go to `me`).
pub fn outcome(me: &Fighter, foe: &Fighter, turns: u32) -> Option<bool> {
    if me.hp > 0 && foe.hp > 0 && turns < MAX_TURNS {
        return None;
    }
    let share = |f: &Fighter| f.hp.max(0) as i64 * 1_000_000 / f.max_hp.max(1) as i64;
    Some(foe.hp <= 0 || (me.hp > 0 && share(me) >= share(foe)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_table_matches_enum_order() {
        for (i, sp) in ALL_SPECIES.iter().enumerate() {
            assert_eq!(*sp as usize, i, "ALL_SPECIES out of order");
            assert_eq!(sp.info().name, format!("{sp:?}"), "INFO row {i} describes the wrong species");
            assert_eq!(Species::from_name(sp.info().name), Some(*sp));
        }
    }

    #[test]
    fn sprites_are_well_formed() {
        fn check(name: &str, spr: Sprite, (max_w, max_h): (usize, usize)) {
            assert!(!spr.is_empty(), "{name} is empty");
            let w = spr[0].len();
            assert!(spr.iter().all(|r| r.len() == w), "{name} has ragged rows");
            assert!(spr.iter().flat_map(|r| r.bytes()).all(|c| b".kwbdahepronygcs".contains(&c)), "{name} has an unknown pixel char");
            assert!(spr.iter().any(|r| r.bytes().any(|c| c != b'.')), "{name} is blank");
            assert!(w <= max_w && spr.len() <= max_h, "{name} is {w}x{}, over {max_w}x{max_h}", spr.len());
        }
        for sp in ALL_SPECIES {
            let info = sp.info();
            let limit = match info.stage {
                Stage::Champion => (20, 20),
                Stage::Ultimate => (24, 24),
                _ => (16, 16),
            };
            for &frame in info.frames {
                check(info.name, frame, limit);
                // Frames are centred by width, so differing widths would make it jitter.
                assert_eq!(frame[0].len(), info.frames[0][0].len(), "{} frames differ in width", info.name);
            }
        }
        let icons = [HEART, MEAT, ZZZ, BANG, STAR, UP, SWEAT, SHOE, DUMBBELL, SHIELD, BOOK, WAVE, SWORD];
        for icon in icons {
            check("icon", icon, (7, 7));
        }
        check("POOP", POOP, (8, 6));
    }

    #[test]
    fn stages_follow_the_tree() {
        use Species::*;
        for sp in ALL_SPECIES {
            let Some(next) = evolution(sp, &[STAT_MAX; 5], 0, u32::MAX) else { continue };
            assert!(next.info().stage > sp.info().stage, "{sp:?} -> {next:?} doesn't move up a stage");
        }
        assert!(!can_evolve(Grumbloo) && !can_evolve(Infernax));
        assert!(can_evolve(Blop) && can_evolve(Pyrorex));
    }

    #[test]
    fn guide_text_uses_the_real_thresholds() {
        let rookie = paths(Species::Raptin);
        assert!(rookie.contains(&format!("{CHAMPION_DUD_MISTAKES}+ care mistakes")));
        assert!(rookie.contains(&format!("under {CHAMPION_MIN_TOTAL}")));
        let champ = paths(Species::Galewing);
        assert!(champ.contains(&format!("{ULTIMATE_MIN_TOTAL}+ total")));
        assert!(champ.contains(&format!("{ULTIMATE_MIN_WINS}+ battle wins")));
        assert!(champ.contains(&format!("more than {ULTIMATE_MAX_MISTAKES} care")));
    }

    #[test]
    fn guide_names_the_tie_winner_and_real_durations() {
        use Species::*;
        for (sp, hi, lo) in [(Raptin, POW, DEF), (Fluffin, SPD, WIS), (Shellby, DEF, LIFE)] {
            let mut s = [60; 5];
            s[hi] = 80;
            s[lo] = 80;
            let winner = evolution(sp, &s, 0, 0).unwrap();
            let text = paths(sp);
            let tie_line = text.lines().find(|l| l.contains('≥')).unwrap();
            assert!(tie_line.contains(winner.info().name), "{sp:?}: tie goes to {winner:?} but guide says {tie_line:?}");
            assert!(text.starts_with(&format!("After {} as a Rookie", stage_time(Stage::Rookie))));
        }
        assert!(paths(Pyrorex).starts_with(&format!("After {} as a Champion", stage_time(Stage::Champion))));
        assert_eq!(stage_time(Stage::Rookie), "1 day");
        assert_eq!(stage_time(Stage::InTraining), "1 hour");
    }

    /// Mean main-stat gain by outcome over many sessions.
    fn gains(happy: f32, elderly: bool, rng: &mut Rng) -> (f32, f32, usize) {
        let (mut good, mut great, mut n_good, mut n_great) = (0u32, 0u32, 0, 0);
        for _ in 0..4000 {
            match train(Drill::Lift, Species::Raptin, &mut [10; 5], happy, elderly, rng) {
                (Outcome::Good, g, _) => (good, n_good) = (good + g as u32, n_good + 1),
                (Outcome::Great, g, _) => (great, n_great) = (great + g as u32, n_great + 1),
                (Outcome::Fail, ..) => {}
            }
        }
        (good as f32 / n_good as f32, great as f32 / n_great as f32, n_great)
    }

    #[test]
    fn great_sessions_double_gains_and_come_from_happiness() {
        let mut rng = Rng(42);
        let (good, great, greats_happy) = gains(100.0, false, &mut rng);
        assert!((1.8..2.2).contains(&(great / good)), "great/good gain ratio {}", great / good);
        let (_, _, greats_ok) = gains(79.0, false, &mut rng);
        assert!(greats_happy > greats_ok + 100, "happy {greats_happy} vs content {greats_ok} great sessions");
    }

    #[test]
    fn elderly_gain_half_as_much() {
        let mut rng = Rng(43);
        let (young, ..) = gains(100.0, false, &mut rng);
        let (old, ..) = gains(100.0, true, &mut rng);
        assert!((0.4..0.6).contains(&(old / young)), "elderly/young gain ratio {}", old / young);
    }

    #[test]
    fn speed_hits_defense_soaks_crits_hurt() {
        let mut rng = Rng(44);
        let f = |pow: u16, def: u16, spd: u16, wis: u16| Fighter::new(Species::Raptin, [50, pow, def, spd, wis]);
        let mut sample = |a: &Fighter, d: &Fighter| {
            let (mut hits, mut hit_dmg, mut crits, mut crit_dmg) = (0, 0, 0, 0);
            for _ in 0..4000 {
                match attack(a, d, &mut rng) {
                    Hit::Hit(x) => (hits, hit_dmg) = (hits + 1, hit_dmg + x),
                    Hit::Crit(x) => (crits, crit_dmg) = (crits + 1, crit_dmg + x),
                    Hit::Miss => {}
                }
            }
            let rate = (hits + crits) as f32 / 4000.0;
            (rate, hit_dmg as f32 / hits.max(1) as f32, crit_dmg as f32 / crits.max(1) as f32)
        };
        let (fast, _, _) = sample(&f(50, 50, 200, 10), &f(50, 50, 10, 10));
        let (slow, _, _) = sample(&f(50, 50, 10, 10), &f(50, 50, 200, 10));
        assert!(fast > slow + 0.15, "hit rate fast {fast} vs slow {slow}");
        let (_, soft, _) = sample(&f(100, 50, 50, 10), &f(50, 0, 50, 10));
        let (_, hard, _) = sample(&f(100, 50, 50, 10), &f(50, 200, 50, 10));
        assert!(soft > hard * 2.0, "damage vs Defense 0: {soft}, vs 200: {hard}");
        let (_, hit, crit) = sample(&f(100, 50, 50, 300), &f(50, 50, 50, 10));
        assert!((1.6..1.9).contains(&(crit / hit)), "crit/hit damage ratio {}", crit / hit);
    }

    #[test]
    fn stat_gain_never_underflows() {
        let mut s = [1200, 998, 0, 0, 0];
        assert_eq!(add(&mut s, 0, 5), 0);
        assert_eq!(add(&mut s, 1, 5), 1);
        assert_eq!(s[..2], [999, 999]);
    }

    #[test]
    fn battle_outcome_rules() {
        let f = |hp: i32, max: i32| Fighter { sp: Species::Raptin, stats: [10; 5], hp, max_hp: max };
        assert_eq!(outcome(&f(10, 50), &f(10, 50), 3), None, "still going");
        assert_eq!(outcome(&f(10, 50), &f(0, 50), 3), Some(true), "KO");
        assert_eq!(outcome(&f(-4, 50), &f(1, 50), 3), Some(false), "knocked out");
        assert_eq!(outcome(&f(30, 100), &f(20, 50), MAX_TURNS), Some(false), "30% < 40% at time-out");
        assert_eq!(outcome(&f(40, 100), &f(20, 50), MAX_TURNS), Some(true), "tie at time-out goes to me");
    }

    #[test]
    fn rookie_follows_best_stat() {
        assert_eq!(evolution(Species::Blop, &[10; 5], 0, 0), Some(Species::Raptin));
        assert_eq!(evolution(Species::Blop, &[10, 10, 10, 30, 10], 0, 0), Some(Species::Fluffin));
        assert_eq!(evolution(Species::Blop, &[10, 10, 10, 10, 30], 0, 0), Some(Species::Fluffin));
        assert_eq!(evolution(Species::Blop, &[30, 10, 10, 10, 10], 0, 0), Some(Species::Shellby));
        assert_eq!(evolution(Species::Blop, &[10, 10, 30, 10, 10], 0, 0), Some(Species::Shellby));
    }

    #[test]
    fn champion_branches_and_dud() {
        let strong = [40, 80, 40, 40, 40];
        assert_eq!(evolution(Species::Raptin, &strong, 0, 0), Some(Species::Pyrorex));
        assert_eq!(evolution(Species::Raptin, &[40, 40, 80, 40, 40], 0, 0), Some(Species::Cragdon));
        assert_eq!(evolution(Species::Raptin, &strong, 5, 0), Some(Species::Grumbloo));
        assert_eq!(evolution(Species::Raptin, &[10; 5], 0, 0), Some(Species::Grumbloo));
        assert_eq!(evolution(Species::Fluffin, &[40, 40, 40, 40, 80], 0, 0), Some(Species::Mystifur));
        assert_eq!(evolution(Species::Shellby, &[80, 40, 40, 40, 40], 0, 0), Some(Species::Tidecrest));
    }

    #[test]
    fn ultimate_needs_stats_wins_and_care() {
        let big = [120; 5];
        assert_eq!(evolution(Species::Galewing, &big, 0, 5), Some(Species::Seraphox));
        assert_eq!(evolution(Species::Galewing, &big, 0, 4), None);
        assert_eq!(evolution(Species::Galewing, &big, 4, 5), None);
        assert_eq!(evolution(Species::Galewing, &[90; 5], 0, 5), None);
        assert_eq!(evolution(Species::Grumbloo, &big, 0, 99), None);
    }

    #[test]
    fn training_is_capped_and_unhappy_fails_more() {
        let mut rng = Rng(12345);
        let mut s = [998; 5];
        for _ in 0..100 {
            train(Drill::Lift, Species::Infernax, &mut s, 100.0, false, &mut rng);
        }
        assert!(s.iter().all(|&v| v <= STAT_MAX));

        let fails = |happy: f32, rng: &mut Rng| {
            (0..2000)
                .filter(|_| train(Drill::Run, Species::Raptin, &mut [10; 5], happy, false, rng).0 == Outcome::Fail)
                .count()
        };
        assert!(fails(0.0, &mut rng) > fails(100.0, &mut rng) + 300);
    }

    fn fight(a: &mut Fighter, b: &mut Fighter, rng: &mut Rng) -> bool {
        let mut a_turn = strikes_first(a, b);
        for turn in 1.. {
            if a_turn { strike(a, b, rng) } else { strike(b, a, rng) };
            if let Some(won) = outcome(a, b, turn) {
                return won;
            }
            a_turn = !a_turn;
        }
        unreachable!()
    }

    #[test]
    fn stronger_monster_usually_wins() {
        let mut rng = Rng(99);
        let wins = (0..500)
            .filter(|_| {
                let mut me = Fighter::new(Species::Pyrorex, [60, 90, 60, 60, 40]);
                let mut foe = opponent(1, &mut rng);
                fight(&mut me, &mut foe, &mut rng)
            })
            .count();
        assert!(wins > 450, "won {wins}/500");

        // Identical monsters: the rules are symmetric, and striking first is an edge, not a lock.
        let twin = || Fighter::new(Species::Raptin, [30; 5]);
        let first_wins = (0..2000).filter(|_| fight(&mut twin(), &mut twin(), &mut rng)).count();
        assert!((1000..=1400).contains(&first_wins), "first striker won {first_wins}/2000");
        let alternating = (0..2000)
            .filter(|i| if i % 2 == 0 { fight(&mut twin(), &mut twin(), &mut rng) } else { !fight(&mut twin(), &mut twin(), &mut rng) })
            .count();
        assert!((900..=1100).contains(&alternating), "with turns alternating, won {alternating}/2000");
    }

    #[test]
    fn opponent_budget_scales_with_rank() {
        let mut rng = Rng(7);
        let mut last = 0;
        for rank in 0..RANKS.len() {
            let avg = (0..50).map(|_| total(&opponent(rank, &mut rng).stats)).sum::<u32>() / 50;
            let budget = RANK_BUDGET[rank];
            assert!(avg > last, "rank {} not tougher than the one below ({avg} <= {last})", RANKS[rank]);
            assert!(avg.abs_diff(budget) * 10 <= budget, "rank {} averages {avg}, budget {budget}", RANKS[rank]);
            last = avg;
        }
    }
}
