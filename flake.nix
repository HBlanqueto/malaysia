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
              seatd
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
                    malaysia-compositor /dev/dri/card0 >/tmp/malaysia.log 2>&1; echo $? >/tmp/exit
                  fi
                '';
                hardware.graphics.enable = true;
                fonts.enableDefaultPackages = true;
                environment.systemPackages = [
                  self.packages.${system}.default
                  pkgs.foot
                ];
                virtualisation.qemu.options = [
                  "-vga none"
                  "-device virtio-gpu-pci"
                ];
                virtualisation.memorySize = 2048;
                virtualisation.useNixStoreImage = true;
                virtualisation.writableStore = false;
                virtualisation.sharedDirectories = lib.mkForce { };
              };
            testScript = ''
              def foot(text):
                  machine.succeed(f"su - alice -c 'XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 foot -o font=monospace:size=24 sh -c \"echo {text}; sleep 600\"' >>/tmp/foot.log 2>&1 &")
                  machine.wait_for_text(text, timeout=300)

              try:
                  machine.wait_for_file("/run/user/1000/wayland-1", timeout=600)
                  foot("malaysia")
                  machine.screenshot("foot")
                  machine.succeed("chvt 2")
                  machine.wait_until_succeeds("grep -q PauseSession /tmp/malaysia.log", timeout=60)
                  machine.wait_for_text("automatic login", timeout=300)
                  machine.screenshot("vt2")
                  machine.succeed("chvt 1")
                  machine.wait_until_succeeds("grep -q ActivateSession /tmp/malaysia.log", timeout=60)
                  foot("resumed")
                  machine.screenshot("resumed")
                  machine.succeed("pkill -x malaysia-compos")
                  machine.wait_until_fails("pgrep -x malaysia-compos", timeout=60)
                  machine.wait_for_file("/tmp/exit", timeout=60)
                  machine.send_chars("tty >/tmp/tty\n")
                  machine.wait_until_succeeds("grep -qx /dev/tty1 /tmp/tty", timeout=60)
                  machine.screenshot("text-vt")
              finally:
                  print(machine.execute("cat /tmp/malaysia.log /tmp/foot.log")[1])
              compositor_log = machine.succeed("cat /tmp/malaysia.log")
              print("exit", machine.succeed("cat /tmp/exit"))
              assert machine.succeed("fgconsole").strip() == "1"
              assert "drm backend" in compositor_log and "mode chosen" in compositor_log
              for line in compositor_log.splitlines():
                  assert "ERROR" not in line, line
                  assert not (("EGL" in line or "GBM" in line) and "fail" in line.lower()), line
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
              seatd
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
