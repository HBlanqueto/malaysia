#!/usr/bin/env bash
# yuki-drm.sh — lanzar la sesión directa (Bloque 8a) sobre el panel real.
#
# Uso (desde la raíz del repo):
#   scripts/yuki-drm.sh setup          # desde GNOME: build + units de la shell
#   scripts/yuki-drm.sh run            # desde un TTY: arranca yuki --drm
#   scripts/yuki-drm.sh client wezterm # desde otro TTY: abre un cliente en Yuki
#   scripts/yuki-drm.sh lock           # desde otro TTY: bloquea la sesión
#   scripts/yuki-drm.sh status         # pid, socket y últimas líneas del log
#   scripts/yuki-drm.sh stop           # mata Yuki (sirve con la sesión bloqueada)
#
# Regla de entorno: solo escribe en el repo y en $XDG_RUNTIME_DIR/yuki.
# No toca ~/nixdots, GDM ni sockets ajenos.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULT="$REPO/result"
RUNDIR="${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR no está definido}/yuki"
LATEST_LOG="$RUNDIR/drm-latest.log"
PROC_NAME=".yuki-wrapped"   # comm del binario envuelto por Nix

die()  { printf 'yuki-drm: %s\n' "$*" >&2; exit 1; }
info() { printf 'yuki-drm: %s\n' "$*"; }

yuki_pid() { pgrep -x "$PROC_NAME" | head -n1 || true; }

socket_name() {
    [[ -f "$LATEST_LOG" ]] || return 1
    grep -oE 'socket=wayland-[0-9]+' "$LATEST_LOG" | tail -n1 | cut -d= -f2
}

need_result() {
    [[ -x "$RESULT/bin/yuki" ]] || die "no existe $RESULT/bin/yuki; ejecuta primero: $0 setup"
}

cmd_setup() {
    cd "$REPO"
    info "compilando el paquete (nix build .#default)…"
    nix build .#default

    local units="$RESULT/lib/systemd/user"
    [[ -f "$units/yuki-session.target" ]] || die "no encuentro las units en $units"

    info "enlazando las units de la shell…"
    systemctl --user link --force "$units/yuki-session.target"
    systemctl --user link --force "$units/yuki-shell@.service"
    systemctl --user daemon-reload
    # Sin --now: no deben arrancar dentro de GNOME. Las arranca Yuki con el target.
    systemctl --user enable \
        yuki-shell@wallpaper.service yuki-shell@menubar.service yuki-shell@dock.service

    info "listo. Ahora: Ctrl+Alt+F3 (emergencia) y Ctrl+Alt+F4 → $0 run"
}

cmd_run() {
    need_result
    [[ -z "${YUKI_DEV_SHELL:-}" ]] || die "estás dentro de nix develop; sal y usa el binario empaquetado"
    if ! tty | grep -qE '^/dev/tty[0-9]+$'; then
        die "ejecútalo desde un TTY de texto (Ctrl+Alt+F4), no desde una terminal gráfica"
    fi
    [[ -z "$(yuki_pid)" ]] || die "ya hay un Yuki corriendo (pid $(yuki_pid)); usa: $0 stop"

    mkdir -p "$RUNDIR"
    local log
    log="$RUNDIR/drm-$(date +%Y%m%d-%H%M%S).log"
    ln -sfn "$log" "$LATEST_LOG"

    info "log: $log"
    info "emergencia (desde otro TTY): $0 stop"
    sleep 1

    cd "$REPO"
    set +e
    "$RESULT/bin/yuki" --drm --log-level="${YUKI_LOG_LEVEL:-info}" 2>&1 | tee "$log"
    local status=${PIPESTATUS[0]}
    set -e
    info "Yuki salió con código $status (log: $log)"
    return "$status"
}

cmd_client() {
    [[ $# -gt 0 ]] || die "uso: $0 client <programa> [args…]"
    [[ -n "$(yuki_pid)" ]] || die "Yuki no está corriendo"
    local sock
    sock="$(socket_name)" || die "no encuentro el socket en $LATEST_LOG"
    [[ -S "$XDG_RUNTIME_DIR/$sock" ]] || die "el socket $sock no existe todavía"

    local name; name="$(basename "$1")"
    local clog="$RUNDIR/client-$name.log"
    info "lanzando '$*' en $sock (log: $clog)"
    # Entorno limpio de display: que el cliente no intente hablar con X ni con GNOME.
    env -u DISPLAY WAYLAND_DISPLAY="$sock" XDG_SESSION_TYPE=wayland \
        setsid -f "$@" >"$clog" 2>&1 </dev/null
    info "vuelve al TTY de Yuki (Ctrl+Alt+F4)"
}

cmd_lock() {
    need_result
    cmd_client "$RESULT/bin/yuki-shell" lock
}

cmd_status() {
    local pid; pid="$(yuki_pid)"
    if [[ -n "$pid" ]]; then info "Yuki corriendo, pid $pid"; else info "Yuki no está corriendo"; fi
    if sock="$(socket_name)"; then info "socket: $sock"; fi
    if [[ -f "$LATEST_LOG" ]]; then
        info "log: $(readlink -f "$LATEST_LOG")"
        tail -n 15 "$LATEST_LOG"
    fi
}

cmd_stop() {
    local pid; pid="$(yuki_pid)"
    [[ -n "$pid" ]] || { info "Yuki no está corriendo"; return 0; }
    info "enviando SIGTERM a $pid…"
    pkill -TERM -x "$PROC_NAME"
    for _ in $(seq 1 50); do
        [[ -z "$(yuki_pid)" ]] && { info "Yuki terminó. Vuelve a GDM con Ctrl+Alt+F1."; return 0; }
        sleep 0.1
    done
    die "Yuki no terminó en 5 s; revisa con: $0 status (y si hace falta, sudo systemctl restart display-manager)"
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
