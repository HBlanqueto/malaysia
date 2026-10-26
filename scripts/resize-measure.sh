#!/usr/bin/env bash
# Medida de resize interactivo con MALAYSIA_TEST_INPUT.
#
# Arrastra la esquina inferior derecha de window_csd y de foot desde 640x420
# hasta 1100x700 en 60 pasos, y reporta configures enviados, commits recibidos
# y tiempo por fotograma.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

if [ "${MALAYSIA_DEV_SHELL:-}" != "1" ]; then
    echo "scripts/resize-measure.sh: not inside the dev shell; run nix develop first" >&2
    exit 7
fi

require() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }
require cargo
require foot

runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/malaysia-measure"
mkdir -p "$runtime_dir"
pidfile="$runtime_dir/malaysia-compositor.pid"
logfile="$runtime_dir/malaysia.log"

start_malaysia() {
    local input="${1:-}"
    : >"$logfile"
    setsid env \
        ${input:+MALAYSIA_TEST_INPUT="$input"} \
        "$repo_root/target/debug/malaysia-compositor" --log-level=debug >>"$logfile" 2>&1 &
    echo $! >"$pidfile"
    local waited=0
    while [ "$waited" -lt 100 ]; do
        socket=$(sed -n 's/.*listening on the Wayland socket socket=\(wayland-[0-9]*\).*/\1/p' "$logfile" 2>/dev/null | tail -n 1)
        [ -n "${socket:-}" ] && break
        sleep 0.1
        waited=$((waited + 1))
    done
    if [ -z "${socket:-}" ]; then
        echo "malaysia-compositor never opened a socket" >&2
        cat "$logfile" >&2
        exit 1
    fi
    echo "$socket"
}

stop_malaysia() {
    if [ -f "$pidfile" ]; then
        pid=$(cat "$pidfile" 2>/dev/null || true)
        if [ -n "${pid:-}" ]; then
            kill "$pid" 2>/dev/null || true
            local waited=0
            while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 50 ]; do
                sleep 0.1
                waited=$((waited + 1))
            done
        fi
    fi
    rm -f "$pidfile"
}

wait_for_window() {
    local app_id="$1"
    local waited=0
    while [ "$waited" -lt 100 ]; do
        if grep -q "window mapped.*app_id=\"$app_id\"" "$logfile" 2>/dev/null; then
            break
        fi
        sleep 0.1
        waited=$((waited + 1))
    done
    if ! grep -q "window mapped.*app_id=\"$app_id\"" "$logfile" 2>/dev/null; then
        echo "window $app_id never mapped" >&2
        cat "$logfile" >&2
        exit 1
    fi
}

window_origin() {
    local app_id="$1"
    grep "window mapped" "$logfile" | grep "app_id=\"$app_id\"" | tail -n 1 | sed -n 's/.*origin=Point { x: \(-*[0-9]*\), y: \(-*[0-9]*\) }.*/\1,\2/p'
}

measure_client() {
    local name="$1"
    local app_id="$2"
    local launch="$3"

    echo "--- $name ---"

    stop_malaysia
    socket=$(start_malaysia "")

    eval "$launch"
    client_pid=$!

    wait_for_window "$app_id"
    origin=$(window_origin "$app_id")
    ox=${origin%,*}
    oy=${origin#*,}

    # The window starts at 640x420; lower-right corner is where we grab.
    start_x=$((ox + 640 - 4))
    start_y=$((oy + 420 - 4))
    end_x=$((ox + 1100 - 4))
    end_y=$((oy + 700 - 4))
    steps=60
    dx=$(awk "BEGIN { printf \"%.2f\", ($end_x - $start_x) / $steps }")
    dy=$(awk "BEGIN { printf \"%.2f\", ($end_y - $start_y) / $steps }")

    # Build the input script: wait for the client, press, drag in steps, release.
    input="wait:2000;move:$start_x,$start_y;press:$start_x,$start_y"
    for i in $(seq 1 "$steps"); do
        x=$(awk "BEGIN { printf \"%.0f\", $start_x + $dx * $i }")
        y=$(awk "BEGIN { printf \"%.0f\", $start_y + $dy * $i }")
        input="$input;move:$x,$y"
    done
    input="$input;release;wait:500"

    kill "$client_pid" 2>/dev/null || true
    stop_malaysia

    # Second run with the scripted drag.
    : >"$logfile"
    socket=$(start_malaysia "$input")
    eval "$launch"
    client_pid=$!

    # Wait for the drag to finish plus a bit.
    sleep 1.5

    kill "$client_pid" 2>/dev/null || true
    stop_malaysia

    configures=$(grep -c 'send_configure' "$logfile" || true)
    commits=$(grep -c 'commit resized window' "$logfile" || true)
    final_requested=$(grep 'send_configure' "$logfile" | tail -n 1 | sed -n 's/.*configured=Size { w: \([0-9]*\), h: \([0-9]*\) }.*/\1x\2/p' || true)
    final_committed=$(grep 'commit resized window' "$logfile" | tail -n 1 | sed -n 's/.*outer_size=Size { w: \([0-9]*\), h: \([0-9]*\) }.*/\1x\2/p' || true)

    # Frame times from "frame presented" debug logs.
    python3 - "$logfile" "$name" "$configures" "$commits" "$final_requested" "$final_committed" <<'PY'
import sys, re
logfile, name, configures, commits = sys.argv[1:5]
final_requested, final_committed = sys.argv[5:7]
configures = int(configures)
commits = int(commits)

deltas = []
prev = None
for line in open(logfile):
    m = re.search(r'frame=(\d+).*delta=Duration \{ secs: (\d+), nanos: (\d+) \}', line)
    if m:
        secs = int(m.group(2))
        nanos = int(m.group(3))
        us = secs * 1_000_000 + nanos // 1000
        if prev is not None:
            deltas.append(us - prev)
        prev = us

if deltas:
    avg = sum(deltas) / len(deltas)
    mx = max(deltas)
else:
    avg = mx = 0

print(f"{name}:")
print(f"  configures sent:    {configures}")
print(f"  commits received:   {commits}")
print(f"  final requested:    {final_requested}")
print(f"  final committed:    {final_committed}")
print(f"  frames measured:    {len(deltas)}")
print(f"  frame time avg/max: {avg:.1f} / {mx:.1f} us")
PY
}

cargo build -p malaysia-compositor -p barrier-kit --examples || exit 1

cd "$repo_root"
measure_client "window_csd" "org.malaysia.demo.csd" \
    'env -u DISPLAY WAYLAND_DISPLAY="$socket" XDG_SESSION_TYPE=wayland "$repo_root/target/debug/examples/window_csd" &'
measure_client "foot" "foot" \
    'env -u DISPLAY WAYLAND_DISPLAY="$socket" XDG_SESSION_TYPE=wayland foot -T foot &'
