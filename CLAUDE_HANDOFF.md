# Respawn+ — Claude takeover handshake

Prepared by Codex on 2026-10-24. Workspace:
`/home/subhradeep/Development/linux-game-launcher-project`.

## Receiving-agent handshake

Read this file, `README.md`, and `docs/companion-apps.md`, then inspect the
working tree before changing anything. Acknowledge the takeover to the user
with the verified result, the remaining limitations, and your next concrete
step. This document transfers context; it does not authorize publishing,
committing, deleting user files, or sending messages to external services.

Do not restart the project from scratch. Preserve existing source, SQLite
state, game prefixes, mods, saves, and backups. Recheck process identity before
acting on any PID below: these are historical test values and can be reused.
No credentials are included here; none are needed to read or develop the repo.

## User intent and accepted decisions

- Build **Respawn+**, an open-source Linux launcher that simplifies Windows
  gaming, mod managers, script extenders, and trainers.
- The project license is **MIT**, superseding the earlier AGPL request.
  Third-party software retains its own licenses.
- Stack: Rust + Tauri v2, SQLite, Vite with a vanilla JavaScript frontend.
- Library: artwork/name cards in a bento-style grid, discovery/import, and
  launching games directly from Respawn+.
- Companion apps belong to a game and share its selected runner and prefix.
- Distribution direction: lite installers using users' own resources, plus
  fuller RPM/DEB editions with properly licensed runtime dependencies.
  Full runtime bundling is groundwork, not a completed distribution.
- The latest implementation request was to provide a shared “room” in which
  Skyrim and its trainer can run concurrently and discover each other.

The user discussed Linux seeing only Respawn+'s PID. Be precise: the current
implementation does **not** hide processes, fake PIDs, or create a private PID
namespace. Linux sees separate host processes. Shared runtime/container state
solves the demonstrated concurrency issue without PID aliasing.

## Work completed in this session

The focused change was the Proton shared-session launch path in
`src-tauri/src/library.rs`:

1. Inspected installed UMU and GE-Proton source and the runtime's launch-client
   and launcher-service interfaces. Checked upstream UMU release information.
2. Identified that separate UMU containers and Proton's
   `waitforexitandrun` behavior were unsuitable for concurrent companions.
   That verb waits for existing prefix processes before starting the target.
3. Removed the local `reuse_prefix` heuristic, which guessed whether to use
   `runinprefix` from launch records and the presence of `system.reg`.
4. Added `proton_command(umu, runner)` and explicitly configured:

   ```text
   PROTONPATH=<selected Proton installation>
   GAMEID=0
   UMU_CONTAINER_NSENTER=1
   PROTON_VERB=run
   UMU_LOG=info
   ```

5. Kept executable arguments literal through Rust `Command`, with no shell
   interpolation. The game and companion still inherit the same `WINEPREFIX`.
6. Enabled UMU's existing shared-container mechanism: the first application
   creates a launcher service; later applications locate it by the canonical
   prefix's hash and enter it through `steam-runtime-launch-client`. UMU selects
   `runinprefix` for reentry. `GAMEID=0` does not select the service: this UMU
   version keys `STEAM_COMPAT_APP_ID` for the service from the prefix hash.
7. Added the backend regression test
   `proton_launches_request_container_reentry_and_never_wait_for_other_apps`.
   It executes a stub to check the child environment and literal arguments;
   it does **not** prove actual container sharing by itself.
8. Expanded `docs/companion-apps.md` with the mechanism, real test evidence,
   repeat procedure, and limitations.

No database migration, new room supervisor, private PID namespace, resource
limit, idle-room daemon, or room-close UI was added in this focused session.
No Git commit was created.

## Real integration result — verified, not inferred

Launched the existing **Skyrim via SKSE64** profile through the actual Tauri
backend, waited for Skyrim, then launched **FLiNG Trainer (compatibility test)**
through the same backend.

- Skyrim reached its main menu and stayed open alongside the trainer.
- The trainer visibly displayed `SkyrimSE.exe` and **Windows process ID 400**.
- Trainer launch output contained:
  `Re-entering container through bus 'com.steampowered.App8cc201c724bc347ed5cbe8b3a9d56ce3'`.
- Skyrim host PID **79193** and trainer host PID **79389** both had mount
  namespace **`mnt:[4026532838]`**.
- Both processes had the same Wine prefix and Proton installation in their
  environments. Both used the host PID namespace.
- Trainer toggles were left disabled. Process detection is confirmed;
  memory patches, cheats, hotkeys, and compatibility of every trainer feature
  with this newer Skyrim version are **not** confirmed.

Local evidence, available at handoff but temporary:

```text
/tmp/respawn-shared-room-skyrim.png
/tmp/respawn-shared-room-trainer.png
~/.local/state/respawn-plus/logs/library/game-2/tool-2/launch-1792828843334469089.log
~/.local/state/respawn-plus/logs/library/game-2/tool-3/launch-1792828871480190044.log
```

**State rechecked while preparing this handoff:** no matching Respawn+, Skyrim,
trainer, or test launcher-service process was found. The last launch rows
9 (SKSE, recorded supervisor PID 78929) and 10 (trainer, supervisor PID 79344)
still contained `running`. Those rows are stale, not proof of current activity.
`Launcher::games()` reconciles launch records using `/proc` PID/start tokens;
calling the app's listing command can mark missing processes `unknown`.
The handoff inspection used SQLite read-only and did not change those rows.

## Validation completed this session

All passed after the implementation:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
# 11 backend tests passed
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm run check
npm run test:packaging
# 3 packaging tests passed
npm run bundle:lite -- --debug --no-bundle
```

The built executable is `src-tauri/target/debug/respawn-plus`.
Use the `custom-protocol` feature for standalone builds; the package script
already supplies it. A plain debug build without that feature may use Vite's
development URL instead of embedded frontend assets.

## Important remaining limitations / suggested next work

1. **Startup race:** UMU's service-discovery retries are bounded. The Rust
   launch mutex serializes spawning, not full container readiness. Opening
   multiple apps immediately during startup is not atomic. Current reliable
   procedure: start SKSE, wait for Skyrim's window, then open the trainer.
   A useful next change is explicit per-prefix readiness and lifecycle control.
2. **Companion working directory:** UMU's current reentry invocation inherits
   the first application's directory. Setting `Command::current_dir` on the
   second UMU process does not forward that directory into the existing room.
   Additional direct launch-client integration should pass `--directory`.
3. **Environment/log inheritance:** the joined app inherits the first app's
   environment, including `PROTON_LOG_DIR`. Respawn+ has separate launch logs,
   but Proton's own verbose logs can go to the first application's directory.
4. **Session lifetime/status:** this uses the runtime's application session;
   there is no independent room kept alive with zero apps. Launch status tracks
   the supervised runtime process, which can outlive a game while other Wine
   apps remain. Improve process-level state reporting before promising exact
   game/tool lifecycle semantics.
5. **MO2 is unverified:** do not claim it was installed or successfully tested.
   MO2 virtual mods require launching the game through MO2's virtual filesystem;
   Respawn+'s direct Play does not apply that virtual deployment.
6. **Security/resources:** the current Wine/runtime setup is not isolation for
   untrusted trainers. No new filesystem restrictions or cgroup controls were
   implemented. No universal “any Windows game” compatibility claim is proven.

## Existing project context from earlier work

Already present before this focused shared-session change:

- SQLite schema v3; games, prefixes, companion profiles, launch history/logs.
- Companion kinds: `mod_manager`, `script_extender`, `trainer`, `utility`.
- Import/discovery, per-game runner settings, artwork cards, native file
  dialogs, polling, launch-log views, duplicate-launch guards.
- SKSE counts as a game session and blocks a second direct launch. Companion
  activity prevents runner changes; profile removal preserves files on disk.
- A small MIT-licensed Windows **Mines** game was installed and tested.
- Vanilla Skyrim and SKSE startup were tested. The user supplied an in-game
  screenshot confirming SkyUI's MCM. Windowed 1280×720 was intentional debug
  configuration, not evidence of a compatibility failure.
- `scripts/prepare-skyrim-test.mjs` stages user-provided SKSE/SkyUI with backups
  and hash records. Do not rerun it without a reason or overwrite user saves.

## Local paths and profile identities

```text
SQLite: ~/.local/share/respawn-plus/respawn-plus.sqlite3
Game 1: Mines
Game 2: Skyrim Special Edition
Tool 2: Skyrim via SKSE64 (script_extender, game 2)
Tool 3: FLiNG Trainer (compatibility test) (trainer, game 2)
Tool 1: temporary Notepad check; previously removed
Game 2 prefix: ~/.local/share/respawn-plus/prefixes/game-2
Effective Wine prefix seen in Proton children: game-2/pfx/
Logs: ~/.local/state/respawn-plus/logs/library/
Backups: ~/.local/share/respawn-plus/backups/skyrim-test-1792827196689/manifest.json
```

The backup manifest records 68 deployed files/settings. Original game
executables were not changed. Preserve the backup and the user's current files.

```text
Skyrim folder:
/home/subhradeep/Downloads/The-Elder-Scrolls-V-Skyrim-Special-Edition-Debug-No-DRM/The Elder Scrolls V Skyrim Special Edition
Game executable: SkyrimSE.exe (runtime 1.7.104)
SKSE executable: skse64_loader.exe (SKSE 2.3.1)
Trainer:
/home/subhradeep/Downloads/The Elder Scrolls V Skyrim Special Edition v1.0-v20231205 Plus 10 Trainer.exe
UMU:
/home/subhradeep/.local/share/gameforge/tools/umu/1.4.4/umu-run
UMU link: ~/.local/bin/umu-run
Proton:
/home/subhradeep/.local/share/Steam/compatibilitytools.d/GE-Proton11-11-x86_64/proton
Runtime: ~/.local/share/umu/steamrt4
```

The old `gameforge` UMU directory is intentional retained local installation
state; do not rename it blindly. The installer script targets Respawn+ paths
for new installations and preserves existing installations.

## Development and inspection entry points

- Backend: `src-tauri/src/library.rs`, `runtime.rs`, `database.rs`,
  `discovery.rs`, `lib.rs`, `main.rs`.
- Frontend: `src/main.js`, frontend styles, `index.html`.
- Documentation: `docs/companion-apps.md`, `docs/library.md`,
  `docs/environment.md`, `docs/distribution.md`, `docs/test-game.md`.
- Packaging validation: `scripts/check-runtime-bundle.mjs`,
  `tests/runtime-bundle.test.mjs`, `packaging/`.

```bash
src-tauri/target/debug/respawn-plus --diagnose
src-tauri/target/debug/respawn-plus --list-games
# --list-games performs launch-state reconciliation; it is not strictly read-only.
npm run dev
npm run tauri -- dev
```

For the real desktop smoke test, Codex launched the embedded app with
`GDK_BACKEND=x11 WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:9224`.
The ignored local helper `.taste-skill/native-eval.mjs` can evaluate JavaScript
in that WebKit inspector when running. Actual backend IPC uses camel-case
arguments, for example:

```javascript
window.__TAURI_INTERNALS__.invoke('launch_tool', { toolId: 2 });
window.__TAURI_INTERNALS__.invoke('launch_tool', { toolId: 3 });
```

These calls launch real applications; do not run them as read-only inspection.
There is no active inspector assumed at handoff. Do not send gameplay inputs
or close a running user game just to automate a check.

## Git and distribution status

Git is initialized on `main` with **no commits**. Project files are currently
untracked. Do not interpret that as permission to discard them. There is no
clean committed baseline or meaningful session-only `git diff` yet.

MIT project license, notices, CI, ignore rules, and lite/full packaging scaffolds
exist. This session rebuilt a debug standalone executable with `--no-bundle`;
it did not rebuild release RPM/DEB installers or publish anything. User mods,
SKSE, and the trainer are local test assets, not MIT project distribution files.
