# Third-party components

Vessel's MIT license covers its original work. It does not replace
the licenses of independently distributed runtimes or third-party dependencies.

The standard development packages currently include the application and its
compiled dependencies, not Wine/Proton runtime payloads. Cargo.lock and
package-lock.json identify the resolved dependencies. A complete generated
dependency notice inventory must accompany a public binary release.

Full runtime packages must preserve the license texts, copyright notices,
and other required materials included in the exact upstream distributions:

| Component | Upstream licensing reference |
| --- | --- |
| Wine | [Wine LICENSE](https://github.com/wine-mirror/wine/blob/master/LICENSE): LGPL-2.1-or-later; retain AUTHORS and COPYING.LIB |
| Proton / GE-Proton | [Proton LICENSE](https://github.com/ValveSoftware/Proton/blob/proton_11.0/LICENSE) and each component's LICENSE/COPYING; Proton is composed of software under multiple licenses |
| UMU | [UMU repository](https://github.com/Open-Wine-Components/umu-launcher): retain its GPL license and bundled dependency notices |
| Steam Linux Runtime | Preserve the selected runtime's component notices and provide the matching source materials required by their licenses |
| Mines developer test game | [Simon Tatham's Portable Puzzles](https://www.chiark.greenend.org.uk/~sgtatham/puzzles/), MIT; original notice in `test-games/licenses/mines-LICENCE.txt`. The game binary is downloaded separately; a gameplay screenshot and its license are included for the library card |
| Skyrim library artwork | Bethesda/ZeniMax artwork loaded from the official Steam CDN at runtime; not bundled or licensed under Vessel's MIT license. See `public/artwork/README.md` |
| Inter and Geist Mono fonts | [Inter](https://github.com/rsms/inter) and [Geist Mono](https://github.com/vercel/geist-font), SIL Open Font License 1.1; bundled into the frontend from `@fontsource-variable/inter` and `@fontsource-variable/geist-mono` 5.3.0. The OFL text ships in each package's `LICENSE` |

Each release must include matching source archives/build materials for bundled
copyleft components, including downstream patches and submodules as applicable.
A link to a moving upstream branch is not the release source archive. The runtime
manifest records checksums for binaries, notice files, and source archives.

This file records the distribution policy, not a completed third-party audit.
The bundle validator checks recorded files and checksums; it cannot determine
whether source archives or licensing materials are legally complete.
