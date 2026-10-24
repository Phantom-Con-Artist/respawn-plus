# Verified development environment

Checked on 2026-10-24 on Fedora 44 KDE, AMD Radeon Vega 8 (RADV).

This is the historical setup report from before the rename to Vessel. Paths,
artifact names, and marker text below record that original check. Those files
are retained. New app state uses `~/.local/share/vessel/vessel.sqlite3`;
the executable is now `vessel`, packages are branded Vessel, and custom
runtime directories use `VESSEL_RUNTIME_DIRS`. See [distribution](distribution.md)
for the Full and Lite installer editions and current packaging scope.

| Component | Version / state |
| --- | --- |
| Rust / Cargo | 1.98.1 |
| Node / npm | 24.18.0 / 11.16.0 |
| Tauri Rust / JavaScript CLI | 2.12.5 / 2.12.4 |
| WebKitGTK / GTK | 2.54.1 / 3.24.52 |
| SQLite | Bundled through rusqlite; versioned schema initialized successfully |
| Wine | 11.0 Staging, Fedora package 11.0-3.fc44 |
| Winetricks | 20260125 |
| UMU | Official 1.4.4 zipapp, SHA-256 checked |
| Proton | GE-Proton11-11-x86_64, SHA-512 checked by UMU |
| Steam Linux Runtime | steamrt4, 4.0.20261021.265997; archive and mtree checked by UMU |
| Vulkan | Working RADV GPU, Mesa 26.2.5, x86_64 and i686 drivers installed |
| Git | Initialized on main; no initial commit or remote created |

The Fedora transaction installed Wine/Winetricks and 32-bit graphics dependencies. It also updated matching Mesa packages and systemd dependencies to 259.10. The machine was not rebooted.

## Verification completed

- Frontend syntax checks and production asset build passed.
- All three Rust tests passed: SQLite persistence and cascade cleanup, repeatable migrations/newer-schema rejection, and Proton discovery with spaces/symlink deduplication.
- Rust formatting and Clippy with warnings denied passed.
- npm audit reported zero known vulnerabilities at setup time.
- Tauri debug build produced Debian and RPM packages under `src-tauri/target/debug/bundle/`.
- The built executable's `--diagnose` command created the state database and detected GE-Proton, UMU, and Wine.
- The desktop executable stayed running for a 10-second startup check with no output in its error log; it was then stopped by the test timeout. Visual layout and interactive behavior were not reviewed in this check.
- UMU/Proton ran Windows `cmd.exe` successfully. It wrote `GAMEFORGE_RUNTIME_OK` to `drive_c/gameforge-runtime-check.txt` inside the dedicated test prefix.

The original empty-string UMU prefix bootstrap created the prefix but returned an executable-not-found error with this GE-Proton build. The subsequent `cmd.exe` check exited successfully. Use the `cmd.exe` initialization command in the README for repeat runs.

## Local paths

- Database: `~/.local/share/gameforge/gameforge.sqlite3`
- Test prefix: `~/.local/share/gameforge/prefixes/runtime-check`
- UMU: `~/.local/share/gameforge/tools/umu/1.4.4/umu-run`
- Proton: `~/.local/share/Steam/compatibilitytools.d/GE-Proton11-11-x86_64`
- Container runtime: `~/.local/share/umu/steamrt4`

The Skyrim SE executable was found in the supplied Downloads installation. The game has not been imported or launched, and game files have not been modified. Game launching, mod deployment, and trainer integration are not implemented by this foundation.
