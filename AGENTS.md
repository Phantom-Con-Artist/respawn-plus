# Agent rules

Rules for any AI agent (Claude, Codex or another) working in this repository. They override
default habits. Read `README.md` and the relevant file in `docs/` before changing behaviour.

## Be agnostic: never hardcode

Vessel is a general launcher. Code must work for any:

- **game**: any Windows game, engine or store, not a list of known titles;
- **platform**: any Linux distribution, desktop (KDE, GNOME, wlroots, X11), GPU vendor,
  filesystem and display setup, including multiple and scaled monitors;
- **runtime**: any Proton, Wine or UMU build the user picks;
- **mod, mod manager, script extender or trainer**: MO2, Vortex, WeMod, standalone trainers and
  tools nobody has heard of yet.

In practice:

1. **No title, executable or tool names in logic.** No `match "skyrimse.exe"`, no branch for
   MO2 or SKSE. Behaviour comes from the game's settings, the companion app's kind
   (`mod_manager`, `script_extender`, `trainer`, `utility`) and what the user configured.
2. **No guessed constants.** No default resolution, user name, path, window size or timeout
   standing in for a fact. Take the value from the user's settings or ask the host (for example,
   the monitor size comes from the windowing system through Tauri). If nothing is known, fail
   with a message that says what to set; do not fall back to a guess.
3. **No platform probes that assume one setup.** Prefer interfaces that every setup provides
   (Tauri, Wayland/X11 protocols, the runtime's own tools) over reading one vendor's or one
   distribution's files.
4. **Act on the room, not on the game's files.** Display mode, isolation and runtime setup are
   applied to the game's room (its bubblewrap sandbox, private Xwayland screen and prefix
   environment), which works for every game. Do not edit a game's own config files.
5. **Game- or tool-specific knowledge is data, not code.** If a game or tool truly needs
   something special, make it a generic, user-editable setting or profile that any game can
   use, with the specific values supplied as data. Never as a code path for one title.
6. **Tests use neutral fixtures** (`Game.exe`, `Manager.exe`, `Loader`), so they prove the
   generic behaviour. Real titles belong only in manual verification notes.
7. **Say what was tested.** Claim support only for what was run; compatibility varies by game
   and tool.

## Known exceptions to remove

These predate the rules. Do not extend them; replace them with generic mechanisms when you touch
the area:

- `src-tauri/src/recovery.rs`: built-in save folders for `skyrimse.exe` and `fallout4.exe`.
  Generic `save_paths`, `game_save_paths` and `home_save_paths` already exist.
- `src-tauri/src/library.rs` `game_name`: special names for `skyrimse` and `mines`, and an
  Unreal `-Win64-Shipping` suffix rule.
- `src/main.js`: Skyrim artwork from a fixed Steam URL, and a recovery note that names
  Skyrim Special Edition and Fallout 4.

## Working conventions

- Stack: Rust + Tauri v2 and SQLite (`src-tauri/`), Vite with vanilla JavaScript (`src/`),
  Python room engine (`src-tauri/room_engine/engine.py`, embedded with `include_str!`).
- Before claiming a change works, run what applies:

  ```sh
  cargo test --manifest-path src-tauri/Cargo.toml --lib --offline
  cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --offline -- -D warnings
  VESSEL_TEST_WINE=1 npm run test:rooms
  npm run check && npm run build && npm run test:packaging
  ```

- Never point destructive tests at the user's real games, prefixes, saves or launcher database.
  Use `src-tauri/examples/room_lab.rs`, which works on disposable copies.
- Commit each verified step with a message that explains why.
