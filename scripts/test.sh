#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

if ! "$repo_root/dev.sh" status >/dev/null 2>&1; then
  echo "malaysia-compositor is not running; start it first: $repo_root/dev.sh run" >&2
  exit 1
fi

component="${1:-wallpaper}"
runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
display="${MALAYSIA_SHELL_DISPLAY:-wayland-1}"
bin="${MALAYSIA_SHELL_BIN:-$repo_root/target/debug/malaysia-compositor-shell}"

export XDG_RUNTIME_DIR="$runtime_dir"
export WAYLAND_DISPLAY="$display"
exec "$bin" "$component"
