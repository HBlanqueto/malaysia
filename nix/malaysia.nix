# NixOS module for Malaysia, exposed as `nixosModules.default`.
{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.malaysia;
  appmenuGtkModule = self.packages.${pkgs.stdenv.hostPlatform.system}.appmenu-gtk-module;
in
{
  options.programs.malaysia = {
    enable = lib.mkEnableOption "the Malaysia desktop";

    globalMenu.libadwaita.enable = lib.mkEnableOption "global menu support for libadwaita applications" // {
      default = true;
    };

    globalMenu.electron.enable = lib.mkEnableOption "global menu support for Electron 43 applications on Wayland (builds Electron from source)";

    globalMenu.gtk3.enable = lib.mkEnableOption "global menu support for GTK 3 applications with a GtkMenuBar" // {
      default = true;
    };

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "self.packages.\${pkgs.stdenv.hostPlatform.system}.default";
      description = "The Malaysia package to install.";
    };

    shellComponents = lib.mkOption {
      type = lib.types.listOf (lib.types.enum [ "menubar" "dock" "notifications" ]);
      default = [ "menubar" "dock" "notifications" ];
      description = "The malaysia-shell components started in malaysia-session.target.";
    };

    wallpaper = {
      enable = lib.mkEnableOption "the Malaysia Vulkan wallpaper client" // {
        default = true;
      };
    };
  };

  config = lib.mkIf cfg.enable {
    # hwdata: «Acerca de este PC» names the GPU from /run/current-system/sw/share/hwdata/pci.ids.
    environment.systemPackages = [
      cfg.package
      pkgs.hwdata
      pkgs.libsForQt5.qt5ct
      self.packages.${pkgs.stdenv.hostPlatform.system}.qt6ct-color-scheme
      pkgs.kdePackages.breeze-icons
      self.packages.${pkgs.stdenv.hostPlatform.system}.malaysia-icon-theme
    ]
    ++ lib.optional cfg.globalMenu.gtk3.enable appmenuGtkModule;

    system.replaceDependencies.replacements =
      lib.optional cfg.globalMenu.libadwaita.enable {
        oldDependency = pkgs.libadwaita;
        newDependency = self.packages.${pkgs.stdenv.hostPlatform.system}.libadwaita-global-menu;
      }
      ++ lib.optional cfg.globalMenu.electron.enable {
        oldDependency = pkgs.electron_43;
        newDependency = self.packages.${pkgs.stdenv.hostPlatform.system}.electron-global-menu;
      };

    environment.sessionVariables = lib.mkIf cfg.globalMenu.gtk3.enable {
      GTK_MODULES = [ "appmenu-gtk-module" ];
      XDG_DATA_DIRS = [ (pkgs.glib.getSchemaDataDirPath appmenuGtkModule) ];
    };

    # A default, not a lock: Settings writes the user's choice over it and «Back to Malaysia» resets to it.
    programs.dconf = {
      enable = lib.mkDefault true;
      profiles.user.databases = [
        { settings."org/gnome/desktop/interface".icon-theme = "Malaysia"; }
      ];
    };

    fonts.packages = [
      pkgs.adwaita-fonts
      pkgs.geist-font
      pkgs.inter
    ];

    services.displayManager.sessionPackages = [ cfg.package ];

    hardware.graphics.enable = lib.mkDefault true;

    # ADR 0067: QT_PLUGIN_PATH finds qt6ct; the compositor sets QT_QPA_PLATFORMTHEME in its session.
    qt.enable = lib.mkDefault true;

    xdg.portal = {
      enable = lib.mkDefault true;
      extraPortals = lib.mkAfter [
        cfg.package
        pkgs.xdg-desktop-portal-gtk
      ];
      config.malaysia = lib.mkDefault {
        default = [ "gtk" ];
        # GTK 4 on Wayland reads only the portal, so gtk adds icon-theme, cursor and fonts from GSettings; malaysia wins per key.
        "org.freedesktop.impl.portal.Settings" = [ "malaysia" "gtk" ];
        "org.freedesktop.impl.portal.Notification" = [ "malaysia" ];
      };
    };

    security.pam.services.malaysia-lock.text = ''
      auth include login
      account include login
      session include login
    '';

    systemd.packages = [ cfg.package ];

    systemd.user.targets.malaysia-session = {
      description = "Malaysia shell session";
      wants =
        map (component: "malaysia-shell@${component}.service") cfg.shellComponents
        ++ lib.optional cfg.wallpaper.enable "malaysia-wallpaper.service"
        # PipeWire starts on demand; without it BlueZ has no audio profile and headphones fail.
        ++ lib.optional config.services.pipewire.wireplumber.enable "wireplumber.service"
        ++ [
          "xdg-desktop-portal-malaysia.service"
          "graphical-session.target"
        ];
    };
  };
}
