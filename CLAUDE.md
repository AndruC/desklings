# CLAUDE.md

A memory-light Windows desktop pet (Digimon / Monster Rancher–style) in Rust using raw Win32 through `windows-sys`. There's no GUI framework and no async, and the only dependency is `windows-sys`. Keep it that way: being memory-light is the point of the project (about 1.5 MB private, ~200 KB exe).

## Commands

- `cargo build --release`: build. The running app locks `target\release\desklings.exe`, so close it first (`taskkill /IM desklings.exe` closes it gracefully and it saves; `/F` skips the save).
- `cargo test --release`: unit tests for game rules, needs, save/load and sprite validity (`monster.rs`, `pet.rs`). This does **not** rebuild the app exe, so run `cargo build` too before launching.
- `python tools/sprite_sheet.py src/sprites.rs docs/sprites.png`: validates sprite rows and characters and renders a preview sheet. Run it after any art change and look at the PNG.

## Architecture

- `src/monster.rs` holds pure game logic: the `Species` enum and `INFO` table (which **must stay in enum order**; a test checks this), `evolution()`, `paths()` (player-facing branch text, keep it in sync with `evolution()`), `train()`, `opponent()`, `attack()` and `Rng` (xorshift). No Win32 here.
- `src/pet.rs` holds the persistent `Pet`: needs, stats, care record, rank, lifespan and generation, plus the `key=value` save file and the append-only Hall of Fame. New fields need a default in `Pet::new`, a line in `serialize`, and a parse arm in `parse`. Old saves must keep loading: *missing* keys fall back to defaults, but a key that's present with a bad value means the save is damaged (it's backed up as `state.bad.<time>.txt` and never overwritten). Time-dependent methods have `*_at(now)` forms; use those in logic and tests, never the real clock.
- `src/game.rs` is the rules and state machine, with no Win32: `Game` (pet, current `Act`, battle, news), `allowed()` (the single source of truth for what the player may do), `second(now, standing)`, `tick(now)`, `command`, `battle_tick`, `settle` (fair outcome when quitting) and `retire`. It returns `Event`s (Hop, Poop, PoopsCleared, BattleStarted, BattleOver, Retire, Save) that the shell carries out. Put new behaviour here, with a test that drives it through these methods.
- `src/main.rs` is the thin Win32 shell: windows, drawing, physics, input and dialogs. It owns a `Game` and carries out its events. `poop_wnds` must stay parallel to `game.pet.poops`.
  - `Surface` is a 32-bit premultiplied DIB drawn in *art pixels* (`scale()` screen px each) and pushed with `UpdateLayeredWindow`.
  - The buddy, each poop and the battle opponent are separate layered, topmost, tool windows, so poop stays put and the foe stands apart.
  - `App` lives in a `thread_local RefCell`. Always go through `with_app` (it uses `try_borrow_mut`) and **never hold the borrow across calls that pump messages**: `TrackPopupMenu`, `MessageBoxW`, `SetCapture`/`ReleaseCapture`. Build the data inside `with_app`, then call the modal API outside it. That's why retirement is announced by posting `WM_RETIRE` instead of opening a box mid-tick.
  - A 100 ms `WM_TIMER` drives `App::tick()`, which runs `Game::second` once per elapsed clock second (a gap over 2 minutes is one `catch_up` instead) and autosaves every 60 s.
  - Menu command IDs are also accepted as `WM_COMMAND`, which is useful for scripted testing.
- `src/sprites.rs` is art as `&[&str]` grids, one char per pixel. The legend is at the top of the file. Monsters face **right**; the renderer mirrors them. Eyes use `he`/`kk` so they close when asleep. Sprites are anchored by their lowest opaque row and centred on `Surface.cx`. Size limits: Rookie 16×16, Champion 20×20, Ultimate 24×24, icons 7×7, enforced by the `sprites_are_well_formed` test.

## Conventions

- Movement and jump speeds are written in "100% DPI" pixels and multiplied by `scale()`.
- Times are Unix seconds from `now()`. Stage timers count from `stage_since`; lifespan counts from `born`.
- Balance numbers live as named consts at the top of `monster.rs` / `pet.rs`. When changing rules, update `paths()`, the README tables, and the tests.

## Testing in the real app

GUI changes need a look at the actual window. A screen capture must use `BitBlt` with `CAPTUREBLT` (0x40CC0020) or layered windows won't show. To test a scenario, back up `%APPDATA%\desklings\state.txt`, edit it (e.g. `species=`, `stats=`, `born=`, `stage_since=`), launch, post `WM_COMMAND` IDs (Battle = 7, Train = 20–24) to the `DesklingsBuddy` window, capture, then restore the backup. Find windows with `EnumWindows` + class name. From PowerShell, `FindWindowW(cls, $null)` passes `""` and fails.

## Releasing

Bump `version` in `Cargo.toml`, commit, then `git tag vX.Y.Z && git push origin main vX.Y.Z`. `.github/workflows/release.yml` tests, builds and publishes `desklings.exe` to a GitHub Release for that tag. The `gh` CLI lives at `C:\Program Files\GitHub CLI\gh.exe` if it isn't on PATH.
