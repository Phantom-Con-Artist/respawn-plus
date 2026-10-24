# Windows test game: Mines

We use **Mines**, from [Simon Tatham's Portable Puzzle Collection](https://www.chiark.greenend.org.uk/~sgtatham/puzzles/),
as a lightweight first Windows compatibility test. It is a genuine Windows GUI
executable, 353,864 bytes (about 346 KiB), version `20260923.616da16`.
The collection is MIT licensed; the original upstream notice is preserved in
`test-games/licenses/mines-LICENCE.txt` and copied alongside the installed game.

```bash
npm run test-game:setup
npm run test-game:status
npm run test-game:play
```

The setup command installs the standalone executable under
`~/.local/share/vessel/test-games/mines/20260923.616da16/`, verifies its exact
SHA-256 and size, and installs the upstream license. No administrator access is
required. Game binaries and generated data are stored outside the repository.

Upstream publishes a rotating download URL. The checked-in manifest pins the
bytes observed over official HTTPS, not an independently signed publisher
checksum. If upstream replaces the download, installation deliberately fails
until the new version/hash is reviewed. Existing verified installations are reused.
The official source archive URL is recorded in `test-games/mines.json`.
The initial setup also downloaded that version's source archive to
`puzzles-source.tar.gz` beside the executable for local source debugging; its
checksum is recorded in the manifest. Repeat setup only downloads the executable.

The play command uses UMU and the installed `GE-Proton11-11-x86_64` by default.
It assigns a dedicated `~/.local/share/vessel/prefixes/mines-test` Wine
prefix and writes UMU/Proton logs to `~/.local/state/vessel/logs/mines/`.
XDG data/state directory overrides are respected. Close the game normally to
return its exit status to the launching terminal.

Custom runtime and prefix overrides:

```bash
VESSEL_PROTON_PATH=/absolute/path/to/Proton \
  VESSEL_UMU_PATH=/absolute/path/to/umu-run \
  VESSEL_TEST_PREFIX=/absolute/path/to/test-prefix \
  npm run test-game:play
```

Optional game arguments can be passed after `--`. For example, a saved game
can be supplied as an absolute path when testing save/load behavior.

## What this establishes

The initial manual check launched the Windows executable through UMU/GE-Proton
in a 240×285 window. Its board rendered correctly, and a targeted mouse click
revealed numbered cells. Closing the window returned exit code 0. This validates
a small Win32 window, input, and normal process exit.
It does not exercise DirectX/Vulkan graphics, trainer injection, mod deployment,
or Skyrim compatibility. Save/load and restart behavior should be checked as
those launcher features are developed.

The npm scripts remain useful as a developer fixture. Vessel's desktop library
also discovers this installation and can launch it from a game card, using its
own per-game prefix and library logs. See [the library guide](library.md).
