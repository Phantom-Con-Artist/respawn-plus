# Contributing to Vessel

Vessel is licensed under MIT. By submitting original contributions,
you agree to distribute them under that same license. Retain upstream notices
when adding third-party material; do not apply Vessel's copyright to it.

Start with the development and verification commands in README.md. Keep code,
runtime manifests, and release documentation in this repository. Runtime binary
payloads, extracted runtimes, game files, and credentials do not belong in Git.

Changes to the app should pass frontend checks/build, Rust formatting, tests,
and Clippy. Distribution changes should also build and inspect RPM/DEB packages.
Runtime updates must pin the selected versions and record binary/source checksums
and licensing materials before building a full installer.
