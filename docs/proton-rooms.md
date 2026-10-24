# Proton Rooms: security and shared dependencies

The production launch path starts a bubblewrap supervisor for each game. Every game and companion command executes inside that room; sandbox setup failures stop launch. There is no automatic unrestricted fallback. Plain Wine and Proton through UMU use the same mechanism, without a game-title whitelist.

Rooms receive a private home, `/tmp`, `/proc`, PID/IPC/user/mount/network namespaces, dropped capabilities and a libseccomp filter. Host environment variables and file descriptors are not inherited by game commands. The launcher database, other rooms, personal home files, SSH agents and host session D-Bus are not mounted. Game and companion folders are explicit writable grants; overly broad folders and folders overlapping another registered room are rejected.

On Wayland desktops, rooms default to native Wayland: the game, its mod manager and its trainer each get their own window, and Wayland keeps them from reading other windows. A private rootful Xwayland display (one window for the whole room, where Vessel sets the display mode) and a headless mode are available per game. Host X11 is not exposed. Proton uses a private D-Bus daemon for UMU launcher-service discovery. All companions share the outer room and UMU reuses its per-prefix inner container.

Online rooms use pasta with a private network namespace, DNS forwarding and automatic port forwarding disabled. This avoids sharing host abstract Unix sockets. Online access includes reachable LAN services; it is not an internet-only firewall. Offline rooms have no external network. GPU rendering and selected joystick devices are exposed. Audio socket access can include recording, which the UI explicitly states.

The shared kernel, GPU/audio services and permitted drivers remain attack surfaces. This is not VM isolation, a security audit, kernel anti-cheat support, a resource-exhaustion guarantee, or proof that every mod manager works. No cgroup memory/CPU limits are implemented. Closing the launcher terminates its rooms; **Stop room apps** explicitly stops all apps in one room.

## Shared Windows runtime packages

Import a prepared package once through **Library → Shared Windows dependencies** or `install_windows_runtime`. Packages are copied into a SHA-256-addressed store, verified, and published with read-only file modes. A room resolves capabilities and exact package pins, then binds required payload files read-only into its own Windows prefix. It does not copy those payloads into each prefix. Registry recipes are imported into each room's private registry; DLL overrides are composed with conflict detection. First-use prefix initialization happens inside the sandbox.

Package schema 1 uses a `manifest.json` such as:

```json
{
  "schema": 1,
  "id": "example-runtime",
  "version": "1.0.0",
  "architecture": "x64",
  "provides": ["example-runtime:x64"],
  "requires": [],
  "detect": {"imports": ["example.dll"]},
  "files": [{
    "source": "payload/example.dll",
    "target": "drive_c/windows/system32/example.dll",
    "sha256": "<actual 64-character lowercase SHA-256>"
  }],
  "registry": ["integration.reg"],
  "dll_overrides": {"example": "native,builtin"}
}
```

Supported architectures are `x86`, `x64`, `arm64` and `any`. A room can contain both x86 and x64 dependencies. Source traversal, symlinks in packages, conflicting targets, checksum failures, dependency cycles and ambiguous providers are rejected. Wine's existing DLL leaf symlinks can be replaced with prefix-local mount placeholders; parent-directory symlinks are rejected. Runtime payloads remain shared inodes across room mounts.

Optional detection matches PE DLL imports, managed PE headers, and `.runtimeconfig.json` framework names/major versions. `detect.frameworks` entries look like `Microsoft.WindowsDesktop.App/8`; `detect.managed: true` matches a managed executable. Detection uses installed trusted package manifests, not game-supplied host commands or permission requests. Ambiguous providers require a pin or one declared `default` provider. Selected identities are persisted before the session checkpoint. This is dependency inference, not a complete analysis of dynamically loaded plugins.

The original import directory is recorded as a repair source. A damaged package can be rebuilt only if that source still matches the original pinned identity. Changed or unavailable sources do not silently upgrade or substitute a runtime. Damaged generations are quarantined. Checkpoints restore package pins and room policy alongside local files.

No Microsoft binaries, prebuilt .NET/VC++ package catalog, automatic vendor installer downloader, or universal .NET Framework registry adapter is included. Those recipes need curation, redistribution review and actual runtime/game testing. This implementation provides the generic package/publication/composition mechanism; importing arbitrary installer executables is not supported.

## Linux runtime compatibility cost

UMU's installed Steam Linux Runtime payload is exposed read-only, with private marker/lock metadata. The current pressure-vessel adapter still builds a writable compatibility cache and can fall back to copying Linux runtime files across bind mounts. **The zero-copy guarantee here applies to the Windows package payload, not every Steam Linux Runtime cache file.** Attempts to force pressure-vessel's no-copy mode on this installed runtime broke loader/alias setup and were removed. Keep this limitation explicit until a portable replacement is verified.

## Validation

`npm run test:rooms` exercises real bubblewrap mount/PID/network namespaces, read-only shared inodes, hidden host files/environment, shared companion namespaces, private Xwayland, host abstract-socket isolation, package poisoning/repair, path traversal and control-socket spoofing. Set `VESSEL_TEST_WINE=1` to include real Wine console startup. Rust tests cover SQLite migration, save preservation, policy restoration, Wine aliases, and interrupted directory switches.

The disposable lab uses independent reflink/copy game trees and a separate database:

```sh
cd src-tauri
cargo run --example room_lab -- /path/to/new-disposable-lab /path/to/game.exe proton /path/to/Proton/proton
```

It requires a real game window, deliberately stops the room and corrupts the copied executable, configuration, registry and private-home settings, then restores them while preserving newer synthetic test saves. It repeats the cycle. Only lab copies are modified. Captures, when ffmpeg is available, show the private Xwayland display rather than the host desktop.

On 2026-10-24, Mines opened through Wine 11 and GE-Proton 11-11/UMU 1.4.4; Skyrim Special Edition opened through that Proton stack. Successful lab runs restored corrupted setup files and retained test saves across two cycles. These are startup/recovery smoke tests, not complete gameplay or mod-manager certification. Native Btrfs snapshot deletion is disallowed by this host's mount policy, so the recovery integration used reflinks/copies.
