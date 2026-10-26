#!/usr/bin/env bash
# malaysia-drm.sh — lanzar la sesión directa (Bloque 8a) sobre el panel real.
#
# Uso (desde la raíz del repo):
#   scripts/malaysia-drm.sh setup          # desde GNOME: build + units de la shell
#   scripts/malaysia-drm.sh run            # desde un TTY: arranca malaysia-compositor --drm
#   scripts/malaysia-drm.sh client wezterm # desde otro TTY: abre un cliente en MalaysiaDreams
#   scripts/malaysia-drm.sh lock           # desde otro TTY: bloquea la sesión
#   scripts/malaysia-drm.sh status         # pid, socket y últimas líneas del log
#   scripts/malaysia-drm.sh stop           # mata MalaysiaDreams (sirve con la sesión bloqueada)
#
# Regla de entorno: solo escribe en el repo y en $XDG_RUNTIME_DIR/malaysia.
# No toca ~/nixdots, GDM ni sockets ajenos.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULT="$REPO/result"
RUNDIR="${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR no está definido}/malaysia"
LATEST_LOG="$RUNDIR/drm-latest.log"
PROC_NAME=".malaysia-compositor-wrapped"   # comm del binario envuelto por Nix

die()  { printf 'malaysia-drm: %s\n' "$*" >&2; exit 1; }
info() { printf 'malaysia-drm: %s\n' "$*"; }

malaysia_compositor_pid() { pgrep -x "$PROC_NAME" | head -n1 || true; }

socket_name() {
    [[ -f "$LATEST_LOG" ]] || return 1
    grep -oE 'socket=wayland-[0-9]+' "$LATEST_LOG" | tail -n1 | cut -d= -f2
}

need_result() {
    [[ -x "$RESULT/bin/malaysia" ]] || die "no existe $RESULT/bin/malaysia; ejecuta primero: $0 setup"
}

cmd_setup() {
    cd "$REPO"
    info "compilando el paquete (nix build .#default)…"
    nix build .#default

    local units="$RESULT/lib/systemd/user"
    [[ -f "$units/malaysia-session.target" ]] || die "no encuentro las units en $units"

    info "enlazando las units de la shell…"
    local user_units="$HOME/.config/systemd/user"
    # `enable` pins each instance to the store path of the template it found.
    # After a rebuild that path is gone and the old links make the units "bad",
    # so `enable` fails. Drop them (and their wants links) before re-linking.
    for unit in malaysia-shell@wallpaper.service malaysia-shell@menubar.service malaysia-shell@dock.service; do
        rm -f "$user_units/$unit" "$user_units/malaysia-session.target.wants/$unit"
    done
    systemctl --user daemon-reload
    systemctl --user link --force "$units/malaysia-session.target"
    systemctl --user link --force "$units/malaysia-shell@.service"
    systemctl --user daemon-reload
    # Sin --now: no deben arrancar dentro de GNOME. Las arranca MalaysiaDreams con el target.
    systemctl --user enable \
        malaysia-shell@wallpaper.service malaysia-shell@menubar.service malaysia-shell@dock.service

    info "listo. Ahora: Ctrl+Alt+F3 (emergencia) y Ctrl+Alt+F4 → $0 run"
}

cmd_run() {
    need_result
    [[ -z "${MALAYSIA_DEV_SHELL:-}" ]] || die "estás dentro de nix develop; sal y usa el binario empaquetado"
    if ! tty | grep -qE '^/dev/tty[0-9]+$'; then
        die "ejecútalo desde un TTY de texto (Ctrl+Alt+F4), no desde una terminal gráfica"
    fi
    [[ -z "$(malaysia_compositor_pid)" ]] || die "ya hay un MalaysiaDreams corriendo (pid $(malaysia_compositor_pid)); usa: $0 stop"

    mkdir -p "$RUNDIR"
    local log
    log="$RUNDIR/drm-$(date +%Y%m%d-%H%M%S).log"
    ln -sfn "$log" "$LATEST_LOG"

    info "log: $log"
    info "emergencia (desde otro TTY): $0 stop"
    sleep 1

    cd "$REPO"
    set +e
    "$RESULT/bin/malaysia" --drm --log-level="${MALAYSIA_LOG_LEVEL:-info}" 2>&1 | tee "$log"
    local status=${PIPESTATUS[0]}
    set -e
    info "MalaysiaDreams salió con código $status (log: $log)"
    return "$status"
}

cmd_client() {
    [[ $# -gt 0 ]] || die "uso: $0 client <programa> [args…]"
    [[ -n "$(malaysia_compositor_pid)" ]] || die "MalaysiaDreams no está corriendo"
    local sock
    sock="$(socket_name)" || die "no encuentro el socket en $LATEST_LOG"
    [[ -S "$XDG_RUNTIME_DIR/$sock" ]] || die "el socket $sock no existe todavía"

    local name; name="$(basename "$1")"
    local clog="$RUNDIR/client-$name.log"
    info "lanzando '$*' en $sock (log: $clog)"
    # Entorno limpio de display: que el cliente no intente hablar con X ni con GNOME.
    env -u DISPLAY WAYLAND_DISPLAY="$sock" XDG_SESSION_TYPE=wayland \
        setsid -f "$@" >"$clog" 2>&1 </dev/null
    info "vuelve al TTY de MalaysiaDreams (Ctrl+Alt+F4)"
}

cmd_lock() {
    need_result
    cmd_client "$RESULT/bin/malaysia-shell" lock
}

cmd_status() {
    local pid; pid="$(malaysia_compositor_pid)"
    if [[ -n "$pid" ]]; then info "MalaysiaDreams corriendo, pid $pid"; else info "MalaysiaDreams no está corriendo"; fi
    if sock="$(socket_name)"; then info "socket: $sock"; fi
    if [[ -f "$LATEST_LOG" ]]; then
        info "log: $(readlink -f "$LATEST_LOG")"
        tail -n 15 "$LATEST_LOG"
    fi
}

cmd_stop() {
    local pid; pid="$(malaysia_compositor_pid)"
    [[ -n "$pid" ]] || { info "MalaysiaDreams no está corriendo"; return 0; }
    info "enviando SIGTERM a $pid…"
    pkill -TERM -x "$PROC_NAME"
    for _ in $(seq 1 50); do
        [[ -z "$(malaysia_compositor_pid)" ]] && { info "MalaysiaDreams terminó. Vuelve a GDM con Ctrl+Alt+F1."; return 0; }
        sleep 0.1
    done
    die "MalaysiaDreams no terminó en 5 s; revisa con: $0 status (y si hace falta, sudo systemctl restart display-manager)"
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
