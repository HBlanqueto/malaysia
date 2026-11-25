#!/bin/sh
set -eu
export PATH=/opt/podman/bin:$PATH
NAME=malaysia-dev
REPO=/Users/humbe/src/malaysia
PROFILE=/root/.nix-profile

in_box() { podman exec -w /work "$NAME" "$@"; }

up() {
	podman machine list --format '{{.Running}}' | grep -q true || podman machine start
	podman machine ssh 'sudo modprobe vkms enable_writeback=1 enable_cursor=1 enable_overlay=1; until [ -e /dev/dri/card0 ]; do sleep 0.2; done; sudo chmod 666 /dev/dri/card0'
	podman container exists "$NAME" || podman run -d --init --name "$NAME" -p 127.0.0.1:5901:5900 --device /dev/dri/card0 \
		-v "$REPO":/work -v malaysia-nix:/nix -v malaysia-cache:/cache -w /work \
		-e CARGO_TARGET_DIR=/cache/target -e LIBGL_ALWAYS_SOFTWARE=1 -e LIBSEAT_BACKEND=noop \
		-e NIX_CONFIG="experimental-features = nix-command flakes" \
		-e XDG_RUNTIME_DIR=/run/user/0 -e WAYLAND_DISPLAY=wayland-1 \
		-e __EGL_VENDOR_LIBRARY_FILENAMES=$PROFILE/share/glvnd/egl_vendor.d/50_mesa.json \
		-e LIBGL_DRIVERS_PATH=$PROFILE/lib/dri \
		docker.io/nixos/nix sleep infinity
	podman start "$NAME" >/dev/null
	in_box sh -c "command -v sway >/dev/null" || in_box nix profile install \
		nixpkgs#sway nixpkgs#wayvnc nixpkgs#mesa nixpkgs#dbus nixpkgs#procps \
		nixpkgs#foot nixpkgs#wayland-utils nixpkgs#mesa-demos
	in_box sh -c "command -v weston-simple-shm >/dev/null" || in_box nix profile add --impure --expr \
		'with import (builtins.getFlake "nixpkgs") {}; weston.overrideAttrs (old: { mesonFlags = lib.remove "-Dsimple-clients=" old.mesonFlags ++ ["-Dsimple-clients=shm"]; })'
	in_box pgrep -x sway >/dev/null || podman exec -d -e DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/0/bus \
		-e WLR_BACKENDS=headless -e WLR_RENDERER=pixman -e WLR_LIBINPUT_NO_DEVICES=1 "$NAME" \
		sh -c 'mkdir -p -m700 /run/user/0; exec sway >/tmp/sway.log 2>&1'
	until in_box test -S /run/user/0/wayland-1; do sleep 1; done
	in_box pgrep -x wayvnc >/dev/null || podman exec -d "$NAME" sh -c 'exec wayvnc 0.0.0.0 5900 >/tmp/wayvnc.log 2>&1'
	echo "vnc://127.0.0.1:5901"
}

shot() {
	podman machine ssh 'mkdir -p /tmp/shot && rm -f /tmp/shot/shot.png && sudo podman run --rm --privileged --device /dev/dri/card0 -v malaysia-shot-nix:/nix -v malaysia-shot-cache:/root/.cache -v /tmp/shot:/out docker.io/nixos/nix nix --extra-experimental-features "nix-command flakes" shell nixpkgs#ffmpeg-headless -c sh -c "for f in bgra bgr0; do ffmpeg -y -loglevel quiet -device /dev/dri/card0 -f kmsgrab -i - -frames:v 1 -vf hwdownload,format=\$f,format=rgb24 /out/shot.png && break; done"'
	podman machine ssh 'cat /tmp/shot/shot.png' >"$1"
}

case "${1:-}" in
up) up ;;
shot) shot "${2:?usage: $0 shot <file.png>}" ;;
test)
	mkdir -p "${2:?usage: $0 test <dir>}"
	in_box nix build --option extra-system-features kvm path:.#checks.$(in_box uname -m)-linux.vm -L -o /tmp/vm
	for png in $(in_box sh -c 'cd /tmp/vm && ls *.png'); do in_box cat "/tmp/vm/$png" >"$2/$png"; done
	;;
sh) podman exec -it -w /work "$NAME" sh ;;
run) shift; podman exec -w /work "$NAME" "$@" ;;
down) podman stop "$NAME"; podman machine stop ;;
*) echo "usage: $0 up|sh|run <cmd...>|shot <file.png>|test <dir>|down" >&2; exit 2 ;;
esac
