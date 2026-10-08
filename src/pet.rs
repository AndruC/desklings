//! The persistent pet: who it is, its needs and stats, its care record, and the save file.

use crate::monster::*;
use crate::now;

pub const MAX_POOPS: usize = 3;

// Needs drain rates, in points per second.
pub const FULL_RATE: f32 = 100.0 / 5400.0; // empty in 90 min
pub const HAPPY_RATE: f32 = 100.0 / 7200.0; // empty in 2 h (faster with poop / hunger)
pub const TIRE_RATE: f32 = 100.0 / 10800.0; // tired after 3 h awake
pub const REST_RATE: f32 = 100.0 / 1800.0; // rested after 30 min of sleep

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

    /// Called every second: a full day of age without a new care mistake earns extra lifespan.
    pub fn review_day(&mut self) {
        let day = self.age() / DAY;
        if day > self.care_day {
            if self.mistakes == self.mistakes_mark {
                self.life_mod += GOOD_DAY_BONUS;
            }
            self.care_day = day;
            self.mistakes_mark = self.mistakes;
        }
    }

    fn path() -> Option<std::path::PathBuf> {
        let appdata = std::path::PathBuf::from(std::env::var_os("APPDATA")?);
        let dir = appdata.join("desklings");
        let old = appdata.join("digidesktop"); // the project's original name
        if !dir.exists() && old.exists() {
            let _ = std::fs::rename(&old, &dir);
        }
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.join("state.txt"))
    }

    fn hall_path() -> Option<std::path::PathBuf> {
        Some(Pet::path()?.with_file_name("halloffame.txt"))
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

    pub fn save(&self, x: f32) {
        let Some(path) = Pet::path() else { return };
        let poops: Vec<String> = self.poops.iter().map(|(x, y)| format!("{x}:{y}")).collect();
        let stats: Vec<String> = self.stats.iter().map(|v| v.to_string()).collect();
        let flags = self.starving as u8 | (self.sulking as u8) << 1;
        let s = format!(
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
            now()
        );
        let _ = std::fs::write(path, s);
    }

    /// Loads the saved pet and applies a gentle catch-up for the time it was closed.
    pub fn load() -> Option<(Pet, Option<f32>)> {
        let text = std::fs::read_to_string(Pet::path()?).ok()?;
        let mut p = Pet::new(0);
        let (mut species, mut seen, mut x, mut saved) = (None, None, None, now());
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "born" => p.born = v.parse().ok()?,
                "species" => species = Species::from_name(v),
                "stage_since" => p.stage_since = v.parse().unwrap_or(p.stage_since),
                "stats" => {
                    let s: Vec<u16> = v.split(',').filter_map(|n| n.parse().ok()).collect();
                    if s.len() == 5 {
                        p.stats.copy_from_slice(&s);
                    }
                }
                "full" => p.full = v.parse().unwrap_or(p.full),
                "happy" => p.happy = v.parse().unwrap_or(p.happy),
                "energy" => p.energy = v.parse().unwrap_or(p.energy),
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
                "generation" => p.generation = v.parse().unwrap_or(1),
                "life_mod" => p.life_mod = v.parse().unwrap_or(0),
                "care_day" => p.care_day = v.parse().unwrap_or(0),
                "mistakes_mark" => p.mistakes_mark = v.parse().unwrap_or(0),
                "x" => x = v.parse().ok(),
                "saved" => saved = v.parse().unwrap_or(saved),
                _ => {}
            }
        }

        p.species = match species {
            Some(s) => s,
            // Saves from before evolution paths: carry on as whatever its age implies.
            None => {
                p.stage_since = now();
                match now().saturating_sub(p.born) {
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

        let away = now().saturating_sub(saved) as f32;
        if p.stage() != Stage::Egg {
            // Half-speed drain while away, and never starve it below 15 just for being closed.
            p.full = (p.full - away * FULL_RATE * 0.5).max(p.full.min(15.0));
            p.happy = (p.happy - away * HAPPY_RATE * 0.5).max(p.happy.min(15.0));
            if away > 1800.0 {
                p.energy = 100.0;
                p.asleep = false;
            }
        }
        Some((p, x))
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
