#![windows_subsystem = "windows"]

mod monster;
mod pet;
mod sprites;

use monster::*;
use pet::*;
use sprites::*;
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// Logical canvas (in art pixels): up to 24x24 monster on the left, speech bubble top-right.
const LW: i32 = 35;
const LH: i32 = 27;
const CX: i32 = 12; // monster's centre column
// Screen pixels per art pixel: 4 at 100% DPI, set once at startup from the system DPI.
static SCALE: AtomicI32 = AtomicI32::new(4);
fn scale() -> i32 {
    SCALE.load(Relaxed)
}
fn cw() -> i32 {
    LW * scale()
}
fn ch() -> i32 {
    LH * scale()
}
const TICK_MS: u32 = 100;
/// Gaps longer than this (PC asleep, clock jump) are handled like time away instead of replayed.
const CATCH_UP_LIMIT: u64 = 120;

/// Posted to the buddy window when its lifespan runs out, so the farewell box opens outside the tick.
const WM_RETIRE: u32 = WM_APP + 1;

const POOP_CLASS: &str = "DesklingsPoop";
const FOE_CLASS: &str = "DesklingsFoe";

const INK: u32 = 0xFF1A1A2E;
const WHITE: u32 = 0xFFFFFFFF;

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Idle,
    Walk,
    Eat,
    Joy,
    Train(Drill),
    Show(Sprite), // react with an icon for a moment
    Battle,
}

/// A premultiplied-BGRA bitmap drawn in art pixels and pushed to a layered window.
struct Surface {
    dc: HDC,
    bits: *mut u32,
    lw: i32,
    lh: i32,
    cx: i32, // monster's centre column; past the middle puts the bubble on the left instead
}

impl Surface {
    fn new(lw: i32, lh: i32) -> Option<Surface> {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = lw * scale();
            bmi.bmiHeader.biHeight = -lh * scale(); // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = null_mut();
            let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if dc.is_null() || bmp.is_null() || bits.is_null() {
                return None;
            }
            SelectObject(dc, bmp);
            Some(Surface { dc, bits: bits as *mut u32, lw, lh, cx: CX })
        }
    }

    fn size(&self) -> (i32, i32) {
        (self.lw * scale(), self.lh * scale())
    }

    fn clear(&mut self) {
        let (w, h) = self.size();
        unsafe { std::slice::from_raw_parts_mut(self.bits, (w * h) as usize) }.fill(0);
    }

    fn put(&mut self, lx: i32, ly: i32, color: u32) {
        if !(0..self.lw).contains(&lx) || !(0..self.lh).contains(&ly) {
            return;
        }
        let (w, h) = self.size();
        let s = scale();
        let buf = unsafe { std::slice::from_raw_parts_mut(self.bits, (w * h) as usize) };
        for y in ly * s..(ly + 1) * s {
            let row = (y * w) as usize;
            buf[row + (lx * s) as usize..row + ((lx + 1) * s) as usize].fill(color);
        }
    }

    /// Draws a sprite. `flash` paints everything but the outline white (hit / evolving).
    fn blit(&mut self, spr: Sprite, ox: i32, oy: i32, mirror: bool, pal: Pal, asleep: bool, flash: bool) {
        let w = spr.iter().map(|r| r.len()).max().unwrap_or(0) as i32;
        for (ry, row) in spr.iter().enumerate() {
            for (rx, c) in row.bytes().enumerate() {
                let color = match c {
                    b'k' => INK,
                    b'w' => WHITE,
                    b'b' => pal.body,
                    b'd' => pal.belly,
                    b'a' => pal.accent,
                    b'p' => 0xFFFF8FA3,
                    b'h' => if asleep { pal.body } else { WHITE },
                    b'e' => if asleep { pal.body } else { INK },
                    b'r' => 0xFFE63946,
                    b'o' => 0xFFC8553D,
                    b'n' => 0xFF8B5A2B,
                    b'y' => 0xFFFFD166,
                    b'g' => 0xFF57CC99,
                    b'c' => 0xFF4CC9F0,
                    b's' => 0xFFB8C0CC,
                    _ => continue,
                };
                let color = if flash && color != INK { WHITE } else { color };
                let x = if mirror { w - 1 - rx as i32 } else { rx as i32 };
                self.put(ox + x, oy + ry as i32, color);
            }
        }
    }

    /// Draws `spr` standing on the bottom of the canvas, centred on `cx`. Returns its top row.
    fn blit_standing(&mut self, spr: Sprite, dx: i32, dy: i32, mirror: bool, pal: Pal, asleep: bool, flash: bool) -> i32 {
        let w = spr.iter().map(|r| r.len()).max().unwrap_or(0) as i32;
        let bottom = spr.iter().rposition(|r| r.bytes().any(|c| c != b'.')).unwrap_or(0) as i32;
        let top = spr.iter().position(|r| r.bytes().any(|c| c != b'.')).unwrap_or(0) as i32;
        let oy = LH - 1 - bottom + dy;
        self.blit(spr, self.cx - w / 2 + dx, oy, mirror, pal, asleep, flash);
        oy + top
    }

    fn hp_bar(&mut self, top: i32, hp: i32, max: i32) {
        let y = (top - 4).max(0);
        let (x0, w) = (self.cx - 9, 19);
        let fill = ((hp.max(0) * (w - 2) + max - 1) / max.max(1)).clamp(0, w - 2);
        let color = if fill * 2 > w - 2 { 0xFF57CC99 } else if fill * 4 > w - 2 { 0xFFFFD166 } else { 0xFFE63946 };
        for x in 0..w {
            self.put(x0 + x, y, INK);
            self.put(x0 + x, y + 2, INK);
            let inner = x > 0 && x < w - 1;
            self.put(x0 + x, y + 1, if !inner { INK } else if x - 1 < fill { color } else { 0xFF3A3A4A });
        }
    }

    /// 11x11 rounded speech bubble beside the monster's head with a tail, holding a 7-wide icon.
    fn bubble(&mut self, icon: Sprite, bob: i32) {
        let left = self.cx > self.lw / 2;
        let (bx, by) = (if left { 0 } else { self.lw - 11 }, bob);
        for y in 0..11 {
            for x in 0..11 {
                let corner = (x == 0 || x == 10) && (y == 0 || y == 10);
                let edge = x == 0 || x == 10 || y == 0 || y == 10;
                if !corner {
                    self.put(bx + x, by + y, if edge { INK } else { WHITE });
                }
            }
        }
        let (t0, t1) = if left { (bx + 8, bx + 9) } else { (bx + 2, bx + 1) };
        self.put(t0, by + 10, WHITE);
        self.put(t0, by + 11, INK);
        self.put(t1, by + 12, INK);
        let ih = icon.len() as i32;
        self.blit(icon, bx + 2, by + 1 + (9 - ih) / 2, false, Pal::default(), false, false);
    }

    fn present(&self, hwnd: HWND, x: i32, y: i32) {
        let (w, h) = self.size();
        unsafe {
            let pt = POINT { x, y };
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(hwnd, null_mut(), &pt, &size, self.dc, &src, 0, &blend, ULW_ALPHA);
        }
    }
}

struct Battle {
    me: Fighter,
    foe: Fighter,
    side: i32, // +1: the foe stands to the right of the buddy
    foe_x: f32,
    foe_target: f32,
    foe_y: f32,
    my_turn: bool,
    step_t: u32,
    lunge: (u32, u32), // ticks left for (me, foe)
    flash: (u32, u32),
    turns: u32,
    result: Option<bool>, // Some(won) once decided
    end_t: u32,
}

struct App {
    hwnd: HWND,
    canvas: Surface,
    poop_gfx: Surface,
    poop_wnds: Vec<HWND>, // parallel to pet.poops
    foe_hwnd: HWND,
    foe_canvas: Surface,
    battle: Option<Battle>,
    pet: Pet,
    news: String, // last training / battle result, shown in the menu
    x: f32,
    y: f32,
    vy: f32,
    dir: i32,
    act: Act,
    act_t: u32,
    frame: u32,
    evo_flash: u32,
    held: Option<(i32, i32, i32, i32)>, // grab offset x/y, cursor start x/y
    dragged: bool,
    rng: Rng,
    retiring: bool, // retirement announced, waiting for the message box
    last_second: u64,
    last_save: u64,
    save_failed: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Runs `f` on the app unless it's missing or already borrowed (re-entrant message).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn cursor() -> POINT {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    p
}

fn work_area(hwnd: HWND) -> RECT {
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi) != 0 {
            return mi.rcWork;
        }
        // Monitors can vanish mid-reconfiguration; fall back to the primary work area.
        let mut r: RECT = std::mem::zeroed();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut c_void, 0);
        r
    }
}

fn duration(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
        _ => format!("{}d {}h", secs / 86400, secs % 86400 / 3600),
    }
}

fn sprite_width(spr: Sprite) -> i32 {
    spr.iter().map(|r| r.len()).max().unwrap_or(0) as i32
}

impl App {
    fn set_act(&mut self, act: Act, ticks: u32) {
        self.act = act;
        self.act_t = ticks;
    }

    fn on_ground(&self) -> bool {
        self.vy == 0.0 && self.held.is_none()
    }

    /// Jump with a velocity given in "100% DPI" pixels per tick.
    fn hop(&mut self, v: f32) {
        if self.on_ground() {
            self.vy = -v * scale() as f32 / 4.0;
        }
    }

    /// Saves, remembering whether it worked so the menu warns until a save succeeds again.
    fn persist(&mut self) -> bool {
        let ok = self.pet.save(self.x);
        self.save_failed = !ok;
        ok
    }

    /// Quitting mid-battle counts as running away: the loss is booked before saving.
    fn forfeit(&mut self) {
        if self.battle.take().is_some() {
            self.pet.record_battle(false);
            unsafe { ShowWindow(self.foe_hwnd, SW_HIDE) };
        }
    }

    fn busy(&self) -> bool {
        self.pet.asleep || self.battle.is_some() || matches!(self.act, Act::Train(_))
    }

    /// The single source of truth for what the player may do right now. The menu greys out
    /// whatever this rejects and `command` refuses it, so scripted WM_COMMANDs obey it too.
    fn allowed(&self, cmd: i32) -> bool {
        let p = &self.pet;
        let scene = self.battle.is_some() || matches!(self.act, Act::Train(_)) || self.retiring;
        let free = !p.asleep && p.stage() != Stage::Egg && !scene;
        match cmd {
            CMD_FEED | CMD_PLAY => free,
            c if (CMD_TRAIN..CMD_TRAIN + Drill::ALL.len() as i32).contains(&c) => free,
            CMD_BATTLE => free && p.stage() >= Stage::InTraining,
            CMD_CLEAN => !p.poops.is_empty() && !scene,
            CMD_SLEEP => p.stage() != Stage::Egg && !scene,
            CMD_RETIRE => !scene && p.stage() >= Stage::Rookie,
            CMD_RESET => !scene,
            CMD_GUIDE | CMD_HALL | CMD_QUIT => true,
            _ => false,
        }
    }

    /// Once per second: evolution, ageing, needs, care mistakes, poop, autosave.
    fn second(&mut self) {
        let t = now();
        if let Some(next) = self.due_evolution(t) {
            self.evolve(next);
        }
        self.pet.review_day();
        if self.pet.life_left() == 0 && !self.retiring && self.battle.is_none() && self.held.is_none() {
            self.retiring = true;
            unsafe { PostMessageW(self.hwnd, WM_RETIRE, 0, 0) };
        }
        let busy = self.battle.is_some() || matches!(self.act, Act::Train(_));
        match self.pet.live_second(!busy) {
            Sleep::Woke => self.set_act(Act::Joy, 15),
            // Reactions don't tick while asleep, so drop any pending one or it sticks.
            Sleep::FellAsleep => self.set_act(Act::Idle, 20),
            Sleep::Unchanged => {}
        }

        // Wait until it's standing on the ground so the poop lands where it is.
        if self.pet.next_poop != 0 && t >= self.pet.next_poop && self.on_ground() && self.battle.is_none() {
            self.pet.next_poop = 0;
            self.drop_poop();
        }
        if t >= self.last_save + 60 || t < self.last_save {
            self.last_save = t;
            self.persist();
        }
    }

    fn due_evolution(&self, t: u64) -> Option<Species> {
        let dur = self.pet.stage().duration()?;
        let retiring = self.pet.life_left() == 0;
        let busy = self.battle.is_some() || self.held.is_some() || matches!(self.act, Act::Train(_));
        if t < self.pet.stage_since + dur || retiring || busy {
            return None;
        }
        evolution(self.pet.species, &self.pet.stats, self.pet.mistakes, self.pet.wins)
    }

    fn evolve(&mut self, next: Species) {
        let from = self.pet.species.info().name;
        self.pet.species = next;
        self.pet.stage_since = now();
        self.pet.seen |= next.bit();
        self.pet.asleep = false;
        evolve_bonus(next, &mut self.pet.stats);
        self.news = format!("{from} evolved into {}!", next.info().name);
        self.evo_flash = 30;
        self.set_act(Act::Show(STAR), 30);
        self.hop(14.0);
        self.persist();
    }

    fn choose_next(&mut self) {
        if self.pet.stage() == Stage::Egg {
            return self.set_act(Act::Idle, 50);
        }
        let p = &self.pet;
        let lazy = p.full < 20.0 || p.happy < 20.0 || p.energy < 25.0 || p.elderly();
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

    fn current_sprite(&self) -> Sprite {
        self.pet.species.info().frames[0]
    }

    fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        // Timers drift and stop while the PC sleeps, so run per-second logic off the clock.
        let t = now();
        if t != self.last_second {
            let gap = t.saturating_sub(self.last_second);
            self.last_second = t;
            if gap > CATCH_UP_LIMIT {
                self.pet.catch_up(gap); // long pause: treat it like time away
                self.second();
            } else {
                for _ in 0..gap {
                    self.second();
                }
            }
        }
        self.evo_flash = self.evo_flash.saturating_sub(1);
        if self.battle.is_some() {
            self.battle_tick();
        }

        if self.held.is_none() {
            let s = scale() as f32;
            let wa = work_area(self.hwnd);
            let ground = (wa.bottom - ch()) as f32;
            let half = sprite_width(self.current_sprite()) / 2;
            let min_x = (wa.left - (CX - half) * scale()) as f32;
            let max_x = (wa.right - cw()) as f32; // keep the bubble on screen too

            if self.y < ground || self.vy < 0.0 {
                self.vy += 0.375 * s;
                self.y += self.vy;
            }
            if self.y >= ground {
                self.y = ground;
                self.vy = 0.0;
            }

            if !self.pet.asleep && self.act != Act::Battle {
                let speed = match self.act {
                    Act::Walk if self.pet.stage() <= Stage::Fresh => 0.17,
                    Act::Walk => 0.35,
                    Act::Train(Drill::Run) => {
                        if self.act_t % 8 == 0 {
                            self.dir = -self.dir;
                        }
                        0.8
                    }
                    _ => 0.0,
                };
                if speed > 0.0 && self.on_ground() {
                    let speed = if self.pet.elderly() { speed * 0.6 } else { speed };
                    self.x += self.dir as f32 * speed * s;
                    if self.x <= min_x {
                        self.dir = 1;
                    } else if self.x >= max_x {
                        self.dir = -1;
                    }
                }
                self.act_t = self.act_t.saturating_sub(1);
                if self.act_t == 0 {
                    match self.act {
                        Act::Train(d) => self.finish_training(d),
                        _ => self.choose_next(),
                    }
                }
            }
            self.x = self.x.clamp(min_x, max_x.max(min_x));
        }
        self.render();
    }

    // ------------------------------------------------------------------ training

    fn start_training(&mut self, d: Drill) {
        if self.busy() || self.pet.stage() == Stage::Egg {
            return;
        }
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

    fn finish_training(&mut self, d: Drill) {
        let elderly = self.pet.elderly();
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
                    self.hop(10.0);
                    STAR
                } else {
                    UP
                }
            }
        };
        self.set_act(Act::Show(icon), 20);
        self.persist();
    }

    // ------------------------------------------------------------------ battles

    fn start_battle(&mut self) {
        if self.busy() || self.pet.stage() < Stage::InTraining {
            return;
        }
        if self.pet.energy < 20.0 {
            self.news = "Too tired to battle. Let it rest.".into();
            return self.set_act(Act::Show(SWEAT), 20);
        }
        self.overwork_check();
        self.pet.energy -= 20.0;
        self.pet.full = (self.pet.full - 8.0).max(0.0);

        let foe = opponent(self.pet.rank as usize, &mut self.rng);
        let me = Fighter::new(self.pet.species, self.pet.stats);
        let s = scale() as f32;
        let wa = work_area(self.hwnd);
        let ground = (wa.bottom - ch()) as f32;
        let mid = (wa.left + wa.right) as f32 / 2.0;
        let side = if self.x + CX as f32 * s < mid { 1 } else { -1 };
        self.foe_canvas.cx = if side > 0 { CX } else { LW - 1 - CX };
        let foe_target = self.x + (CX + side * 35 - self.foe_canvas.cx) as f32 * s;
        self.dir = side;
        self.y = ground;
        self.vy = 0.0;
        self.battle = Some(Battle {
            my_turn: strikes_first(&me, &foe),
            me,
            foe,
            side,
            foe_x: foe_target + side as f32 * 30.0 * s, // walks in from further away
            foe_target,
            foe_y: ground,
            step_t: 6,
            lunge: (0, 0),
            flash: (0, 0),
            turns: 0,
            result: None,
            end_t: 0,
        });
        self.set_act(Act::Battle, 0);
        self.render();
        unsafe {
            ShowWindow(self.foe_hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn battle_tick(&mut self) {
        let s = scale() as f32;
        let Some(b) = self.battle.as_mut() else { return };
        b.lunge = (b.lunge.0.saturating_sub(1), b.lunge.1.saturating_sub(1));
        b.flash = (b.flash.0.saturating_sub(1), b.flash.1.saturating_sub(1));

        let gap = b.foe_x - b.foe_target;
        if gap.abs() > 0.5 {
            b.foe_x -= gap.signum() * (1.5 * s).min(gap.abs());
            return;
        }
        if let Some(won) = b.result {
            b.end_t = b.end_t.saturating_sub(1);
            if b.end_t == 0 {
                self.finish_battle(won);
            }
            return;
        }
        if b.step_t > 0 {
            b.step_t -= 1;
            return;
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
    }

    fn finish_battle(&mut self, won: bool) {
        let Some(b) = self.battle.take() else { return };
        unsafe { ShowWindow(self.foe_hwnd, SW_HIDE) };
        let foe = b.foe.sp.info().name;
        let rank = RANKS[self.pet.rank as usize];
        let promoted = self.pet.record_battle(won);
        if won {
            let i = self.rng.below(5) as usize;
            add(&mut self.pet.stats, i, 2);
            self.news = format!("Beat a wild {foe} (Rank {rank})! {} +2", STAT_NAMES[i]);
            if let Some(r) = promoted {
                self.news += &format!(" Promoted to Rank {}!", RANKS[r as usize]);
            }
            self.set_act(Act::Show(STAR), 25);
            self.hop(12.0);
        } else {
            self.news = format!("Lost to a wild {foe} (Rank {rank}). Train up and try again!");
            self.set_act(Act::Show(SWEAT), 25);
        }
        self.persist();
    }

    // ------------------------------------------------------------------ poop

    /// Leaves a poop on the ground just behind the buddy, which then wanders off.
    fn drop_poop(&mut self) {
        if self.pet.poops.len() >= MAX_POOPS {
            // Nowhere left to go: that's on you.
            self.pet.mistake();
            self.pet.happy = (self.pet.happy - 10.0).max(0.0);
            return;
        }
        let s = scale();
        let half = sprite_width(self.current_sprite()) / 2;
        let behind = if self.dir > 0 { CX - half - 8 } else { CX + half };
        let x = self.x as i32 + behind * s;
        let y = self.y as i32 + ch() - POOP.len() as i32 * s;
        if let Some(landed) = self.spawn_poop(x, y) {
            self.pet.poops.push(landed);
        }
        if !self.busy() {
            self.set_act(Act::Walk, 30);
        }
    }

    /// Creates a poop window near (x, y), snapped onto that monitor's taskbar. Returns where it
    /// landed, or None if the window couldn't be created.
    fn spawn_poop(&mut self, x: i32, y: i32) -> Option<(i32, i32)> {
        let (w, h) = self.poop_gfx.size();
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                wide(POOP_CLASS).as_ptr(),
                wide("poop").as_ptr(),
                WS_POPUP,
                x,
                y,
                w,
                h,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if hwnd.is_null() {
                return None;
            }
            let wa = work_area(hwnd);
            let x = x.clamp(wa.left, (wa.right - w).max(wa.left));
            let y = wa.bottom - h;
            self.poop_gfx.present(hwnd, x, y);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            // Keep the buddy in front of its mess.
            SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            self.poop_wnds.push(hwnd);
            Some((x, y))
        }
    }

    fn clean_poop(&mut self, hwnd: HWND) {
        if let Some(i) = self.poop_wnds.iter().position(|&h| h == hwnd) {
            self.poop_wnds.remove(i);
            self.pet.poops.remove(i);
            unsafe { DestroyWindow(hwnd) };
            self.pet.happy = (self.pet.happy + 2.0).min(100.0);
            self.persist();
        }
    }

    fn clean_all(&mut self) {
        for h in self.poop_wnds.drain(..) {
            unsafe { DestroyWindow(h) };
        }
        self.pet.poops.clear();
    }

    // ------------------------------------------------------------------ drawing

    fn render(&mut self) {
        self.canvas.clear();
        let f = self.frame;
        let info = self.pet.species.info();
        let asleep = self.pet.asleep;
        let moving = !asleep && matches!(self.act, Act::Walk | Act::Joy | Act::Eat | Act::Train(_));
        let airborne = self.vy != 0.0 || self.held.is_some();
        let beat = if asleep { (f / 15) % 2 } else if moving { (f / 3) % 2 } else { (f / 8) % 2 };

        let (mut dx, mut dy) = (0, 0);
        let mut spr = info.frames[0];
        if info.stage == Stage::Egg {
            let fast = now() >= self.pet.stage_since + 45;
            match f % if fast { 6 } else { 20 } {
                0 | 1 => dx += 1,
                2 | 3 => dx -= 1,
                _ => {}
            }
        } else if beat == 1 && !airborne {
            match info.frames.get(1) {
                Some(&alt) => spr = alt,
                None => dy -= 1,
            }
        }
        match self.act {
            Act::Train(Drill::Endure) => dx += if f % 4 < 2 { 1 } else { -1 },
            Act::Train(Drill::Lift) => dy -= ((f / 5) % 2) as i32,
            _ => {}
        }

        let mut flash = self.evo_flash > 0 && (f / 2) % 2 == 0;
        let mut bar = None;
        let mut result_icon = None;
        if let Some(b) = &self.battle {
            if b.lunge.0 > 0 {
                dx += 2 * b.side;
            }
            flash |= b.flash.0 > 0;
            bar = Some((b.me.hp, b.me.max_hp));
            let approaching = (b.foe_x - b.foe_target).abs() > 0.5;
            result_icon = b.result.map(|won| if won { STAR } else { SWEAT }).or(approaching.then_some(SWORD));
        }

        let top = self.canvas.blit_standing(spr, dx, dy, self.dir < 0, info.pal, asleep, flash);
        if let Some((hp, max)) = bar {
            self.canvas.hp_bar(top, hp, max);
        }

        let blink = (f / 5) % 2 == 0;
        let p = &self.pet;
        let needy = info.stage != Stage::Egg && blink;
        let icon = match self.act {
            _ if result_icon.is_some() => result_icon,
            _ if asleep => Some(ZZZ),
            Act::Show(icon) => Some(icon),
            Act::Battle => None,
            Act::Eat => Some(MEAT),
            Act::Joy => Some(HEART),
            Act::Train(d) => Some(d.icon()),
            _ if needy && p.full < 25.0 => Some(MEAT),
            _ if needy && (p.happy < 25.0 || p.poops.len() >= 2) => Some(BANG),
            _ => None,
        };
        if let Some(icon) = icon {
            self.canvas.bubble(icon, if asleep { ((f / 10) % 2) as i32 } else { 0 });
        }
        self.canvas.present(self.hwnd, self.x as i32, self.y as i32);

        if self.battle.is_some() {
            self.render_foe();
        }
    }

    fn render_foe(&mut self) {
        let Some(b) = &self.battle else { return };
        let c = &mut self.foe_canvas;
        c.clear();
        let info = b.foe.sp.info();
        let f = self.frame;
        let walking = (b.foe_x - b.foe_target).abs() > 0.5;
        let mut spr = info.frames[0];
        let mut dy = 0;
        if (f / if walking { 3 } else { 8 }) % 2 == 1 {
            match info.frames.get(1) {
                Some(&alt) => spr = alt,
                None => dy = -1,
            }
        }
        let dx = if b.lunge.1 > 0 { -2 * b.side } else { 0 };
        // Sprites face right; a foe on the right has to face left.
        let top = c.blit_standing(spr, dx, dy, b.side > 0, info.pal.wild(), false, b.flash.1 > 0);
        if !walking {
            c.hp_bar(top, b.foe.hp, b.foe.max_hp);
        }
        if let Some(won) = b.result {
            c.bubble(if won { SWEAT } else { STAR }, 0);
        }
        c.present(self.foe_hwnd, b.foe_x as i32, b.foe_y as i32);
    }

    // ------------------------------------------------------------------ interaction

    fn poke(&mut self) {
        if self.busy() || self.pet.stage() == Stage::Egg {
            return;
        }
        self.pet.happy = (self.pet.happy + 3.0).min(100.0);
        self.set_act(Act::Joy, 12);
        self.hop(6.0);
    }

    fn command(&mut self, cmd: i32) {
        if !self.allowed(cmd) {
            return;
        }
        if let Some(&d) = Drill::ALL.get((cmd - CMD_TRAIN) as usize) {
            return self.start_training(d);
        }
        let poop_delay = 180 + self.rng.below(420) as u64;
        let p = &mut self.pet;
        match cmd {
            CMD_FEED => {
                if p.full > 90.0 {
                    p.happy = (p.happy - 5.0).max(0.0); // overfed
                }
                p.full = (p.full + 30.0).min(100.0);
                if p.next_poop == 0 {
                    p.next_poop = now() + poop_delay;
                }
                self.set_act(Act::Eat, 20);
            }
            CMD_PLAY => {
                p.happy = (p.happy + 20.0).min(100.0);
                p.energy = (p.energy - 8.0).max(0.0);
                p.full = (p.full - 5.0).max(0.0);
                self.set_act(Act::Joy, 25);
                self.hop(14.0);
            }
            CMD_BATTLE => return self.start_battle(),
            CMD_CLEAN => {
                self.clean_all();
                let p = &mut self.pet;
                p.happy = (p.happy + 5.0).min(100.0);
                self.set_act(Act::Joy, 10);
            }
            CMD_SLEEP => {
                p.asleep = !p.asleep;
                self.set_act(Act::Idle, 20);
            }
            CMD_RESET => {
                self.clean_all();
                self.battle = None;
                unsafe { ShowWindow(self.foe_hwnd, SW_HIDE) };
                self.pet = Pet::new(self.pet.seen);
                self.news.clear();
                self.set_act(Act::Idle, 20);
            }
            _ => return,
        }
        self.persist();
    }

    fn farewell(&self) -> String {
        let p = &self.pet;
        let name = p.species.info().name;
        format!(
            "{name} has lived a full life of {} and retires to the Hall of Fame.\n\n\
             A new egg arrives, carrying on a little of {name}'s strength (Generation {}).",
            duration(p.age()),
            p.generation + 1
        )
    }

    fn retire(&mut self) {
        let enshrined = self.pet.enshrine();
        self.clean_all();
        let name = self.pet.species.info().name;
        self.pet = self.pet.successor();
        self.news = format!("{name} retired. Generation {} begins!", self.pet.generation);
        if !enshrined {
            self.news += " (Couldn't write the Hall of Fame file.)";
        }
        self.retiring = false;
        self.battle = None;
        unsafe { ShowWindow(self.foe_hwnd, SW_HIDE) };
        self.set_act(Act::Idle, 20);
        self.persist();
    }

    fn guide(&self) -> String {
        let p = &self.pet;
        let info = p.species.info();
        let t = now();
        let next = match p.stage().duration() {
            None => "Fully evolved.".to_string(),
            Some(_) if !can_evolve(p.species) => "This is its final form.".into(),
            Some(d) if t < p.stage_since + d => format!("Next evolution in {}.", duration(p.stage_since + d - t)),
            Some(_) if evolution(p.species, &p.stats, p.mistakes, p.wins).is_none() => {
                "Old enough to evolve — still missing a requirement.".into()
            }
            Some(_) => "Evolving any moment now!".into(),
        };
        let stats: Vec<String> = (0..5).map(|i| format!("{} {}", STAT_NAMES[i], p.stats[i])).collect();
        let raised: Vec<&str> = ALL_SPECIES[1..]
            .iter()
            .filter(|s| p.seen & s.bit() != 0)
            .map(|s| s.info().name)
            .collect();
        format!(
            "{} — {} · Generation {}\n\"{}\"\n\n{}\n{}\n\n{}\n\nStats: {} (total {})\nCare mistakes: {} · Battles: {} won, {} lost · Rank {}\n\n\
             Monsters raised ({}/{}): {}",
            info.name,
            info.stage.name(),
            p.generation,
            info.blurb,
            next,
            paths(p.species),
            lifespan_text(p),
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

const CMD_FEED: i32 = 1;
const CMD_PLAY: i32 = 2;
const CMD_CLEAN: i32 = 3;
const CMD_SLEEP: i32 = 4;
const CMD_QUIT: i32 = 5;
const CMD_RESET: i32 = 6;
const CMD_BATTLE: i32 = 7;
const CMD_GUIDE: i32 = 8;
const CMD_HALL: i32 = 9;
const CMD_RETIRE: i32 = 10;
const CMD_TRAIN: i32 = 20; // + drill index

fn lifespan_text(p: &Pet) -> String {
    let left = duration(p.life_left());
    let tip = "Each mistake-free day adds half a day; care mistakes and training while tired take time away.";
    if p.elderly() {
        format!("In its twilight years: about {left} left before it retires. It moves slower and gains less from training.\n{tip}")
    } else {
        format!("Lifespan: about {left} left. {tip}")
    }
}

fn hearts(v: f32) -> String {
    let n = ((v / 20.0).ceil() as usize).min(5);
    "♥".repeat(n) + &"♡".repeat(5 - n)
}

struct MenuState {
    lines: Vec<String>,
    enabled: Vec<i32>,
    asleep: bool,
    rank: &'static str,
    born: u64, // identifies the pet, so a confirmation box can't act on its successor
}

fn show_menu(hwnd: HWND) {
    let Some(st) = with_app(|a| {
        let p = &a.pet;
        let info = p.species.info();
        let mut lines = vec![format!("{} · {}\t{}", info.name, info.stage.name(), duration(now().saturating_sub(p.born)))];
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
            let life = if p.elderly() { "Twilight years" } else { "Lifespan" };
            lines.push(format!("Gen {} · {life}\t~{} left", p.generation, duration(p.life_left())));
        }
        if a.save_failed {
            lines.push(String::new());
            lines.push("⚠ Couldn't save. Check that %APPDATA%\\desklings is writable.".into());
        }
        if !a.news.is_empty() {
            lines.push(String::new());
            lines.push(a.news.clone());
        }
        MenuState {
            lines,
            enabled: (0..CMD_TRAIN + Drill::ALL.len() as i32).filter(|&c| a.allowed(c)).collect(),
            asleep: p.asleep,
            rank: RANKS[p.rank as usize],
            born: p.born,
        }
    }) else {
        return;
    };

    let gray = |cmd: i32| if st.enabled.contains(&cmd) { 0 } else { MF_GRAYED };
    let cmd = unsafe {
        let m = CreatePopupMenu();
        for l in &st.lines {
            if l.is_empty() {
                AppendMenuW(m, MF_SEPARATOR, 0, null());
            } else {
                AppendMenuW(m, MF_STRING | MF_GRAYED, 0, wide(l).as_ptr());
            }
        }
        AppendMenuW(m, MF_SEPARATOR, 0, null());
        AppendMenuW(m, MF_STRING | gray(CMD_FEED), CMD_FEED as usize, wide("Feed").as_ptr());
        let train = CreatePopupMenu();
        for (i, d) in Drill::ALL.iter().enumerate() {
            AppendMenuW(train, MF_STRING, (CMD_TRAIN + i as i32) as usize, wide(d.label()).as_ptr());
        }
        AppendMenuW(m, MF_POPUP | gray(CMD_TRAIN), train as usize, wide("Train").as_ptr());
        let battle = format!("Battle (Rank {})", st.rank);
        AppendMenuW(m, MF_STRING | gray(CMD_BATTLE), CMD_BATTLE as usize, wide(&battle).as_ptr());
        AppendMenuW(m, MF_STRING | gray(CMD_PLAY), CMD_PLAY as usize, wide("Play").as_ptr());
        AppendMenuW(m, MF_STRING | gray(CMD_CLEAN), CMD_CLEAN as usize, wide("Clean up").as_ptr());
        let sleep_lbl = if st.asleep { "Wake up" } else { "Lights out" };
        AppendMenuW(m, MF_STRING | gray(CMD_SLEEP), CMD_SLEEP as usize, wide(sleep_lbl).as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, null());
        AppendMenuW(m, MF_STRING, CMD_GUIDE as usize, wide("Evolution guide…").as_ptr());
        AppendMenuW(m, MF_STRING, CMD_HALL as usize, wide("Hall of Fame…").as_ptr());
        AppendMenuW(m, MF_STRING | gray(CMD_RETIRE), CMD_RETIRE as usize, wide("Retire…").as_ptr());
        AppendMenuW(m, MF_STRING | gray(CMD_RESET), CMD_RESET as usize, wide("Start over…").as_ptr());
        AppendMenuW(m, MF_STRING, CMD_QUIT as usize, wide("Quit").as_ptr());
        let pt = cursor();
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null());
        DestroyMenu(m);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        cmd
    };

    run_command(hwnd, cmd, st.born);
}

/// Carries out a command from the menu or a WM_COMMAND message. Commands that need a dialog run
/// it here, outside any borrow of the app; everything goes through `App::allowed`.
fn run_command(hwnd: HWND, cmd: i32, born: u64) {
    // A dialog may have been open a while: only act if it's still the same pet and still allowed.
    let still_ok = |a: &App| a.pet.born == born && a.allowed(cmd);
    if with_app(|a| still_ok(a)) != Some(true) {
        return;
    }
    match cmd {
        CMD_QUIT => unsafe {
            DestroyWindow(hwnd);
        },
        CMD_GUIDE => {
            if let Some(text) = with_app(|a| a.guide()) {
                unsafe { MessageBoxW(hwnd, wide(&text).as_ptr(), wide("Evolution guide").as_ptr(), MB_OK) };
            }
        }
        CMD_HALL => {
            let hall = Pet::hall_of_fame(20);
            let text = if hall.is_empty() {
                "Nobody has retired yet.\n\nMonsters retire when their lifespan runs out, or when you choose Retire… for a Rookie or older.".to_string()
            } else {
                hall.join("\n")
            };
            unsafe { MessageBoxW(hwnd, wide(&text).as_ptr(), wide("Hall of Fame").as_ptr(), MB_OK) };
        }
        CMD_RETIRE => {
            let Some(name) = with_app(|a| a.pet.species.info().name) else { return };
            let ask = format!(
                "Retire {name} to the Hall of Fame now?\n\nA new egg will inherit a tenth of its stats."
            );
            let ok = unsafe { MessageBoxW(hwnd, wide(&ask).as_ptr(), wide("Retire").as_ptr(), MB_YESNO | MB_ICONQUESTION) };
            if ok == IDYES {
                with_app(|a| {
                    if still_ok(a) {
                        a.retire();
                    }
                });
            }
        }
        CMD_RESET => {
            let ok = unsafe {
                MessageBoxW(
                    hwnd,
                    wide("Say goodbye and start over with a new egg?\n(Your list of monsters raised is kept.)").as_ptr(),
                    wide("desklings").as_ptr(),
                    MB_YESNO | MB_ICONQUESTION,
                )
            };
            if ok == IDYES {
                with_app(|a| {
                    if still_ok(a) {
                        a.command(CMD_RESET);
                    }
                });
            }
        }
        c => {
            with_app(|a| a.command(c));
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            with_app(|a| a.tick());
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN => {
            let c = cursor();
            let grabbed = with_app(|a| {
                if a.battle.is_some() {
                    return false; // no running away mid-fight
                }
                a.held = Some((c.x - a.x as i32, c.y - a.y as i32, c.x, c.y));
                a.dragged = false;
                a.vy = 0.0;
                true
            });
            if grabbed == Some(true) {
                SetCapture(hwnd);
            }
            0
        }
        WM_MOUSEMOVE => {
            let c = cursor();
            with_app(|a| {
                if let Some((gx, gy, sx, sy)) = a.held {
                    if (c.x - sx).abs() + (c.y - sy).abs() > 4 {
                        a.dragged = true;
                    }
                    if a.dragged {
                        a.x = (c.x - gx) as f32;
                        a.y = (c.y - gy) as f32;
                        a.render();
                    }
                }
            });
            0
        }
        WM_LBUTTONUP => {
            with_app(|a| {
                if a.held.take().is_some() && !a.dragged {
                    a.poke();
                }
            });
            ReleaseCapture();
            0
        }
        WM_CAPTURECHANGED => {
            with_app(|a| a.held = None);
            0
        }
        WM_RBUTTONUP => {
            show_menu(hwnd);
            0
        }
        WM_RETIRE => {
            if let Some((text, born)) = with_app(|a| (a.farewell(), a.pet.born)) {
                MessageBoxW(hwnd, wide(&text).as_ptr(), wide("A long life").as_ptr(), MB_OK | MB_ICONINFORMATION);
                with_app(|a| {
                    if a.pet.born == born {
                        a.retire();
                    } else {
                        a.retiring = false; // that pet is already gone; don't leave the menu locked
                    }
                });
            }
            0
        }
        // Menu commands can also arrive as messages (handy for scripting and testing).
        WM_COMMAND => {
            if let Some(born) = with_app(|a| a.pet.born) {
                run_command(hwnd, (wp & 0xFFFF) as i32, born);
            }
            0
        }
        WM_ENDSESSION if wp != 0 => {
            with_app(|a| {
                a.forfeit();
                a.persist()
            });
            0
        }
        WM_DESTROY => {
            let saved = with_app(|a| {
                a.forfeit();
                a.persist()
            });
            if saved == Some(false) {
                let msg = wide("Desklings couldn't save before closing, so the last minute or so of progress may be lost.");
                MessageBoxW(null_mut(), msg.as_ptr(), wide("Desklings").as_ptr(), MB_OK | MB_ICONWARNING);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Poops are their own little windows so they stay where they were dropped. Click one to clean it.
unsafe extern "system" fn poop_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONUP => {
            with_app(|a| a.clean_poop(hwnd));
            0
        }
        WM_RBUTTONUP => {
            if let Some(pet) = with_app(|a| a.hwnd) {
                show_menu(pet);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// The wild opponent during a battle. Purely visual.
unsafe extern "system" fn foe_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn popup(class: &[u16], hinst: HINSTANCE, w: i32, h: i32) -> HWND {
    CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide("desklings").as_ptr(),
        WS_POPUP,
        0,
        0,
        w,
        h,
        null_mut(),
        null_mut(),
        hinst,
        null(),
    )
}

fn main() {
    unsafe {
        CreateMutexW(null(), 1, wide("Local\\desklings").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        // Draw at real pixels so the art stays crisp instead of being bitmap-stretched.
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        SCALE.store(((4 * GetDpiForSystem() + 48) / 96).max(1) as i32, Relaxed);

        let hinst = GetModuleHandleW(null());
        let class = wide("DesklingsBuddy");
        let poop_class = wide(POOP_CLASS);
        let foe_class = wide(FOE_CLASS);
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.hInstance = hinst;
        wc.hCursor = LoadCursorW(null_mut(), IDC_HAND);
        for (name, proc_) in [
            (&class, wndproc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT),
            (&poop_class, poop_wndproc),
            (&foe_class, foe_wndproc),
        ] {
            wc.lpfnWndProc = Some(proc_);
            wc.lpszClassName = name.as_ptr();
            RegisterClassW(&wc);
        }

        let hwnd = popup(&class, hinst, cw(), ch());
        let foe_hwnd = popup(&foe_class, hinst, cw(), ch());
        if hwnd.is_null() || foe_hwnd.is_null() {
            return;
        }

        // Poop art never changes, so draw it once and reuse it for every poop window.
        let (Some(mut poop_gfx), Some(canvas), Some(foe_canvas)) =
            (Surface::new(POOP[0].len() as i32, POOP.len() as i32), Surface::new(LW, LH), Surface::new(LW, LH))
        else {
            return;
        };
        poop_gfx.blit(POOP, 0, 0, false, Pal::default(), false, false);

        let wa = work_area(hwnd);
        let (pet, saved_x, warning) = match Pet::load() {
            Loaded::Fresh => (Pet::new(0), None, None),
            Loaded::Ok(p, x) => (p, x, None),
            Loaded::Damaged(p, note) => (p, None, Some(note)),
            Loaded::Unreadable(why) => {
                // Starting anyway would autosave a new egg over the real pet.
                let msg = format!(
                    "Desklings couldn't open its save file:\n{why}\n\nIt will close now rather than risk overwriting your pet. \
                     Try again in a moment; if it keeps happening, check that nothing else has the file open."
                );
                MessageBoxW(null_mut(), wide(&msg).as_ptr(), wide("Desklings").as_ptr(), MB_OK | MB_ICONWARNING);
                return;
            }
        };
        let mut app = App {
            hwnd,
            canvas,
            poop_gfx,
            poop_wnds: Vec::new(),
            foe_hwnd,
            foe_canvas,
            battle: None,
            pet,
            news: warning.unwrap_or_default(),
            x: saved_x.unwrap_or((wa.right - cw() - 200) as f32),
            y: (wa.bottom - ch() - 150) as f32, // drop in from a little above the taskbar
            vy: 0.0,
            dir: -1,
            act: Act::Idle,
            act_t: 20,
            frame: 0,
            evo_flash: 0,
            held: None,
            dragged: false,
            rng: Rng(now() | 1),
            retiring: false,
            last_second: now(),
            last_save: now(),
            save_failed: false,
        };
        for (x, y) in std::mem::take(&mut app.pet.poops) {
            if let Some(landed) = app.spawn_poop(x, y) {
                app.pet.poops.push(landed);
            }
        }
        app.render();
        APP.with(|a| *a.borrow_mut() = Some(app));

        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, 1, TICK_MS, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
