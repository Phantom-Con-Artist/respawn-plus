#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Installs a per-user desktop entry and icons for a locally built Vessel, so desktop
# shells (KDE, GNOME) show its logo and name in the taskbar and app menu. Wayland matches the
# window's app_id ("vessel") to vessel.desktop. Packaged builds install their own.
# Usage: scripts/install-desktop-entry.sh [path/to/vessel]   (default: the debug build)
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
binary=$(realpath "${1:-$root/src-tauri/target/debug/vessel}")
[ -x "$binary" ] || { echo "Build Vessel first: $binary is missing" >&2; exit 1; }

data=${XDG_DATA_HOME:-$HOME/.local/share}
icons="$data/icons/hicolor"
for size in 32 64 128; do
  install -Dm644 "$root/src-tauri/icons/${size}x${size}.png" "$icons/${size}x${size}/apps/vessel.png"
done
install -Dm644 "$root/src-tauri/icons/128x128@2x.png" "$icons/256x256/apps/vessel.png"
install -Dm644 "$root/src-tauri/icons/icon.png" "$icons/512x512/apps/vessel.png"
install -Dm644 "$root/public/brand/icon.svg" "$icons/scalable/apps/vessel.svg"

install -d "$data/applications"
cat > "$data/applications/vessel.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Vessel
GenericName=Game Launcher
Comment=Play Windows games on Linux with their mods and trainers
Exec="$binary"
Icon=vessel
Terminal=false
Categories=Game;
StartupWMClass=vessel
EOF

update-desktop-database "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
echo "Installed vessel.desktop → $binary"
