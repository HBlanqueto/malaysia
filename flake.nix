{
  description = "Yuki, a fixed Wayland compositor built on Smithay.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    { self, nixpkgs, crane }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;

      nativeDeps = pkgs: with pkgs; [
        libinput
        libglvnd
        libdrm
        libgbm
        seatd
        pam
        libxkbcommon
        mesa
        wayland
        udev
      ];
    in
    {
      packages = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.symlinkJoin {
          name = "rust-toolchain";
          paths = [ p.cargo p.rustc p.clippy p.rustfmt ];
        });
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./crates
            ./tools/sctk-probe
            ./Cargo.toml
            ./Cargo.lock
            ./share
          ];
        };
        skia = pkgs.callPackage ./nix/skia.nix { };
        commonArgs = {
          pname = "yuki";
          version = "0.1.0";
          inherit src;
          strictDeps = true;

          cargoLock = ./Cargo.lock;
          outputHashes = {
            "git+https://github.com/Smithay/smithay?rev=023b78b1c5a05ca512767cacc07efca55fae8be4#023b78b1c5a05ca512767cacc07efca55fae8be4" =
              "sha256-UZK11JYGEYdrNu/ByRo2U98QzApob+IjT55k9eqRdbE=";
          };

          # The per-crate build script refuses to compile without this marker
          # (ADR 0007). The package build is a sanctioned, hermetic build path.
          env = {
            YUKI_DEV_SHELL = "1";
            SKIA_BINARIES_URL = "file://${skia}/skia-binaries.tar.gz";
          };

          # The test suite needs an EGL device and system fonts; the Nix build
          # sandbox has neither. Tests run in the dev shell (cargo test).
          doCheck = false;

          # The installed build uses the fat-LTO profile; dependencies are
          # cached separately by buildDepsOnly.
          CARGO_PROFILE = "dist";

          nativeBuildInputs = with pkgs; [ diffutils pkg-config makeWrapper ];
          buildInputs = nativeDeps pkgs;

          # Skia is prebuilt. When laye-rs lands, this fails if its version or
          # features drift from nix/skia-prebuild; today it is a no-op.
          preBuild = ''
            if cargo tree --offline -i skia-bindings --depth 0 --prefix none >/dev/null 2>&1; then
              version="$(cargo tree --offline -i skia-bindings --depth 0 --prefix none \
                | sed -n 's/.*skia-bindings v\([0-9.]*\).*/\1/p' | head -n1)"
              if [ "$version" != "0.93.1" ] && [ "$version" != "0.93.0" ]; then
                echo "skia-bindings $version no coincide con la derivación 0.93" >&2
                exit 1
              fi
              cargo tree --offline -e features -i skia-bindings --prefix none \
                | sed -n 's/^skia-bindings feature "\([^"]*\)".*/\1/p' | sort -u > skia-features.txt
              if ! diff -u ${skia}/features.txt skia-features.txt; then
                echo "las features de Skia cambiaron: actualiza nix/skia-prebuild/Cargo.toml" >&2
                exit 1
              fi
            else
              echo "skia-bindings no está en el grafo todavía; Skia precompilado queda listo" >&2
            fi
          '';
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        yuki = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          postInstall = ''
            install -Dm644 share/wayland-sessions/yuki.desktop \
              $out/share/wayland-sessions/yuki.desktop
            # The session file names the wrapped binary, not whatever `yuki`
            # PATH happens to resolve to.
            sed -i "s|@out@|$out|g" $out/share/wayland-sessions/yuki.desktop

            # The binary must start outside `nix develop`. The RUNPATH covers
            # the shared libraries, but libglvnd/Mesa also read these three
            # environment variables, so bake the flake's Mesa into a wrapper.
            for program in yuki yuki-shell; do
              wrapProgram "$out/bin/$program" \
                --run 'export YUKI_ORIG___EGL_VENDOR_LIBRARY_DIRS="''${YUKI_ORIG___EGL_VENDOR_LIBRARY_DIRS-''${__EGL_VENDOR_LIBRARY_DIRS-}}"' \
                --run 'export YUKI_ORIG_LIBGL_DRIVERS_PATH="''${YUKI_ORIG_LIBGL_DRIVERS_PATH-''${LIBGL_DRIVERS_PATH-}}"' \
                --run 'export YUKI_ORIG_GBM_BACKENDS_PATH="''${YUKI_ORIG_GBM_BACKENDS_PATH-''${GBM_BACKENDS_PATH-}}"' \
                --run 'export YUKI_ORIG_LD_LIBRARY_PATH="''${YUKI_ORIG_LD_LIBRARY_PATH-''${LD_LIBRARY_PATH-}}"' \
                --set __EGL_VENDOR_LIBRARY_DIRS "${pkgs.mesa}/share/glvnd/egl_vendor.d" \
                --set LIBGL_DRIVERS_PATH "${pkgs.mesa}/lib/dri" \
                --set GBM_BACKENDS_PATH "${pkgs.mesa}/lib/gbm" \
                --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath (nativeDeps pkgs)}"
            done
            # The portal backend (ADR 0031), its portal registration and its
            # D-Bus activation file. `xdg-desktop-portal` starts it on demand.
            install -Dm644 share/xdg-desktop-portal/portals/yuki.portal \
              $out/share/xdg-desktop-portal/portals/yuki.portal
            mkdir -p $out/share/dbus-1/services
            sed "s|@out@|$out|g" \
              share/dbus-1/services/org.freedesktop.impl.portal.desktop.yuki.service \
              > $out/share/dbus-1/services/org.freedesktop.impl.portal.desktop.yuki.service

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
          passthru = {
            providedSessions = [ "yuki" ];
            skiaBinaries = skia;
          };
        });
      in {
        default = yuki;
        skia-binaries = skia;
      });

      checks = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        checkSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [ ./crates ./scripts ];
        };
      in {
        # The vendored Neptunia outputs must match default.nix.
        neptunia-generated = let
          neptunia = import ./crates/yuki-design/neptunia;
        in pkgs.runCommand "check-neptunia-generated" {
          nativeBuildInputs = with pkgs; [ diffutils ];
        } ''
          cd ${checkSrc}
          diff -u crates/yuki-design/neptunia/generated/neptunia.slint \
            ${pkgs.writeText "neptunia.slint" neptunia.slint}
          diff -u crates/yuki-design/neptunia/generated/neptunia.css \
            ${pkgs.writeText "neptunia.css" neptunia.css}
          diff -u crates/yuki-design/neptunia/generated/Neptunia.qml \
            ${pkgs.writeText "Neptunia.qml" neptunia.qml}
          touch $out
        '';

        # No colour or UI measurement may live outside yuki-design.
        design-tokens = pkgs.runCommand "check-design-tokens" {
          nativeBuildInputs = with pkgs; [ bash coreutils findutils gawk gnugrep ];
        } ''
          cd ${checkSrc}
          bash scripts/check-design-tokens.sh
          touch $out
        '';
      });

      nixosModules = {
        default = import ./nix/yuki.nix { inherit self; };
      };

      devShells = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
        deps = nativeDeps pkgs;
        skia = pkgs.callPackage ./nix/skia.nix { };
        # weston with its simple test clients enabled. nixpkgs builds weston
        # with `-Dsimple-clients=`, so `weston-simple-egl`, `weston-simple-shm`
        # and `weston-simple-dmabuf-feedback` do not exist; Bloque 7 drives
        # Yuki's dmabuf/EGL path with them, so this override turns them on.
        weston = pkgs.weston.overrideAttrs (final: prev: {
          mesonFlags =
            builtins.filter
            (flag: flag != pkgs.lib.mesonOption "simple-clients" "")
            (prev.mesonFlags
              ++ [
                (pkgs.lib.mesonOption "simple-clients"
                  "damage,im,egl,shm,touch,dmabuf-feedback,dmabuf-egl")
              ]);
          # `weston-simple-dmabuf-feedback` links pixman-1.
          buildInputs = prev.buildInputs ++ [ pkgs.pixman ];
        });
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
            # Scriptable smoke client (it negotiates server-side). Manual GPU
            # acceptance uses the host's wezterm, which does not.
            foot
            wayland-utils
            # Test tools for the GPU-acceleration blocks (Bloque 7): the weston
            # simple clients drive dmabuf/EGL against Yuki, and amdgpu_top
            # watches VRAM on the Vega iGPU. They are shell-only; they are never
            # installed on the system.
            weston
            amdgpu_top
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
          SKIA_BINARIES_URL = "file://${skia}/skia-binaries.tar.gz";

          # Make the runtime libraries (wayland, xkbcommon, mesa/EGL, ...)
          # findable by the built binary when run from the dev shell.
          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath deps}$LD_LIBRARY_PATH"
            # libglvnd reads __EGL_VENDOR_LIBRARY_DIRS (two underscores), not EGL_VENDOR_LIBRARY_DIRS.
            export __EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d''${__EGL_VENDOR_LIBRARY_DIRS:+:$__EGL_VENDOR_LIBRARY_DIRS}"
            # Without these, Mesa's GBM/DRI loader falls back to the host's
            # /run/opengl-driver, which is built against a different glibc.
            export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri''${LIBGL_DRIVERS_PATH:+:$LIBGL_DRIVERS_PATH}"
            export GBM_BACKENDS_PATH="${pkgs.mesa}/lib/gbm''${GBM_BACKENDS_PATH:+:$GBM_BACKENDS_PATH}"
          '';
        };
      });
    };
}