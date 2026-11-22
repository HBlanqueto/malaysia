{
  description = "Malaysia, a Wayland desktop for NixOS; its compositor is built on Smithay.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    barrierkit = {
      url = "github:HBlanqueto/barrierkit/70926f18cd960782f370cd971db38cd7add8f7fa";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.crane.follows = "crane";
    };
    malaysia-icon-theme = {
      url = "github:HBlanqueto/malaysia-icon-theme/1e6c26fbfb572d0c442f2e1298adc79cc273054f";
      flake = false;
    };
  };

  outputs =
    { self, nixpkgs, crane, barrierkit, malaysia-icon-theme }:
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
        libheif
        libva
      ];
      hasFreyaSkia = builtins.any (package: package.name == "freya-skia-bindings")
        (builtins.fromTOML (builtins.readFile ./Cargo.lock)).package;
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
        commonArgs = {
          pname = "malaysia";
          version = "0.1.0";
          inherit src;
          strictDeps = true;

          cargoLock = ./Cargo.lock;
          outputHashes = {
            "git+https://github.com/Smithay/smithay?rev=65aa86fa5236e41775c313408102a5d5c2d1e2ca#65aa86fa5236e41775c313408102a5d5c2d1e2ca" =
              "sha256-YD7P5iZAr+0XdeURhq6DkRkFws6Dpn0zSU5Y9qvsyZQ=";
            "git+https://github.com/HBlanqueto/barrierkit?rev=70926f18cd960782f370cd971db38cd7add8f7fa#70926f18cd960782f370cd971db38cd7add8f7fa" =
              "sha256-q4T4ock5XTUATFZd0ukW4LClybodB7a9v6fmCkYJg1g=";
            "git+https://github.com/HBlanqueto/ferricast?rev=eb29421bb27aed41a7c2bd76fcb1044b5453d30c#eb29421bb27aed41a7c2bd76fcb1044b5453d30c" =
              "sha256-iH4ZhFUm228dgC0jlB3SNzMNMoUZvI5iDMpYwQDd1VY=";
            "git+https://github.com/SergioRibera/cros-libva?branch=fix/vp9-encpicparam-fields#627695c5a145e5d3d9d0ba78f37bfb3db0178cba" =
              "sha256-5qnXi+n3CFJzabfpzJtwZb/PT/8v5zZvQH7W8tRwuE8=";
          };

          env = {
            MALAYSIA_DEV_SHELL = "1";
          }
          // lib.optionalAttrs hasFreyaSkia {
            SKIA_BINARIES_URL = "file://${skia}/skia-binaries.tar.gz";
          };

          doCheck = false;

          CARGO_PROFILE = "dist";

          nativeBuildInputs = with pkgs; [ diffutils pkg-config makeWrapper rustPlatform.bindgenHook ];
          buildInputs = nativeDeps pkgs;

          preBuild = ''
            if cargo tree --offline -i freya-skia-bindings --depth 0 --prefix none >/dev/null 2>&1; then
              version="$(cargo tree --offline -i freya-skia-bindings --depth 0 --prefix none \
                | sed -n 's/.*freya-skia-bindings v\([0-9.]*\).*/\1/p' | head -n1)"
              if [ "$version" != "${skia.version}" ]; then
                echo "freya-skia-bindings $version no coincide con la derivación ${skia.version}" >&2
                exit 1
              fi
              cargo tree --offline -e features -i freya-skia-bindings --prefix none \
                | sed -n 's/^freya-skia-bindings feature "\([^"]*\)".*/\1/p' | sort -u > skia-features.txt
              if ! diff -u ${skia}/features.txt skia-features.txt; then
                echo "las features de Skia cambiaron: actualiza nix/skia-prebuild en barrierkit" >&2
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
            install -Dm644 share/fontconfig/malaysia.conf \
              $out/share/fontconfig/malaysia.conf
            install -Dm644 share/xdg-desktop-portal/portals/malaysia.portal \
              $out/share/xdg-desktop-portal/portals/malaysia.portal
            mkdir -p $out/share/dbus-1/services
            for service in share/dbus-1/services/*.service; do
              sed "s|@out@|$out|g" "$service" > "$out/share/dbus-1/services/$(basename "$service")"
            done

            mkdir -p $out/share/wallpapers
            cp -r share/wallpapers/solid-colors $out/share/wallpapers/
            install -Dm644 share/wallpapers/bandura.jpg \
              $out/share/wallpapers/bandura.jpg
            install -Dm644 share/wallpapers/tide.frag \
              $out/share/wallpapers/tide.frag
            cp -r share/wallpapers/previews $out/share/wallpapers/
            install -Dm644 NOTICE \
              $out/share/doc/malaysia/NOTICE
            install -Dm644 share/applications/io.github.hblanqueto.malaysia.settings.desktop \
              $out/share/applications/io.github.hblanqueto.malaysia.settings.desktop
            install -Dm644 share/icons/hicolor/scalable/apps/io.github.hblanqueto.malaysia.settings.svg \
              $out/share/icons/hicolor/scalable/apps/io.github.hblanqueto.malaysia.settings.svg
            install -Dm644 share/icons/hicolor/scalable/apps/io.github.hblanqueto.malaysia.screenshot.svg \
              $out/share/icons/hicolor/scalable/apps/io.github.hblanqueto.malaysia.screenshot.svg
            mkdir -p $out/lib/systemd/user
            for unit in malaysia-shell@.service malaysia-wallpaper.service malaysia-session.target \
                xdg-desktop-portal-malaysia.service; do
              sed "s|@out@|$out|g" "share/systemd/user/$unit" \
                > "$out/lib/systemd/user/$unit"
            done

            wrapProgram "$out/bin/malaysia-compositor" \
              --set-default FONTCONFIG_FILE "$out/share/fontconfig/malaysia.conf" \
              --prefix PATH : "${pkgs.xwayland-satellite}/bin"

            wrapProgram "$out/bin/malaysia-wallpaper" \
              --run 'export MALAYSIA_ORIG_VK_DRIVER_FILES="''${MALAYSIA_ORIG_VK_DRIVER_FILES-''${VK_DRIVER_FILES-}}"' \
              --run 'export MALAYSIA_ORIG_LD_LIBRARY_PATH="''${MALAYSIA_ORIG_LD_LIBRARY_PATH-''${LD_LIBRARY_PATH-}}"' \
              --set VK_DRIVER_FILES "${pkgs.mesa}/share/vulkan/icd.d" \
              --set LIBVA_DRIVERS_PATH "${pkgs.mesa}/lib/dri" \
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
        appmenu-gtk-module = pkgs.callPackage ./nix/appmenu-gtk-module.nix { };
        malaysia-icon-theme = pkgs.callPackage ./nix/malaysia-icon-theme.nix { src = malaysia-icon-theme; };
        electron-global-menu = pkgs.electron_43.override {
          electron-unwrapped = pkgs.electron_43.unwrapped.overrideAttrs (old: {
            patches = old.patches ++ [ ./nix/patches/electron-wayland-appmenu.patch ];
            postFixup = old.postFixup + ''
              patchelf --add-rpath "${pkgs.libdbusmenu}/lib" $out/libexec/electron/electron
            '';
          });
        };
        qt6ct-color-scheme = pkgs.qt6Packages.qt6ct.overrideAttrs (old: {
          patches = (old.patches or []) ++ [ ./nix/patches/qt6ct-color-scheme.patch ];
        });
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
            <include>${./share/fontconfig/malaysia.conf}</include>
            <dir>${pkgs.adwaita-fonts}/share/fonts</dir>
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
            rustPlatform.bindgenHook
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
            pkgs.adwaita-fonts
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
            export LD_LIBRARY_PATH="${libadwaitaPatched}/lib:${pkgs.lib.makeLibraryPath deps}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            export GIO_EXTRA_MODULES="${pkgs.glib-networking}/lib/gio/modules"
            export LIBVA_DRIVERS_PATH="${pkgs.mesa}/lib/dri"
            export __EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d''${__EGL_VENDOR_LIBRARY_DIRS:+:$__EGL_VENDOR_LIBRARY_DIRS}"
            export VK_DRIVER_FILES="${pkgs.mesa}/share/vulkan/icd.d''${VK_DRIVER_FILES:+:$VK_DRIVER_FILES}"
            export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri''${LIBGL_DRIVERS_PATH:+:$LIBGL_DRIVERS_PATH}"
            export GBM_BACKENDS_PATH="${pkgs.mesa}/lib/gbm''${GBM_BACKENDS_PATH:+:$GBM_BACKENDS_PATH}"
          '';
        };
      });
    };
}
