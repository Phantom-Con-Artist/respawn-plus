# The Respawn+ game library

Start with `npm run tauri dev`, or run the built desktop executable. The app
opens a persistent local library. On the first empty-library launch, it scans
common game folders. **Find games** repeats that scan; **Scan folder** searches
a folder selected with the native file picker; **Add game** imports a selected
Windows `.exe`. There is also an explicit-path import dialog.

The bento library cards show artwork, title, running/readiness status, and a
Play action. Select a card to edit its name, runtime, and arguments. Custom
Wine/Proton executables can be chosen from disk; save or discard edits before
switching games. Known Mines and Skyrim executables have artwork mappings;
other games and failed remote image loads have a name fallback.

## Discovery scope

Default roots are Respawn+'s installed test games, `~/Games`, the configured
Downloads folder plus `~/Downloads`, and common native/Flatpak Steam library
locations. Discovery is heuristic: it searches for Windows executables, prefers
a game over an adjacent launcher, and excludes common installers, uninstallers,
crash reporters, and redistributable folders. A found executable is a candidate,
not proof that it will run with the selected runner.

Scans inspect at most 20,000 directory entries, descend at most seven levels,
and skip nested symlink trees. A limit notice suggests scanning a more specific
folder. External drives and nonstandard Steam libraries can be scanned explicitly.
Repeated scans preserve saved names, runtime selections, and arguments. Scanning
adds metadata; it does not execute candidates or change their installation files.

This milestone covers directly launched **Windows executables**. It does not
yet enumerate native Linux desktop games or route Steam-managed/DRM titles
through `steam -applaunch`; those integrations need their own discovery and
launch adapters. Manual import covers Windows games outside the scanned roots.

## Launch behavior

Proton uses UMU, with the selected Proton installation and a per-game prefix
under `~/.local/share/respawn-plus/prefixes/game-<id>`. Wine uses the chosen Wine
executable and that same game's prefix. Close the game before changing runners;
runner switching can modify an existing prefix, and automatic prefix backups
are not part of this milestone.

The executable's parent is the working directory. Arguments are one per line
and are passed as literal process arguments; a shell is not used. The app
tracks the runtime process, prevents duplicate launches it knows are running,
updates the running filter, and records exit status and last-played time.
Games started externally are not attached to Respawn+'s launch tracking.

Launch logs are under `~/.local/state/respawn-plus/logs/library/game-<id>/`.
**View launch log** displays up to the last 64 KiB of the latest launch's output;
Proton's separate detailed log is retained in that game's log directory too.
Closing Respawn+ does not terminate games. On reopening, Linux process start
tokens identify a live tracked runtime; if it ended while the app was closed,
the outcome is marked unknown rather than inventing a successful exit code.

SQLite schema v2 adds runtime/argument settings and launch history; v3 adds
companion-app working folders and separate launch ownership while preserving
existing game records. App data follows XDG directories. The local
library can also be inspected with:

```bash
src-tauri/target/debug/respawn-plus --list-games
src-tauri/target/debug/respawn-plus --scan-games
```

Mines is the launch verification target. Skyrim can appear in the library
without being launched; compatibility, mods, and trainers remain separate tests.
