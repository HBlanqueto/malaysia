{
  description = "MalaysiaDreams, a fixed Wayland compositor built on Smithay.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    barrierkit = {
      url = "github:HBlanqueto/barrierkit/6cb11d5";
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
        vulkan-loader
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
            ./protocols
            ./Cargo.toml
            ./Cargo.lock
            ./share
            ./NOTICE
          ];
        };
        skia = barrierkit.packages.${system}.skia-binaries;
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
            "git+https://github.com/HBlanqueto/barrierkit?rev=610f852#610f852606d7dc50fa9a7d24faeb202fc1304fcd" =
              "sha256-9QJh5GJ3GSjCy7AS7p72EE2sOku/+Wa0vE5MOL6ltvU=";
          };

          env = {
            MALAYSIA_DEV_SHELL = "1";
          }
          // lib.optionalAttrs hasFreyaSkia {
            SKIA_BINARIES_URL = "file://${skia}/skia-binaries.tar.gz";
          };

          doCheck = false;

          CARGO_PROFILE = "dist";

          nativeBuildInputs = with pkgs; [ diffutils pkg-config makeWrapper ];
          buildInputs = nativeDeps pkgs;

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
            sed -i "s|@out@|$out|g" $out/share/wayland-sessions/malaysia.desktop

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
            install -Dm644 share/xdg-desktop-portal/portals/malaysia.portal \
              $out/share/xdg-desktop-portal/portals/malaysia.portal
            mkdir -p $out/share/dbus-1/services
            sed "s|@out@|$out|g" \
              share/dbus-1/services/org.freedesktop.impl.portal.desktop.malaysia.service \
              > $out/share/dbus-1/services/org.freedesktop.impl.portal.desktop.malaysia.service

            mkdir -p $out/share/wallpapers
            cp -r share/wallpapers/solid-colors $out/share/wallpapers/
            install -Dm644 share/wallpapers/bandura.jpg \
              $out/share/wallpapers/bandura.jpg
            install -Dm644 NOTICE \
              $out/share/doc/malaysia/NOTICE
            install -Dm644 share/applications/io.github.hblanqueto.malaysia.settings.desktop \
              $out/share/applications/io.github.hblanqueto.malaysia.settings.desktop
            install -Dm644 share/icons/hicolor/scalable/apps/io.github.hblanqueto.malaysia.settings.svg \
              $out/share/icons/hicolor/scalable/apps/io.github.hblanqueto.malaysia.settings.svg
            mkdir -p $out/lib/systemd/user
            for unit in malaysia-shell@.service malaysia-wallpaper.service malaysia-session.target \
                xdg-desktop-portal-malaysia.service; do
              sed "s|@out@|$out|g" "share/systemd/user/$unit" \
                > "$out/lib/systemd/user/$unit"
            done

            wrapProgram "$out/bin/malaysia-compositor" \
              --prefix PATH : "${pkgs.xwayland-satellite}/bin"

            wrapProgram "$out/bin/malaysia-wallpaper" \
              --run 'export MALAYSIA_ORIG_VK_DRIVER_FILES="''${MALAYSIA_ORIG_VK_DRIVER_FILES-''${VK_DRIVER_FILES-}}"' \
              --run 'export MALAYSIA_ORIG_LD_LIBRARY_PATH="''${MALAYSIA_ORIG_LD_LIBRARY_PATH-''${LD_LIBRARY_PATH-}}"' \
              --set VK_DRIVER_FILES "${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json" \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath (nativeDeps pkgs)}"
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
        libadwaita-global-menu = pkgs.libadwaita.overrideAttrs (old: {
          patches = (old.patches or []) ++ [
            ./nix/patches/libadwaita-global-menu.patch
            ./nix/patches/libadwaita-focus-mode.patch
            ./nix/patches/libadwaita-live-appearance.patch
          ];
          nativeBuildInputs = (old.nativeBuildInputs or []) ++ [ pkgs.wayland-scanner ];
          buildInputs = (old.buildInputs or []) ++ [ pkgs.wayland ];
        });
      });

      checks = forAllSystems (system: let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        checkSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [ ./crates ./scripts ];
        };
      in {
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
        skia = barrierkit.packages.${system}.skia-binaries;
        hasFreyaSkia = pkgs.lib.hasInfix "freya-skia-bindings" (builtins.readFile ./Cargo.lock);
        weston = pkgs.weston.overrideAttrs (final: prev: {
          mesonFlags =
            builtins.filter
            (flag: flag != pkgs.lib.mesonOption "simple-clients" "")
            (prev.mesonFlags
              ++ [
                (pkgs.lib.mesonOption "simple-clients"
                  "damage,im,egl,shm,touch,dmabuf-feedback,dmabuf-egl")
              ]);
          buildInputs = prev.buildInputs ++ [ pkgs.pixman ];
        });
        fontsConf = pkgs.writeText "malaysia-fonts.conf" ''
          <?xml version="1.0"?>
          <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
          <fontconfig>
            <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
            <dir>${pkgs.geist-font}/share/fonts</dir>
            <dir>${pkgs.inter}/share/fonts</dir>
          </fontconfig>
        '';
        labApps = with pkgs; [
          dbus
          nautilus
          zenity
          gnome-calculator
        ];

        libadwaitaPatched = self.packages.${system}.libadwaita-global-menu;
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
            xwayland-satellite
            weston
            amdgpu_top
          ] ++ labApps;

          buildInputs = deps ++ [
            pkgs.geist-font
            pkgs.inter
          ];

          RUSTFLAGS = "-C link-arg=-fuse-ld=mold -C link-arg=-Wl,-rpath,${pkgs.lib.makeLibraryPath deps}";

          FONTCONFIG_FILE = fontsConf;

          MALAYSIA_DEV_SHELL = "1";

          shellHook = ''
            ${pkgs.lib.optionalString hasFreyaSkia ''
              export SKIA_BINARIES_URL="file://${skia}/skia-binaries.tar.gz"
            ''}
            export LD_LIBRARY_PATH="${libadwaitaPatched}/lib:${pkgs.lib.makeLibraryPath deps}$LD_LIBRARY_PATH"
            # libglvnd reads __EGL_VENDOR_LIBRARY_DIRS (two underscores), not EGL_VENDOR_LIBRARY_DIRS.
            export __EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d''${__EGL_VENDOR_LIBRARY_DIRS:+:$__EGL_VENDOR_LIBRARY_DIRS}"
            export VK_DRIVER_FILES="${pkgs.mesa}/share/vulkan/icd.d/radeon_icd.x86_64.json''${VK_DRIVER_FILES:+:$VK_DRIVER_FILES}"
            export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri''${LIBGL_DRIVERS_PATH:+:$LIBGL_DRIVERS_PATH}"
            export GBM_BACKENDS_PATH="${pkgs.mesa}/lib/gbm''${GBM_BACKENDS_PATH:+:$GBM_BACKENDS_PATH}"
          '';
        };
      });
    };
}
