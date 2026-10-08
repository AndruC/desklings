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
pub const MIN_NAP: u64 = 600; // once asleep it sleeps at least this long, even with full energy

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
    pub asleep_since: u64,  // when it last went to sleep (not saved: a reload starts the nap afresh)
}

const DAY: u64 = 86_400;
const BASE_LIFESPAN: u64 = 14 * DAY;
const MIN_LIFESPAN: u64 = 4 * DAY;
pub const MISTAKE_COST: i64 = 12 * 3600;
pub const OVERWORK_COST: i64 = 3 * 3600;
const GOOD_DAY_BONUS: i64 = 12 * 3600;

impl Pet {
    /// A new egg laid at time `t`.
    pub fn new_at(seen: u32, t: u64) -> Pet {
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
            asleep_since: 0,
        }
    }

    /// A fresh egg for the next generation, inheriting a tenth of the retiree's stats (up to +50 each).
    pub fn successor_at(&self, now: u64) -> Pet {
        let mut p = Pet::new_at(self.seen, now);
        p.generation = self.generation.saturating_add(1);
        for (s, parent) in p.stats.iter_mut().zip(self.stats) {
            *s += (parent / 10).min(50);
        }
        p
    }

    pub fn stage(&self) -> Stage {
        self.species.info().stage
    }

    /// Age at time `now`. A clock that's behind the birth time counts as no time passed.
    pub fn age_at(&self, now: u64) -> u64 {
        now.saturating_sub(self.born)
    }

    pub fn mistake(&mut self) {
        self.mistakes = self.mistakes.saturating_add(1);
        self.life_mod = self.life_mod.saturating_sub(MISTAKE_COST);
    }

    /// Total lifespan in seconds: a base that good care stretches and neglect or overwork shortens.
    pub fn lifespan(&self) -> u64 {
        let base = if self.species == Species::Grumbloo { BASE_LIFESPAN * 3 / 4 } else { BASE_LIFESPAN };
        (base as i64 + self.life_mod).max(MIN_LIFESPAN as i64) as u64
    }

    pub fn life_left_at(&self, now: u64) -> u64 {
        self.lifespan().saturating_sub(self.age_at(now))
    }

    /// In its twilight years: the last 15% of its life.
    pub fn elderly_at(&self, now: u64) -> bool {
        self.age_at(now) * 100 >= self.lifespan() * 85
    }

    pub fn elderly(&self) -> bool {
        self.elderly_at(now())
    }

    /// Called every second. Each full day of age earns extra lifespan, whether or not the app was
    /// running, except that every care mistake made since the last review spoils one day's bonus.
    pub fn review_day(&mut self, now: u64) {
        let day = self.age_at(now) / DAY;
        if day > self.care_day {
            let spoiled = self.mistakes.saturating_sub(self.mistakes_mark) as u64;
            let good_days = (day - self.care_day).saturating_sub(spoiled);
            self.life_mod += GOOD_DAY_BONUS * good_days as i64;
            self.care_day = day;
            self.mistakes_mark = self.mistakes;
        }
    }

    /// Lights out: it goes to sleep now, and naps at least MIN_NAP even if it isn't tired.
    pub fn sleep_at(&mut self, now: u64) {
        self.asleep = true;
        self.asleep_since = now;
    }

    /// One second of life: hunger, happiness and energy drift, sleep, and care mistakes.
    /// `may_doze` is false while it's busy (battling, training) so it finishes before nodding off.
    pub fn live_second(&mut self, now: u64, may_doze: bool) -> Sleep {
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
            if self.energy >= 100.0 && now >= self.asleep_since.saturating_add(MIN_NAP) {
                self.asleep = false;
                change = Sleep::Woke;
            }
        } else {
            let tire = if self.elderly_at(now) { 1.5 } else { 1.0 };
            self.energy = (self.energy - TIRE_RATE * tire).max(0.0);
            if self.energy < DOZE_BELOW && may_doze {
                self.sleep_at(now);
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
            self.losses = self.losses.saturating_add(1);
            self.happy = (self.happy - 8.0).max(0.0);
            return None;
        }
        self.wins = self.wins.saturating_add(1);
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

    /// Adds this monster to the Hall of Fame file, once: each line ends with a hidden ` #<born>`
    /// tag so a retirement interrupted before the save is not recorded twice. Returns false if
    /// the file couldn't be written.
    pub fn enshrine(&self, now: u64) -> bool {
        use std::io::Write;
        let Some(path) = Pet::hall_path() else { return false };
        let tag = format!(" #{}", self.born);
        // If the file exists but can't be read, don't append: it might already hold this entry.
        let Some(existing) = read_text(&path) else { return false };
        if existing.lines().any(|l| l.ends_with(&tag)) {
            return true;
        }
        let info = self.species.info();
        let line = format!(
            "Gen {} · {} ({}) · lived {}d · Rank {} · {}W {}L · stats {} (total {}){tag}\n",
            self.generation,
            info.name,
            info.stage.name(),
            self.age_at(now) / DAY,
            RANKS[self.rank as usize],
            self.wins,
            self.losses,
            self.stats.map(|v| v.to_string()).join("/"),
            total(&self.stats),
        );
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| f.write_all(line.as_bytes()))
            .is_ok()
    }

    /// The most recent Hall of Fame entries, newest first.
    pub fn hall_of_fame(limit: usize) -> Vec<String> {
        let text = Pet::hall_path().and_then(|p| read_text(&p)).unwrap_or_default();
        let shown = |l: &str| l.rsplit_once(" #").map_or(l, |(entry, _)| entry).to_string();
        text.lines().rev().take(limit).map(shown).collect()
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
    /// file that looks damaged is an error: cut off, a value that doesn't parse, or an unknown
    /// species. Times are kept exactly as saved even if the clock is currently behind them: ages
    /// and timers just treat that as no time passed until the clock catches up.
    pub fn parse(text: &str, now: u64) -> Result<(Pet, Option<f32>), String> {
        fn val<T: std::str::FromStr>(k: &str, v: &str) -> Result<T, String> {
            v.parse().map_err(|_| format!("bad {k}= value {v:?}"))
        }
        fn need(k: &str, v: &str) -> Result<f32, String> {
            let f: f32 = val(k, v)?;
            if f.is_finite() { Ok(f.clamp(0.0, 100.0)) } else { Err(format!("bad {k}= value {v:?}")) }
        }
        // Editors like Notepad add an invisible byte-order mark; it isn't part of the data.
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
        // Every save ends with `saved=...` and a newline; anything else was cut off mid-write.
        if !text.ends_with('\n') {
            return Err("file ends mid-line (truncated?)".into());
        }
        let mut p = Pet::new_at(0, now);
        let (mut born, mut species, mut seen, mut x, mut saved) = (None, None, None, None, None);
        let mut has_lifespan = false;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match k {
                "born" => born = Some(val::<u64>(k, v)?),
                "species" => species = Some(Species::from_name(v).ok_or_else(|| format!("unknown species {v:?}"))?),
                "stage_since" => p.stage_since = val(k, v)?,
                "stats" => {
                    let s = v.split(',').map(|n| val::<u16>(k, n)).collect::<Result<Vec<_>, _>>()?;
                    if s.len() != 5 {
                        return Err(format!("stats= has {} values, not 5", s.len()));
                    }
                    for (dst, src) in p.stats.iter_mut().zip(s) {
                        *dst = src.min(STAT_MAX);
                    }
                }
                "full" => p.full = need(k, v)?,
                "happy" => p.happy = need(k, v)?,
                "energy" => p.energy = need(k, v)?,
                // The very first save format stored just a count, with no positions to restore.
                "poops" if v.is_empty() || (!v.contains(':') && v.parse::<u8>().is_ok()) => p.poops.clear(),
                "poops" => {
                    p.poops = v
                        .split(',')
                        .map(|xy| {
                            let (x, y) = xy.split_once(':').ok_or_else(|| format!("bad poops= entry {xy:?}"))?;
                            Ok((val(k, x)?, val(k, y)?))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    p.poops.truncate(MAX_POOPS);
                }
                "asleep" => p.asleep = val::<u8>(k, v)? != 0,
                "next_poop" => p.next_poop = val(k, v)?,
                "mistakes" => p.mistakes = val(k, v)?,
                "flags" => {
                    let f: u8 = val(k, v)?;
                    p.starving = f & 1 != 0;
                    p.sulking = f & 2 != 0;
                }
                "wins" => p.wins = val(k, v)?,
                "losses" => p.losses = val(k, v)?,
                "rank" => p.rank = val::<u8>(k, v)?.min(RANKS.len() as u8 - 1),
                "rank_wins" => p.rank_wins = val(k, v)?,
                "seen" => seen = Some(val(k, v)?),
                "generation" => p.generation = val::<u32>(k, v)?.max(1),
                "life_mod" => {
                    p.life_mod = val(k, v)?;
                    has_lifespan = true;
                }
                "care_day" => p.care_day = val(k, v)?,
                "mistakes_mark" => p.mistakes_mark = val(k, v)?,
                "x" => {
                    let f: f32 = val(k, v)?;
                    if !f.is_finite() {
                        return Err(format!("bad x= value {v:?}"));
                    }
                    x = Some(f);
                }
                "saved" => saved = Some(val::<u64>(k, v)?),
                _ => {}
            }
        }

        let saved = saved.ok_or("no saved= line (file truncated?)")?;
        p.born = born.ok_or("no born= line")?;

        // Values of the right type but absurd size (hand edits, corruption) are damage too:
        // left in, they'd overflow timers and counters. Times may be somewhat ahead of `saved`
        // (the clock can be set back), but not by a year.
        let year = 365 * DAY;
        let ranges = [
            (p.born <= saved.saturating_add(year), "born="),
            (p.stage_since <= saved.saturating_add(year), "stage_since="),
            (p.next_poop <= saved.saturating_add(year), "next_poop="),
            (p.rank_wins < WINS_TO_RANK_UP, "rank_wins="),
            (p.generation <= 1_000_000, "generation="),
            (p.life_mod.unsigned_abs() <= 1000 * DAY, "life_mod="),
            (p.care_day <= 100_000, "care_day="),
            (p.mistakes.max(p.mistakes_mark).max(p.wins).max(p.losses) <= 10_000_000, "a counter"),
        ];
        if let Some((_, what)) = ranges.iter().find(|(ok, _)| !ok) {
            return Err(format!("{what} out of range"));
        }
        p.species = match species {
            Some(s) => s,
            // Saves from before evolution paths: carry on as whatever its age implies.
            None => {
                p.stage_since = now;
                match now.saturating_sub(p.born) {
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
        if !has_lifespan {
            // Saved before lifespans existed: start its lifespan from today rather than charging
            // it for its whole past (an old pet would otherwise retire the moment it loads).
            let age = p.age_at(now);
            p.life_mod = age as i64;
            p.care_day = age / DAY;
        }
        p.catch_up(now.saturating_sub(saved));
        Ok((p, x))
    }

    /// Loads the saved pet.
    ///
    /// - No save file: a new egg.
    /// - A damaged save is kept aside as `state.bad.<time>.txt` and a new egg starts, keeping the
    ///   collection if it can be read; `Damaged` carries the message for the player.
    /// - A save that exists but can't be read (locked by another program, permissions) is
    ///   `Unreadable`: the caller must not start, or its autosave would overwrite the real save.
    pub fn load() -> Loaded {
        let Some(path) = Pet::path() else { return Loaded::Fresh };
        let mut tries = 0;
        let bytes = loop {
            match std::fs::read(&path) {
                Ok(b) => break b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Fresh,
                // Antivirus and backup tools hold files briefly; give them a moment.
                Err(_) if tries < 10 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                Err(e) => return Loaded::Unreadable(format!("{} ({e})", path.display())),
            }
        };
        let text = String::from_utf8_lossy(&bytes);
        let parsed = if text.contains('\u{FFFD}') {
            Err("unreadable characters in the file".to_string())
        } else {
            Pet::parse(&text, now())
        };
        match parsed {
            Ok((p, x)) => Loaded::Ok(p, x),
            Err(why) => {
                // Keep every damaged save under its own name; copying works even if moving doesn't.
                let bad = path.with_file_name(format!("state.bad.{}.txt", now()));
                let kept = std::fs::rename(&path, &bad).is_ok() || std::fs::copy(&path, &bad).is_ok();
                let seen = text.lines().find_map(|l| l.strip_prefix("seen=")?.parse().ok()).unwrap_or(0);
                if !kept {
                    // Starting fresh now would overwrite the only copy at the next autosave.
                    return Loaded::Unreadable(format!("{} is damaged ({why}) and couldn't be backed up", path.display()));
                }
                let note = format!("Save file was damaged ({why}); kept a copy as {}.", bad.file_name().unwrap_or_default().to_string_lossy());
                Loaded::Damaged(Pet::new_at(seen, now()), note)
            }
        }
    }
}

/// Reads a text file, tolerating bytes that aren't UTF-8 (e.g. re-saved as ANSI by an editor).
/// A missing file reads as empty; any other error is None.
fn read_text(path: &std::path::Path) -> Option<String> {
    match std::fs::read(path) {
        Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Err(_) => None,
    }
}

pub enum Loaded {
    Fresh,
    Ok(Pet, Option<f32>),
    Damaged(Pet, String),
    Unreadable(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successor_inherits_a_tenth_capped() {
        let mut p = Pet::new_at(0, NOW);
        p.stats = [999, 300, 45, 0, 120];
        p.generation = 3;
        let kid = p.successor_at(NOW);
        assert_eq!(kid.stats, [60, 40, 14, 10, 22]);
        assert_eq!(kid.generation, 4);
        assert_eq!(kid.species, Species::Egg);
    }

    #[test]
    fn care_changes_lifespan() {
        let mut p = Pet::new_at(0, NOW);
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
        let mut p = Pet::new_at(0b1011, NOW);
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
        p.sulking = true;
        p.wins = 7;
        p.losses = 3;
        p.rank = 2;
        p.rank_wins = 2;
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
        assert_eq!((b.poops.clone(), b.asleep, b.starving, b.sulking, b.life_mod, b.seen), (vec![(100, 200), (-50, 1300)], true, true, true, -3600, 0b1011));
        assert_eq!((b.rank_wins, b.mistakes, b.mistakes_mark, b.care_day, b.losses, b.next_poop), (2, 2, 2, 3, 3, NOW + 99));
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
        assert_eq!(p.live_second(NOW, false), Sleep::Unchanged);
        assert!(!p.asleep);
        assert_eq!(p.live_second(NOW, true), Sleep::FellAsleep);
        assert!(p.asleep);
        p.energy = 99.99;
        assert_eq!(p.live_second(NOW + MIN_NAP, true), Sleep::Woke);
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
            p.live_second(NOW, true);
        }
        assert_eq!(p.mistakes, before + 1);
        p.full = 50.0;
        p.live_second(NOW, true);
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
        let text = sample().serialize(0.0, NOW).replace("stats=70,50,60,90,140", "stats=1200,5,65535,0,999").replace("happy=77", "happy=-5");
        let (p, _) = Pet::parse(&text, NOW).unwrap();
        assert_eq!(p.stats, [999, 5, 999, 0, 999]);
        assert_eq!(p.happy, 0.0);
    }

    #[test]
    fn present_but_garbled_values_are_damage() {
        let text = sample().serialize(0.0, NOW);
        for (from, to) in [
            ("stats=70,50,60,90,140", "stats=70,50,60,9O,140"),
            ("stats=70,50,60,90,140", "stats=70,50,60,90"),
            ("generation=4", "generation=4x"),
            ("full=42.5", "full=NaN"),
            ("wins=7", "wins=seven"),
            ("asleep=1", "asleep=yes"),
            ("poops=100:200,-50:1300", "poops=100:200,-50"),
        ] {
            assert!(Pet::parse(&text.replace(from, to), NOW).is_err(), "{to} should be rejected");
        }
    }

    #[test]
    fn clock_set_back_is_not_damage_and_doesnt_age_it() {
        let text = sample().serialize(0.0, NOW);
        let behind = NOW - 5 * DAY;
        let (p, _) = Pet::parse(&text, behind).expect("a clock behind the save must still load");
        assert_eq!(p.born, NOW - 3 * DAY, "birth time kept exactly as saved");
        assert_eq!(p.stage_since, NOW - DAY);
        assert_eq!(p.age_at(behind), 0, "clock behind: no time has passed");
        // Saved while the clock was wrong, then loaded once it's fixed: true age, not age + skew.
        let (again, _) = Pet::parse(&p.serialize(0.0, behind), NOW).unwrap();
        assert_eq!(again.age_at(NOW), 3 * DAY);
    }

    #[test]
    fn absurd_values_are_damage_not_overflow() {
        let text = sample().serialize(0.0, NOW);
        for (from, to) in [
            (format!("stage_since={}", NOW - DAY), "stage_since=18446744073709551605".to_string()),
            (format!("born={}", NOW - 3 * DAY), "born=18446744073709551605".to_string()),
            ("rank_wins=2".to_string(), "rank_wins=255".to_string()),
            ("generation=4".to_string(), "generation=4294967295".to_string()),
            ("life_mod=-3600".to_string(), "life_mod=-9223372036854775807".to_string()),
            ("wins=7".to_string(), "wins=4294967295".to_string()),
            ("x=0".to_string(), "x=NaN".to_string()),
        ] {
            assert!(text.contains(&from), "fixture lacks {from}");
            assert!(Pet::parse(&text.replace(&from, &to), NOW).is_err(), "{to} should be rejected");
        }
        // A clock set back a few days is still fine (see clock_set_back_is_not_damage...).
        let behind = text.replace(&format!("saved={NOW}"), &format!("saved={}", NOW - 5 * DAY));
        assert!(Pet::parse(&behind, NOW).is_ok());
    }

    #[test]
    fn very_old_saves_get_a_fresh_lifespan() {
        let born = NOW - 30 * DAY; // the first save format had no lifespan at all
        let text = format!("born={born}\nfull=50\nhappy=60\nenergy=70\npoops=\nasleep=0\nnext_poop=0\nx=5\nsaved={NOW}\n");
        let (p, _) = Pet::parse(&text, NOW).unwrap();
        assert!(p.life_left_at(NOW) >= BASE_LIFESPAN, "a 30-day-old legacy pet must not retire on load");
        assert_eq!(p.care_day, 30, "and doesn't get 30 days of back-pay bonuses either");
        let mut q = p;
        q.review_day(NOW);
        assert_eq!(q.life_left_at(NOW), BASE_LIFESPAN);
    }

    #[test]
    fn lights_out_at_full_energy_still_naps() {
        let mut p = sample();
        p.asleep = false;
        p.energy = 100.0;
        p.sleep_at(NOW);
        assert_eq!(p.live_second(NOW + 1, true), Sleep::Unchanged, "mustn't pop awake at once");
        assert!(p.asleep);
        assert_eq!(p.live_second(NOW + MIN_NAP, true), Sleep::Woke);
    }

    #[test]
    fn successor_is_born_now_and_generation_never_wraps() {
        let mut p = sample();
        p.generation = u32::MAX;
        let kid = p.successor_at(NOW + 7);
        assert_eq!((kid.born, kid.stage_since, kid.generation), (NOW + 7, NOW + 7, u32::MAX));
    }

    #[test]
    fn hall_of_fame_text_survives_an_ansi_resave() {
        let dir = std::env::temp_dir().join(format!("desklings-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("halloffame.txt");
        // "Gen 1 · Raptin ... #123" written back by an editor as Windows-1252: '·' is byte 0xB7.
        std::fs::write(&file, b"Gen 1 \xB7 Raptin (Rookie) #123\n").unwrap();
        let text = read_text(&file).expect("readable");
        assert!(text.lines().any(|l| l.ends_with(" #123")), "dedupe tag still found: {text:?}");
        assert_eq!(read_text(&dir.join("missing.txt")), Some(String::new()), "missing file reads as empty");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn byte_order_mark_is_ignored() {
        let text = format!("\u{FEFF}{}", sample().serialize(0.0, NOW));
        let (p, _) = Pet::parse(&text, NOW).expect("BOM must not count as damage");
        assert_eq!(p.born, NOW - 3 * DAY);
    }

    #[test]
    fn first_format_poop_count_still_loads() {
        let text = format!("born={}\nfull=50\nhappy=60\nenergy=70\npoops=2\nasleep=0\nnext_poop=0\nx=5\nsaved={NOW}\n", NOW - 600);
        let (p, _) = Pet::parse(&text, NOW).unwrap();
        assert!(p.poops.is_empty());
    }

    #[test]
    fn sulking_is_one_mistake_per_episode() {
        let mut p = sample();
        p.asleep = false;
        p.full = 100.0;
        p.happy = 0.001;
        p.sulking = false;
        let before = p.mistakes;
        for _ in 0..100 {
            p.live_second(NOW, true);
        }
        assert_eq!(p.mistakes, before + 1);
    }

    #[test]
    fn needs_drain_at_the_documented_rates() {
        let fresh = || {
            let mut p = sample();
            p.asleep = false;
            p.poops.clear();
            p.full = 80.0;
            p.happy = 80.0;
            p.energy = 80.0;
            p.born = NOW; // young, so not elderly
            p.life_mod = 0;
            p
        };
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let drop = |p: &mut Pet| {
            let (f, h, e) = (p.full, p.happy, p.energy);
            p.live_second(NOW, false);
            (f - p.full, h - p.happy, e - p.energy)
        };

        let (df, dh, de) = drop(&mut fresh());
        assert!(close(df, FULL_RATE) && close(dh, HAPPY_RATE) && close(de, TIRE_RATE));

        let mut messy = fresh();
        messy.poops = vec![(0, 0), (1, 1)];
        assert!(close(drop(&mut messy).1, 3.0 * HAPPY_RATE), "two poops triple the sadness");

        let mut hungry = fresh();
        hungry.full = 10.0;
        assert!(close(drop(&mut hungry).1, 2.0 * HAPPY_RATE), "hunger doubles it");

        let mut sleepy = fresh();
        sleepy.asleep = true;
        let (df, dh, de) = drop(&mut sleepy);
        assert!(close(df, 0.5 * FULL_RATE) && close(dh, 0.5 * HAPPY_RATE) && close(de, -REST_RATE));

        let mut old = fresh();
        old.born = NOW - old.lifespan() * 9 / 10;
        assert!(old.elderly_at(NOW));
        let (_, _, de) = drop(&mut old);
        assert!(close(de, 1.5 * TIRE_RATE), "elderly tire faster");
    }

    #[test]
    fn grumbloo_lives_shorter() {
        let mut p = Pet::new_at(0, NOW);
        let normal = p.lifespan();
        p.species = Species::Grumbloo;
        assert_eq!(p.lifespan(), normal * 3 / 4);
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
        let mut p = Pet::new_at(0, NOW);
        p.born = NOW - DAY - 5;
        let base = p.lifespan();
        p.mistake();
        p.review_day(NOW);
        assert_eq!(p.lifespan(), base - MISTAKE_COST as u64, "a day with a mistake earns nothing");

        let mut gap = Pet::new_at(0, NOW);
        gap.born = NOW - 4 * DAY - 5;
        let base = gap.lifespan();
        gap.mistake();
        gap.review_day(NOW);
        assert_eq!(gap.lifespan(), base - MISTAKE_COST as u64 + 3 * GOOD_DAY_BONUS as u64, "4 days, 1 spoiled");
    }

    #[test]
    fn mistake_free_day_adds_life() {
        let mut p = Pet::new_at(0, NOW);
        p.born = NOW - DAY - 5;
        let base = p.lifespan();
        p.review_day(NOW);
        assert_eq!(p.lifespan(), base + GOOD_DAY_BONUS as u64);
        p.review_day(NOW); // same day: no double bonus
        assert_eq!(p.lifespan(), base + GOOD_DAY_BONUS as u64);
    }

    #[test]
    fn elderly_in_last_fifteen_percent() {
        let mut p = Pet::new_at(0, NOW);
        let at = |p: &mut Pet, pct: u64| {
            p.born = NOW - p.lifespan() * pct / 100;
            p.elderly_at(NOW)
        };
        assert!(!at(&mut p, 84));
        assert!(at(&mut p, 85));
        assert!(at(&mut p, 86));
    }
}
