#![windows_subsystem = "windows"]

mod game;
mod monster;
mod pet;
mod sprites;

use game::*;
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

/// Where the opponent stands on screen during a battle.
struct FoeView {
    side: i32, // +1: the foe stands to the right of the buddy
    x: f32,
    target: f32,
    y: f32,
}

impl FoeView {
    fn arrived(&self) -> bool {
        (self.x - self.target).abs() <= 0.5
    }
}

/// The Windows side: windows, drawing, physics and input. The rules live in `Game`.
struct App {
    game: Game,
    hwnd: HWND,
    canvas: Surface,
    poop_gfx: Surface,
    poop_wnds: Vec<HWND>, // parallel to game.pet.poops
    foe_hwnd: HWND,
    foe_canvas: Surface,
    foe: Option<FoeView>,
    x: f32,
    y: f32,
    vy: f32,
    frame: u32,
    held: Option<(i32, i32, i32, i32)>, // grab offset x/y, cursor start x/y
    dragged: bool,
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

fn sprite_width(spr: Sprite) -> i32 {
    spr.iter().map(|r| r.len()).max().unwrap_or(0) as i32
}

impl App {
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
        let ok = self.game.pet.save(self.x);
        self.save_failed = !ok;
        ok
    }

    /// Carries out what the game asked for.
    fn handle(&mut self, events: Vec<Event>) {
        for e in events {
            match e {
                Event::Hop(v) => self.hop(v),
                Event::Poop => self.drop_poop(),
                Event::PoopsCleared => {
                    for h in self.poop_wnds.drain(..) {
                        unsafe { DestroyWindow(h) };
                    }
                }
                Event::BattleStarted => self.stage_battle(),
                Event::BattleOver => {
                    self.foe = None;
                    unsafe { ShowWindow(self.foe_hwnd, SW_HIDE) };
                }
                Event::Retire => unsafe {
                    PostMessageW(self.hwnd, WM_RETIRE, 0, 0);
                },
                Event::Save => {
                    self.persist();
                }
            }
        }
        debug_assert_eq!(self.poop_wnds.len(), self.game.pet.poops.len());
    }

    fn command(&mut self, cmd: i32) {
        let ev = self.game.command(cmd, now());
        self.handle(ev);
    }

    fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        // Timers drift and stop while the PC sleeps, so run per-second logic off the clock.
        let t = now();
        if t != self.last_second {
            let gap = t.saturating_sub(self.last_second);
            self.last_second = t;
            if gap > CATCH_UP_LIMIT {
                self.game.pet.catch_up(gap); // long pause: treat it like time away
            }
            // Replay short gaps second by second; a long one was just caught up in one go.
            let reps = if gap > CATCH_UP_LIMIT { 1 } else { gap };
            for _ in 0..reps {
                let ev = self.game.second(t, self.on_ground());
                self.handle(ev);
            }
            if t >= self.last_save + 60 || t < self.last_save {
                self.last_save = t;
                self.persist();
            }
        }
        self.game.evo_flash = self.game.evo_flash.saturating_sub(1);

        if let Some(f) = self.foe.as_mut() {
            let gap = f.x - f.target;
            if gap.abs() > 0.5 {
                f.x -= gap.signum() * (1.5 * scale() as f32).min(gap.abs());
            }
            let arrived = f.arrived();
            let ev = self.game.battle_tick(arrived);
            self.handle(ev);
        }

        if self.held.is_none() {
            let s = scale() as f32;
            let wa = work_area(self.hwnd);
            let ground = (wa.bottom - ch()) as f32;
            let half = sprite_width(self.current_sprite()) / 2;
            let min_x = (wa.left - (CX - half) * scale()) as f32;
            let max_x = ((wa.right - cw()) as f32).max(min_x); // keep the bubble on screen too

            if self.y < ground || self.vy < 0.0 {
                self.vy += 0.375 * s;
                self.y += self.vy;
            }
            if self.y >= ground {
                self.y = ground;
                self.vy = 0.0;
            }

            let g = &self.game;
            if !g.pet.asleep && self.on_ground() {
                let speed = match g.act {
                    Act::Walk if g.pet.stage() <= Stage::Fresh => 0.17,
                    Act::Walk => 0.35,
                    Act::Train(Drill::Run) => 0.8,
                    _ => 0.0,
                };
                let speed = if g.pet.elderly() { speed * 0.6 } else { speed };
                self.x += g.dir as f32 * speed * s;
                if self.x <= min_x {
                    self.game.dir = 1;
                } else if self.x >= max_x {
                    self.game.dir = -1;
                }
            }
            let ev = self.game.tick(t);
            self.handle(ev);
            self.x = self.x.clamp(min_x, max_x);
        }
        self.render();
    }

    fn current_sprite(&self) -> Sprite {
        self.game.pet.species.info().frames[0]
    }

    /// Puts the buddy and the incoming opponent in position for a battle the game just started.
    fn stage_battle(&mut self) {
        let s = scale() as f32;
        let wa = work_area(self.hwnd);
        let ground = (wa.bottom - ch()) as f32;
        let mid = (wa.left + wa.right) as f32 / 2.0;
        let side = if self.x + CX as f32 * s < mid { 1 } else { -1 };
        // Foe stands 35 art px away, with its bubble on the far side from the buddy.
        self.foe_canvas.cx = if side > 0 { CX } else { LW - 1 - CX };
        let target = self.x + (CX + side * 35 - self.foe_canvas.cx) as f32 * s;
        self.game.dir = side;
        self.y = ground;
        self.vy = 0.0;
        self.foe = Some(FoeView { side, x: target + side as f32 * 30.0 * s, target, y: ground });
        self.render();
        unsafe {
            ShowWindow(self.foe_hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    // ------------------------------------------------------------------ poop

    /// Leaves a poop on the ground just behind the buddy.
    fn drop_poop(&mut self) {
        let s = scale();
        let half = sprite_width(self.current_sprite()) / 2;
        let behind = if self.game.dir > 0 { CX - half - 8 } else { CX + half };
        let x = self.x as i32 + behind * s;
        let y = self.y as i32 + ch() - POOP.len() as i32 * s;
        if let Some(landed) = self.spawn_poop(x, y) {
            self.game.pet.poops.push(landed);
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
        let Some(i) = self.poop_wnds.iter().position(|&h| h == hwnd) else { return };
        if self.game.clean_one(i) {
            self.poop_wnds.remove(i);
            unsafe { DestroyWindow(hwnd) };
            self.persist();
        }
    }

    // ------------------------------------------------------------------ drawing

    fn render(&mut self) {
        self.canvas.clear();
        let f = self.frame;
        let g = &self.game;
        let info = g.pet.species.info();
        let asleep = g.pet.asleep;
        let moving = !asleep && matches!(g.act, Act::Walk | Act::Joy | Act::Eat | Act::Train(_));
        let airborne = self.vy != 0.0 || self.held.is_some();
        let beat = if asleep { (f / 15) % 2 } else if moving { (f / 3) % 2 } else { (f / 8) % 2 };

        let (mut dx, mut dy) = (0, 0);
        let mut spr = info.frames[0];
        if info.stage == Stage::Egg {
            let fast = now() >= g.pet.stage_since + 45;
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
        match g.act {
            Act::Train(Drill::Endure) => dx += if f % 4 < 2 { 1 } else { -1 },
            Act::Train(Drill::Lift) => dy -= ((f / 5) % 2) as i32,
            _ => {}
        }

        let mut flash = g.evo_flash > 0 && (f / 2) % 2 == 0;
        let mut bar = None;
        let mut result_icon = None;
        if let (Some(b), Some(v)) = (&g.battle, &self.foe) {
            if b.lunge.0 > 0 {
                dx += 2 * v.side;
            }
            flash |= b.flash.0 > 0;
            bar = Some((b.me.hp, b.me.max_hp));
            result_icon = b.result.map(|won| if won { STAR } else { SWEAT }).or((!v.arrived()).then_some(SWORD));
        }

        let top = self.canvas.blit_standing(spr, dx, dy, g.dir < 0, info.pal, asleep, flash);
        if let Some((hp, max)) = bar {
            self.canvas.hp_bar(top, hp, max);
        }

        let blink = (f / 5) % 2 == 0;
        let p = &g.pet;
        let needy = info.stage != Stage::Egg && blink;
        let icon = match g.act {
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
        self.render_foe();
    }

    fn render_foe(&mut self) {
        let (Some(b), Some(v)) = (&self.game.battle, &self.foe) else { return };
        let c = &mut self.foe_canvas;
        c.clear();
        let info = b.foe.sp.info();
        let f = self.frame;
        let walking = !v.arrived();
        let mut spr = info.frames[0];
        let mut dy = 0;
        if (f / if walking { 3 } else { 8 }) % 2 == 1 {
            match info.frames.get(1) {
                Some(&alt) => spr = alt,
                None => dy = -1,
            }
        }
        let dx = if b.lunge.1 > 0 { -2 * v.side } else { 0 };
        // Sprites face right; a foe on the right has to face left.
        let top = c.blit_standing(spr, dx, dy, v.side > 0, info.pal.wild(), false, b.flash.1 > 0);
        if !walking {
            c.hp_bar(top, b.foe.hp, b.foe.max_hp);
        }
        if let Some(won) = b.result {
            c.bubble(if won { SWEAT } else { STAR }, 0);
        }
        c.present(self.foe_hwnd, v.x as i32, v.y as i32);
    }
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
        let g = &a.game;
        MenuState {
            lines: g.status_lines(now(), a.save_failed),
            enabled: (CMD_FEED..=CMD_LAST).filter(|&c| g.allowed(c)).collect(),
            asleep: g.pet.asleep,
            rank: RANKS[g.pet.rank as usize],
            born: g.pet.born,
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

/// Retires the pet: Hall of Fame entry first (it's idempotent), then the successor arrives.
fn retire(a: &mut App) {
    let enshrined = a.game.pet.enshrine();
    let ev = a.game.retire(enshrined);
    a.handle(ev);
}

/// Carries out a command from the menu or a WM_COMMAND message. Commands that need a dialog run
/// it here, outside any borrow of the app; everything goes through `Game::allowed`.
fn run_command(hwnd: HWND, cmd: i32, born: u64) {
    // A dialog may have been open a while: only act if it's still the same pet and still allowed.
    let still_ok = |a: &App| a.game.pet.born == born && a.game.allowed(cmd);
    if with_app(|a| still_ok(a)) != Some(true) {
        return;
    }
    match cmd {
        CMD_QUIT => unsafe {
            DestroyWindow(hwnd);
        },
        CMD_GUIDE => {
            if let Some(text) = with_app(|a| a.game.guide(now())) {
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
            let Some(name) = with_app(|a| a.game.pet.species.info().name) else { return };
            let ask = format!("Retire {name} to the Hall of Fame now?\n\nA new egg will inherit a tenth of its stats.");
            let ok = unsafe { MessageBoxW(hwnd, wide(&ask).as_ptr(), wide("Retire").as_ptr(), MB_YESNO | MB_ICONQUESTION) };
            if ok == IDYES {
                with_app(|a| {
                    if still_ok(a) {
                        retire(a);
                    }
                });
            }
        }
        CMD_RESET => {
            let ok = unsafe {
                MessageBoxW(
                    hwnd,
                    wide("Say goodbye and start over with a new egg?\n(Your list of monsters raised is kept.)").as_ptr(),
                    wide("Desklings").as_ptr(),
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

/// The app is going away: settle anything in progress, then save. Returns whether the save worked.
fn shut_down(a: &mut App) -> bool {
    let ev = a.game.settle(now());
    a.handle(ev);
    a.persist()
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
                if a.game.battle.is_some() {
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
                    let ev = a.game.poke();
                    a.handle(ev);
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
            if let Some((text, born)) = with_app(|a| (a.game.farewell(now()), a.game.pet.born)) {
                MessageBoxW(hwnd, wide(&text).as_ptr(), wide("A long life").as_ptr(), MB_OK | MB_ICONINFORMATION);
                with_app(|a| {
                    if a.game.pet.born == born {
                        retire(a);
                    } else {
                        a.game.retiring = false; // that pet is already gone; don't leave the menu locked
                    }
                });
            }
            0
        }
        // Menu commands can also arrive as messages (handy for scripting and testing).
        WM_COMMAND => {
            if let Some(born) = with_app(|a| a.game.pet.born) {
                run_command(hwnd, (wp & 0xFFFF) as i32, born);
            }
            0
        }
        WM_ENDSESSION if wp != 0 => {
            with_app(shut_down);
            0
        }
        WM_DESTROY => {
            if with_app(shut_down) == Some(false) {
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
        let mut game = Game::new(pet, now());
        game.news = warning.unwrap_or_default();
        let mut app = App {
            game,
            hwnd,
            canvas,
            poop_gfx,
            poop_wnds: Vec::new(),
            foe_hwnd,
            foe_canvas,
            foe: None,
            x: saved_x.unwrap_or((wa.right - cw() - 200) as f32),
            y: (wa.bottom - ch() - 150) as f32, // drop in from a little above the taskbar
            vy: 0.0,
            frame: 0,
            held: None,
            dragged: false,
            last_second: now(),
            last_save: now(),
            save_failed: false,
        };
        for (x, y) in std::mem::take(&mut app.game.pet.poops) {
            if let Some(landed) = app.spawn_poop(x, y) {
                app.game.pet.poops.push(landed);
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
