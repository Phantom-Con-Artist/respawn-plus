# Respawn+

Respawn+ is an open-source Linux game launcher foundation for running Windows games with Proton/Wine and managing the surrounding modding environment: prefixes, mods, trainers, load order, and per-game launch state. Original project work is licensed under **MIT**; see [LICENSE](LICENSE) and [NOTICE](NOTICE).

Two RPM/DEB editions are planned: **Full** bundles the tested Wine/Proton runtime stack, while **Lite** lets users supply their own runtimes. Lite packaging is available now; Full staging and integration requirements are documented in [distribution](docs/distribution.md). Third-party software retains its upstream licenses; see [third-party notices](THIRD_PARTY_NOTICES.md). Contributions follow [CONTRIBUTING.md](CONTRIBUTING.md).

See [the verified environment report](docs/environment.md) for installed versions, completed checks, and local runtime paths.

For a quick Windows test before Skyrim, [install and run Mines](docs/test-game.md)
with `npm run test-game:setup` and `npm run test-game:play`.

## Development prerequisites

- Rust stable and Cargo
- Node.js 22.12+ (or 24 LTS) and npm
- Tauri Linux dependencies (WebKitGTK, GTK, and build tools; see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/))
- Proton (usually provided by Steam) and/or Wine for running Windows games

The app discovers installed Windows games, saves them in a local bento library, and launches them with a selected Wine or Proton runner. See [the library guide](docs/library.md) for discovery scope, custom runners, and launch logs.

## Run

```bash
npm ci
npm run tauri dev
```

Build Lite Debian and RPM packages with `npm run bundle:lite`. Build Full packages after staging the runtime bundle with `npm run bundle:full`. To build only the executable, use `npm run bundle:lite -- --no-bundle`. These commands embed the frontend so the packaged app runs without Vite.

## Verify

```bash
npm run doctor
npm run check
npm run build
npm run test:packaging
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path src-tauri/Cargo.toml -- --diagnose
```

`--diagnose` initializes the local SQLite database and prints the actual detected runtimes without opening a window. Tests use temporary databases and do not change your game state.

## Fedora host setup

Tauri build dependencies, from the [official prerequisites](https://v2.tauri.app/start/prerequisites/):

```bash
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel gcc gcc-c++ make
```

For this AMD graphics machine, Wine and 32-bit graphics support:

```bash
sudo dnf install wine winetricks mesa-vulkan-drivers.i686 vulkan-loader.i686
vulkaninfo --summary
```

For non-Steam games, use [UMU](https://github.com/Open-Wine-Components/umu-launcher) with GE-Proton or UMU-Proton. Steam is not required for UMU. The installer uses the official UMU 1.4.4 zipapp release under `~/.local/share/respawn-plus/tools/umu/1.4.4`, linked from `~/.local/bin/umu-run`. Its SHA-256 was verified against the GitHub release asset digest. Existing UMU installations are retained. No third-party binaries are checked into this repository.

Run `npm run setup:runtime` to reproduce the UMU installation with a pinned checksum. This requires Python 3.10+ and installs in your user directory.

UMU can download Proton and the Steam Linux Runtime on its first run. To initialize a dedicated test prefix without starting a game:

```bash
WINEPREFIX="$HOME/.local/share/respawn-plus/prefixes/runtime-check" \
  GAMEID=0 PROTONPATH=GE-Proton umu-run cmd.exe /c 'exit 0'
```

See the [UMU manual](https://github.com/Open-Wine-Components/umu-launcher/blob/main/docs/umu.1.scd) for runner settings. Plain Wine uses a separate path and may need DXVK/vkd3d configured; Proton includes its own graphics components.

## Current foundation

- Tauri v2 desktop shell
- Artwork cards, search, running-game filter, folder discovery, and manual executable import
- Per-game runtime/argument settings, isolated prefixes, launch supervision, exit status, and logs
- [Companion apps](docs/companion-apps.md): launch Windows mod managers, SKSE/script extenders, trainers, and utilities in the game's shared environment
- Rust backend with SQLite state persistence
- Runtime discovery for UMU, Wine, native/Flatpak Steam Proton, and Lutris Wine
- Versioned SQLite schema with transactions, WAL, and foreign keys for games, runtimes, prefixes, mods, launch profiles, and companion tools
- No game files are copied or modified during discovery

The database is created at `$XDG_DATA_HOME/respawn-plus/respawn-plus.sqlite3` (normally `~/.local/share/respawn-plus/respawn-plus.sqlite3`). Additional Steam libraries/custom runners can be discovered by setting `RESPAWN_RUNTIME_DIRS` to colon-separated parent directories containing Proton/Wine installation folders. External Steam libraries are not automatically parsed yet.

The initial setup used the temporary GameForge name. Its test database, prefix, and installed UMU files are preserved at their original paths; new app state uses Respawn+'s directory. No previously created local data was moved or removed during the rename.

## Next milestone: Skyrim modding environment

Discovery can add the existing `SkyrimSE.exe` to the library. Verify basic Skyrim launching next, then integrate Mod Organizer 2, SKSE, and trainers using the same game environment so they can see the same registry, files, and Windows processes. The schema reserves companion tool entries; tool supervision and mod deployment are not implemented yet.

After basic launching, implement reversible mod deployment, conflict/load-order tracking, prefix backups, and tool launch profiles. Verify Skyrim/SKSE/game version compatibility for each setup. Existing [MO2 Linux integration](https://github.com/Furglitch/modorganizer2-linux-installer) provides useful reference material, but should be tested with this installation before integration. Compatibility varies by game and tool; universal Windows parity is an objective, not an established capability.

