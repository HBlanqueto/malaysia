{
  description = "Neptune, a fixed Wayland compositor built on Smithay.";

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
          pname = "neptune";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # The per-crate build script refuses to compile without this marker
          # (ADR 0007). The package build is a sanctioned, hermetic build path,
          # so it sets the marker deliberately; it does not use mold.
          NEPTUNE_DEV_SHELL = "1";

          nativeBuildInputs = with pkgs; [ pkg-config ];

          buildInputs = nativeDeps pkgs;

          postInstall = ''
            install -Dm644 share/wayland-sessions/neptune.desktop \
              $out/share/wayland-sessions/neptune.desktop
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
          ];

          buildInputs = deps;

          # The mold linker is part of this shell, so request it here rather
          # than in .cargo/config.toml or through PATH probing in dev.sh: a
          # fixed value keeps every in-shell build byte-for-byte identical and
          # the cargo cache warm. The `cc` driver understands -fuse-ld.
          RUSTFLAGS = "-C link-arg=-fuse-ld=mold";

          # Explicit marker so dev.sh can refuse to build outside this shell,
          # where the linker flag above would be missing and the binary would
          # be linked differently (ADR 0007).
          NEPTUNE_DEV_SHELL = "1";

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