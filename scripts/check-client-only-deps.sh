#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

graph() {
  cargo tree "$@" --prefix none --format '{p}' -e normal,build ||
    { echo "check-client-only-deps: cargo tree $* failed" >&2; exit 1; }
}

absent() {
  local target=$1
  shift
  local tree
  tree=$(graph $target)
  for package in "$@"; do
    if grep -q "^$package v" <<<"$tree"; then
      echo "check-client-only-deps: $package must not be in the graph ($target)" >&2
      fail=1
    fi
  done
}

absent "-p malaysia-compositor" freya-core freya-components freya-animation freya-engine \
  freya-skia-bindings freya-skia-safe skia-bindings torin ragnarok wgpu
absent "-p malaysia-wallpaper" freya-core freya-engine freya-skia-bindings freya-skia-safe \
  skia-bindings
absent "-p malaysia-shell" winit wgpu
absent "--workspace" freya freya-winit tokio gtk glutin tray-icon otto-kit \
  otto-foundations otto-dbus otto-surface-style laye-rs

exit "$fail"
