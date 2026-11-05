#!/bin/sh

set -eu

MALAYSIA_POWER_DRY_RUN=${MALAYSIA_POWER_DRY_RUN:-1}
export MALAYSIA_POWER_DRY_RUN

EXIT_USAGE=2
EXIT_ALREADY_RUNNING=3
EXIT_MISSING_TOOL=4
EXIT_RUN_FAILED=5
EXIT_NO_LOG=6
EXIT_NOT_DEVSHELL=7
EXIT_WATCH=8
EXIT_SMOKE=9
EXIT_DRM_VKMS=10
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
    video [--dark|--light]
                        Like demo, with only Marea (the barrierkit media app) on the
                        desktop; builds that app alone
    lab [--dark|--light]
                        Nested session with shell + demo windows + optional real
                        apps (nautilus, zenity, kate, telegram, firefox) for manual
                        testing; uses a temporary config (Ctrl-C ends it)
    app <name>          Run tengen-app-show, marea or ondina (pinned rev, or ../barrierkit with kit-local) in the demo
                        (--sin-menubar closes the demo menubar first)
    watch [--release]   Watch sources; rebuild and relaunch on change (Ctrl-C stops)
    stop                Stop the background instance and remove the PID file
    restart             Stop, then run (no rebuild)
    status              Report whether the background instance is running
    log [-f]            Print the log, or follow it with -f
    clean               Remove build artifacts and runtime files
    check               Run formatting, clippy and tests
    wallpaper-check     Build, start the compositor, capture the wallpaper and run the ignored pixel test
    kit-local           Use ../barrierkit as a path override for the kit crates
                        (the rev in Cargo.toml must already be pushed; kit-local only swaps the source)
    kit-remote          Remove the ../barrierkit path override
    smoke [--release]   Build and run the toplevel mapping smoke test
    block4 [--release]  Build and run the 50-window open/close acceptance test
    move [--release]    Build and run the client-initiated move/resize probe
    pointers [--release] Build and run the pointer lock and relative motion probes
    screencopy [--release] Build and run the output capture probe
    screencopy-window   Build and run the window capture probe
    presentation        Build and run the wp_presentation feedback probe
    frames              Build and run the frame-callback pacing probe (no damage, no input)
    activation          Build and run the xdg-activation probe
    titlebar            Press close, minimize and maximize on a real window
    decoration          Build and run the decoration-policy probe over real apps
    drm-vkms            Test --drm on vkms without touching the real panel or input
EOF
}

fail() {
    _code=$1
    shift
    printf 'dev.sh: %s\n' "$*" >&2
    exit "$_code"
}

require() {
    command -v "$1" >/dev/null 2>&1 ||
        fail "$EXIT_MISSING_TOOL" "missing required tool: $1"
}

require_devshell() {
    [ "${MALAYSIA_DEV_SHELL:-}" = "1" ] ||
        fail "$EXIT_NOT_DEVSHELL" "not inside the dev shell; run 'nix develop' first (see ADR 0007)"
}

require_vulkan() {
    _vk_files=${VK_DRIVER_FILES:-}
    if [ -z "$_vk_files" ]; then
        fail "$EXIT_MISSING_TOOL" \
            "VK_DRIVER_FILES no está definido o apunta a archivos que no existen (${_vk_files}); cierra y reabre nix develop y ejecuta ./dev.sh build"
    fi
    _missing=
    _old_ifs=$IFS
    IFS=:
    for _path in $_vk_files; do
        [ -e "$_path" ] || _missing="$_missing $_path"
    done
    IFS=$_old_ifs
    if [ -n "$_missing" ]; then
        fail "$EXIT_MISSING_TOOL" \
            "VK_DRIVER_FILES no está definido o apunta a archivos que no existen (${_vk_files}); cierra y reabre nix develop y ejecuta ./dev.sh build"
    fi
}

check_wallpaper_ready() {
    _prefix=$1
    _pid_file="$RUNTIME_DIR/demo-wallpaper.pid"
    _log_file="$RUNTIME_DIR/demo-wallpaper.log"
    _pid=$(cat "$_pid_file" 2>/dev/null || true)
    _waited=0
    while [ "$_waited" -lt 50 ]; do
        if [ -n "$_pid" ] && ! kill -0 "$_pid" 2>/dev/null; then
            _code=$?
            printf '%s: malaysia-wallpaper terminó al arrancar (código %s); últimas líneas:\n' \
                "$_prefix" "$_code" >&2
            if [ -f "$_log_file" ]; then
                tail -n 20 "$_log_file" >&2
            fi
            return
        fi
        if [ -f "$_log_file" ] && grep -q "output configured" "$_log_file" 2>/dev/null; then
            return 0
        fi
        _waited=$((_waited + 1))
        sleep 0.1
    done
    printf '%s: malaysia-wallpaper no se configuró en 5 s; últimas líneas:\n' "$_prefix" >&2
    if [ -f "$_log_file" ]; then
        tail -n 20 "$_log_file" >&2
    fi
}

barrierkit_dir() {
    _dir="$script_dir/../barrierkit"
    if [ -d "$_dir/crates/barrier-kit" ]; then
        CDPATH='' cd -- "$_dir" && pwd
    fi
}

kit_is_local() {
    [ -f "$script_dir/.cargo/config.toml" ]
}

build_kit_apps() {
    _kit_dir=$(barrierkit_dir)
    if kit_is_local && [ -n "$_kit_dir" ]; then
        (cd "$_kit_dir" && cargo build $(printf -- '-p %s ' "$@")) ||
            fail "$EXIT_RUN_FAILED" "could not build $* in ../barrierkit"
        _kit_bin="$_kit_dir/target/debug"
    else
        _kit_rev=$(sed -n 's/^barrier-kit = .*rev = "\([0-9a-f]*\)".*/\1/p' "$script_dir/Cargo.toml")
        _kit_root="$script_dir/target/barrierkit-apps"
        _kit_bin="$_kit_root/bin"
        _kit_build="$_kit_root/build/$_kit_rev"
        find "$_kit_root/build" -mindepth 1 -maxdepth 1 ! -name "$_kit_rev" -exec rm -rf {} + 2>/dev/null || true
        printf 'dev.sh: building %s from barrierkit %s (the first time takes a few minutes)\n' "$*" "$_kit_rev" >&2
        # Cargo hashes workspace crates by their relative path, so two revs sharing a target dir reuse stale build-script output.
        cargo install --debug --locked --git https://github.com/HBlanqueto/barrierkit \
            --rev "$_kit_rev" --root "$_kit_root" --target-dir "$_kit_build" "$@" ||
            fail "$EXIT_RUN_FAILED" "could not build $* from barrierkit $_kit_rev"
    fi
}

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)

case "${RUSTFLAGS:-}" in
    *-fuse-ld=mold*) ;;
    *) RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"; export RUSTFLAGS ;;
esac

checkout_hash() {
    _root=$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null ||
        printf '%s' "$script_dir")
    printf '%s' "$_root" | cksum | cut -d' ' -f1
}

runtime_dir() {
    _hash=$(checkout_hash)
    if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -d "$XDG_RUNTIME_DIR" ]; then
        printf '%s\n' "$XDG_RUNTIME_DIR/malaysia/dev-$_hash"
    else
        printf '%s\n' "/tmp/malaysia-$(id -u)-$_hash"
    fi
}

RUNTIME_DIR=$(runtime_dir)
PID_FILE="$RUNTIME_DIR/malaysia-compositor.pid"
LOG_FILE="$RUNTIME_DIR/malaysia-compositor.log"

ensure_runtime_dir() {
    mkdir -p "$RUNTIME_DIR"
}

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

pid_is_malaysia_compositor() {
    kill -0 "$1" 2>/dev/null || return 1
    case "$(cat "/proc/$1/comm" 2>/dev/null)" in
        malaysia-compositor | .malaysia-compositor-wrapped) return 0 ;;
        *) return 1 ;;
    esac
}

is_running() {
    _saved=$(read_pid) || return 1
    pid_is_malaysia_compositor "$_saved"
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
        fail "$EXIT_ALREADY_RUNNING" "malaysia-compositor is already running (pid $_saved)"
    fi
    if [ -n "${MALAYSIA_BIN:-}" ]; then
        _bin=$MALAYSIA_BIN
    elif [ -z "${MALAYSIA_DEV_SHELL:-}" ] && [ -x "$script_dir/result/bin/malaysia-compositor" ]; then
        _bin="$script_dir/result/bin/malaysia-compositor"
    elif [ -n "$_release" ]; then
        _bin="$script_dir/target/release/malaysia-compositor"
    else
        _bin="$script_dir/target/debug/malaysia-compositor"
    fi
    if [ ! -x "$_bin" ]; then
        fail "$EXIT_RUN_FAILED" "binary not built; run './dev.sh build' first"
    fi
    rm -f "$PID_FILE"
    ensure_runtime_dir
    require setsid
    : >"$LOG_FILE"
    setsid "$_bin" >>"$LOG_FILE" 2>&1 &
    _saved=$!
    printf '%s\n' "$_saved" >"$PID_FILE"
    sleep 0.2
    if ! kill -0 "$_saved" 2>/dev/null; then
        rm -f "$PID_FILE"
        printf 'dev.sh: malaysia-compositor exited immediately; log follows:\n' >&2
        if [ -f "$LOG_FILE" ]; then
            tail -n 20 "$LOG_FILE" >&2
        fi
        exit "$EXIT_RUN_FAILED"
    fi
}

logged_socket() {
    sed -n 's/.*listening on the Wayland socket socket=\(wayland-[0-9]*\).*/\1/p' \
        "$LOG_FILE" 2>/dev/null | tail -n 1
}

wait_socket() {
    _waited=0
    while [ "$_waited" -lt 100 ]; do
        _sock=$(logged_socket)
        [ -n "$_sock" ] && return 0
        _waited=$((_waited + 1))
        sleep 0.1
    done
    return 1
}

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

    set -- -w "$script_dir/crates" --ignore "$script_dir/target"
    if [ -d "$script_dir/src" ]; then
        set -- "$@" -w "$script_dir/src"
    fi

    _cycle="$script_dir/dev.sh build"
    if [ -n "$_release" ]; then
        _cycle="$_cycle --release"
    fi
    _cycle="$_cycle && $script_dir/dev.sh restart"
    if [ -n "$_release" ]; then
        _cycle="$_cycle --release"
    fi

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

stop() {
    _saved=$(read_pid) || return 0
    if ! pid_is_malaysia_compositor "$_saved"; then
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
        printf 'malaysia-compositor is running (pid %s)\n' "$_saved"
        return 0
    fi
    if [ -f "$PID_FILE" ]; then
        rm -f "$PID_FILE"
        printf 'malaysia-compositor is not running (removed stale pid file)\n'
        return "$EXIT_STATUS_DEAD"
    fi
    printf 'malaysia-compositor is not running\n'
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

check_display_env() {
    _sentinel=$(mktemp -d)
    _probe="$_sentinel/probe"
    _record="$_sentinel/record"
    cat >"$_probe" <<'PROBE'
#!/bin/sh
printf '%s\n%s\n' "${WAYLAND_DISPLAY-}" "${XDG_RUNTIME_DIR-}" >"$MALAYSIA_ENV_RECORD"
sleep 5
PROBE
    chmod +x "$_probe"
    _want_display="malaysia-check-wayland"
    _want_runtime="$_sentinel"
    if ! MALAYSIA_BIN="$_probe" MALAYSIA_ENV_RECORD="$_record" \
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

kit_local() {
    _dir=$(barrierkit_dir)
    [ -n "$_dir" ] || fail "$EXIT_USAGE" "../barrierkit not found; clone it next to this repo"
    mkdir -p "$script_dir/.cargo"
    cat >"$script_dir/.cargo/config.toml" <<EOF
[patch."https://github.com/HBlanqueto/barrierkit"]
barrier-kit = { path = "$_dir/crates/barrier-kit" }
vow-design = { path = "$_dir/crates/vow-design" }
vow-appearance = { path = "$_dir/crates/vow-appearance" }
infinity-protocol = { path = "$_dir/crates/infinity-protocol" }
EOF
    printf 'dev.sh: kit-local enabled: %s\n' "$_dir"
}

kit_remote() {
    if [ -f "$script_dir/.cargo/config.toml" ]; then
        rm -f "$script_dir/.cargo/config.toml"
        rmdir "$script_dir/.cargo" 2>/dev/null || true
        printf 'dev.sh: kit-remote enabled: removed local barrierkit override\n'
    else
        printf 'dev.sh: kit-remote: no local override to remove\n'
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
    "$script_dir/scripts/check-client-only-deps.sh"
    cargo clippy --workspace --all-targets -- -D warnings
    cargo build -p malaysia-compositor
    "$script_dir/scripts/check-client-dispatch.sh"
    cargo test --workspace
}

smoke() {
    require_devshell
    require cargo
    build_probes "$@"
    MALAYSIA_BIN="$script_dir/target/$_profile/malaysia-compositor" \
        "$script_dir/target/$_profile/smoke" ||
        fail "$EXIT_SMOKE" "smoke test failed"
}

block4() {
    require_devshell
    require cargo
    build_probes "$@"
    MALAYSIA_BIN="$script_dir/target/$_profile/malaysia-compositor" \
        MALAYSIA_STRESS_BIN="$script_dir/target/$_profile/window_stress" \
        MALAYSIA_RESIZE_BIN="$script_dir/target/$_profile/resize_client" \
        "$script_dir/target/$_profile/block4" ||
        fail "$EXIT_SMOKE" "block4 test failed"
}

titlebar() {
    require_devshell
    require cargo
    require foot
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for titlebar: $1"
    fi
    cargo build -p malaysia-compositor || fail "$EXIT_SMOKE" "could not build the compositor"
    for scenario in close minimize maximize; do
        case "$scenario" in
            close) expected="window removed" ;;
            minimize) expected="window minimized" ;;
            maximize) expected="window maximized" ;;
        esac
        stop >/dev/null 2>&1 || true
        MALAYSIA_TEST_INPUT=1 MALAYSIA_TEST_TITLEBAR_BUTTON="$scenario" start
        if ! wait_socket; then
            stop >/dev/null 2>&1 || true
            fail "$EXIT_SMOKE" "titlebar: the compositor never opened a socket"
        fi
        WAYLAND_DISPLAY="$_sock" foot -a malaysia-titlebar >/dev/null 2>&1 &
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

decoration() {
    require_devshell
    require cargo
    build_probes "$@"
    MALAYSIA_BIN="$script_dir/target/$_profile/malaysia-compositor" \
        "$script_dir/target/$_profile/decoration_probe" ||
        fail "$EXIT_SMOKE" "decoration probe failed"
}

move() {
    require_devshell
    require cargo
    build_probes "$@"
    MALAYSIA_BIN="$script_dir/target/$_profile/malaysia-compositor" \
        MALAYSIA_MOVE_BIN="$script_dir/target/$_profile/move_client" \
        MALAYSIA_HOVER_BIN="$script_dir/target/$_profile/hover_client" \
        "$script_dir/target/$_profile/move_probe" ||
        fail "$EXIT_SMOKE" "move probe failed"
}

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
        cargo build --release -p malaysia-compositor -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    else
        _profile=debug
        cargo build -p malaysia-compositor -p sctk-probe ||
            fail "$EXIT_SMOKE" "could not build the compositor and the probes"
    fi
}

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
        MALAYSIA_TEST_INPUT=1 start
    else
        start
    fi
    if ! wait_socket; then
        stop >/dev/null 2>&1 || true
        fail "$EXIT_SMOKE" "$_probe: the compositor never opened a socket"
    fi
    if ! env "$@" WAYLAND_DISPLAY="$_sock" "$script_dir/target/$_profile/$_probe"; then
        stop >/dev/null 2>&1 || true
        fail "$EXIT_SMOKE" "$_probe failed"
    fi
    stop >/dev/null 2>&1 || true
}

pointers() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe pointer_lock --test-input
    run_one_probe relative_pointer --test-input
}

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

screencopy_window() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe screencopy_window
    run_one_probe screencopy_window CAPTURE_DMABUF=1
    run_one_probe screencopy_window CAPTURE_SSD=1
}

presentation() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe presentation
}

frames() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe frame_callbacks
}

activation() {
    require_devshell
    require cargo
    build_probes "$@"
    run_one_probe activation --test-input
}

session_client() {
    _name=$1
    shift
    env -u DISPLAY WAYLAND_DISPLAY="$_sock" XDG_SESSION_TYPE=wayland \
        MALAYSIA_SHELL_EXE="$_shell_bin" \
        "$@" >"$RUNTIME_DIR/demo-$_name.log" 2>&1 &
    _pids="$_pids $!"
    printf '%s\n' "$!" >"$RUNTIME_DIR/demo-$_name.pid"
}

wait_session_end() {
    _compositor=$(read_pid)
    while kill -0 "$_compositor" 2>/dev/null; do
        if [ -n "$_wallpaper_pid" ] && ! kill -0 "$_wallpaper_pid" 2>/dev/null; then
            _wp_code=0
            wait "$_wallpaper_pid" 2>/dev/null || _wp_code=$?
            printf '%s: malaysia-wallpaper terminó (código %s); log: %s\n' \
                "$1" "$_wp_code" "$RUNTIME_DIR/demo-wallpaper.log" >&2
            _wallpaper_pid=
        fi
        sleep 1
    done
}

isolate_session_dirs() {
    _session_dirs=$(mktemp -d) ||
        fail "$EXIT_RUN_FAILED" "could not create the session directories"
    MALAYSIA_CONFIG_DIR="$_session_dirs/config"
    MALAYSIA_DATA_DIR="$_session_dirs/data"
    mkdir -p "$MALAYSIA_CONFIG_DIR" "$MALAYSIA_DATA_DIR" "$_session_dirs/home" ||
        fail "$EXIT_RUN_FAILED" "could not create the session directories"
    _user_config="${XDG_CONFIG_HOME:-$HOME/.config}/malaysia"
    _user_data="${XDG_DATA_HOME:-$HOME/.local/share}/malaysia"
    if [ -d "$_user_config" ]; then
        cp -R "$_user_config/." "$MALAYSIA_CONFIG_DIR/" ||
            fail "$EXIT_RUN_FAILED" "could not copy $_user_config"
    fi
    if [ -d "$_user_data" ]; then
        cp -R "$_user_data/." "$MALAYSIA_DATA_DIR/" ||
            fail "$EXIT_RUN_FAILED" "could not copy $_user_data"
    fi
    export MALAYSIA_CONFIG_DIR MALAYSIA_DATA_DIR
    [ -n "${1:-}" ] || return 0
    _appearance="$MALAYSIA_CONFIG_DIR/appearance.domain"
    if [ -f "$_appearance" ]; then
        awk -v scheme="$1" '
            BEGIN { print "color_scheme = \"" scheme "\"" }
            /^[[:space:]]*\[/ { table = 1 }
            !table && /^[[:space:]]*color_scheme[[:space:]]*=/ { next }
            { print }
        ' "$_appearance" >"$_appearance.tmp" &&
            mv "$_appearance.tmp" "$_appearance" ||
            fail "$EXIT_RUN_FAILED" "could not write $_appearance"
    else
        printf 'color_scheme = "%s"\n' "$1" >"$_appearance"
    fi
}

demo() {
    require_devshell
    require_vulkan
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
    _apps=${_demo_apps-tengen-app-show ondina}
    if is_running; then
        fail "$EXIT_ALREADY_RUNNING" "malaysia-compositor is already running (pid $(read_pid))"
    fi
    isolate_session_dirs "$_scheme"
    _pids=
    _demo_cleanup() {
        trap - EXIT INT TERM
        for _pid in ${_pids:-}; do
            kill "$_pid" 2>/dev/null || true
        done
        stop
        rm -rf "$_session_dirs"
    }
    trap _demo_cleanup EXIT INT TERM
    cargo build -p malaysia-compositor -p malaysia-shell -p malaysia-wallpaper -p malaysia-settings ||
        fail "$EXIT_RUN_FAILED" "could not build the demo binaries"
    _shell_bin="$script_dir/target/debug/malaysia-shell"
    _wallpaper_bin="$script_dir/target/debug/malaysia-wallpaper"
    # shellcheck disable=SC2086
    build_kit_apps $_apps
    _apps_bin=$_kit_bin

    start
    if ! wait_socket; then
        stop
        fail "$EXIT_RUN_FAILED" "the compositor did not announce a socket; see: ./dev.sh log"
    fi

    _data_dirs="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
    XDG_DATA_DIRS="$script_dir/share:$_data_dirs"
    export XDG_DATA_DIRS

    session_client wallpaper "$_wallpaper_bin"
    session_client menubar "$_shell_bin" menubar
    session_client dock "$_shell_bin" dock
    check_wallpaper_ready demo
    _wallpaper_pid=$(cat "$RUNTIME_DIR/demo-wallpaper.pid" 2>/dev/null || true)
    sleep 0.5
    if [ -z "${_demo_apps+set}" ]; then
        if command -v foot >/dev/null 2>&1; then
            session_client foot-1 foot -T "Malaysia"
            session_client foot-2 foot -T "Segunda ventana"
        elif command -v wezterm >/dev/null 2>&1; then
            session_client wezterm wezterm start
        else
            printf 'demo: no foot or wezterm found; the dock will be empty\n' >&2
        fi
    fi
    for _app in $_apps; do
        if [ -x "$_apps_bin/$_app" ]; then
            session_client "$_app" "$_apps_bin/$_app"
        else
            printf 'demo: %s not found in %s; skipping it\n' "$_app" "$_apps_bin" >&2
        fi
    done

    printf 'demo: wallpaper, menubar and dock on %s\n' "$_sock"
    printf 'demo: more clients: WAYLAND_DISPLAY=%s <program>\n' "$_sock"
    printf 'demo: logs: %s/demo-*.log\n' "$RUNTIME_DIR"
    printf 'demo: close the host window or press Ctrl-C to end\n'

    wait_session_end demo
}

video() {
    _demo_apps=marea
    demo "$@"
}

lab() {
    require_devshell
    require_vulkan
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
        fail "$EXIT_ALREADY_RUNNING" "malaysia-compositor is already running (pid $(read_pid))"
    fi

    isolate_session_dirs "$_scheme"

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
        rm -rf "$_session_dirs"
    }
    trap _lab_cleanup EXIT INT TERM

    _shell_bin="$script_dir/target/debug/malaysia-shell"
    _wallpaper_bin="$script_dir/target/debug/malaysia-wallpaper"
    cargo build -p malaysia-compositor -p malaysia-shell -p malaysia-wallpaper -p malaysia-settings ||
        fail "$EXIT_RUN_FAILED" "could not build the lab binaries"
    build_kit_apps tengen-app-show ondina
    _show="$_kit_bin/tengen-app-show"
    _ondina="$_kit_bin/ondina"

    if ! command -v dbus-daemon >/dev/null 2>&1; then
        fail "$EXIT_RUN_FAILED" \
            "dbus-daemon is not installed; the lab needs a private session bus"
    fi
    _lab_bus_address_file="$_session_dirs/dbus.address"
    _lab_bus_pid_file="$_session_dirs/dbus.pid"
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
    if ! wait_socket; then
        stop
        fail "$EXIT_RUN_FAILED" "the compositor did not announce a socket; see: ./dev.sh log"
    fi

    _data_dirs="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
    XDG_DATA_DIRS="$script_dir/share:$_data_dirs"
    export XDG_DATA_DIRS

    lab_app_client() {
        _name=$1
        shift
        session_client "$_name" env \
            HOME="$_session_dirs/home" \
            XDG_CONFIG_HOME="$_session_dirs/home/.config" \
            XDG_DATA_HOME="$_session_dirs/home/.local/share" \
            XDG_STATE_HOME="$_session_dirs/home/.local/state" \
            XDG_CACHE_HOME="$_session_dirs/home/.cache" \
            QT_QPA_PLATFORM=wayland \
            QT_QPA_PLATFORMTHEME=kde \
            XDG_CURRENT_DESKTOP=Malaysia \
            "$@"
    }

    session_client wallpaper "$_wallpaper_bin"
    session_client menubar "$_shell_bin" menubar
    session_client dock "$_shell_bin" dock
    check_wallpaper_ready lab
    _wallpaper_pid=$(cat "$RUNTIME_DIR/demo-wallpaper.pid" 2>/dev/null || true)
    sleep 0.5

    if command -v foot >/dev/null 2>&1; then
        session_client foot-1 foot -T "Malaysia"
        session_client foot-2 foot -T "Segunda ventana"
    elif command -v wezterm >/dev/null 2>&1; then
        session_client wezterm wezterm start
    else
        printf 'lab: no foot or wezterm found; the dock will be empty\n' >&2
    fi

    if [ -x "$_show" ]; then
        session_client tengen-app-show "$_show"
    else
        printf 'lab: tengen-app-show not found; skipping it\n' >&2
    fi

    if [ -z "${MALAYSIA_TEST_INPUT:-}" ]; then
        if command -v nautilus >/dev/null 2>&1; then
            lab_app_client nautilus nautilus
        else
            printf 'lab: nautilus not found; skipping GTK4/libadwaita test\n' >&2
        fi

        if command -v zenity >/dev/null 2>&1; then
            lab_app_client zenity zenity --info --text "Prueba GTK sin headerbar"
        else
            printf 'lab: zenity not found; skipping GTK dialog test\n' >&2
        fi

        if command -v gnome-calculator >/dev/null 2>&1; then
            lab_app_client gnome-calculator gnome-calculator
        else
            printf 'lab: gnome-calculator not found; skipping GTK/libadwaita test\n' >&2
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
            _lab_firefox_profile="$_session_dirs/firefox"
            mkdir -p "$_lab_firefox_profile" ||
                fail "$EXIT_RUN_FAILED" "could not create the lab Firefox profile"
            lab_app_client firefox firefox \
                --no-remote --profile "$_lab_firefox_profile"
        else
            printf 'lab: firefox not found; skipping Firefox test\n' >&2
        fi
    fi

    sleep 0.5
    if [ -x "$_ondina" ]; then
        session_client ondina "$_ondina"
    fi

    cat <<'EOF'

lab session running. Manual checklist:
    1) Ventana > Trasladar > Izquierda/Superior izquierdo/Regresar con foot.
    2) ^Win+Left y ^Win+Return (if the host eats the combo, use the menu).
    3) Resize Tengen App Show and Ondina from 4 corners and 4 edges:
       no jumps, buttons keep responding; Ondina changes layout at 900 and 620.
    4) Resize foot with another window behind it: the back window does not focus.
    5) Nautilus must NOT have the Malaysia titlebar; kate MUST have it.
    6) Help > type "oc": no other menu opens; Down reveals Foot.
    7) Many results ("s"): list scrolls inside the screen.
    8) Minimize, move and restore: returns where it was.

EOF
    printf 'lab: logs: %s/demo-*.log\n' "$RUNTIME_DIR"
    printf 'lab: follow compositor log: ./dev.sh log -f\n'
    printf 'lab: close the host window or press Ctrl-C to end\n'

    wait_session_end lab
}

app() {
    require_devshell
    require cargo
    _sin_menubar=
    _name=
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --sin-menubar) _sin_menubar=1 ;;
            -*) fail "$EXIT_USAGE" "unknown option for app: $1" ;;
            *)
                [ -z "$_name" ] || fail "$EXIT_USAGE" "usage: dev.sh app [--sin-menubar] tengen-app-show|marea|ondina"
                _name=$1
                ;;
        esac
        shift
    done
    if [ -z "$_name" ]; then
        fail "$EXIT_USAGE" "usage: dev.sh app [--sin-menubar] tengen-app-show|marea|ondina"
    fi
    case "$_name" in
        tengen-app-show | marea | ondina) ;;
        *) fail "$EXIT_USAGE" "unknown app: $_name (tengen-app-show, marea or ondina)" ;;
    esac
    if ! is_running; then
        fail "$EXIT_RUN_FAILED" "the demo is not running; start it with: ./dev.sh demo"
    fi
    _sock=$(logged_socket)
    [ -n "$_sock" ] ||
        fail "$EXIT_RUN_FAILED" "the demo has no Wayland socket yet; see: ./dev.sh log"

    build_kit_apps "$_name"
    _bin="$_kit_bin/$_name"
    [ -x "$_bin" ] || fail "$EXIT_RUN_FAILED" "the app binary is missing: $_bin"

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

    _run=$(mktemp -d "${TMPDIR:-/tmp}/malaysia-vkms.XXXXXX")
    _log="$_run/malaysia-compositor.log"
    _pattern_log="$_run/pattern.log"
    _pidfile="$_run/malaysia-compositor.pid"
    _client_pidfile="$_run/pattern.pid"
    _malaysia_pid=
    _client_pid=

    cleanup() {
        _status=$?
        trap - EXIT HUP INT TERM
        if [ -n "${_client_pid:-}" ]; then
            sudo kill -TERM "$_client_pid" 2>/dev/null || true
        fi
        [ -n "${_malaysia_pid:-}" ] || _malaysia_pid=$(cat "$_pidfile" 2>/dev/null || true)
        if [ -n "${_malaysia_pid:-}" ]; then
            sudo kill -TERM "$_malaysia_pid" 2>/dev/null || true
            sleep 0.3
            sudo kill -KILL "$_malaysia_pid" 2>/dev/null || true
        fi
        sudo rm -rf "$_run" 2>/dev/null || rm -rf "$_run" 2>/dev/null || true
        exit "$_status"
    }
    trap cleanup EXIT HUP INT TERM

    lsmod | grep -q '^vkms ' ||
        fail "$EXIT_DRM_VKMS" "vkms is not loaded; run 'sudo modprobe vkms' first"

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

    cargo build -p malaysia-compositor -p sctk-probe --bin malaysia-compositor --bin pattern_live ||
        fail "$EXIT_DRM_VKMS" "could not build malaysia-compositor and pattern_live"
    _malaysia="$script_dir/target/debug/malaysia-compositor"
    _pattern="$script_dir/target/debug/pattern_live"

    # shellcheck disable=SC2024
    sudo env LIBSEAT_BACKEND=noop XDG_RUNTIME_DIR="$_run" \
        LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" \
        __EGL_VENDOR_LIBRARY_DIRS="${__EGL_VENDOR_LIBRARY_DIRS:-}" \
        LIBGL_DRIVERS_PATH="${LIBGL_DRIVERS_PATH:-}" \
        GBM_BACKENDS_PATH="${GBM_BACKENDS_PATH:-}" \
        sh -c 'printf "%s\n" "$$" >"$1"; shift; exec "$@"' sh "$_pidfile" \
        "$_malaysia" --drm --drm-device "$_vkms" --no-input --log-level=debug \
        >"$_log" 2>&1 &

    _tries=0
    while [ "$_tries" -lt 300 ] && [ ! -s "$_pidfile" ]; do
        _tries=$((_tries + 1))
        sleep 0.1
    done
    [ -s "$_pidfile" ] ||
        fail "$EXIT_DRM_VKMS" "malaysia-compositor did not start; log follows: $(tail -n 20 "$_log" 2>/dev/null)"
    _malaysia_pid=$(cat "$_pidfile")

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

    _drm_fds=$(sudo sh -c 'for fd in /proc/$1/fd/*; do readlink "$fd"; done' sh "$_malaysia_pid" |
        grep '^/dev/dri/' || true)
    printf '%s\n' "$_drm_fds" | grep -qx "$_vkms" ||
        fail "$EXIT_DRM_VKMS" "malaysia-compositor did not open the vkms card $_vkms"
    _other_card=$(printf '%s\n' "$_drm_fds" | grep '^/dev/dri/card' | grep -vx "$_vkms" || true)
    [ -z "$_other_card" ] ||
        fail "$EXIT_DRM_VKMS" "malaysia-compositor opened another DRM card: $_other_card"
    _input_fds=$(sudo sh -c 'for fd in /proc/$1/fd/*; do readlink "$fd"; done' sh "$_malaysia_pid" |
        grep '^/dev/input/' || true)
    [ -z "$_input_fds" ] ||
        fail "$EXIT_DRM_VKMS" "malaysia-compositor opened input devices: $_input_fds"

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
    sudo kill -USR1 "$_malaysia_pid" || fail "$EXIT_DRM_VKMS" "could not send SIGUSR1"

    _capture=
    _tries=0
    while [ "$_tries" -lt 50 ]; do
        _capture=$(find "$_run/malaysia" -name 'capture-*.png' 2>/dev/null | sort | tail -n 1)
        [ -n "$_capture" ] && break
        _tries=$((_tries + 1))
        sleep 0.1
    done
    [ -n "$_capture" ] ||
        fail "$EXIT_DRM_VKMS" "no capture written; log follows: $(tail -n 30 "$_log" 2>/dev/null)"
    sudo chmod -R a+rX "$_run"
    python3 "$script_dir/scripts/verify-captures.py" "$_capture" ||
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
}

wallpaper_check() {
    require_devshell
    require_vulkan
    if [ "$#" -ne 0 ]; then
        fail "$EXIT_USAGE" "unexpected argument for wallpaper-check: $1"
    fi

    cargo build -p malaysia-compositor -p malaysia-wallpaper ||
        fail "$EXIT_RUN_FAILED" "could not build wallpaper-check binaries"

    _wallpaper_bin="$script_dir/target/debug/malaysia-wallpaper"
    _config_dir=$(mktemp -d) ||
        fail "$EXIT_RUN_FAILED" "could not create the wallpaper-check config directory"
    _wallpaper_log="$RUNTIME_DIR/wallpaper-check-wallpaper.log"

    trap 'stop; rm -rf "$_config_dir"' EXIT INT TERM

    start
    if ! wait_socket; then
        fail "$EXIT_RUN_FAILED" "the compositor did not announce a socket; see: ./dev.sh log"
    fi

    env -u DISPLAY WAYLAND_DISPLAY="$_sock" XDG_SESSION_TYPE=wayland \
        MALAYSIA_CONFIG_DIR="$_config_dir" MALAYSIA_DATA_DIR="$_config_dir" \
        "$_wallpaper_bin" >"$_wallpaper_log" 2>&1 &
    _wallpaper_pid=$!

    _waited=0
    while [ "$_waited" -lt 100 ]; do
        if ! kill -0 "$_wallpaper_pid" 2>/dev/null; then
            fail "$EXIT_RUN_FAILED" \
                "malaysia-wallpaper exited before first frame; log follows: $(tail -n 20 "$_wallpaper_log" 2>/dev/null)"
        fi
        if grep -q "first frame presented" "$_wallpaper_log" 2>/dev/null; then
            break
        fi
        _waited=$((_waited + 1))
        sleep 0.1
    done
    if ! grep -q "first frame presented" "$_wallpaper_log" 2>/dev/null; then
        fail "$EXIT_RUN_FAILED" \
            "malaysia-wallpaper did not present a frame in 10 s; log follows: $(tail -n 20 "$_wallpaper_log" 2>/dev/null)"
    fi

    _compositor_pid=$(read_pid)
    _before_capture=$(mktemp) || fail "$EXIT_RUN_FAILED" "could not create capture reference file"
    kill -USR1 "$_compositor_pid" || fail "$EXIT_RUN_FAILED" "could not send SIGUSR1"

    _capture_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/malaysia"
    _capture=
    _waited=0
    while [ -z "$_capture" ] && [ "$_waited" -lt 100 ]; do
        _capture=$(find "$_capture_dir" -name 'capture-*.png' -newer "$_before_capture" 2>/dev/null | sort | tail -n 1)
        [ -n "$_capture" ] && break
        _waited=$((_waited + 1))
        sleep 0.1
    done
    rm -f "$_before_capture"
    [ -n "$_capture" ] ||
        fail "$EXIT_RUN_FAILED" "no capture written to $_capture_dir"

    MALAYSIA_WALLPAPER_CAPTURE="$_capture" cargo test -p malaysia-wallpaper -- --ignored -- --nocapture capture_matches_reference ||
        fail "$EXIT_RUN_FAILED" "capture_matches_reference failed"

    _alive=1
    _watched=0
    while [ "$_watched" -lt 30 ]; do
        if ! kill -0 "$_wallpaper_pid" 2>/dev/null; then
            _alive=0
            break
        fi
        _watched=$((_watched + 1))
        sleep 1
    done
    if [ "$_alive" -eq 0 ]; then
        _wp_exit=0
        wait "$_wallpaper_pid" 2>/dev/null || _wp_exit=$?
        _wp_signal=
        case "$_wp_exit" in
            129) _wp_signal=HUP ;;
            130) _wp_signal=INT ;;
            131) _wp_signal=QUIT ;;
            132) _wp_signal=ILL ;;
            133) _wp_signal=TRAP ;;
            134) _wp_signal=ABRT ;;
            136) _wp_signal=FPE ;;
            137) _wp_signal=KILL ;;
            139) _wp_signal=SEGV ;;
            143) _wp_signal=TERM ;;
        esac
        if [ -n "$_wp_signal" ]; then
            _wp_status="signal $_wp_signal (exit $_wp_exit)"
        else
            _wp_status="exit code $_wp_exit"
        fi
        fail "$EXIT_RUN_FAILED" \
            "malaysia-wallpaper died during wallpaper-check ($_wp_status); log tail:\n$(tail -n 40 "$_wallpaper_log" 2>/dev/null)"
    fi

    printf 'wallpaper-check: PASS\n'
    printf '  capture: %s\n' "$_capture"
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
    wallpaper-check) wallpaper_check "$@" ;;
    kit-local) kit_local ;;
    kit-remote) kit_remote ;;
    smoke) smoke "$@" ;;
    block4) block4 "$@" ;;
    move) move "$@" ;;
    pointers) pointers "$@" ;;
    screencopy) screencopy "$@" ;;
    screencopy-window) screencopy_window "$@" ;;
    presentation) presentation "$@" ;;
    frames) frames "$@" ;;
    activation) activation "$@" ;;
    titlebar) titlebar "$@" ;;
    decoration) decoration "$@" ;;
    demo) demo "$@" ;;
    video) video "$@" ;;
    lab) lab "$@" ;;
    app) app "$@" ;;
    drm-vkms) drm_vkms "$@" ;;
    -h | --help) usage ;;
    *)
        usage >&2
        fail "$EXIT_USAGE" "unknown command: $command_name"
        ;;
esac
