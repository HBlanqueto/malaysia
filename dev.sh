#!/bin/sh
# dev.sh - build, run and inspect Yuki during development.
#
# Wraps `cargo` for the common actions and manages a background instance of
# the compositor through a PID file and a log file. It adds nothing else.
#
# POSIX shell only: no bashisms, and no tools beyond coreutils and cargo.
# The devShell provides mold; the fallback below makes it explicit for every
# cargo invocation made by this script.

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
EXIT_DRM_VKMS=10
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
    demo [--dark|--light]
                        Nested session with the shell and demo windows; the flag
                        forces that appearance in a temporary config (Ctrl-C ends it)
    lab [--dark|--light]
                        Nested session with shell + demo windows + optional real
                        apps (nautilus, zenity, kate, telegram, firefox) for manual
                        testing; uses a temporary config (Ctrl-C ends it)
    example <name>      Run target/debug/examples/<name> against the running demo
                        (--sin-menubar closes the demo menubar first)
    watch [--release]   Watch sources; rebuild and relaunch on change (Ctrl-C stops)
    stop                Stop the background instance and remove the PID file
    restart             Stop, then run (no rebuild)
    status              Report whether the background instance is running
    log [-f]            Print the log, or follow it with -f
    clean               Remove build artifacts and runtime files
    check               Run formatting, clippy and tests
    smoke [--release]   Build and run the toplevel mapping smoke test
    block4 [--release]  Build and run the 50-window open/close acceptance test
    move [--release]    Build and run the client-initiated move/resize probe
    pointers [--release] Build and run the pointer lock and relative motion probes
    screencopy [--release] Build and run the output capture probe
    screencopy-window   Build and run the window capture probe
    presentation        Build and run the wp_presentation feedback probe
    activation          Build and run the xdg-activation probe
    titlebar            Press close, minimize and maximize on a real window
    decoration          Build and run the decoration-policy probe over real apps
    drm-vkms            Test --drm on vkms without touching the real panel or input
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
    [ "${YUKI_DEV_SHELL:-}" = "1" ] ||
        fail "$EXIT_NOT_DEVSHELL" "not inside the dev shell; run 'nix develop' first (see ADR 0007)"
}

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)

# The dev shell already exports RUSTFLAGS with mold and the runtime rpath. Keep
# mold explicit for the script's own cargo calls too, without duplicating the
# flag when the shell already set it.
case "${RUSTFLAGS:-}" in
    *-fuse-ld=mold*) ;;
    *) RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"; export RUSTFLAGS ;;
esac

# Runtime files (the pid and the log) live under XDG_RUNTIME_DIR, or a
# UID-scoped directory in /tmp when it is not set. They are kept in a
# dev-specific directory so a pid or log never shares a path with anything the
# host session owns. The compositor's Wayland socket is not here: it is
# auto-named `wayland-N` in the compositor's own XDG_RUNTIME_DIR, which `start`
# passes through from the caller.
runtime_dir() {
    if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -d "$XDG_RUNTIME_DIR" ]; then
        printf '%s\n' "$XDG_RUNTIME_DIR/yuki-dev"
    else
        printf '%s\n' "/tmp/yuki-$(id -u)"
    fi
}

RUNTIME_DIR=$(runtime_dir)
PID_FILE="$RUNTIME_DIR/yuki.pid"
LOG_FILE="$RUNTIME_DIR/yuki.log"

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

# pid_is_yuki PID - succeed only if the PID is alive and its comm is
# "yuki" or the wrapper's ".yuki-wrapped". Linux /proc is assumed: this
# project is single-machine.
pid_is_yuki() {
    kill -0 "$1" 2>/dev/null || return 1
    case "$(cat "/proc/$1/comm" 2>/dev/null)" in
        # The Nix package's makeWrapper renames the real binary to
        # `.yuki-wrapped`; the wrapper execs it, so that is the comm.
        yuki | .yuki-wrapped) return 0 ;;
        *) return 1 ;;
    esac
}

# is_running - succeed only if the saved PID is a live Yuki process.
is_running() {
    _saved=$(read_pid) || return 1
    pid_is_yuki "$_saved"
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
        fail "$EXIT_ALREADY_RUNNING" "yuki is already running (pid $_saved)"
    fi
    if [ -n "${YUKI_BIN:-}" ]; then
        _bin=$YUKI_BIN
    elif [ -z "${YUKI_DEV_SHELL:-}" ] && [ -x "$script_dir/result/bin/yuki" ]; then
        # Outside the dev shell the raw target binary has the RUNPATH but not
        # the EGL vendor/driver environment. Prefer the packaged wrapper, which
        # sets all three variables. Inside the shell the target binary is the
        # dev build and already runs with the shell's environment.
        _bin="$script_dir/result/bin/yuki"
    elif [ -n "$_release" ]; then
        _bin="$script_dir/target/release/yuki"
    else
        _bin="$script_dir/target/debug/yuki"
    fi
    if [ ! -x "$_bin" ]; then
        fail "$EXIT_RUN_FAILED" "binary not built; run './dev.sh build' first"
    fi
    # A stale pid file is not an error.
    rm -f "$PID_FILE"
    ensure_runtime_dir
    require setsid
    # Each run writes a fresh log. Otherwise a failed start prints the tail of a
    # file that still holds earlier successful runs, and the actual failure is
    # buried among stale lines. `log` shows the current run for the same reason.
    : >"$LOG_FILE"
    # setsid puts the compositor in its own session, so a watcher that kills its
    # own children (cargo-watch does) or a closing terminal does not take the
    # running instance down with it. It execs without forking here, so $! is the
    # compositor's real PID.
    #
    # The compositor is itself a client of the host session, so winit must
    # resolve the host display exactly as the calling shell does: the caller's
    # environment is passed through unchanged, including XDG_RUNTIME_DIR and
    # WAYLAND_DISPLAY. Rewriting either here broke startup, because a relative
    # WAYLAND_DISPLAY then resolved against this script's runtime directory
    # instead of the host's. `WAYLAND_DISPLAY` is only ever assigned for a
    # client the compositor spawns, and nothing here spawns one. The compositor
    # picks its own server socket with `ListeningSocketSource`, which binds the
    # first free `wayland-1` (never the host's `wayland-0`).
    setsid "$_bin" >>"$LOG_FILE" 2>&1 &
    _saved=$!
    printf '%s\n' "$_saved" >"$PID_FILE"
    sleep 0.2
    if ! kill -0 "$_saved" 2>/dev/null; then
        rm -f "$PID_FILE"
        printf 'dev.sh: yuki exited immediately; log follows:\n' >&2
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
    if ! pid_is_yuki "$_saved"; then
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
        printf 'yuki is running (pid %s)\n' "$_saved"
        return 0
    fi
    if [ -f "$PID_FILE" ]; then
        rm -f "$PID_FILE"
        printf 'yuki is not running (removed stale pid file)\n'
        return "$EXIT_STATUS_DEAD"
    fi
    printf 'yuki is not running\n'
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

# check_display_env - fail unless `run` hands the compositor the caller's
# display environment unchanged.
#
# A stub binary records what it inherited and sleeps, so this needs no display
# and no real compositor. `YUKI_BIN` points `run` at the stub. The values are
# deliberately synthetic: if `run` rewrites either, the recorded pair differs
# and the check fails.
check_display_env() {
    _sentinel=$(mktemp -d)
    _probe="$_sentinel/probe"
    _record="$_sentinel/record"
    cat >"$_probe" <<'PROBE'
#!/bin/sh
printf '%s\n%s\n' "${WAYLAND_DISPLAY-}" "${XDG_RUNTIME_DIR-}" >"$YUKI_ENV_RECORD"
sleep 5
PROBE
    chmod +x "$_probe"
    _want_display="yuki-check-wayland"
    _want_runtime="$_sentinel"
    if ! YUKI_BIN="$_probe" YUKI_ENV_RECORD="$_record" \
        WAYLAND_DISPLAY="$_want_display" XDG_RUNTIME_DIR="$_want_runtime" \
        "$script_dir/dev.sh" run >/dev/null 2>&1
    then
        rm -rf "$_sentinel"
        fail "$EXIT_RUN_FAILED" "display-environment check: 'run' could not start its stub"
    fi
    XDG_RUNTIME_DIR="$_want_runtime" "$script_dir/dev.sh" stop >/dev/null 2>&1 || true
    _got_display=
    _got_runtime=
    if [ -f "$_record" ]; then
        {
            IFS= read -r _got_display || _got_display=
            IFS= read -r _got_runtime || _got_runtime=
        } <"$_record"
    fi
    rm -rf "$_sentinel"
    if [ "$_got_display" != "$_want_display" ] || [ "$_got_runtime" != "$_want_runtime" ]; then
        fail "$EXIT_RUN_FAILED" \
            "display-environment check: 'run' changed it (WAYLAND_DISPLAY='${_got_display}', XDG_RUNTIME_DIR='${_got_runtime}')"
    fi
}

check() {
    require_devshell
    require cargo
    require cargo-fmt
    require cargo-clippy
    check_display_env
    cargo fmt --all -- --check
    "$script_dir/scripts/check-design-tokens.sh"
    "$script_dir/scripts/check-neptunia-generated.sh"
    "$script_dir/scripts/check-client-only-deps.sh"
    if command -v slint-compiler >/dev/null 2>&1; then
        _slint_out=$(mktemp)
        slint-compiler "$script_dir/crates/yuki-design/neptunia/demo.slint" -o "$_slint_out"
        rm -f "$_slint_out"
    else
        printf 'dev.sh: slint-compiler not found; skipping demo.slint\n' >&2
    fi
    cargo clippy --workspace --all-targets -- -D warnings
    cargo build -p yuki
    "$script_dir/scripts/check-client-dispatch.sh"
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
        cargo build --release -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the smoke test"
    else
        _profile=debug
        cargo build -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the smoke test"
    fi
    # YUKI_BIN pins the binary just built, so the harness does not fall back
    # to a different profile's compositor.
    YUKI_BIN="$script_dir/target/$_profile/yuki" \
        "$script_dir/target/$_profile/smoke" ||
        fail "$EXIT_SMOKE" "smoke test failed"
}

# block4 - open and close fifty windows and assert the scene returns to empty.
#
# Like smoke, the harness launches its own compositor and the stress client, so
# it must run in the graphical session. The harness's exit code is the verdict.
block4() {
    require_devshell
    require cargo
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for block4: $1"
    fi
    if [ -n "$_release" ]; then
        _profile=release
        cargo build --release -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    else
        _profile=debug
        cargo build -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    fi
    YUKI_BIN="$script_dir/target/$_profile/yuki" \
        YUKI_STRESS_BIN="$script_dir/target/$_profile/window_stress" \
        YUKI_RESIZE_BIN="$script_dir/target/$_profile/resize_client" \
        "$script_dir/target/$_profile/block4" ||
        fail "$EXIT_SMOKE" "block4 test failed"
}

# titlebar - press and release each titlebar button with the scripted input and
# check the compositor log. A real click cannot be synthesized from a probe, so
# this uses the same test hook that moves the pointer to an exact button centre.
titlebar() {
    require_devshell
    require cargo
    require foot
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for titlebar: $1"
    fi
    cargo build -p yuki || fail "$EXIT_SMOKE" "could not build the compositor"
    for scenario in close minimize maximize; do
        case "$scenario" in
            close) expected="window removed" ;;
            minimize) expected="window minimized" ;;
            maximize) expected="window maximized" ;;
        esac
        stop >/dev/null 2>&1 || true
        YUKI_TEST_INPUT=1 YUKI_TEST_TITLEBAR_BUTTON="$scenario" start
        _socket=
        _waited=0
        while [ -z "$_socket" ] && [ "$_waited" -lt 100 ]; do
            _socket=$(sed -n \
                's/.*listening on the Wayland socket socket=\(wayland-[0-9]*\).*/\1/p' \
                "$LOG_FILE" 2>/dev/null | tail -n 1)
            [ -n "$_socket" ] && break
            sleep 0.1
            _waited=$((_waited + 1))
        done
        if [ -z "$_socket" ]; then
            stop >/dev/null 2>&1 || true
            fail "$EXIT_SMOKE" "titlebar: the compositor never opened a socket"
        fi
        WAYLAND_DISPLAY="$_socket" foot -a yuki-titlebar >/dev/null 2>&1 &
        _waited=0
        while ! grep -q "$expected" "$LOG_FILE" 2>/dev/null && [ "$_waited" -lt 50 ]; do
            sleep 0.1
            _waited=$((_waited + 1))
        done
        if ! grep -q "$expected" "$LOG_FILE" 2>/dev/null; then
            tail -n 15 "$LOG_FILE" >&2
            stop >/dev/null 2>&1 || true
            fail "$EXIT_SMOKE" "titlebar: $scenario did not reach '$expected'"
        fi
        stop >/dev/null 2>&1 || true
        printf 'titlebar %s: PASS\n' "$scenario"
    done
}

# decoration - run real applications inside Yuki and assert the decoration
# protocol and mode each one negotiated (ADR 0019).
#
# Applications are found through YUKI_APP_* variables (see the sctk-probe
# README); one that is not available is reported UNVERIFIED and does not fail
# the run, so the command works on a machine with no GTK or Qt demos.
decoration() {
    require_devshell
    require cargo
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for decoration: $1"
    fi
    if [ -n "$_release" ]; then
        _profile=release
        cargo build --release -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    else
        _profile=debug
        cargo build -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    fi
    YUKI_BIN="$script_dir/target/$_profile/yuki" \
        "$script_dir/target/$_profile/decoration_probe" ||
        fail "$EXIT_SMOKE" "decoration probe failed"
}

# move - exercise client-initiated xdg_toplevel.move/resize with valid and
# fabricated serials. Like the other harnesses it launches its own compositor.
move() {
    require_devshell
    require cargo
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for move: $1"
    fi
    if [ -n "$_release" ]; then
        _profile=release
        cargo build --release -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    else
        _profile=debug
        cargo build -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    fi
    YUKI_BIN="$script_dir/target/$_profile/yuki" \
        YUKI_MOVE_BIN="$script_dir/target/$_profile/move_client" \
        YUKI_HOVER_BIN="$script_dir/target/$_profile/hover_client" \
        "$script_dir/target/$_profile/move_probe" ||
        fail "$EXIT_SMOKE" "move probe failed"
}

# build_probes [--release] - build the compositor and the probe package.
#
# Sets `_profile` to the directory the probe binaries live in.
build_probes() {
    _release=
    if [ "${1:-}" = "--release" ]; then
        _release=1
        shift
    fi
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for the probes: $1"
    fi
    if [ -n "$_release" ]; then
        _profile=release
        cargo build --release -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    else
        _profile=debug
        cargo build -p yuki -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    fi
}

# run_one_probe PROBE [--test-input] - run one probe against a fresh compositor.
#
# Starts the compositor, waits for its socket, runs the probe binary and stops
# the compositor again. `--test-input` starts it with `YUKI_TEST_INPUT=1`, whose
# scripted press a probe cannot make itself. Fails the script when the probe
# does.
run_one_probe() {
    _probe=$1
    shift
    _with_input=
    if [ "${1:-}" = "--test-input" ]; then
        _with_input=1
        shift
    fi
    stop >/dev/null 2>&1 || true
    if [ -n "$_with_input" ]; then
        YUKI_TEST_INPUT=1 start
    else
        start
    fi
    _socket=
    _waited=0
    while [ -z "$_socket" ] && [ "$_waited" -lt 100 ]; do
        _socket=$(sed -n \
            's/.*listening on the Wayland socket socket=\(wayland-[0-9]*\).*/\1/p' \
            "$LOG_FILE" 2>/dev/null | tail -n 1)
        [ -n "$_socket" ] && break
        sleep 0.1
        _waited=$((_waited + 1))
    done
    if [ -z "$_socket" ]; then
        stop >/dev/null 2>&1 || true
        fail "$EXIT_SMOKE" "$_probe: the compositor never opened a socket"
    fi
    if ! env "$@" WAYLAND_DISPLAY="$_socket" "$script_dir/target/$_profile/$_probe"; then
        stop >/dev/null 2>&1 || true
        fail "$EXIT_SMOKE" "$_probe failed"
    fi
    stop >/dev/null 2>&1 || true
}

# pointers - run the pointer lock and relative motion probes, each against a
# fresh compositor with the scripted input enabled.
#
# `pointer_lock` asserts the `locked` event arrives for a constraint on the
# focused surface; `relative_pointer` asserts a non-zero relative motion reaches
# the client. Each probe needs its own compositor: the scripted press is
# injected once and stays held.
pointers() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe pointer_lock --test-input
    run_one_probe relative_pointer --test-input
}

# screencopy - capture the output through `ext-image-copy-capture-v1`.
#
# `screencopy` opens capture sessions for the output over SHM and dma-buf, with
# the cursor painted and omitted, and asserts the frame is ready and not blank.
screencopy() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe screencopy
    run_one_probe screencopy CAPTURE_EXPECT_NO_CURSOR=1
    run_one_probe screencopy CAPTURE_PAINT_CURSORS=1 CAPTURE_EXPECT_CURSOR=1
    run_one_probe screencopy CAPTURE_DMABUF=1 CAPTURE_EXPECT_NO_CURSOR=1
    run_one_probe screencopy CAPTURE_DMABUF=1 CAPTURE_PAINT_CURSORS=1 CAPTURE_EXPECT_CURSOR=1
}

# screencopy-window - capture a window through `ext-image-copy-capture-v1`.
#
# `screencopy_window` maps a window, creates a capture source from its
# `ext-foreign-toplevel-list` handle and asserts the frame is ready and shows
# the window's own content, over SHM and dma-buf. `CAPTURE_SSD=1` requests
# server-side decorations to check that the titlebar stays out of the capture.
screencopy_window() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe screencopy_window
    run_one_probe screencopy_window CAPTURE_DMABUF=1
    run_one_probe screencopy_window CAPTURE_SSD=1
}

# presentation - assert `wp_presentation` feedback reaches a client.
#
# `presentation` commits two damaged frames and one without damage, asking for
# feedback on each. It asserts increasing timestamps, a response to every
# commit, and (with PRESENTATION_REQUIRE_VSYNC=1) a non-zero refresh and Vsync.
presentation() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe presentation
}

# activation - assert `xdg-activation-v1` focuses a window in the same client.
#
# `activation` runs two clients: A gets the scripted press and creates tokens,
# B activates itself with the valid one. Tokens without a serial, from an
# unfocused surface and older than 30 s must not move focus to B.
activation() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe activation --test-input
}


# demo - a nested session that shows the whole desktop, for a quick look.
#
# Starts the winit compositor (as `run` does), then connects the native shell
# components and a couple of demo windows to its socket: wallpaper, menubar
# and dock with tiles in it. Closing the host window or Ctrl-C ends it all.
# `YUKI_SCALE` passes through, so `YUKI_SCALE=1.5 ./dev.sh demo` shows the
# fractional-scale path.
demo() {
    _scheme=
    case "${1:-}" in
        --dark | --light)
            _scheme=${1#--}
            shift
            ;;
    esac
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for demo: $1"
    fi
    if is_running; then
        fail "$EXIT_ALREADY_RUNNING" "yuki is already running (pid $(read_pid))"
    fi
    if [ -n "$_scheme" ]; then
        # A throwaway preferences directory: the demo never writes to
        # ~/.config/yuki. Copy the user's config so the rest of it still
        # applies, then force color_scheme at the top level of appearance.domain
        # (TOML keys must precede any table).
        _demo_config=$(mktemp -d) ||
            fail "$EXIT_RUN_FAILED" "could not create the demo config directory"
        if [ -d "$HOME/.config/yuki" ]; then
            cp -R "$HOME/.config/yuki/." "$_demo_config/" ||
                fail "$EXIT_RUN_FAILED" "could not copy ~/.config/yuki"
        fi
        _appearance="$_demo_config/appearance.domain"
        if [ -f "$_appearance" ]; then
            awk -v scheme="$_scheme" '
                BEGIN { print "color_scheme = \"" scheme "\"" }
                /^[[:space:]]*\[/ { table = 1 }
                !table && /^[[:space:]]*color_scheme[[:space:]]*=/ { next }
                { print }
            ' "$_appearance" >"$_appearance.tmp" &&
                mv "$_appearance.tmp" "$_appearance" ||
                fail "$EXIT_RUN_FAILED" "could not write $_appearance"
        else
            printf 'color_scheme = "%s"\n' "$_scheme" >"$_appearance"
        fi
        YUKI_CONFIG_DIR="$_demo_config"
        export YUKI_CONFIG_DIR
    fi
    _pids=
    _demo_cleanup() {
        trap - EXIT INT TERM
        for _pid in ${_pids:-}; do
            kill "$_pid" 2>/dev/null || true
        done
        stop
        if [ -n "${_demo_config:-}" ]; then
            rm -rf "$_demo_config"
        fi
    }
    trap _demo_cleanup EXIT INT TERM
    if [ -n "${YUKI_DEV_SHELL:-}" ]; then
        _shell_bin="$script_dir/target/debug/yuki-shell"
        if [ ! -x "$script_dir/target/debug/yuki" ] || [ ! -x "$_shell_bin" ]; then
            "$script_dir/dev.sh" build
        fi
        if [ ! -x "$script_dir/target/debug/examples/window_csd" ] ||
            [ ! -x "$script_dir/target/debug/examples/window_ssd" ] ||
            [ ! -x "$script_dir/target/debug/examples/dialog" ]; then
            cargo build -p celestial-kit --examples ||
                fail "$EXIT_RUN_FAILED" "could not build the window examples"
        fi
        _window_csd="$script_dir/target/debug/examples/window_csd"
        _window_ssd="$script_dir/target/debug/examples/window_ssd"
        _dialog="$script_dir/target/debug/examples/dialog"
    else
        _shell_bin="$script_dir/result/bin/yuki-shell"
        if [ ! -x "$script_dir/result/bin/yuki" ] || [ ! -x "$_shell_bin" ]; then
            fail "$EXIT_RUN_FAILED" "no existe result/bin/yuki-shell; ejecuta: nix build .#default"
        fi
        _window_csd="$script_dir/result/bin/window_csd"
        _window_ssd="$script_dir/result/bin/window_ssd"
        _dialog="$script_dir/result/bin/dialog"
    fi

    start
    _sock=
    _waited=0
    while [ -z "$_sock" ] && [ "$_waited" -lt 100 ]; do
        _sock=$(sed -n \
            's/.*listening on the Wayland socket socket=\(wayland-[0-9]*\).*/\1/p' \
            "$LOG_FILE" 2>/dev/null | tail -n 1)
        [ -n "$_sock" ] && break
        _waited=$((_waited + 1))
        sleep 0.1
    done
    if [ -z "$_sock" ]; then
        stop
        fail "$EXIT_RUN_FAILED" "the compositor did not announce a socket; see: ./dev.sh log"
    fi

    # Every component is its own Wayland client; one log per component.
    demo_client() {
        _name=$1
        shift
        env -u DISPLAY WAYLAND_DISPLAY="$_sock" XDG_SESSION_TYPE=wayland \
            YUKI_SHELL_EXE="$_shell_bin" \
            "$@" >"$RUNTIME_DIR/demo-$_name.log" 2>&1 &
        _pids="$_pids $!"
        printf '%s\n' "$!" >"$RUNTIME_DIR/demo-$_name.pid"
    }
    demo_client wallpaper "$_shell_bin" wallpaper
    demo_client menubar "$_shell_bin" menubar
    demo_client dock "$_shell_bin" dock
    # Give the background layer a moment, then open windows so the dock has
    # tiles and the menubar an active app.
    sleep 0.5
    if command -v foot >/dev/null 2>&1; then
        demo_client foot-1 foot -T "Yuki"
        demo_client foot-2 foot -T "Segunda ventana"
    elif command -v wezterm >/dev/null 2>&1; then
        demo_client wezterm wezterm start
    else
        printf 'demo: no foot or wezterm found; the dock will be empty\n' >&2
    fi
    if [ -x "$_window_csd" ] && [ -x "$_window_ssd" ] && [ -x "$_dialog" ]; then
        demo_client window-csd "$_window_csd"
        demo_client window-ssd "$_window_ssd"
        demo_client dialog "$_dialog"
    else
        printf 'demo: window examples not found; skipping them\n' >&2
    fi

    printf 'demo: wallpaper, menubar and dock on %s\n' "$_sock"
    printf 'demo: more clients: WAYLAND_DISPLAY=%s <program>\n' "$_sock"
    printf 'demo: logs: %s/demo-*.log\n' "$RUNTIME_DIR"
    printf 'demo: close the host window or press Ctrl-C to end\n'

    _compositor=$(read_pid)
    wait "$_compositor" 2>/dev/null || true
}

# lab - interactive manual-test session with shell, demo windows and optional
# third-party apps.  It behaves like `demo`, but also launches nautilus, zenity
# (or gnome-calculator), kate/kwrite, telegram-desktop and firefox when they are
# available, each with its own log.  The session keeps running until the host
# window closes or the user presses Ctrl-C.
lab() {
    _scheme=
    case "${1:-}" in
        --dark | --light)
            _scheme=${1#--}
            shift
            ;;
    esac
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for lab: $1"
    fi
    if is_running; then
        fail "$EXIT_ALREADY_RUNNING" "yuki is already running (pid $(read_pid))"
    fi

    # Always use a throwaway config directory so we can clean it up on exit.
    _lab_config=$(mktemp -d) ||
        fail "$EXIT_RUN_FAILED" "could not create the lab config directory"
    if [ -d "$HOME/.config/yuki" ]; then
        cp -R "$HOME/.config/yuki/." "$_lab_config/" ||
            fail "$EXIT_RUN_FAILED" "could not copy ~/.config/yuki"
    fi
    if [ -n "$_scheme" ]; then
        _appearance="$_lab_config/appearance.domain"
        if [ -f "$_appearance" ]; then
            awk -v scheme="$_scheme" '
                BEGIN { print "color_scheme = \"" scheme "\"" }
                /^[[:space:]]*\[/ { table = 1 }
                !table && /^[[:space:]]*color_scheme[[:space:]]*=/ { next }
                { print }
            ' "$_appearance" >"$_appearance.tmp" &&
                mv "$_appearance.tmp" "$_appearance" ||
                fail "$EXIT_RUN_FAILED" "could not write $_appearance"
        else
            printf 'color_scheme = "%s"\n' "$_scheme" >"$_appearance"
        fi
    fi
    YUKI_CONFIG_DIR="$_lab_config"
    export YUKI_CONFIG_DIR

    _pids=
    _lab_bus_pid=
    _lab_cleanup() {
        trap - EXIT
        trap '' INT TERM
        for _pid in ${_pids:-}; do
            kill "$_pid" 2>/dev/null || true
        done
        stop
        if [ -n "${_lab_bus_pid:-}" ]; then
            kill -TERM "$_lab_bus_pid" 2>/dev/null || true
            _lab_waited=0
            while kill -0 "$_lab_bus_pid" 2>/dev/null; do
                _lab_waited=$((_lab_waited + 1))
                if [ "$_lab_waited" -ge 50 ]; then
                    kill -KILL "$_lab_bus_pid" 2>/dev/null || true
                    break
                fi
                sleep 0.1
            done
        fi
        if [ -n "${_lab_config:-}" ]; then
            rm -rf "$_lab_config"
        fi
    }
    trap _lab_cleanup EXIT INT TERM

    if [ -n "${YUKI_DEV_SHELL:-}" ]; then
        _shell_bin="$script_dir/target/debug/yuki-shell"
        if [ ! -x "$script_dir/target/debug/yuki" ] || [ ! -x "$_shell_bin" ]; then
            "$script_dir/dev.sh" build
        fi
        if [ ! -x "$script_dir/target/debug/examples/window_csd" ] ||
            [ ! -x "$script_dir/target/debug/examples/window_ssd" ] ||
            [ ! -x "$script_dir/target/debug/examples/dialog" ]; then
            cargo build -p celestial-kit --examples ||
                fail "$EXIT_RUN_FAILED" "could not build the window examples"
        fi
        _window_csd="$script_dir/target/debug/examples/window_csd"
        _window_ssd="$script_dir/target/debug/examples/window_ssd"
        _dialog="$script_dir/target/debug/examples/dialog"
    else
        _shell_bin="$script_dir/result/bin/yuki-shell"
        if [ ! -x "$script_dir/result/bin/yuki" ] || [ ! -x "$_shell_bin" ]; then
            fail "$EXIT_RUN_FAILED" "no existe result/bin/yuki-shell; ejecuta: nix build .#default"
        fi
        _window_csd="$script_dir/result/bin/window_csd"
        _window_ssd="$script_dir/result/bin/window_ssd"
        _dialog="$script_dir/result/bin/dialog"
    fi

    if ! command -v dbus-daemon >/dev/null 2>&1; then
        fail "$EXIT_RUN_FAILED" \
            "dbus-daemon is not installed; the lab needs a private session bus"
    fi
    _lab_bus_address_file="$_lab_config/dbus.address"
    _lab_bus_pid_file="$_lab_config/dbus.pid"
    if ! dbus-daemon --session --fork \
        --print-address=3 --print-pid=4 \
        3>"$_lab_bus_address_file" 4>"$_lab_bus_pid_file"; then
        fail "$EXIT_RUN_FAILED" "could not start the lab D-Bus session bus"
    fi
    DBUS_SESSION_BUS_ADDRESS=$(cat "$_lab_bus_address_file")
    _lab_bus_pid=$(cat "$_lab_bus_pid_file")
    if [ -z "$DBUS_SESSION_BUS_ADDRESS" ]; then
        fail "$EXIT_RUN_FAILED" "the lab D-Bus session bus printed no address"
    fi
    case "$_lab_bus_pid" in
        '' | *[!0-9]*)
            fail "$EXIT_RUN_FAILED" "the lab D-Bus session bus printed no pid"
            ;;
    esac
    export DBUS_SESSION_BUS_ADDRESS
    printf 'lab: D-Bus session bus: %s (pid %s)\n' \
        "$DBUS_SESSION_BUS_ADDRESS" "$_lab_bus_pid"

    start
    _sock=
    _waited=0
    while [ -z "$_sock" ] && [ "$_waited" -lt 100 ]; do
        _sock=$(sed -n \
            's/.*listening on the Wayland socket socket=\(wayland-[0-9]*\).*/\1/p' \
            "$LOG_FILE" 2>/dev/null | tail -n 1)
        [ -n "$_sock" ] && break
        _waited=$((_waited + 1))
        sleep 0.1
    done
    if [ -z "$_sock" ]; then
        stop
        fail "$EXIT_RUN_FAILED" "the compositor did not announce a socket; see: ./dev.sh log"
    fi

    lab_client() {
        _name=$1
        shift
        env -u DISPLAY WAYLAND_DISPLAY="$_sock" XDG_SESSION_TYPE=wayland \
            YUKI_SHELL_EXE="$_shell_bin" \
            "$@" >"$RUNTIME_DIR/demo-$_name.log" 2>&1 &
        _pids="$_pids $!"
        printf '%s\n' "$!" >"$RUNTIME_DIR/demo-$_name.pid"
    }

    # Launch a third-party app so it lands in the nested session and not in the
    # host GNOME session.  It inherits the lab's private D-Bus bus (app and
    # menubar must share it for the global menu) and the Wayland backend is
    # forced so activation/portals do not hand off to the host.
    lab_app_client() {
        _name=$1
        shift
        lab_client "$_name" env \
            GDK_BACKEND=wayland \
            MOZ_ENABLE_WAYLAND=1 \
            QT_QPA_PLATFORM=wayland \
            QT_QPA_PLATFORMTHEME=kde \
            XDG_CURRENT_DESKTOP=Yuki \
            "$@"
    }

    lab_client wallpaper "$_shell_bin" wallpaper
    lab_client menubar "$_shell_bin" menubar
    lab_client dock "$_shell_bin" dock
    sleep 0.5

    if command -v foot >/dev/null 2>&1; then
        lab_client foot-1 foot -T "Yuki"
        lab_client foot-2 foot -T "Segunda ventana"
    elif command -v wezterm >/dev/null 2>&1; then
        lab_client wezterm wezterm start
    else
        printf 'lab: no foot or wezterm found; the dock will be empty\n' >&2
    fi

    if [ -x "$_window_csd" ] && [ -x "$_window_ssd" ] && [ -x "$_dialog" ]; then
        lab_client window-csd "$_window_csd"
        lab_client window-ssd "$_window_ssd"
        lab_client dialog "$_dialog"
    else
        printf 'lab: window examples not found; skipping them\n' >&2
    fi

    # Optional third-party apps, only if installed.  Each gets its own log.
    if command -v nautilus >/dev/null 2>&1; then
        lab_app_client nautilus nautilus
    else
        printf 'lab: nautilus not found; skipping GTK4/libadwaita test\n' >&2
    fi

    if command -v zenity >/dev/null 2>&1; then
        lab_app_client zenity zenity --info --text "Prueba GTK sin headerbar"
    elif command -v gnome-calculator >/dev/null 2>&1; then
        lab_app_client gnome-calculator gnome-calculator
    else
        printf 'lab: zenity or gnome-calculator not found; skipping GTK test\n' >&2
    fi

    if command -v kate >/dev/null 2>&1; then
        lab_app_client kate kate
    elif command -v kwrite >/dev/null 2>&1; then
        lab_app_client kwrite kwrite
    else
        printf 'lab: kate/kwrite not found; skipping Qt test\n' >&2
    fi

    if command -v telegram-desktop >/dev/null 2>&1; then
        lab_app_client telegram telegram-desktop
    else
        printf 'lab: telegram-desktop not found; skipping Telegram test\n' >&2
    fi

    if command -v firefox >/dev/null 2>&1; then
        _lab_firefox_profile="$_lab_config/firefox"
        mkdir -p "$_lab_firefox_profile" ||
            fail "$EXIT_RUN_FAILED" "could not create the lab Firefox profile"
        lab_app_client firefox firefox \
            --no-remote --profile "$_lab_firefox_profile"
    else
        printf 'lab: firefox not found; skipping Firefox test\n' >&2
    fi

    cat <<'EOF'

lab session running. Manual checklist:
    1) Ventana > Trasladar > Izquierda/Superior izquierdo/Regresar con foot.
    2) ^Win+Left y ^Win+Return (if the host eats the combo, use the menu).
    3) Resize window_csd, window_ssd and "Padre" from 4 corners and 4 edges:
       no jumps, buttons keep responding.
    4) Resize foot with another window behind it: the back window does not focus.
    5) Nautilus must NOT have the Yuki titlebar; kate MUST have it.
    6) Help > type "oc": no other menu opens; Down reveals Foot.
    7) Many results ("s"): list scrolls inside the screen.
    8) Minimize, move and restore: returns where it was.

EOF
    printf 'lab: logs: %s/demo-*.log\n' "$RUNTIME_DIR"
    printf 'lab: follow compositor log: ./dev.sh log -f\n'
    printf 'lab: close the host window or press Ctrl-C to end\n'

    _compositor=$(read_pid)
    wait "$_compositor" 2>/dev/null || true
}

# example - run a CelestialKit example against the demo that is already running.
#
# Uses the same client environment as demo_client: the demo socket, no DISPLAY,
# XDG_SESSION_TYPE=wayland and the dev shell's EGL/Mesa variables. LD_LIBRARY_PATH
# keeps only Nix store entries, so a host glibc cannot leak into the client.
# With --sin-menubar the demo's menubar process is closed first, so a test bar
# owns the top strip.
example() {
    require_devshell
    require cargo
    _sin_menubar=
    _name=
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --sin-menubar) _sin_menubar=1 ;;
            -*) fail "$EXIT_USAGE" "unknown option for example: $1" ;;
            *)
                [ -z "$_name" ] || fail "$EXIT_USAGE" "usage: dev.sh example [--sin-menubar] <name>"
                _name=$1
                ;;
        esac
        shift
    done
    if [ -z "$_name" ]; then
        fail "$EXIT_USAGE" "usage: dev.sh example [--sin-menubar] <name>"
    fi
    case "$_name" in
        '' | *[!A-Za-z0-9_-]*)
            fail "$EXIT_USAGE" "invalid example name: $_name"
            ;;
    esac
    if ! is_running; then
        fail "$EXIT_RUN_FAILED" "the demo is not running; start it with: ./dev.sh demo"
    fi
    _sock=$(sed -n \
        's/.*listening on the Wayland socket socket=\(wayland-[0-9]*\).*/\1/p' \
        "$LOG_FILE" 2>/dev/null | tail -n 1)
    [ -n "$_sock" ] ||
        fail "$EXIT_RUN_FAILED" "the demo has no Wayland socket yet; see: ./dev.sh log"

    cargo build -p celestial-kit --example "$_name" ||
        fail "$EXIT_RUN_FAILED" "could not build the example $_name"
    _bin="$script_dir/target/debug/examples/$_name"
    [ -x "$_bin" ] || fail "$EXIT_RUN_FAILED" "the example binary is missing: $_bin"

    if [ -n "$_sin_menubar" ]; then
        _pid_file="$RUNTIME_DIR/demo-menubar.pid"
        if [ -f "$_pid_file" ]; then
            _pid=$(cat "$_pid_file" 2>/dev/null || true)
            case "$_pid" in
                '' | *[!0-9]*) ;;
                *)
                    kill "$_pid" 2>/dev/null || true
                    _waited=0
                    while kill -0 "$_pid" 2>/dev/null && [ "$_waited" -lt 50 ]; do
                        sleep 0.02
                        _waited=$((_waited + 1))
                    done
                    rm -f "$_pid_file"
                    ;;
            esac
        fi
    fi

    _ld=
    _old_ifs=$IFS
    IFS=:
    for _path in ${LD_LIBRARY_PATH:-}; do
        case "$_path" in
            /nix/store/*) _ld="${_ld:+$_ld:}$_path" ;;
        esac
    done
    IFS=$_old_ifs
    unset _path
    env -u DISPLAY WAYLAND_DISPLAY="$_sock" XDG_SESSION_TYPE=wayland \
        LD_LIBRARY_PATH="$_ld" "$_bin"
}

# drm-vkms - exercise the direct session on vkms, isolated from the panel.
#
# Loads vkms (if needed), runs the compositor with LIBSEAT_BACKEND=noop and
# --drm-device <vkms> --no-input, connects pattern_live, asks for a SIGUSR1
# capture, and checks that the four quadrants are in the presented frame. The
# EXIT trap kills the compositor and unloads vkms even when a check fails.
# `sudo` is required; run `sudo -v` first so the commands are non-interactive.
drm_vkms() {
    require_devshell
    require cargo
    require sudo
    require modprobe
    require lsmod
    require udevadm
    require python3
    require ffmpeg
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for drm-vkms: $1"
    fi
    sudo -n true 2>/dev/null ||
        fail "$EXIT_DRM_VKMS" "drm-vkms needs sudo; run 'sudo -v' first"

    _run=$(mktemp -d "${TMPDIR:-/tmp}/yuki-vkms.XXXXXX")
    _log="$_run/yuki.log"
    _pattern_log="$_run/pattern.log"
    _presentation_log="$_run/presentation.log"
    _pidfile="$_run/yuki.pid"
    _client_pidfile="$_run/pattern.pid"
    _yuki_pid=
    _client_pid=
    _vkms_loaded=
    chmod 0777 "$_run"

    cleanup() {
        _status=$?
        trap - EXIT HUP INT TERM
        if [ -n "${_client_pid:-}" ]; then
            sudo kill -TERM "$_client_pid" 2>/dev/null || true
        fi
        if [ -n "${_yuki_pid:-}" ]; then
            sudo kill -TERM "$_yuki_pid" 2>/dev/null || true
            sleep 0.3
            sudo kill -KILL "$_yuki_pid" 2>/dev/null || true
        else
            sudo pkill -TERM -f -- "--drm --drm-device $_vkms" 2>/dev/null || true
        fi
        if [ -n "${_vkms_loaded:-}" ]; then
            sudo modprobe -r vkms 2>/dev/null ||
                printf 'dev.sh: warning: could not unload vkms\n' >&2
        fi
        sudo rm -rf "$_run" 2>/dev/null || rm -rf "$_run" 2>/dev/null || true
        exit "$_status"
    }
    trap cleanup EXIT HUP INT TERM

    if ! lsmod | grep -q '^vkms '; then
        sudo modprobe vkms || fail "$EXIT_DRM_VKMS" "could not load vkms"
        _vkms_loaded=1
    fi

    _vkms=
    _tries=0
    while [ "$_tries" -lt 25 ]; do
        for _dev in /dev/dri/card*; do
            [ -e "$_dev" ] || continue
            _driver=$(udevadm info -q property -n "$_dev" 2>/dev/null |
                sed -n 's/^DRIVER=//p')
            if [ "$_driver" = vkms ]; then
                _vkms=$_dev
                break
            fi
            # Kernels with the faux bus expose vkms as faux_driver, so identify
            # the device by its sysfs path instead of a card number.
            _sys=$(readlink -f "/sys/class/drm/$(basename "$_dev")/device" 2>/dev/null || true)
            case "$_sys" in
                */vkms | */vkms/*)
                    _vkms=$_dev
                    break
                    ;;
            esac
        done
        [ -n "$_vkms" ] && break
        _tries=$((_tries + 1))
        sleep 0.2
    done
    [ -n "$_vkms" ] || fail "$EXIT_DRM_VKMS" "no DRM card with driver=vkms"

    cargo build -p yuki -p sctk-probe --bin yuki --bin pattern_live --bin presentation ||
        fail "$EXIT_DRM_VKMS" "could not build yuki, pattern_live and presentation"
    _yuki="$script_dir/target/debug/yuki"
    _pattern="$script_dir/target/debug/pattern_live"
    _presentation="$script_dir/target/debug/presentation"

    # The user's shell owns the log redirects, so the test can read them.
    # shellcheck disable=SC2024
    sudo env LIBSEAT_BACKEND=noop XDG_RUNTIME_DIR="$_run" \
        LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" \
        __EGL_VENDOR_LIBRARY_DIRS="${__EGL_VENDOR_LIBRARY_DIRS:-}" \
        LIBGL_DRIVERS_PATH="${LIBGL_DRIVERS_PATH:-}" \
        GBM_BACKENDS_PATH="${GBM_BACKENDS_PATH:-}" \
        sh -c 'printf "%s\n" "$$" >"$1"; shift; exec "$@"' sh "$_pidfile" \
        "$_yuki" --drm --drm-device "$_vkms" --no-input --log-level=debug \
        >"$_log" 2>&1 &

    # The first sudo prompt can take a while; give it 30 seconds.
    _tries=0
    while [ "$_tries" -lt 300 ] && [ ! -s "$_pidfile" ]; do
        _tries=$((_tries + 1))
        sleep 0.1
    done
    [ -s "$_pidfile" ] ||
        fail "$EXIT_DRM_VKMS" "yuki did not start; log follows: $(tail -n 20 "$_log" 2>/dev/null)"
    _yuki_pid=$(cat "$_pidfile")

    _socket=
    _tries=0
    while [ "$_tries" -lt 300 ]; do
        _socket=$(sed -n 's/.*socket *= *\(wayland-[0-9]*\).*/\1/p' "$_log" | tail -n 1)
        [ -n "$_socket" ] && break
        _tries=$((_tries + 1))
        sleep 0.1
    done
    [ -n "$_socket" ] ||
        fail "$EXIT_DRM_VKMS" "no Wayland socket; log follows: $(tail -n 30 "$_log" 2>/dev/null)"

    _drm_fds=$(sudo sh -c 'for fd in /proc/$1/fd/*; do readlink "$fd"; done' sh "$_yuki_pid" |
        grep '^/dev/dri/' || true)
    printf '%s\n' "$_drm_fds" | grep -qx "$_vkms" ||
        fail "$EXIT_DRM_VKMS" "yuki did not open the vkms card $_vkms"
    _other_card=$(printf '%s\n' "$_drm_fds" | grep '^/dev/dri/card' | grep -vx "$_vkms" || true)
    [ -z "$_other_card" ] ||
        fail "$EXIT_DRM_VKMS" "yuki opened another DRM card: $_other_card"
    _input_fds=$(sudo sh -c 'for fd in /proc/$1/fd/*; do readlink "$fd"; done' sh "$_yuki_pid" |
        grep '^/dev/input/' || true)
    [ -z "$_input_fds" ] ||
        fail "$EXIT_DRM_VKMS" "yuki opened input devices: $_input_fds"

    # shellcheck disable=SC2024
    sudo env XDG_RUNTIME_DIR="$_run" WAYLAND_DISPLAY="$_socket" \
        sh -c 'printf "%s\n" "$$" >"$1"; shift; exec "$@"' sh "$_client_pidfile" \
        "$_pattern" >"$_pattern_log" 2>&1 &

    _tries=0
    while [ "$_tries" -lt 100 ]; do
        grep -q 'pattern_live: rendered' "$_pattern_log" 2>/dev/null && break
        _tries=$((_tries + 1))
        sleep 0.1
    done
    grep -q 'pattern_live: rendered' "$_pattern_log" 2>/dev/null ||
        fail "$EXIT_DRM_VKMS" "pattern_live did not render; log follows: $(tail -n 20 "$_pattern_log" 2>/dev/null)"
    [ -s "$_client_pidfile" ] && _client_pid=$(cat "$_client_pidfile")

    sleep 1
    sudo kill -USR1 "$_yuki_pid" || fail "$EXIT_DRM_VKMS" "could not send SIGUSR1"

    _capture=
    _tries=0
    while [ "$_tries" -lt 50 ]; do
        _capture=$(find "$_run/yuki" -name 'capture-*.png' 2>/dev/null | sort | tail -n 1)
        [ -n "$_capture" ] && break
        _tries=$((_tries + 1))
        sleep 0.1
    done
    [ -n "$_capture" ] ||
        fail "$EXIT_DRM_VKMS" "no capture written; log follows: $(tail -n 30 "$_log" 2>/dev/null)"
    sudo chmod -R a+rX "$_run"
    python3 "$script_dir/scripts/verify-captures.py" "$_capture" pattern ||
        fail "$EXIT_DRM_VKMS" "the vkms capture does not contain the pattern"

    grep -q 'mode chosen' "$_log" ||
        fail "$EXIT_DRM_VKMS" "the log has no chosen mode"
    _vblanks=$(grep -c 'vblank' "$_log" || true)
    [ "$_vblanks" -ge 2 ] ||
        fail "$EXIT_DRM_VKMS" "only $_vblanks vblank log lines"
    _queued=$(grep -c 'frame queued with damage' "$_log" || true)
    [ "$_queued" -ge 1 ] ||
        fail "$EXIT_DRM_VKMS" "no frame with damage was queued"
    grep -q 'frame skipped: no damage' "$_log" ||
        fail "$EXIT_DRM_VKMS" "no empty frame was logged; cannot prove damage-only frames"
    grep -q "DRM scanout device.*path=$_vkms" "$_log" ||
        fail "$EXIT_DRM_VKMS" "the log does not name $_vkms as the scanout device"
    grep -q 'DRM render device.*path=/dev/dri/renderD' "$_log" ||
        fail "$EXIT_DRM_VKMS" "the log does not name a render node"

    printf 'drm-vkms: PASS\n'
    printf '  vkms scanout: %s\n' "$_vkms"
    printf '%s\n' "$_drm_fds" | sed 's/^/  open: /'
    printf '  capture: %s\n' "$_capture"
    printf '  mode: %s\n' "$(sed -n 's/.*mode chosen \(.*\)/\1/p' "$_log" | tail -n 1)"
    printf '  vblanks: %s, damaged frames queued: %s\n' "$_vblanks" "$_queued"
    printf '  presentation: %s\n' "$(grep 'presentation: presented' "$_presentation_log" | tail -n 1)"
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
    block4) block4 "$@" ;;
    move) move "$@" ;;
    pointers) pointers "$@" ;;
    screencopy) screencopy "$@" ;;
    screencopy-window) screencopy_window "$@" ;;
    presentation) presentation "$@" ;;
    activation) activation "$@" ;;
    titlebar) titlebar "$@" ;;
    decoration) decoration "$@" ;;
    demo) demo "$@" ;;
    lab) lab "$@" ;;
    example) example "$@" ;;
    drm-vkms) drm_vkms "$@" ;;
    -h | --help) usage ;;
    *)
        usage >&2
        fail "$EXIT_USAGE" "unknown command: $command_name"
        ;;
esac
