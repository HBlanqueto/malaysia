#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

absent() {
  local package=$1
  shift
  if cargo tree "$@" -i "$package" 2>/dev/null | grep -v '^warning:' | grep -q .; then
    echo "check-client-only-deps: $package must not be in the graph ($*)" >&2
    fail=1
  fi
}

absent freya-engine -p malaysia-compositor
absent freya-skia-bindings -p malaysia-compositor
absent freya-skia-safe -p malaysia-compositor
absent skia-bindings -p malaysia-compositor
absent wgpu -p malaysia-compositor

absent freya-core -p malaysia-wallpaper
absent freya-engine -p malaysia-wallpaper
absent freya-skia-bindings -p malaysia-wallpaper
absent freya-skia-safe -p malaysia-wallpaper
absent skia-bindings -p malaysia-wallpaper

absent winit -p malaysia-shell
absent tokio --workspace
absent gtk --workspace
absent glutin --workspace
absent tray-icon --workspace

exit "$fail"
