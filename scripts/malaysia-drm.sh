#!/usr/bin/env bash

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULT="$REPO/result"
RUNDIR="${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR no está definido}/malaysia"
LATEST_LOG="$RUNDIR/drm-latest.log"
PROC_NAME=".malaysia-compositor-wrapped"

die()  { printf 'malaysia-drm: %s\n' "$*" >&2; exit 1; }
info() { printf 'malaysia-drm: %s\n' "$*"; }

malaysia_compositor_pid() { pgrep -x "$PROC_NAME" | head -n1 || true; }

socket_name() {
    [[ -f "$LATEST_LOG" ]] || return 1
    grep -oE 'socket=wayland-[0-9]+' "$LATEST_LOG" | tail -n1 | cut -d= -f2
}

need_result() {
    [[ -x "$RESULT/bin/malaysia-compositor" ]] || die "no existe $RESULT/bin/malaysia-compositor; ejecuta primero: $0 setup"
}

cmd_setup() {
    cd "$REPO"
    info "compilando el paquete (nix build .#default)…"
    nix build .#default

    local units="$RESULT/lib/systemd/user"
    [[ -f "$units/malaysia-session.target" ]] || die "no encuentro las units en $units"

    info "enlazando las units de la shell…"
    local user_units="$HOME/.config/systemd/user"
    for unit in malaysia-wallpaper.service malaysia-shell@menubar.service malaysia-shell@dock.service; do
        rm -f "$user_units/$unit" "$user_units/malaysia-session.target.wants/$unit"
    done
    systemctl --user daemon-reload
    systemctl --user link --force "$units/malaysia-session.target"
    systemctl --user link --force "$units/malaysia-shell@.service"
    systemctl --user link --force "$units/malaysia-wallpaper.service"
    systemctl --user daemon-reload
    systemctl --user enable \
        malaysia-wallpaper.service malaysia-shell@menubar.service malaysia-shell@dock.service

    info "listo. Ahora: Ctrl+Alt+F3 (emergencia) y Ctrl+Alt+F4 → $0 run"
}

cmd_run() {
    need_result
    [[ -z "${MALAYSIA_DEV_SHELL:-}" ]] || die "estás dentro de nix develop; sal y usa el binario empaquetado"
    if ! tty | grep -qE '^/dev/tty[0-9]+$'; then
        die "ejecútalo desde un TTY de texto (Ctrl+Alt+F4), no desde una terminal gráfica"
    fi
    [[ -z "$(malaysia_compositor_pid)" ]] || die "ya hay un Malaysia corriendo (pid $(malaysia_compositor_pid)); usa: $0 stop"

    mkdir -p "$RUNDIR"
    local log
    log="$RUNDIR/drm-$(date +%Y%m%d-%H%M%S).log"
    ln -sfn "$log" "$LATEST_LOG"

    info "log: $log"
    info "emergencia (desde otro TTY): $0 stop"
    sleep 1

    cd "$REPO"
    set +e
    "$RESULT/bin/malaysia-compositor" --drm --log-level="${MALAYSIA_LOG_LEVEL:-info}" 2>&1 | tee "$log"
    local status=${PIPESTATUS[0]}
    set -e
    info "Malaysia salió con código $status (log: $log)"
    return "$status"
}

cmd_client() {
    [[ $# -gt 0 ]] || die "uso: $0 client <programa> [args…]"
    [[ -n "$(malaysia_compositor_pid)" ]] || die "Malaysia no está corriendo"
    local sock
    sock="$(socket_name)" || die "no encuentro el socket en $LATEST_LOG"
    [[ -S "$XDG_RUNTIME_DIR/$sock" ]] || die "el socket $sock no existe todavía"

    local name; name="$(basename "$1")"
    local clog="$RUNDIR/client-$name.log"
    info "lanzando '$*' en $sock (log: $clog)"
    env -u DISPLAY WAYLAND_DISPLAY="$sock" XDG_SESSION_TYPE=wayland \
        setsid -f "$@" >"$clog" 2>&1 </dev/null
    info "vuelve al TTY de Malaysia (Ctrl+Alt+F4)"
}

cmd_lock() {
    need_result
    cmd_client "$RESULT/bin/malaysia-shell" lock
}

cmd_status() {
    local pid; pid="$(malaysia_compositor_pid)"
    if [[ -n "$pid" ]]; then info "Malaysia corriendo, pid $pid"; else info "Malaysia no está corriendo"; fi
    if sock="$(socket_name)"; then info "socket: $sock"; fi
    if [[ -f "$LATEST_LOG" ]]; then
        info "log: $(readlink -f "$LATEST_LOG")"
        tail -n 15 "$LATEST_LOG"
    fi
}

cmd_stop() {
    local pid; pid="$(malaysia_compositor_pid)"
    [[ -n "$pid" ]] || { info "Malaysia no está corriendo"; return 0; }
    info "enviando SIGTERM a $pid…"
    pkill -TERM -x "$PROC_NAME"
    for _ in $(seq 1 50); do
        [[ -z "$(malaysia_compositor_pid)" ]] && { info "Malaysia terminó. Vuelve a GDM con Ctrl+Alt+F1."; return 0; }
        sleep 0.1
    done
    die "Malaysia no terminó en 5 s; revisa con: $0 status (y si hace falta, sudo systemctl restart display-manager)"
}

case "${1:-}" in
    setup)  shift; cmd_setup "$@" ;;
    run)    shift; cmd_run "$@" ;;
    client) shift; cmd_client "$@" ;;
    lock)   shift; cmd_lock "$@" ;;
    status) shift; cmd_status "$@" ;;
    stop)   shift; cmd_stop "$@" ;;
    *) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
