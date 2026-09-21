#!/bin/sh
# dev.sh - build, run and inspect Neptune during development.
#
# Wraps `cargo` for the common actions and manages a background instance of
# the compositor through a PID file and a log file. It adds nothing else.
#
# POSIX shell only: no bashisms, and no tools beyond coreutils and cargo.
# The linker setup lives in the devShell (RUSTFLAGS), not here.

set -eu

# Exit codes, grouped so callers can branch on the failure class.
EXIT_USAGE=2
EXIT_ALREADY_RUNNING=3
EXIT_MISSING_TOOL=4
EXIT_RUN_FAILED=5
EXIT_NO_LOG=6
EXIT_NOT_DEVSHELL=7
EXIT_WATCH=8
EXIT_SMOKE=9
# status follows the LSB init-script conventions: 0 running, 1 dead with a
# leftover pid file, 3 not running. Interrupting `watch` with Ctrl-C exits 130
# (128 + SIGINT).
EXIT_STATUS_DEAD=1
EXIT_STATUS_NOT_RUNNING=3

usage() {
    cat <<'EOF'
Usage: dev.sh <command> [options]

Commands:
    build [--release]   Compile the workspace (debug, or release with --release)
    run [--release]     Start the compositor in the background and write a PID file
    watch [--release]   Watch sources; rebuild and relaunch on change (Ctrl-C stops)
    stop                Stop the background instance and remove the PID file
    restart             Stop, then run (no rebuild)
    status              Report whether the background instance is running
    log [-f]            Print the log, or follow it with -f
    clean               Remove build artifacts and runtime files
    check               Run formatting, clippy and tests
    smoke [--release]   Build and run the toplevel mapping smoke test
EOF
}

# fail CODE MESSAGE... - report to stderr and exit.
fail() {
    _code=$1
    shift
    printf 'dev.sh: %s\n' "$*" >&2
    exit "$_code"
}

# require TOOL - fail if a tool is not on PATH.
require() {
    command -v "$1" >/dev/null 2>&1 ||
        fail "$EXIT_MISSING_TOOL" "missing required tool: $1"
}

# require_devshell - compiling outside `nix develop` would omit the mold
# linker flag and produce a differently linked binary, so it is forbidden. The
# dev shell sets the marker (ADR 0007).
require_devshell() {
    [ "${NEPTUNE_DEV_SHELL:-}" = "1" ] ||
        fail "$EXIT_NOT_DEVSHELL" "not inside the dev shell; run 'nix develop' first (see ADR 0007)"
}

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)

# Runtime files live under XDG_RUNTIME_DIR, or a UID-scoped directory in /tmp
# when it is not set. The directory is "neptune-dev", not "neptune": a running
# compositor may create $XDG_RUNTIME_DIR/neptune as its Wayland socket, and a
# socket and a directory cannot share a path.
runtime_dir() {
    if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -d "$XDG_RUNTIME_DIR" ]; then
        printf '%s\n' "$XDG_RUNTIME_DIR/neptune-dev"
    else
        printf '%s\n' "/tmp/neptune-$(id -u)"
    fi
}

RUNTIME_DIR=$(runtime_dir)
PID_FILE="$RUNTIME_DIR/neptune.pid"
LOG_FILE="$RUNTIME_DIR/neptune.log"

ensure_runtime_dir() {
    mkdir -p "$RUNTIME_DIR"
}

# read_pid - print the saved PID, or fail if there is none or it is malformed.
read_pid() {
    if [ ! -f "$PID_FILE" ]; then
        return 1
    fi
    _saved=$(cat "$PID_FILE" 2>/dev/null) || return 1
    case "$_saved" in
        '' | *[!0-9]*) return 1 ;;
    esac
    printf '%s\n' "$_saved"
}

# pid_is_neptune PID - succeed only if the PID is alive and its comm is
# "neptune". Linux /proc is assumed: this project is single-machine.
pid_is_neptune() {
    kill -0 "$1" 2>/dev/null || return 1
    [ "$(cat "/proc/$1/comm" 2>/dev/null)" = "neptune" ]
}

# is_running - succeed only if the saved PID is a live Neptune process.
is_running() {
    _saved=$(read_pid) || return 1
    pid_is_neptune "$_saved"
}

build() {
    require_devshell
    require cargo
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for build: $1"
    fi
    if [ -n "$_release" ]; then
        cargo build --release
    else
        cargo build
    fi
}

start() {
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for run: $1"
    fi
    if is_running; then
        _saved=$(read_pid)
        fail "$EXIT_ALREADY_RUNNING" "neptune is already running (pid $_saved)"
    fi
    _bin="$script_dir/target/debug/neptune"
    if [ -n "$_release" ]; then
        _bin="$script_dir/target/release/neptune"
    fi
    if [ ! -x "$_bin" ]; then
        fail "$EXIT_RUN_FAILED" "binary not built; run './dev.sh build' first"
    fi
    # A stale pid file is not an error.
    rm -f "$PID_FILE"
    ensure_runtime_dir
    require setsid
    # setsid puts the compositor in its own session, so a watcher that kills its
    # own children (cargo-watch does) or a closing terminal does not take the
    # running instance down with it. It execs without forking here, so $! is the
    # compositor's real PID.
    #
    # Give the compositor its own XDG_RUNTIME_DIR, so its Wayland socket cannot
    # be confused with the host session's. The nested winit client still has to
    # reach the host compositor, so when the host is Wayland its socket is
    # passed as an absolute path, which libwayland accepts independently of
    # XDG_RUNTIME_DIR. When the host is X11, DISPLAY is left for winit to use.
    _host_xdg=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
    if [ -n "${WAYLAND_DISPLAY:-}" ] && [ "${WAYLAND_DISPLAY#/}" = "${WAYLAND_DISPLAY}" ]; then
        XDG_RUNTIME_DIR="$RUNTIME_DIR" \
            WAYLAND_DISPLAY="$_host_xdg/$WAYLAND_DISPLAY" \
            setsid "$_bin" >>"$LOG_FILE" 2>&1 &
    else
        XDG_RUNTIME_DIR="$RUNTIME_DIR" setsid "$_bin" >>"$LOG_FILE" 2>&1 &
    fi
    _saved=$!
    printf '%s\n' "$_saved" >"$PID_FILE"
    sleep 0.2
    if ! kill -0 "$_saved" 2>/dev/null; then
        rm -f "$PID_FILE"
        printf 'dev.sh: neptune exited immediately; log follows:\n' >&2
        if [ -f "$LOG_FILE" ]; then
            tail -n 20 "$LOG_FILE" >&2
        fi
        exit "$EXIT_RUN_FAILED"
    fi
}

# watch - rebuild and relaunch on every source change, in the foreground.
#
# Each change runs a build and then restarts the background instance. A failed
# build aborts the cycle before the restart, so the instance on screen is left
# alone and the compiler output stays on the terminal. Ctrl-C exits the watcher
# and the EXIT trap stops the compositor, so nothing outlives the watcher.
watch() {
    require_devshell
    require cargo
    require cargo-watch
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for watch: $1"
    fi

    # Watch the source trees. cargo-watch requires each path to exist, and the
    # workspace has no top-level src/ yet.
    set -- -w "$script_dir/crates" --ignore "$script_dir/target"
    if [ -d "$script_dir/src" ]; then
        set -- "$@" -w "$script_dir/src"
    fi

    # `&&` is deliberate: a failed build must not reach the restart.
    _cycle="$script_dir/dev.sh build"
    if [ -n "$_release" ]; then
        _cycle="$_cycle --release"
    fi
    _cycle="$_cycle && $script_dir/dev.sh restart"
    if [ -n "$_release" ]; then
        _cycle="$_cycle --release"
    fi

    # A signal only raises a flag. `stop` is left to the EXIT trap so it runs
    # to completion; exiting from inside a signal trap can cut it short before
    # the compositor is signalled.
    trap 'stop' EXIT
    _interrupted=0
    trap '_interrupted=1' INT TERM

    _status=0
    cargo watch "$@" -s "$_cycle" || _status=$?
    if [ "$_interrupted" -ne 0 ]; then
        exit 130
    fi
    if [ "$_status" -ne 0 ]; then
        fail "$EXIT_WATCH" "cargo-watch exited unexpectedly (status $_status)"
    fi
}

# stop - terminate the saved process by PID. Signals the process only, never
# the process group, so an invoking shell is never caught in the blast radius.
stop() {
    _saved=$(read_pid) || return 0
    if ! pid_is_neptune "$_saved"; then
        rm -f "$PID_FILE"
        return 0
    fi
    kill -TERM "$_saved" 2>/dev/null || true
    _waited=0
    while kill -0 "$_saved" 2>/dev/null; do
        _waited=$((_waited + 1))
        if [ "$_waited" -ge 50 ]; then
            kill -KILL "$_saved" 2>/dev/null || true
            break
        fi
        sleep 0.1
    done
    rm -f "$PID_FILE"
}

status() {
    if is_running; then
        _saved=$(read_pid)
        printf 'neptune is running (pid %s)\n' "$_saved"
        return 0
    fi
    if [ -f "$PID_FILE" ]; then
        rm -f "$PID_FILE"
        printf 'neptune is not running (removed stale pid file)\n'
        return "$EXIT_STATUS_DEAD"
    fi
    printf 'neptune is not running\n'
    return "$EXIT_STATUS_NOT_RUNNING"
}

show_log() {
    _follow=
    if [ "${1:-}" = "-f" ]; then
        _follow=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for log: $1"
    fi
    if [ ! -f "$LOG_FILE" ]; then
        fail "$EXIT_NO_LOG" "no log file at $LOG_FILE"
    fi
    if [ -n "$_follow" ]; then
        tail -f "$LOG_FILE"
    else
        cat "$LOG_FILE"
    fi
}

clean() {
    require_devshell
    require cargo
    stop
    cargo clean
    rm -rf "$RUNTIME_DIR"
}

check() {
    require_devshell
    require cargo
    require cargo-fmt
    require cargo-clippy
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
}

# smoke - build the compositor and the probe harness, then run the mapping
# smoke test.
#
# The harness launches its own compositor and a real client, so it must run in
# the graphical session the compositor runs in; it fails loudly when there is no
# host display. The harness's exit code is the verdict.
smoke() {
    require_devshell
    require cargo
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for smoke: $1"
    fi
    if [ -n "$_release" ]; then
        _profile=release
        cargo build --release -p neptune -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the smoke test"
    else
        _profile=debug
        cargo build -p neptune -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the smoke test"
    fi
    # NEPTUNE_BIN pins the binary just built, so the harness does not fall back
    # to a different profile's compositor.
    NEPTUNE_BIN="$script_dir/target/$_profile/neptune" \
        "$script_dir/target/$_profile/smoke" ||
        fail "$EXIT_SMOKE" "smoke test failed"
}

if [ "$#" -eq 0 ]; then
    usage >&2
    exit "$EXIT_USAGE"
fi

command_name=$1
shift

case "$command_name" in
    build) build "$@" ;;
    run) start "$@" ;;
    watch) watch "$@" ;;
    stop) stop ;;
    restart)
        stop
        start "$@"
        ;;
    status) status ;;
    log) show_log "$@" ;;
    clean) clean ;;
    check) check ;;
    smoke) smoke "$@" ;;
    -h | --help) usage ;;
    *)
        usage >&2
        fail "$EXIT_USAGE" "unknown command: $command_name"
        ;;
esac
