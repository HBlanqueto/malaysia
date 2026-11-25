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
        in
        {
          vm = pkgs.testers.runNixOSTest {
            name = "malaysia-drm";
            enableOCR = true;
            nodes.machine =
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
                try:
                    machine.wait_for_file("/run/user/1000/wayland-1", timeout=600)
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
                    machine.send_monitor_command("mouse_move 200 200")
                    machine.wait_until_succeeds("grep -q 'wl_pointer#[0-9]*\\.enter' /tmp/foot.log", timeout=60)
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
                for line in compositor_log.splitlines():
                    assert "ERROR" not in line, line
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
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              pkg-config
            ];
            buildInputs = with pkgs; [
              libxkbcommon
              libgbm
              libinput
              seatd
              udev
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
