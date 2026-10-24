#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -uo pipefail

failures=0
for binary in rustc cargo node npm cc pkg-config; do
  if command -v "$binary" >/dev/null 2>&1; then
    printf 'OK      %s: %s\n' "$binary" "$(command -v "$binary")"
  else
    printf 'MISSING %s\n' "$binary"
    failures=$((failures + 1))
  fi
done

if command -v pkg-config >/dev/null 2>&1; then
  for library in webkit2gtk-4.1 gtk+-3.0 openssl librsvg-2.0; do
    if pkg-config --exists "$library"; then
      printf 'OK      %s: %s\n' "$library" "$(pkg-config --modversion "$library")"
    else
      printf 'MISSING %s\n' "$library"
      failures=$((failures + 1))
    fi
  done
fi

for binary in umu-run wine winetricks vulkaninfo; do
  if command -v "$binary" >/dev/null 2>&1; then
    printf 'OK      %s: %s\n' "$binary" "$(command -v "$binary")"
  else
    printf 'OPTIONAL %s (needed for the corresponding runtime workflow)\n' "$binary"
  fi
done

if command -v rpm >/dev/null 2>&1; then
  for package in mesa-vulkan-drivers.i686 vulkan-loader.i686; do
    if rpm -q "$package" >/dev/null 2>&1; then
      printf 'OK      %s\n' "$package"
    else
      printf 'OPTIONAL %s (Fedora AMD/Intel 32-bit graphics support)\n' "$package"
    fi
  done
fi

printf '\nRequired development checks missing: %s\n' "$failures"
exit "$((failures > 0))"
