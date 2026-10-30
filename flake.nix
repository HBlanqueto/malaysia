{
  description = "MalaysiaDreams, a fixed Wayland compositor built on Smithay.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    barrierkit = {
      url = "github:HBlanqueto/barrierkit/4cc8d6c";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, crane, barrierkit }:
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
        freetype
        fontconfig
      ];

      # The mockup extractor is hermetic: Chromium and Python's Playwright come
      # from the same nixpkgs as the flake, and fontconfig only sees Geist.
      mockupExtract = pkgs: pkgs.writeShellApplication {
        name = "mockup-extract";
        text = ''
          export PLAYWRIGHT_BROWSERS_PATH="${pkgs.playwright-driver.browsers}"
          export FONTCONFIG_FILE="${pkgs.makeFontsConf {
            fontDirectories = [ pkgs.geist-font ];
          }}"
          exec ${pkgs.python3.withPackages (ps: [ ps.playwright ])}/bin/python3 \
            ${./scripts/mockup-extract.py} "$@"
        '';
      };
    in
    {
      apps = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
      in {
        mockup-extract = {
          type = "app";
          program = "${mockupExtract pkgs}/bin/mockup-extract";
        };
      });

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
            ./protocols
            ./Cargo.toml
            ./Cargo.lock
            ./share
          ];
        };
        skia = barrierkit.packages.${system}.skia-binaries;
        # Skia only enters the build when the workspace actually pulls
        # freya-skia-bindings (BarrierKit on Freya). Until then the package
        # and the dev shell must not advertise a prebuild that nothing uses.
        hasFreyaSkia = lib.hasInfix "freya-skia-bindings" (builtins.readFile ./Cargo.lock);
        commonArgs = {
          pname = "malaysia";
          version = "0.1.0";
          inherit src;
          strictDeps = true;

          cargoLock = ./Cargo.lock;
          outputHashes = {
            "git+https://github.com/Smithay/smithay?rev=023b78b1c5a05ca512767cacc07efca55fae8be4#023b78b1c5a05ca512767cacc07efca55fae8be4" =
              "sha256-UZK11JYGEYdrNu/ByRo2U98QzApob+IjT55k9eqRdbE=";
            "git+https://github.com/HBlanqueto/barrierkit?rev=4cc8d6c#4cc8d6cda06289079ab74ab9ad34d85f3f6f45c7" =
              "sha256-6raDd+e8+yKTs2xmgwZj3iXU8pf9vAlerEQLL10YBj8=";
          };

          # The per-crate build script refuses to compile without this marker
          # (ADR 0007). The package build is a sanctioned, hermetic build path.
          env = {
            MALAYSIA_DEV_SHELL = "1";
          }
          // lib.optionalAttrs hasFreyaSkia {
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

          # Skia is prebuilt for freya-skia-bindings 0.98.0. This fails if the
          # version or the resolved feature set drifts from nix/skia-prebuild.
          preBuild = ''
            if cargo tree --offline -i freya-skia-bindings --depth 0 --prefix none >/dev/null 2>&1; then
              version="$(cargo tree --offline -i freya-skia-bindings --depth 0 --prefix none \
                | sed -n 's/.*freya-skia-bindings v\([0-9.]*\).*/\1/p' | head -n1)"
              if [ "$version" != "0.98.0" ]; then
                echo "freya-skia-bindings $version no coincide con la derivación 0.98.0" >&2
                exit 1
              fi
              cargo tree --offline -e features -i freya-skia-bindings --prefix none \
                | sed -n 's/^freya-skia-bindings feature "\([^"]*\)".*/\1/p' | sort -u > skia-features.txt
              if ! diff -u ${skia}/features.txt skia-features.txt; then
                echo "las features de Skia cambiaron: actualiza nix/skia-prebuild" >&2
                exit 1
              fi
            else
              echo "freya-skia-bindings no está en el grafo todavía; Skia precompilado queda listo" >&2
            fi
          '';
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        malaysia = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          postInstall = ''
            install -Dm644 share/wayland-sessions/malaysia.desktop \
              $out/share/wayland-sessions/malaysia.desktop
            # The session file names the wrapped binary, not whatever `malaysia-compositor`
            # PATH happens to resolve to.
            sed -i "s|@out@|$out|g" $out/share/wayland-sessions/malaysia.desktop

            # The binary must start outside `nix develop`. The RUNPATH covers
            # the shared libraries, but libglvnd/Mesa also read these three
            # environment variables, so bake the flake's Mesa into a wrapper.
            for program in malaysia-compositor malaysia-shell malaysia-settings; do
              wrapProgram "$out/bin/$program" \
                --run 'export MALAYSIA_ORIG___EGL_VENDOR_LIBRARY_DIRS="''${MALAYSIA_ORIG___EGL_VENDOR_LIBRARY_DIRS-''${__EGL_VENDOR_LIBRARY_DIRS-}}"' \
                --run 'export MALAYSIA_ORIG_LIBGL_DRIVERS_PATH="''${MALAYSIA_ORIG_LIBGL_DRIVERS_PATH-''${LIBGL_DRIVERS_PATH-}}"' \
                --run 'export MALAYSIA_ORIG_GBM_BACKENDS_PATH="''${MALAYSIA_ORIG_GBM_BACKENDS_PATH-''${GBM_BACKENDS_PATH-}}"' \
                --run 'export MALAYSIA_ORIG_LD_LIBRARY_PATH="''${MALAYSIA_ORIG_LD_LIBRARY_PATH-''${LD_LIBRARY_PATH-}}"' \
                --set __EGL_VENDOR_LIBRARY_DIRS "${pkgs.mesa}/share/glvnd/egl_vendor.d" \
                --set LIBGL_DRIVERS_PATH "${pkgs.mesa}/lib/dri" \
                --set GBM_BACKENDS_PATH "${pkgs.mesa}/lib/gbm" \
                --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath (nativeDeps pkgs)}"
            done
            # The portal backend (ADR 0031), its portal registration and its
            # D-Bus activation file. `xdg-desktop-portal` starts it on demand.
            install -Dm644 share/xdg-desktop-portal/portals/malaysia.portal \
              $out/share/xdg-desktop-portal/portals/malaysia.portal
            mkdir -p $out/share/dbus-1/services
            sed "s|@out@|$out|g" \
              share/dbus-1/services/org.freedesktop.impl.portal.desktop.malaysia.service \
              > $out/share/dbus-1/services/org.freedesktop.impl.portal.desktop.malaysia.service

            # Ship the system wallpapers, like GNOME installs its own to
            # /usr/share/backgrounds. `[wallpaper].dir` defaults to this path.
            mkdir -p $out/share/wallpapers
            cp -r share/wallpapers/solid-colors $out/share/wallpapers/
            # The Settings app launcher entry.
            install -Dm644 share/applications/io.github.hblanqueto.malaysia.settings.desktop \
              $out/share/applications/io.github.hblanqueto.malaysia.settings.desktop
            # Ship the systemd user units that supervise the shell components
            # (ADR 0023), with the package's binary path substituted in.
            mkdir -p $out/lib/systemd/user
            for unit in malaysia-shell@.service malaysia-session.target; do
              sed "s|@out@|$out|g" "share/systemd/user/$unit" \
                > "$out/lib/systemd/user/$unit"
            done
          '';
          passthru = {
            providedSessions = [ "malaysia" ];
            skiaBinaries = skia;
          };
        });
      in {
        default = malaysia;
        "malaysia-shell" = malaysia;
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
        # No colour or UI measurement may live outside vow-design in the
        # desktop crates (the kit checks live in barrierkit).
        design-tokens = pkgs.runCommand "check-design-tokens" {
          nativeBuildInputs = with pkgs; [ bash coreutils findutils gawk gnugrep ];
        } ''
          cd ${checkSrc}
          bash scripts/check-design-tokens.sh
          touch $out
        '';
      });

      nixosModules = {
        default = import ./nix/malaysia.nix { inherit self; };
      };

      devShells = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
        deps = nativeDeps pkgs;
        mockup-extract = mockupExtract pkgs;
        skia = barrierkit.packages.${system}.skia-binaries;
        hasFreyaSkia = pkgs.lib.hasInfix "freya-skia-bindings" (builtins.readFile ./Cargo.lock);
        # weston with its simple test clients enabled. nixpkgs builds weston
        # with `-Dsimple-clients=`, so `weston-simple-egl`, `weston-simple-shm`
          # and `weston-simple-dmabuf-feedback` do not exist; Bloque 7 drives
          # MalaysiaDreams's dmabuf/EGL path with them, so this override turns them on.
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
        # Fontconfig only sees a dev shell's fonts when FONTCONFIG_FILE is
        # explicit. Include the host config so the nested session keeps its
        # system fonts, then add Geist and the shortcut symbol fallback.
        fontsConf = pkgs.writeText "malaysia-fonts.conf" ''
          <?xml version="1.0"?>
          <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
          <fontconfig>
            <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
            <dir>${pkgs.geist-font}/share/fonts</dir>
            <dir>${pkgs.inter}/share/fonts</dir>
          </fontconfig>
        '';
        # Optional apps for `./dev.sh lab` (manual testing). They are not
        # required to build or run MalaysiaDreams; the lab script skips any that are
        # missing. Keep this list short so it does not bloat devShell entry.
        labApps = with pkgs; [
          dbus
          nautilus
          zenity
        ];
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
          # simple clients drive dmabuf/EGL against MalaysiaDreams, and amdgpu_top
          # watches VRAM on the Vega iGPU. They are shell-only; they are never
            # installed on the system.
            weston
            amdgpu_top
            mockup-extract
          ] ++ labApps;

          # `geist-font` is the UI typeface; `inter` is the fixed fallback for
          # the shortcut glyphs (Geist has none of them). Being in buildInputs
          # lets fontconfig's setup hook expose both to the nested session.
          buildInputs = deps ++ [
            pkgs.geist-font
            pkgs.inter
          ];

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

          # `fc-match Geist` resolves to Geist inside the dev shell and the
          # nested session; `/etc/fonts/fonts.conf` keeps the host fonts.
          FONTCONFIG_FILE = fontsConf;

          # Explicit marker so dev.sh can refuse to build outside this shell,
          # where the linker flag above would be missing and the binary would
          # be linked differently (ADR 0007).
          MALAYSIA_DEV_SHELL = "1";

          # Make the runtime libraries (wayland, xkbcommon, mesa/EGL, ...)
          # findable by the built binary when run from the dev shell.
          shellHook = ''
            ${pkgs.lib.optionalString hasFreyaSkia ''
              export SKIA_BINARIES_URL="file://${skia}/skia-binaries.tar.gz"
            ''}
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