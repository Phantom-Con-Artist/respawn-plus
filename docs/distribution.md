# Vessel distribution

Vessel uses one source repository and offers two installer editions. Both
editions produce an RPM and a Debian package and use the same app identifier,
executable (`vessel`), and data directory. They are alternative installations
of the same app; they are not intended to be installed side by side.

| Edition | Game runtimes | Build command |
| --- | --- | --- |
| Lite | User supplies Wine/Proton/UMU | `npm run bundle:lite` |
| Full | Includes staged Wine, Proton, UMU, and Steam Linux Runtime archives | `npm run bundle:full` |

Lite does not depend on the distro's Wine/Proton packages. Its desktop system
libraries (WebKitGTK/GTK etc.) are still normal RPM/DEB dependencies. Users can
use installed runners, or set `VESSEL_RUNTIME_DIRS` to colon-separated parent
directories containing custom Proton/Wine installations. A graphical runtime
picker is part of future development.

Full also declares Python 3 for the UMU zipapp. GPU drivers, the kernel, and
desktop system libraries remain host-provided. A single installer cannot make
one graphics-driver build portable across every Linux system.

## Runtime versions and repository contents

At release preparation time, select the newest upstream versions that pass our
game/tool compatibility tests, then pin their exact versions and checksums for
that Vessel release. Avoid floating `latest` inside a published installer or
silently replacing an existing game's selected runner.

Keep app source, build scripts, manifests, and packaging configuration in this
repository. Stage binary and source archives under ignored `build/runtime-bundle/`;
publish packages and matching sources as assets from the same project. Multi-GB
runtime payloads are not checked into Git history.

## Preparing Full

Stage this layout before running `npm run bundle:full`:

```text
build/runtime-bundle/
  manifest.json
  binaries/      # Versioned upstream runtime archives
  licenses/      # Exact upstream notices / combined notice files
  sources/       # Matching source archives and required build materials
```

The manifest has `format_version: 1`, `architecture: "x86_64"`, and a `components`
array. Each record has these fields (the hash values below are placeholders):

```json
{
  "id": "wine",
  "version": "PINNED_RELEASE_VERSION",
  "upstream_url": "https://upstream.example/releases/PINNED_RELEASE_VERSION",
  "archive": "binaries/wine-PINNED_RELEASE_VERSION.tar.xz",
  "archive_sha256": "64-lowercase-hex-characters",
  "license": "licenses/wine-NOTICES.txt",
  "license_sha256": "64-lowercase-hex-characters",
  "source_archive": "sources/wine-PINNED_RELEASE_VERSION-source.tar.xz",
  "source_archive_sha256": "64-lowercase-hex-characters"
}
```

Required IDs are `wine`, `proton`, `umu`, and `steam-runtime`. Additional entries,
such as `winetricks`, can use the same format. Preserve every upstream component's
licensing materials in its payload/source distribution; a manifest notice record
does not replace nested licenses. Source material must correspond to the exact
binary build, including any downstream patches and submodules.

`npm run bundle:check` checks required entries, pinned versions, contained file
paths, and SHA-256 hashes for all binary/notice/source files. It fails when the
staging directory is missing or incomplete. It is a packaging integrity check,
not an audit of source completeness or legal compliance.

`packaging/tauri.full.conf.json` adds the validated staged bundle as application
resources. Base configuration always includes Vessel's LICENSE, NOTICE, and
third-party notice policy. For a debug packaging check use:

```bash
npm run bundle:lite -- --debug
npm run bundle:full -- --debug
```

The output folders are `src-tauri/target/{release,debug}/bundle/{rpm,deb}`.
Tag published filenames with `-lite` or `-full` to distinguish editions; the
installed application's identity stays the same.

## Current implementation boundary

Lite is buildable now. Full has a guarded packaging configuration; actual
redistributable Wine/Proton/runtime archives and matching source materials have
not been staged, so no complete Full installer has been produced. Selecting,
unpacking, and executing bundled runners at app startup is part of the runtime
development milestone. Do not publish a Full package as playable before that
integration and game testing are complete.

## License and publication

Vessel original work is MIT. See LICENSE and THIRD_PARTY_NOTICES.md.
Wine remains under its upstream LGPL terms, and Proton has multiple component
licenses ([Wine LICENSE](https://github.com/wine-mirror/wine/blob/master/LICENSE),
[Proton LICENSE](https://github.com/ValveSoftware/Proton/blob/proton_11.0/LICENSE)).
Preserve the MIT copyright and permission notice when redistributing Vessel.
Distribute corresponding sources and other materials where required by bundled
components' upstream licenses; do not relabel third-party software as MIT code.

The repository is currently local. A public remote and source/download URL must
be established before publication; no hosted repository or release has been
created by this setup task.
