# Mod manager integration — design

Status: approved for planning, 2026-10-24. Generalised the same day: Vessel integrates
mod managers in general (Mod Organizer 2, Vortex and others), not MO2 alone.

## Scope: any mod manager, through adapters

Vessel is a general launcher to run, play and configure Windows games and their mods the way
players do on Windows. Any mod manager, script extender or trainer can already be added to a game
as a companion app (any Windows executable). This design adds *guided* integration on top:

- A `ModManagerAdapter` per manager knows how to install or locate it, prepare its per-game
  instance, and build the command that launches the game through it. Each adapter declares what
  it supports; the UI offers only those actions.
- **Mod Organizer 2 is the first adapter**, because its virtual filesystem is proven under Proton
  (below). **Vortex is the next**; it deploys mods with links into the game folder and needs its own
  feasibility check under Proton before an adapter is written. Other managers follow the same trait.
- The per-game toggle is generic: "Launch through mod manager" points at whichever companion
  profile carries an adapter, not at MO2 specifically.
- Trainers stay generic companion apps: standalone trainers and trainer apps such as WeMod are
  added the same way and join the game's room.

The sections below describe the MO2 adapter in detail; a Vortex adapter gets its own spec after
its spike.

## Goal

Manage Skyrim SE mods in Mod Organizer 2 and play the modded game from
Vessel. Mods are applied through MO2's virtual filesystem (USVFS); the game
folder is not modified. MO2 runs in the game's prefix and shared UMU room, so
companions such as the trainer keep working alongside it.

Done means: Play in Vessel with "Launch through MO2" on starts MO2 → SKSE →
Skyrim, and a mod installed only in MO2 (SkyUI) loads in game.

Out of scope: Nexus login, automatic mod downloads, games other than Skyrim SE.

## Decisions (from the user)

- MO2 source: both an **Install MO2** action and **Use existing MO2**.
- Play behaviour: a **per-game toggle**, "Launch through mod manager".
- Loose SkyUI files in the game's Data folder: **move them into MO2**.
- Approach A: one shared MO2 install, one MO2 instance per game, mods kept
  outside the prefix.

## Feasibility evidence (throwaway spike, 2026-10-24)

MO2 2.5.2 (Nexus installer, SHA-256
`d3f699d4042ff209f596d98dacd7ebaa99d27e01c3a17cd5d4e19c2bab6ed006`) was
installed silently (`/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOICONS /DIR=`)
inside game 2's room under GE-Proton 11-11 / UMU 1.4.4 / steamrt4.

- A portable instance with a probe mod made `Data\VesselProbe\hello.txt`
  visible to `cmd.exe` launched through MO2, while the real Data folder had no
  such file. USVFS works under Proton.
- `moshortcut://:SKSE` started SKSE 2.3.1 and Skyrim 1.7.104 to the main menu;
  `usvfs_x64.dll` was mapped into `SkyrimSE.exe`.
- MO2 shows "waiting on an application to close before exiting" while the game
  runs (expected; MO2 must stay alive for USVFS) and exits about 20 s after the
  launched program ends.

Required preconfiguration, otherwise MO2 blocks on dialogs:

```ini
[General]
game_edition=Steam          ; this copy uses steam_api64 (Steam variant)
first_start=false
[DialogChoices]
steamQuery=65536            ; "Continue without starting Steam"
```

Write paths as `Z:\…`: Proton's game-drive letter (`X:`, `S:`) depends on the
launch directory and is not stable.

## Design

### 1. Getting MO2 — `src-tauri/src/mo2.rs`

- **Install MO2:** download the official `Mod.Organizer-2.5.2.exe` GitHub
  release asset, verify a pinned SHA-256 (reject any mismatch), run it silently
  in the game's room into `~/.local/share/vessel/tools/mo2/2.5.2/`.
  Before pinning, confirm the GitHub asset hash equals the Nexus hash above.
- **Use existing MO2:** pick a folder containing `ModOrganizer.exe`, or an MO2
  installer, which is installed the same way.

### 2. Per-game instance

- A global MO2 instance in the game's prefix
  (`AppData\Local\ModOrganizer\<instance>`), with `base_directory` pointing to
  `~/.local/share/vessel/mods/game-<id>/` so mods, downloads and profiles
  survive a prefix rebuild.
- Vessel writes `ModOrganizer.ini`: game name, path, edition, the dialog
  answers above, a `Default` profile, and an `SKSE` executable when
  `skse64_loader.exe` exists.
- Launch with `moshortcut://<instance>:SKSE`.
- **First implementation task:** verify MO2 selects a named global instance
  from `moshortcut://<instance>:…`. Fallback if not: a portable MO2 copy per
  game (the configuration the spike proved).

### 3. Data model — schema v5 (v4 added display modes)

- MO2 is the game's existing `mod_manager` companion profile (opens MO2's UI).
- `games.launch_via_tool_id INTEGER NULL REFERENCES tools(id) ON DELETE SET NULL`
- `games.launch_via_shortcut TEXT NOT NULL DEFAULT 'SKSE'`
- With the toggle on, Play launches the MO2 profile with the shortcut argument.
  It counts as a game session (last played, duplicate-launch guard) and
  companions still join the same room.

### 4. UI — game detail panel

"Mod Organizer 2" section: **Install MO2**, **Use existing…**, **Set up for this
game**, **Open MO2**, and the **Launch through MO2** toggle.

### 5. SkyUI migration (local, one-off)

A developer script beside `scripts/prepare-skyrim-test.mjs`, not product code:
restore Data from
`~/.local/share/vessel/backups/skyrim-test-1792827196689/manifest.json`
(hash-checked before any deletion; stop on mismatch; SKSE loader and DLL stay),
then install SkyUI from the user's zip as an MO2 mod and enable its plugin.

### 6. Testing

- Unit: ini generation; v4→v5 migration preserves data; Play command with the
  toggle on/off; checksum rejection.
- Real: with a clean Data folder, Play → MO2 → SKSE → Skyrim; SKSE's log reads
  SkyUI's translations (only possible through MO2), then confirm the MCM in game.

## Known limitations carried over

- Companions joining a running room inherit the first app's working directory.
- No PID hiding or isolation; MO2 and trainers are ordinary Wine processes.
