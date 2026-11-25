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
              machine.wait_for_unit("multi-user.target", timeout=600)
              machine.succeed("XDG_RUNTIME_DIR=/tmp malaysia-compositor /dev/dri/card0 >/tmp/malaysia.log 2>&1 &")
              machine.wait_for_file("/tmp/wayland-1", timeout=300)
              machine.succeed("XDG_RUNTIME_DIR=/tmp WAYLAND_DISPLAY=wayland-1 foot -o font=monospace:size=24 sh -c 'echo malaysia; sleep 600' >/tmp/foot.log 2>&1 &")
              machine.wait_for_text("malaysia", timeout=300)
              machine.screenshot("foot")
              compositor_log = machine.succeed("cat /tmp/malaysia.log")
              print(compositor_log)
              print(machine.succeed("cat /tmp/foot.log"))
              machine.succeed("pgrep -x malaysia-compos")
              assert "drm backend" in compositor_log and "mode chosen" in compositor_log
              for line in compositor_log.splitlines():
                  assert "ERROR" not in line and not (("EGL" in line or "GBM" in line) and "fail" in line.lower()), line
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
