#!/bin/sh
# Guard against client filters that freeze the compositor (ADR 0039).
#
# Starts the real compositor without MALAYSIA_DEV, waits for its own socket and
# connects a plain client (`wayland-info`) with a 5 s deadline. The client must
# finish, and the private Infinity global must stay hidden from it.
set -eu

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
bin=${MALAYSIA_BIN:-$repo_root/target/debug/malaysia-compositor}
if [ ! -x "$bin" ]; then
    printf 'check-client-dispatch: missing %s; build it first\n' "$bin" >&2
    exit 1
fi

tmp=$(mktemp -d)
log="$tmp/compositor.log"
pid=
cleanup() {
    if [ -n "${pid:-}" ]; then
        kill "$pid" 2>/dev/null || true
        i=0
        while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 20 ]; do
            sleep 0.1
            i=$((i + 1))
        done
        kill -9 "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
    rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

# Never inherit a developer's bypass: this exercises the filtered path.
unset MALAYSIA_DEV MALAYSIA_SHELL_EXE
MALAYSIA_CONFIG_DIR="$tmp/config" MALAYSIA_DATA_DIR="$tmp/data" \
    "$bin" >"$log" 2>&1 &
pid=$!

socket=
i=0
while [ "$i" -lt 100 ]; do
    socket=$(sed -n 's/.*socket=\(wayland-[0-9][0-9]*\).*/\1/p' "$log" | tail -n 1)
    [ -n "$socket" ] && break
    if ! kill -0 "$pid" 2>/dev/null; then
        printf 'check-client-dispatch: the compositor exited during startup\n' >&2
        tail -n 20 "$log" >&2
        exit 1
    fi
    sleep 0.1
    i=$((i + 1))
done
if [ -z "$socket" ]; then
    printf 'check-client-dispatch: the compositor never announced a socket\n' >&2
    tail -n 20 "$log" >&2
    exit 1
fi

if ! WAYLAND_DISPLAY="$socket" timeout 5 wayland-info >"$tmp/globals.txt" 2>&1; then
    printf 'check-client-dispatch: a client did not finish within 5 s (compositor stalled)\n' >&2
    tail -n 20 "$log" >&2
    exit 1
fi

if ! grep -q "interface: 'wl_compositor'" "$tmp/globals.txt"; then
    printf 'check-client-dispatch: the client received no globals\n' >&2
    exit 1
fi

if grep -q "infinity_window_manager_v1" "$tmp/globals.txt"; then
    printf 'check-client-dispatch: the Infinity global leaked to an untrusted client\n' >&2
    exit 1
fi

printf 'check-client-dispatch: untrusted client connected; compositor kept dispatching\n'
