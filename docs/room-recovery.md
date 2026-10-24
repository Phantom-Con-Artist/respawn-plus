# Room recovery

Respawn+ can restore a supported game's setup while keeping its current saves. Open the game's **Overview → Room recovery**, choose a restore point, and select **Restore setup · keep saves**. Close the game and companions first. A **Before restore — undo** checkpoint preserves the setup you are replacing.

## Current support

Save preservation currently supports Skyrim Special Edition (`SkyrimSE.exe`) and Fallout 4 (`Fallout4.exe`):

- Each Wine user's `Documents/My Games/<game>/Saves` directory, including co-located script-extender save files.
- Portable Mod Organizer 2 `profiles/<profile>/saves` under its registered executable or working directory, including profiles created after the checkpoint.
- Wine Documents links to external directories are left untouched when the link has not changed and points outside every captured tree.

Other games can enable recovery through data-driven room profiles: `save_paths` relative to the effective Wine prefix, `game_save_paths` relative to the executable directory, and `home_save_paths` relative to the private room home. Without a known mapping, recovery remains disabled. Wine aliases between captured directories are resolved before staging. Changed external Documents links, registry-based progress and custom MO2 base directories need additional mappings or adapters. Do not use this feature to promise save preservation for those layouts. A mapped save kept through rollback may still require the mods or game version that created it.

## What is restored

Before the first game or companion process opens a supported room, Respawn+ checkpoints:

- The room's Wine/Proton prefix, registry, private home, and pinned shared-package/profile references. Shared runtime payloads stay in the global store.
- The directory containing the game executable.
- Each registered companion's executable directory and working directory.
- Game launch/display settings and companion profiles in the launcher database.

Nested paths are captured once. Files outside these directories are not captured; this includes runtime binaries installed elsewhere, external mod libraries and external save directories. Runtime paths are recorded, but their binary contents are not pinned or restored by this feature. A game's executable directory may be below its installation root: only the captured directory is protected. General installation-root detection remains future work.

Restoring resets captured files, additions, deletions, registry state and launch settings. Current mapped save directories replace the checkpoint's save directories in a staged copy before anything is switched. This also preserves a current save directory's absence. Session records, playtime, and other games' database records are retained; discarded companion profiles are archived so their historical launches remain intact.

The last three automatic **Before session** checkpoints are retained. Manual and undo checkpoints are currently retained indefinitely. Copies on filesystems without reflink support can use substantial disk space. Checkpoint preparation is shown before launch; it is not guaranteed instantaneous.

## Storage and consistency

New prefixes use Btrfs subvolumes only when deletion is supported by the mount policy; otherwise they use normal directories and reflinks/copies. Existing prefixes are not automatically converted. A source that is an accessible Btrfs subvolume with no nested subvolumes uses a native snapshot. When nested subvolumes exist, or inspection/snapshotting is unavailable, Respawn+ uses GNU `cp --archive --reflink=auto`: reflinks where supported and independent copies otherwise. Writable hardlink trees are never used. Native Btrfs checkpoint snapshots are read-only; copied checkpoints live in a private application directory, not an enforced read-only security boundary.

One process holds the library's advisory lock, and launch/settings/recovery mutations share a mutex. Before checkpoints or restores, Respawn+ checks recorded launches and visible same-user `/proc` environments for processes using the prefix. This catches ordinary Wine children left behind after a launcher exits. Unreadable environments and independent programs writing the same folders cannot be proven idle by this check; robust adversarial process ownership needs the future room supervisor/sandbox.

Snapshots and clones are flushed before publication. SQLite contains manifests and a durable restore intent, not copies of live databases. The current launcher's WAL database remains outside captured trees. Restores stage each tree beside its destination for same-filesystem renames, publish the intent, then switch directories and transactionally restore the room's database configuration. Multiple directory switches are not atomic as a group: the journal lets startup finish an interrupted restore before allowing new launches or edits. Previous directories are collected only after commit. Failed preparation and retired checkpoint trees are tracked for cleanup.

Recovery refuses broad/system directories, directories overlapping another registered room, changed room root locations, and unsafe save links. Checkpoint failures stop a supported room from launching with an unprotected session. Unregistered external writers must be closed too.

## Limits and next work

This is file/setup recovery on restart, not a snapshot of running game memory. Snapshots share underlying storage and do not replace independent backups or repair downloads for damaged disk blocks. Cloud/server-side progress cannot be rolled back here.

Native Btrfs behavior still needs integration testing on supported distributions; automated tests exercise the reflink/copy backend and simulated crashes at directory rename boundaries. Game/Proton/mod-manager combinations need real-session testing before a stable release.

Bubblewrap confinement and a shared Windows package store are now implemented; see [Proton Rooms](proton-rooms.md). The Windows package catalog still needs curated vendor recipes and broader real-runtime validation.
