# CLAUDE.md

A memory-light Windows desktop pet (Digimon / Monster Rancher–style) in Rust using raw Win32 through `windows-sys`. There's no GUI framework and no async, and the only dependency is `windows-sys`. Keep it that way: being memory-light is the point of the project (about 1.5 MB private, ~200 KB exe).

## Commands

- `cargo build --release`: build. The running app locks `target\release\digidesktop.exe`, so close it first (`taskkill /IM digidesktop.exe` closes it gracefully and it saves; `/F` skips the save).
- `cargo test --release`: game-rule unit tests in `monster.rs` and `pet.rs`. This does **not** rebuild the app exe, so run `cargo build` too before launching.
- `python tools/sprite_sheet.py src/sprites.rs docs/sprites.png`: validates sprite rows and characters and renders a preview sheet. Run it after any art change and look at the PNG.

## Architecture

- `src/monster.rs` holds pure game logic: the `Species` enum and `INFO` table (which **must stay in enum order**; a test checks this), `evolution()`, `paths()` (player-facing branch text, keep it in sync with `evolution()`), `train()`, `opponent()`, `attack()` and `Rng` (xorshift). No Win32 here.
- `src/pet.rs` holds the persistent `Pet`: needs, stats, care record, rank, lifespan and generation, plus the `key=value` save file and the append-only Hall of Fame. New fields need a default in `Pet::new`, a line in `save`, and a parse arm in `load`. Old saves must keep loading, since unknown or missing keys fall back to defaults.
- `src/main.rs` is the Win32 shell:
  - `Surface` is a 32-bit premultiplied DIB drawn in *art pixels* (`scale()` screen px each) and pushed with `UpdateLayeredWindow`.
  - The buddy, each poop and the battle opponent are separate layered, topmost, tool windows, so poop stays put and the foe stands apart.
  - `App` lives in a `thread_local RefCell`. Always go through `with_app` (it uses `try_borrow_mut`) and **never hold the borrow across calls that pump messages**: `TrackPopupMenu`, `MessageBoxW`, `SetCapture`/`ReleaseCapture`. Build the data inside `with_app`, then call the modal API outside it. That's why retirement is announced by posting `WM_RETIRE` instead of opening a box mid-tick.
  - A 100 ms `WM_TIMER` drives `tick()`. `second()` runs once per second for needs, evolution, ageing and autosave (every 60 s).
  - Menu command IDs are also accepted as `WM_COMMAND`, which is useful for scripted testing.
- `src/sprites.rs` is art as `&[&str]` grids, one char per pixel. The legend is at the top of the file. Monsters face **right**; the renderer mirrors them. Eyes use `he`/`kk` so they close when asleep. Sprites are anchored by their lowest opaque row and centred on `Surface.cx`. Size limits: Rookie 16×16, Champion 20×20, Ultimate 24×24, icons 7×7.

## Conventions

- Movement and jump speeds are written in "100% DPI" pixels and multiplied by `scale()`.
- Times are Unix seconds from `now()`. Stage timers count from `stage_since`; lifespan counts from `born`.
- Balance numbers live as named consts at the top of `monster.rs` / `pet.rs`. When changing rules, update `paths()`, the README tables, and the tests.

## Testing in the real app

GUI changes need a look at the actual window. A screen capture must use `BitBlt` with `CAPTUREBLT` (0x40CC0020) or layered windows won't show. To test a scenario, back up `%APPDATA%\digidesktop\state.txt`, edit it (e.g. `species=`, `stats=`, `born=`, `stage_since=`), launch, post `WM_COMMAND` IDs (Battle = 7, Train = 20–24) to the `DigiDesktopBuddy` window, capture, then restore the backup. Find windows with `EnumWindows` + class name. From PowerShell, `FindWindowW(cls, $null)` passes `""` and fails.
