#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Installs a per-user desktop entry and icons for a locally built Respawn+ Launcher, so desktop
# shells (KDE, GNOME) show its logo and name in the taskbar and app menu. Wayland matches the
# window's app_id ("respawn-plus") to respawn-plus.desktop. Packaged builds install their own.
# Usage: scripts/install-desktop-entry.sh [path/to/respawn-plus]   (default: the debug build)
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
binary=$(realpath "${1:-$root/src-tauri/target/debug/respawn-plus}")
[ -x "$binary" ] || { echo "Build Respawn+ first: $binary is missing" >&2; exit 1; }

data=${XDG_DATA_HOME:-$HOME/.local/share}
icons="$data/icons/hicolor"
for size in 32 64 128; do
  install -Dm644 "$root/src-tauri/icons/${size}x${size}.png" "$icons/${size}x${size}/apps/respawn-plus.png"
done
install -Dm644 "$root/src-tauri/icons/128x128@2x.png" "$icons/256x256/apps/respawn-plus.png"
install -Dm644 "$root/src-tauri/icons/icon.png" "$icons/512x512/apps/respawn-plus.png"
install -Dm644 "$root/public/brand/icon.svg" "$icons/scalable/apps/respawn-plus.svg"

install -d "$data/applications"
cat > "$data/applications/respawn-plus.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Respawn+ Launcher
GenericName=Game Launcher
Comment=Play Windows games on Linux with their mods and trainers
Exec="$binary"
Icon=respawn-plus
Terminal=false
Categories=Game;
StartupWMClass=respawn-plus
EOF

update-desktop-database "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
echo "Installed respawn-plus.desktop → $binary"
