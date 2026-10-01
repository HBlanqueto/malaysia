{
  description = "Yuki, a fixed Wayland compositor built on Smithay.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;

      nativeDeps = pkgs: with pkgs; [
        libinput
        libglvnd
        seatd
        libxkbcommon
        mesa
        wayland
        udev
      ];
    in
    {
      packages = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
      in rec {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "yuki";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # The per-crate build script refuses to compile without this marker
          # (ADR 0007). The package build is a sanctioned, hermetic build path,
          # so it sets the marker deliberately; it does not use mold.
          YUKI_DEV_SHELL = "1";

          nativeBuildInputs = with pkgs; [ pkg-config ];

          buildInputs = nativeDeps pkgs;

          postInstall = ''
            install -Dm644 share/wayland-sessions/yuki.desktop \
              $out/share/wayland-sessions/yuki.desktop
            # Ship the system wallpapers, like GNOME installs its own to
            # /usr/share/backgrounds. `[wallpaper].dir` defaults to this path.
            mkdir -p $out/share/wallpapers
            cp -r share/wallpapers/solid-colors $out/share/wallpapers/
            # Ship the systemd user units that supervise the shell components
            # (ADR 0023), with the package's binary path substituted in.
            mkdir -p $out/lib/systemd/user
            for unit in yuki-shell@.service yuki-session.target; do
              sed "s|@out@|$out|g" "share/systemd/user/$unit" \
                > "$out/lib/systemd/user/$unit"
            done
          '';
        };
      });

      devShells = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
        deps = nativeDeps pkgs;
      in {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            pkg-config
            mold
            shellcheck
            cargo-watch
            foot
            wayland-utils
          ];

          buildInputs = deps;

          # The mold linker is part of this shell, so request it here rather
          # than in .cargo/config.toml or through PATH probing in dev.sh: a
          # fixed value keeps every in-shell build byte-for-byte identical and
          # the cargo cache warm. The `cc` driver understands -fuse-ld.
          #
          # The runtime search path is baked in for the same reason it is
          # fixed: winit loads libwayland-client through dlopen, which only
          # searches the caller's RUNPATH or LD_LIBRARY_PATH, so a binary
          # without it cannot start outside the shell (ADR 0007 keeps `run`
          # working there). The paths are the same store paths the shell
          # exports, so nothing runs that the shell could not run.
          RUSTFLAGS = "-C link-arg=-fuse-ld=mold -C link-arg=-Wl,-rpath,${pkgs.lib.makeLibraryPath deps}";

          # Explicit marker so dev.sh can refuse to build outside this shell,
          # where the linker flag above would be missing and the binary would
          # be linked differently (ADR 0007).
          YUKI_DEV_SHELL = "1";

          # Make the runtime libraries (wayland, xkbcommon, mesa/EGL, ...)
          # findable by the built binary when run from the dev shell.
          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath deps}$LD_LIBRARY_PATH"
            export EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d''${EGL_VENDOR_LIBRARY_DIRS:+:$EGL_VENDOR_LIBRARY_DIRS}"
          '';
        };
      });
    };
}