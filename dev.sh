#!/bin/sh
set -eu
[ "$#" -eq 1 ] && [ "$1" = demo ] || { echo "Usage: dev.sh demo" >&2; exit 2; }
cd "$(dirname "$0")"
cargo build
mkdir -p "$XDG_RUNTIME_DIR/malaysia"
log="$XDG_RUNTIME_DIR/malaysia/demo.log"
target/debug/malaysia-compositor >"$log" 2>&1 &
compositor=$!
client=
trap 'kill "$compositor" $client 2>/dev/null || true; wait "$compositor" 2>/dev/null || true' EXIT
trap exit INT TERM
socket=
while [ -z "$socket" ]; do
    kill -0 "$compositor" 2>/dev/null || { cat "$log" >&2; exit 1; }
    sleep 0.1
    socket=$(sed -n '/listening/s/.*\(wayland-[0-9]*\).*/\1/p' "$log")
done
WAYLAND_DISPLAY=$socket foot &
client=$!
echo "$socket  (more clients: WAYLAND_DISPLAY=$socket <program>)"
echo "log: $log  (close the window or press Ctrl-C to end)"
wait "$compositor"
