#!/usr/bin/env bash
# Development harness for the shell (ADR 0023).
#
# The shell components run as separate processes, supervised by systemd user
# units in a real session (see `share/systemd/user/` and the flake). In
# development this script launches a component by hand against the running
# compositor — `./dev.sh run` first, then `scripts/test.sh wallpaper`.
#
# Usage:
#   scripts/test.sh [component]   # default: wallpaper
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