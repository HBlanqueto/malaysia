{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [
        "aarch64-linux"
        "x86_64-linux"
      ];
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "malaysia-compositor";
            version = "0.1.0";
            src = pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = pkgs.lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./crates
              ];
            };
            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };
            nativeBuildInputs = [ pkgs.pkg-config ];
            postInstall = ''
              install -Dm644 crates/malaysia-compositor/malaysia-expansion.desktop -t $out/share/applications
            '';
            buildInputs = with pkgs; [
              libxkbcommon
              libgbm
              libglvnd
              libinput
              seatd
              udev
              wayland
            ];
            RUSTFLAGS = map (arg: "-C link-arg=${arg}") [
              "-Wl,--push-state,--no-as-needed"
              "-lEGL"
              "-lwayland-client"
              "-Wl,--pop-state"
            ];
          };
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          machineConfig =
            { lib, ... }:
            {
              users.users.alice = {
                isNormalUser = true;
                uid = 1000;
              };
              services.getty.autologinUser = "alice";
              programs.bash.loginShellInit = ''
                if [ "$(tty)" = /dev/tty1 ]; then
                  malaysia-compositor >/tmp/malaysia.log 2>&1; echo $? >/tmp/exit
                fi
              '';
              hardware.graphics.enable = true;
              security.polkit.enable = true;
              fonts.enableDefaultPackages = true;
              environment.systemPackages = [
                self.packages.${system}.default
                pkgs.foot
                pkgs.mousepad
              ];
              virtualisation.qemu.options = [
                "-vga none"
                "-device virtio-gpu-pci"
                "-device virtio-mouse-pci"
              ];
              virtualisation.memorySize = 2048;
              virtualisation.useNixStoreImage = true;
              virtualisation.writableStore = false;
              virtualisation.sharedDirectories = lib.mkForce { };
            };
          hidpiEdid = pkgs.runCommand "hidpi-edid" { } ''
            mkdir -p $out/lib/firmware/edid
            ${pkgs.qemu_test}/bin/qemu-edid -x 1600 -y 900 -d 225 -o $out/lib/firmware/edid/hidpi.bin
          '';
        in
        {
          vm = pkgs.testers.runNixOSTest {
            name = "malaysia-drm";
            enableOCR = true;
            nodes.machine = machineConfig;
            nodes.hidpi = {
              imports = [ machineConfig ];
              hardware.display.edid.packages = [ hidpiEdid ];
              hardware.display.outputs."Virtual-1".edid = "hidpi.bin";
              boot.initrd.extraFirmwarePaths = [ "edid/hidpi.bin" ];
              virtualisation.qemu.options = [
                "-global virtio-gpu-device.xres=1600"
                "-global virtio-gpu-device.yres=900"
              ];
            };
            testScript =
              let
                typist = pkgs.writeShellScript "typist" ''
                  echo ready
                  touch /tmp/ready
                  read -r line
                  echo "$line" >/tmp/typed1
                  read -r line
                  echo "$line" >/tmp/typed2
                  sleep 600
                '';
              in
              ''
                import subprocess
                import time
                from typing import Any

                def screen(name, node=machine):
                    node.screenshot(name)
                    ppm = subprocess.run(f"pngtopnm {node.out_dir}/{name}.png | ppmtoppm", shell=True, capture_output=True, check=True).stdout
                    _, size, _, data = ppm.split(b"\n", 3)
                    return int(size.split()[0]), data

                def bounds(points):
                    xs = [x for x, _ in points]
                    ys = [y for _, y in points]
                    return min(xs), min(ys), max(xs) - min(xs) + 1, max(ys) - min(ys) + 1

                def white_points(width, data):
                    return [(i // 3 % width, i // 3 // width) for i in range(0, len(data), 3) if data[i:i + 3] == b"\xff\xff\xff"]

                def white(name, node=machine):
                    return white_points(*screen(name, node))

                def has_fallback_cursor(points):
                    lit = set(points)
                    return any(
                        {(x - 1, y), (x, y - 1), (x + 16, y), (x, y + 16)}.isdisjoint(lit)
                        and all((x + dx, y + dy) in lit for dx in range(16) for dy in range(16))
                        for x, y in lit
                    )

                def point_at(x, y):
                    events: Any = [
                        {"type": "abs", "data": {"axis": "x", "value": round(x * 0x7fff / width)}},
                        {"type": "abs", "data": {"axis": "y", "value": round(y * 0x7fff / height)}}]
                    qmp = machine.qmp_client
                    assert qmp
                    qmp.send("input-send-event", {"events": events})
                    time.sleep(2)

                def click():
                    machine.send_monitor_command("mouse_button 1")
                    machine.send_monitor_command("mouse_button 0")

                try:
                    machine.wait_for_file("/run/user/1000/wayland-1", timeout=600)
                    machine.wait_until_succeeds("[ $(grep -c 'New device' /tmp/malaysia.log) -ge $(ls /dev/input/event* | wc -l) ]", timeout=120)
                    machine.send_monitor_command("mouse_move 300 300")
                    for _ in range(60):
                        fallback = white("cursor-fallback")
                        if fallback and bounds(fallback)[:2] != (0, 0):
                            break
                        time.sleep(1)
                    print("fallback", bounds(fallback), len(fallback))
                    assert bounds(fallback)[2:] == (16, 16) and len(fallback) == 256 and bounds(fallback)[:2] != (0, 0)
                    machine.succeed("su - alice -c 'XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 WAYLAND_DEBUG=1 foot -o font=monospace:size=24 ${typist}' >/tmp/foot.log 2>&1 &")
                    machine.wait_for_file("/tmp/ready", timeout=300)
                    machine.wait_for_text("ready", timeout=300)
                    machine.send_chars("malaysia\n")
                    machine.wait_until_succeeds("[ \"$(cat /tmp/typed1)\" = malaysia ]", timeout=120)
                    machine.screenshot("typed")
                    machine.send_key("ctrl-alt-f2")
                    machine.wait_until_succeeds("[ $(fgconsole) = 2 ]", timeout=60)
                    machine.wait_until_succeeds("grep -q PauseSession /tmp/malaysia.log", timeout=60)
                    machine.succeed("chvt 1")
                    machine.wait_until_succeeds("grep -q ActivateSession /tmp/malaysia.log", timeout=60)
                    machine.wait_until_succeeds("[ $(sed -n '/ActivateSession/,$p' /tmp/malaysia.log | grep -c 'New device') -ge $(grep -c 'Removed device' /tmp/malaysia.log) ]", timeout=60)
                    machine.send_chars("again\n")
                    machine.wait_until_succeeds("[ \"$(cat /tmp/typed2)\" = again ]", timeout=120)
                    machine.screenshot("resumed")
                    machine.send_monitor_command("mouse_move -5000 -5000")
                    machine.send_monitor_command("mouse_move 200 200")
                    machine.wait_until_succeeds("grep -q 'wl_pointer#[0-9]*\\.enter' /tmp/foot.log", timeout=60)
                    machine.wait_until_succeeds("grep -q 'wl_pointer#[0-9]*\\.set_cursor' /tmp/foot.log", timeout=60)
                    for _ in range(60):
                        width, client = screen("cursor-client")
                        if not has_fallback_cursor(white_points(width, client)):
                            break
                        time.sleep(1)
                    machine.send_monitor_command("mouse_move 5000 5000")
                    machine.wait_until_succeeds("grep -q 'wl_pointer#[0-9]*\\.leave' /tmp/foot.log", timeout=60)
                    for _ in range(60):
                        _, away = screen("cursor-away")
                        if away[-3:] == b"\xff\xff\xff":
                            break
                        time.sleep(1)
                    corner = len(client) - 3
                    changed = [(i // 3 % width, i // 3 // width) for i in range(0, corner, 3) if client[i:i + 3] != away[i:i + 3]]
                    print("client cursor", bounds(changed) if changed else None, len(changed))
                    assert changed and max(bounds(changed)[2:]) <= 64
                    assert any(client[3 * (y * width + x):3 * (y * width + x) + 3] != b"\xff\xff\xff" for x, y in changed)
                    machine.succeed("su - alice -c 'XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 GDK_BACKEND=wayland mousepad >/tmp/mousepad.log 2>&1 &'")
                    machine.wait_for_text("Mousepad", timeout=300)
                    machine.send_key("alt-f")
                    machine.wait_for_text("Save ?As", timeout=300)
                    machine.screenshot("menu")
                    machine.send_key("esc")
                    machine.send_chars("zebra")
                    machine.wait_for_text("zebra", timeout=300)
                    machine.screenshot("menu-closed")
                    machine.send_key("f11")
                    time.sleep(3)
                    _, data = screen("full-screen")
                    assert data[:3] != bytes([25, 25, 31]), "the editor does not fill the display"
                    machine.send_key("ctrl-alt-f")
                    time.sleep(3)
                    _, data = screen("left-full-screen")
                    assert data[:3] == bytes([25, 25, 31]), "the editor did not leave full screen"
                    machine.succeed("${pkgs.desktop-file-utils}/bin/desktop-file-validate /run/current-system/sw/share/applications/malaysia-expansion.desktop")
                    toggle = "su - alice -c 'XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 malaysia-expansion'"
                    machine.succeed(toggle)
                    time.sleep(2)
                    _, data = screen("expansion-by-request")
                    assert data[:3] == bytes([33, 33, 39]), "malaysia-expansion did not open Expansion"
                    machine.succeed(toggle)
                    time.sleep(2)
                    _, data = screen("expansion-closed-by-request")
                    assert data[:3] == bytes([25, 25, 31]), "malaysia-expansion did not close Expansion"
                    machine.send_key("ctrl-alt-right")
                    time.sleep(3)
                    width, data = screen("split-view")
                    height = len(data) // 3 // width
                    middle = 3 * (height // 2 * width + width // 2)
                    assert data[middle:middle + 3] == bytes([0, 0, 0]), "no black divider in the middle of a Split View"
                    point_at(width // 4, height // 2)
                    click()
                    time.sleep(1)
                    machine.send_key("ctrl-alt-f")
                    time.sleep(3)
                    machine.send_key("ctrl-right")
                    time.sleep(2)
                    machine.send_key("ctrl-alt-f")
                    time.sleep(3)
                    _, data = screen("split-view-left")
                    assert data[:3] == bytes([25, 25, 31]), "Split View did not end"
                    machine.send_key("ctrl-up")
                    time.sleep(2)
                    width, data = screen("strip")
                    height = len(data) // 3 // width
                    assert data[:3] == bytes([33, 33, 39]), f"the strip is not the blurred Desktop under its veil: {list(data[:3])}"
                    point_at(width // 2, 20)
                    point_at(width - 27, 77)
                    click()
                    time.sleep(2)
                    _, data = screen("strip-added")
                    left = round(width / 2 + 16)
                    under_second_thumbnail = [3 * (y * width + x) for y in range(120, 146) for x in range(left, left + 90 * width // height)]
                    assert any(min(data[i:i + 3]) >= 200 for i in under_second_thumbnail), "no name under the second thumbnail"
                    point_at(round(width / 2 + 16 + 45 * width / height), 74)
                    click()
                    for _ in range(60):
                        if "zebra" not in machine.get_screen_text():
                            break
                        time.sleep(1)
                    else:
                        raise AssertionError("the first Desktop is still shown")
                    time.sleep(2)
                    machine.screenshot("second-desktop")
                    machine.send_key("ctrl-left")
                    machine.wait_for_text("zebra", timeout=300)
                    machine.send_key("ctrl-alt-backspace")
                    machine.wait_until_fails("pgrep -x malaysia-compos", timeout=60)
                    machine.wait_for_file("/tmp/exit", timeout=60)
                    machine.send_chars("tty >/tmp/tty\n")
                    machine.wait_until_succeeds("grep -qx /dev/tty1 /tmp/tty", timeout=60)
                    machine.screenshot("text-vt")
                finally:
                    print(machine.execute("cat /tmp/malaysia.log /tmp/typed1 /tmp/typed2; grep -c wl_pointer /tmp/foot.log; grep -m5 'wl_pointer#[0-9]*\\.\\(enter\\|motion\\)' /tmp/foot.log")[1])
                compositor_log = machine.succeed("cat /tmp/malaysia.log")
                assert machine.succeed("cat /tmp/exit").strip() == "0"
                assert machine.succeed("fgconsole").strip() == "1"
                assert "drm backend path=/dev/dri/card0" in compositor_log
                assert "scale=1.0" in compositor_log
                for line in compositor_log.splitlines():
                    assert "ERROR" not in line, line
                machine.shutdown()

                try:
                    hidpi.wait_for_file("/run/user/1000/wayland-1", timeout=600)
                    hidpi.succeed("grep -q 'mode chosen.*width=1600 height=900.*scale=1.6666666666666667' /tmp/malaysia.log")
                    hidpi.wait_until_succeeds("[ $(grep -c 'New device' /tmp/malaysia.log) -ge $(ls /dev/input/event* | wc -l) ]", timeout=120)
                    hidpi.send_monitor_command("mouse_move 300 300")
                    for _ in range(60):
                        fallback = white("hidpi-cursor-fallback", hidpi)
                        if fallback and bounds(fallback)[:2] != (0, 0):
                            break
                        time.sleep(1)
                    print("hidpi fallback", bounds(fallback), len(fallback))
                    assert bounds(fallback)[2:] == (27, 27) and len(fallback) == 729
                    hidpi.send_monitor_command("mouse_move 5000 5000")
                    for _ in range(60):
                        corner = white("hidpi-corner", hidpi)
                        if corner and bounds(corner)[:2] == (1598, 898):
                            break
                        time.sleep(1)
                    print("hidpi corner", bounds(corner), len(corner))
                    assert bounds(corner) == (1598, 898, 2, 2) and len(corner) == 4
                    hidpi.succeed("su - alice -c 'XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 WAYLAND_DEBUG=1 foot -o font=monospace:size=24 ${typist}' >/tmp/foot.log 2>&1 &")
                    hidpi.wait_for_file("/tmp/ready", timeout=300)
                    hidpi.send_chars("malaysia\n")
                    hidpi.wait_until_succeeds("[ \"$(cat /tmp/typed1)\" = malaysia ]", timeout=120)
                    hidpi.wait_for_text("malaysia", timeout=300)
                    hidpi.screenshot("hidpi-typed")
                    hidpi.succeed("grep -q 'wl_output#[0-9]*\\.scale(2)' /tmp/foot.log")
                    hidpi.succeed("grep -q 'preferred_scale(200)' /tmp/foot.log")
                    hidpi.succeed("grep -q 'set_destination(' /tmp/foot.log")
                finally:
                    print(hidpi.execute("grep -v 'GL Extensions\\|EGL.*extensions' /tmp/malaysia.log")[1])
              '';
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.default ];
            packages = with pkgs; [
              clippy
              foot
              rustfmt
            ];
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (
              with pkgs;
              [
                libglvnd
                mesa
                wayland
              ]
            );
            __EGL_VENDOR_LIBRARY_DIRS = "${pkgs.mesa}/share/glvnd/egl_vendor.d";
            GBM_BACKENDS_PATH = "${pkgs.mesa}/lib/gbm";
            hardeningDisable = [ "fortify" ];
          };
        }
      );
    };
}
