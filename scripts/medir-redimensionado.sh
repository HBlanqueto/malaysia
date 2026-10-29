#!/usr/bin/env bash
# Medición de redimensionado interactivo L1f-2.
#
# Uso (dentro de `nix develop` en ~/src/malaysia):
#   scripts/medir-redimensionado.sh [debug|release]
#
# Arrastra la esquina inferior derecha de window_csd y de foot desde su tamaño
# inicial hasta 1100×700, y reporta configures/s, latencia configure→ack y
# configure→commit, y tiempos de draw/swap del cliente.
set -euo pipefail

profile="${1:-debug}"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
barrierkit="${BARRIERKIT_ROOT:-$HOME/src/barrierkit}"

if [ "${MALAYSIA_DEV_SHELL:-}" != "1" ]; then
    echo "run inside nix develop" >&2
    exit 7
fi

mkdir -p /tmp/l1f-measure
pidfile="/tmp/l1f-measure/malaysia.pid"
logbase="/tmp/l1f-measure/malaysia"
logfile="$logbase.log"

build() {
    cargo build -p malaysia-compositor --profile "$profile"
    (cd "$barrierkit" && cargo build -p barrier-kit --examples --profile "$profile")
}

compositor_bin() {
    if [ "$profile" = "release" ]; then
        echo "$repo_root/target/release/malaysia-compositor"
    else
        echo "$repo_root/target/debug/malaysia-compositor"
    fi
}

example_bin() {
    if [ "$profile" = "release" ]; then
        echo "$barrierkit/target/release/examples/window_csd"
    else
        echo "$barrierkit/target/debug/examples/window_csd"
    fi
}

start_malaysia() {
    local input="${1:-}"
    : >"$logfile"
    setsid env \
        MALAYSIA_TEST_POINTER_STEP_MS=8 \
        MALAYSIA_RESIZE_CONFIGURE_TIMEOUT_MS=8 \
        ${input:+MALAYSIA_TEST_INPUT="$input"} \
        "$(compositor_bin)" --log-level=debug >>"$logfile" 2>&1 &
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
    grep "window mapped" "$logfile" | grep "app_id=\"$app_id\"" | tail -n 1 | sed -n 's/.*origin=Point<[^>]*> { x: \(-*[0-9]*\), y: \(-*[0-9]*\) }.*/\1,\2/p'
}

measure_client() {
    local name="$1"
    local app_id="$2"
    local launch="$3"
    local client_log="/tmp/l1f-measure/client-$name-$profile.log"
    logfile="$logbase-$name-$profile.log"

    echo "--- $name ($profile) ---"

    stop_malaysia
    socket=$(start_malaysia "")

    eval "$launch" >"$client_log" 2>&1 &
    client_pid=$!

    wait_for_window "$app_id"
    origin=$(window_origin "$app_id")
    ox=${origin%,*}
    oy=${origin#*,}

    size=$(grep "window mapped" "$logfile" | grep "app_id=\"$app_id\"" | tail -n 1 | sed -n 's/.*size=Size<[^>]*> { w: \([0-9]*\), h: \([0-9]*\) }.*/\1,\2/p')
    if [ -z "$size" ]; then
        size=$(grep 'send_configure' "$logfile" | grep -v 'configured=Size<[^>]*> { w: 0, h: 0 }' | head -n 1 | sed -n 's/.*configured=Size<[^>]*> { w: \([0-9]*\), h: \([0-9]*\) }.*/\1,\2/p')
    fi
    if [ -z "$size" ]; then
        echo "could not read mapped size for $app_id; using 640,420" >&2
        size="640,420"
    fi
    sw=${size%,*}
    sh=${size#*,}
    sw=${sw%.*}
    sh=${sh%.*}

    start_x=$((ox + sw - 1))
    start_y=$((oy + sh - 1))
    end_x=$((ox + 1100 - 1))
    end_y=$((oy + 700 - 1))
    steps=60
    dx=$(awk "BEGIN { printf \"%.2f\", ($end_x - $start_x) / $steps }")
    dy=$(awk "BEGIN { printf \"%.2f\", ($end_y - $start_y) / $steps }")

    # El ritmo del puntero sintético lo controla MALAYSIA_TEST_POINTER_STEP_MS
    # (por defecto 100 ms); lo fijamos a 8 ms para medir a ~120 Hz.
    input="wait:3000;move:$start_x,$start_y;press:$start_x,$start_y"
    for i in $(seq 1 "$steps"); do
        x=$(awk "BEGIN { printf \"%.0f\", $start_x + $dx * $i }")
        y=$(awk "BEGIN { printf \"%.0f\", $start_y + $dy * $i }")
        input="$input;move:$x,$y"
    done
    input="$input;release;wait:500"

    kill "$client_pid" 2>/dev/null || true
    stop_malaysia

    : >"$logfile"
    : >"$client_log"
    socket=$(start_malaysia "$input")
    eval "$launch" >"$client_log" 2>&1 &
    client_pid=$!

    # 60 movimientos × 8 ms + pasos iniciales/finales ≈ 4 s; dormimos 7 s.
    sleep 7

    kill "$client_pid" 2>/dev/null || true
    stop_malaysia

    python3 - "$logfile" "$client_log" "$name" <<'PY'
import sys, re
comp_log, client_log, name = sys.argv[1:4]

ts_re = re.compile(r'^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+)Z')

def parse_ts(line):
    m = ts_re.match(line)
    if not m:
        return None
    s = m.group(1)
    h, m, sec = s.split('T')[1].split(':')
    return int(h)*3600 + int(m)*60 + float(sec)

def events(path, pattern):
    out = []
    for line in open(path, errors='replace'):
        plain = re.sub(r'\x1b\[[0-9;]*m', '', line)
        if pattern in plain:
            t = parse_ts(plain)
            if t is not None:
                out.append((t, line))
    return out

send = events(comp_log, 'send_configure')
ack = events(comp_log, 'ack_configure')
commit = events(comp_log, 'commit resized window')

# Pair send -> ack -> first commit after that ack by serial, but only if no
# newer send arrives before the commit. Otherwise the configure was replaced.
ack_by_serial = {}
for t, line in ack:
    m = re.search(r'serial=(\d+)', line)
    if m:
        ack_by_serial[int(m.group(1))] = t

send_serials = []
for t, line in send:
    m = re.search(r'serial=(\d+)', line)
    if m:
        send_serials.append((t, int(m.group(1))))

commit_times = [t for t, _ in commit]

ack_deltas = []
commit_deltas = []
replaced = 0
no_commit = 0

for i, (t_send, serial) in enumerate(send_serials):
    t_ack = ack_by_serial.get(serial)
    if t_ack is None:
        continue
    ack_deltas.append((t_ack - t_send) * 1000)

    first_commit = None
    for t_commit in commit_times:
        if t_commit > t_ack:
            first_commit = t_commit
            break
    if first_commit is None:
        no_commit += 1
        continue
    if i + 1 < len(send_serials) and send_serials[i + 1][0] < first_commit:
        replaced += 1
        continue
    commit_deltas.append((first_commit - t_send) * 1000)

# client draw/swap times
draw_start = []
swap_done = []
swap_us = []
for line in open(client_log, errors='replace'):
    plain = re.sub(r'\x1b\[[0-9;]*m', '', line)
    if 'draw start' in plain:
        ts = parse_ts(plain)
        if ts is not None:
            draw_start.append(ts)
    elif 'swap done' in plain:
        ts = parse_ts(plain)
        if ts is not None:
            swap_done.append(ts)
        m = re.search(r'swap_us=(\d+)', plain)
        if m:
            swap_us.append(int(m.group(1)))

draw_deltas = []
for s, e in zip(draw_start, swap_done):
    if e >= s:
        draw_deltas.append((e - s) * 1000)

if send:
    duration = send[-1][0] - send[0][0]
else:
    duration = 0

def stats(vals):
    if not vals:
        return 0, 0.0, 0.0, 0.0, 0.0
    vals = sorted(vals)
    n = len(vals)
    mean = sum(vals) / n
    median = vals[n // 2] if n % 2 else (vals[n // 2 - 1] + vals[n // 2]) / 2
    p95 = vals[int(n * 0.95)] if n > 1 else vals[0]
    mx = vals[-1]
    return n, mean, median, p95, mx

ack_n, ack_mean, ack_med, ack_p95, ack_max = stats(ack_deltas)
commit_n, commit_mean, commit_med, commit_p95, commit_max = stats(commit_deltas)
draw_n, draw_mean, draw_med, draw_p95, draw_max = stats(draw_deltas)
swap_n, swap_mean, swap_med, swap_p95, swap_max = stats(swap_us)

print(f"{name}:")
print(f"  configures sent:        {len(send)}")
print(f"  duration (s):           {duration:.3f}")
if duration > 0:
    print(f"  configures/sec:         {len(send)/duration:.1f}")
else:
    print("  configures/sec:         -")
print(f"  ack latency  n/mean/med/p95/max: {ack_n} / {ack_mean:.1f} / {ack_med:.1f} / {ack_p95:.1f} / {ack_max:.1f} ms")
print(f"  replaced / no commit:   {replaced} / {no_commit}")
print(f"  commit latency n/mean/med/p95/max: {commit_n} / {commit_mean:.1f} / {commit_med:.1f} / {commit_p95:.1f} / {commit_max:.1f} ms")
print(f"  client draw n/mean/med/p95/max: {draw_n} / {draw_mean:.1f} / {draw_med:.1f} / {draw_p95:.1f} / {draw_max:.1f} ms")
print(f"  client swap_us n/mean/med/p95/max: {swap_n} / {swap_mean:.1f} / {swap_med:.1f} / {swap_p95:.1f} / {swap_max:.1f} us")
PY
}

build

measure_client "window_csd" "io.github.hblanqueto.malaysia.demo.csd" \
    'env -u DISPLAY WAYLAND_DISPLAY="$socket" XDG_SESSION_TYPE=wayland RUST_LOG=barrier_kit::resize=debug "$(example_bin)" &'

measure_client "foot" "foot" \
    'env -u DISPLAY WAYLAND_DISPLAY="$socket" XDG_SESSION_TYPE=wayland foot -T foot &'
