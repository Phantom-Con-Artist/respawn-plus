#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -euo pipefail

# Official release zipapp, containing UMU and its Python dependencies.
version=1.4.4
checksum=eb590691841f7fad3fc3ad8fd5db4ccb87849fe7948e62b28ece7a4ee48cc851
data_directory="${XDG_DATA_HOME:-$HOME/.local/share}"
destination="$data_directory/respawn-plus/tools/umu/$version"
launcher="$HOME/.local/bin/umu-run"

if [[ -e "$launcher" || -L "$launcher" ]]; then
  if [[ "$(readlink -f "$launcher")" != "$destination/umu-run" ]]; then
    printf 'An existing UMU installation is present at %s; leaving it in place.\n' "$launcher"
    exit 0
  fi
fi

if [[ ! -x "$destination/umu-run" ]]; then
  archive="$(mktemp -t respawn-plus-umu.XXXXXXXX.tar)"
  trap 'rm -f -- "$archive"' EXIT
  curl --fail --location --proto '=https' --tlsv1.2 \
    "https://github.com/Open-Wine-Components/umu-launcher/releases/download/$version/umu-launcher-$version-zipapp.tar" \
    --output "$archive"
  printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --status
  mkdir -p "$destination"
  tar --extract --file "$archive" --directory "$destination" --strip-components=1 umu/umu-run umu/umu_run.py
  chmod +x "$destination/umu-run"
fi
mkdir -p "$HOME/.local/bin"
if [[ ! -e "$launcher" && ! -L "$launcher" ]]; then
  ln -s "$destination/umu-run" "$launcher"
fi
"$launcher" --version
printf 'UMU installed. Make sure %s is on PATH.\n' "$HOME/.local/bin"
