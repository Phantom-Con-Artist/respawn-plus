# Mod managers, script extenders, and trainers

Select a game card, open **Companion apps**, and choose **Add app**. Give the
profile a name, choose its type, and select its Windows executable. Optional
arguments are one per line. The working folder defaults to the executable's
folder, or can be chosen explicitly. Save the profile, then use **Open**.

Profiles support mod managers (such as MO2), script extenders (such as SKSE64),
trainers, and utilities. Every profile inherits the selected game's saved Wine
or Proton runner and its prefix. The file picker does not install software;
point it at an installed/portable app, or run an installer as a setup step first.
Native Linux manager binaries and MSI packages are not supported by this picker.

Each profile has its own running state, launch history, and **Log** action. Edit
and removal are blocked while that app is running. Removing a profile deletes
only Vessel's metadata/history; executable files, mods, logs on disk, and the
game prefix are retained. Runtime changes are blocked while the game or any
of its companion apps are active.

## Running the game and trainer together

Start the game (or its SKSE profile) and open the trainer from the same game's
Companion apps tab. Both use one prefix and one selected runner so they can see
the same Windows processes.

For Proton, Vessel enables UMU's `UMU_CONTAINER_NSENTER=1` integration. The
first application starts the container's launcher service. UMU identifies that
service by a hash of the canonical prefix path; later applications use
`steam-runtime-launch-client` to enter the existing container with Proton's
`runinprefix` verb. The first launch uses `run`. `waitforexitandrun` would wait
for existing prefix processes to exit and serialize the game and trainer.
The tested runtime is UMU 1.4.4 with GE-Proton 11-11 and steamrt4.

This is an application session, whose lifetime follows the running Wine apps;
there is no separate idle-room service or Close room button yet. Start the game
and let its window appear before opening companions. UMU's discovery retry is
bounded, so simultaneous launches during initial container startup are not yet
an atomic operation. The shared container keeps separate Linux processes and
uses the host PID namespace. Linux still sees their real PIDs; this does not
implement PID hiding or a security boundary against untrusted executables.

UMU's current reentry client inherits the first application's working directory
and environment. A companion's custom working folder applies when it starts the
container, but is not forwarded when joining an existing one. Tools that
require their own working directory need additional launch-client integration.

Runtime output goes only to each app's own Vessel launch log. Vessel does
not enable `PROTON_LOG`: a companion joining the room inherits the game's
environment, and Proton reopens `steam-<id>.log` for writing on every launch, so
a trainer would truncate the game's log and destroy its crash output.

A script-extender profile counts as a game session: its activity blocks a
second direct launch of that game and contributes to last-played time. A
manager/trainer launch has independent history and does not by itself count
as playing the game. A trainer opening does not establish that its memory
patterns or hotkeys support the installed game version; verify attachment
and each feature separately.

## Mod-manager behavior

MO2 uses a virtual filesystem. Launch the modded game from inside MO2 when
using its virtual mods. Vessel's separate direct Play action does not apply
MO2's virtual deployment. Runtime prerequisites and manager compatibility
remain specific to the tool; Vessel does not yet install MO2, runtimes, or
mod packs automatically.

The backend tests cover inherited prefixes/runners, literal arguments, custom
working folders, independent logs and exit codes, preservation across restarts,
and preventing cross-game profile edits. A real Windows Notepad companion
profile was used for the basic GUI smoke check and then removed.

## Play through a companion app

On the game's Launch tab, set **Play starts** to one of the game's companion apps. Play then opens that app, with its own arguments, in the game's room, and the time counts as play. Use it for any mod manager that can start the game itself, for example Mod Organizer 2 with the argument `moshortcut://:SKSE` (an executable named SKSE inside MO2) or Vortex. Opening the same app with its own Open button is not counted as play. Other companion apps keep running alongside it.
