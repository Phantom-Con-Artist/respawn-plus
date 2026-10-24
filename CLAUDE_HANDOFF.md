# Respawn+ — Claude takeover handshake

## Latest update — 2026-10-24: rooms, shared Windows dependencies and rollback

This section supersedes the historical implementation/Git status below. The user
explicitly requested implementation, broad security/corruption testing, and a local
Git commit with this handoff updated. They also asked to finish quickly because
only 10% of their quota remained. Do not expand scope before reviewing this state.

### Implemented

- Production launches always use a per-game bubblewrap supervisor. Games and
  companions execute in the same outer mount/PID/IPC/user/network namespaces.
  Proton still uses UMU's shared inner container and a private session D-Bus.
- Private home/tmp/proc, dropped capabilities, architecture-aware libseccomp,
  no inherited host secrets/FDs, no host home/SSH agent/host session bus mounts,
  no automatic unrestricted fallback. Online uses pasta in a private network
  namespace; offline has no external networking. Host X11 is not exposed;
  Wayland defaults to a private rootful Xwayland display.
- Generic SHA-256 Windows package store with path/checksum/conflict validation,
  read-only file mounts into prefixes, registry recipes, DLL overrides, capability
  dependencies, PE/runtimeconfig-based detection, persisted pins, and repair only
  from an unchanged original import source. No game-title whitelist in the room
  or runtime package engine. No Microsoft runtime binaries are committed.
- SQLite schema v6/v7: checkpoint/restore journals, archived tool profiles and
  immutable historical launch labels, room profiles and runtime repair sources.
- Automatic session checkpoints, manual restore/undo, private-home/prefix/game/
  companion snapshots, current mapped saves preserved, resume interrupted
  directory switches. Uses native Btrfs only when rootless cleanup is supported;
  otherwise reflinks/copies, never writable hardlink trees.
- Existing Skyrim/Fallout save adapters plus generic `save_paths`,
  `game_save_paths`, `home_save_paths`. Wine Documents aliases are resolved and
  redistributed between captured roots before staging. Entire-root save aliases
  and unsafe changed external links are rejected.
- UI: shared dependency import/catalog, per-room compatibility preferences,
  Stop room apps, and save-preserving Room recovery controls.

Main files: `src-tauri/src/rooms.rs`, `src-tauri/room_engine/engine.py`,
`src-tauri/src/recovery.rs`, schema v6/v7, launch integration in `library.rs`.
Read `docs/proton-rooms.md` and `docs/room-recovery.md` for contracts and limits.

### Verified tests and real-game results

- 32 Rust tests passed, including arbitrary-game mappings, current saves,
  private-home/policy rollback, aliases between room roots, original-file
  independence, seven simulated crash positions across three root switches,
  archive/history retention, orphan Wine processes and exclusive-library locking.
- 13 Python room-engine tests passed with `RESPAWN_TEST_WINE=1`: real namespaces,
  shared read-only inode identity, hidden host files/environment/processes,
  isolated online/offline abstract Unix sockets, private Xwayland, real Wine
  console startup, spoofed host control sockets, malformed packages/PE data,
  provider detection and poisoned-runtime repair with exact identity checks.
- Frontend syntax/build and all three packaging checks passed. Strict Clippy passed. The browser harness passed after excluding test
  directories from Vite file watching: command arguments, running/unsupported
  locks and layouts at 1440/768/390 pixels.
- Mines opened real game windows through Wine 11 and GE-Proton 11-11 / UMU 1.4.4.
  Skyrim Special Edition opened through that Proton stack. Successful disposable
  labs ran two stop/corrupt/restore cycles and retained newer synthetic test saves.
  Skyrim's Documents-alias restore bug was found and fixed. Later successful
  Skyrim cycles also poisoned registry and private-home settings.
- This proves startup/recovery smoke behavior, not complete gameplay, every mod
  manager/trainer feature, or real .NET/VC++ compatibility. Never claim otherwise.

### Important remaining limitations

1. Windows dependency payloads are zero-copy read-only mounts. The Steam **Linux**
   Runtime adapter still builds a per-room compatibility cache and can copy files
   across bind mounts. Several no-copy experiments broke loaders/alias setup;
   those experiments were removed before final validation. Do not reintroduce
   that broken path or describe the entire stack as zero-copy.
2. Prepared package publication/composition exists, but there is no curated
   vendor .NET/VC++ recipe catalog or automatic vendor installer downloader.
   Actual Microsoft runtime sets and registry integration need dedicated testing.
3. Shared kernel/device attack surface remains; no cgroup CPU/memory quotas,
   anti-cheat guarantee, security audit, hotplug device broker, portal/file-picker
   broker, or automatic legacy host-Documents import is implemented. Online
   permits reachable LAN services. Audio can include recording (shown in UI).
4. The game capture root is its executable directory, not always the complete
   installation root. Registry-based progress and nonstandard saves need profiles.
   Manual/undo checkpoints and quarantined runtime generations need retention UX.
5. This Btrfs mount permits unprivileged subvolume creation but denies deletion.
   New-prefix creation was corrected to use ordinary directories here. An empty
   ignored `.room-lab/btrfs-probe` remains; removing it requires administrator
   permission (`sudo btrfs subvolume delete .room-lab/btrfs-probe`). No user games
   were modified to perform these tests.

A final regression lab (`mines-final`) passed two Proton corruption/recovery
cycles on the final compatibility path. Its `cycle-0.png` was visually inspected
and shows the Mines board. Vite was following Wine drive symlinks in lab
checkpoints and exhausting file watchers; `vite.config.js` now ignores
`.room-lab`, `.taste-skill` and Rust build output.

### Evidence and safe continuation

`src-tauri/examples/room_lab.rs` creates an independent reflink/copy game tree and
separate database, requires a visible game window, corrupts only lab data, restores
and repeats. It refuses an existing destination. Do not point destructive tests at
original games or the user's normal launcher database. All original game assets,
mods, prefixes and saves were kept untouched.

Ignored `.room-lab/evidence/` retains reports/logs from the experiments; obsolete
large disposable game/prefix copies are cleaned up after evidence is retained.
Final regression reports and visually inspected Mines screenshots are in
`.room-lab/evidence/mines-final`; disposable game/prefix copies were removed. Browser
mock fixtures/screenshots are under ignored `.taste-skill/recovery/`.

Commands:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib --offline
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --offline -- -D warnings
RESPAWN_TEST_WINE=1 npm run test:rooms
npm run check && npm run build && npm run test:packaging
cargo run --manifest-path src-tauri/Cargo.toml --example room_lab -- NEW_LAB GAME_EXE proton PROTON_SCRIPT
```

No push, deployment, release package publication or external message was requested.
The next high-value work is curated runtime recipes and actual .NET/VC++ validation,
then control-plane crash/kill fuzzing, resource limits and portal integration.

## Historical handoff (superseded where it conflicts with the update above)

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
