#!/usr/bin/env bash
# run.sh <tag> <with-global:0|1> <command...>   (inside the sim nix shell)
here=$(cd "$(dirname "$0")" && pwd); work=${SIM_DIR:-/tmp/malaysia-sim}
tag=$1; global=$2; shift 2
out=$work/out/$tag; rm -rf "$out"; mkdir -p "$out"
export XDG_RUNTIME_DIR=$work/run-$tag; rm -rf "$XDG_RUNTIME_DIR"; mkdir -p -m 700 "$XDG_RUNTIME_DIR"
mods=""; [ "$global" = 1 ] && mods="--modules=$work/focus-sim.so"
FOCUS_SIM_BARS="${FOCUS_SIM_BARS-4000:1,6000:0,8000:1,10000:0,14000:1}" \
  weston --backend=headless --renderer=pixman --shell=desktop --socket=sim-$tag \
  --width=1280 --height=800 --idle-time=0 $mods >"$out/weston.log" 2>&1 &
wpid=$!
for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/sim-$tag" ] && break; sleep 0.2; done
env -u DISPLAY WAYLAND_DISPLAY=sim-$tag GDK_BACKEND=wayland GSK_RENDERER=cairo \
  LD_LIBRARY_PATH="$PATCHED/lib" ADW_DISABLE_PORTAL=1 NO_AT_BRIDGE=1 GTK_A11Y=none "$@" >"$out/app.log" 2>&1 &
apid=$!
sleep "${RUN_SECONDS:-17}"
grep -o '/nix/store/[^ ]*libadwaita[^/]*' /proc/$apid/maps 2>/dev/null | sort -u >"$out/libs.txt"
kill $apid 2>/dev/null; sleep 0.5; kill $wpid 2>/dev/null; wait 2>/dev/null
grep "focus-sim" "$out/weston.log" > "$out/sim.txt"
