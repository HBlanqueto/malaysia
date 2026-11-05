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
      type = lib.types.listOf (lib.types.enum [ "menubar" "dock" ]);
      default = [ "menubar" "dock" ];
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

    fonts.packages = [
      pkgs.geist-font
      pkgs.inter
    ];

    services.displayManager.sessionPackages = [ cfg.package ];

    hardware.graphics.enable = lib.mkDefault true;

    xdg.portal = {
      enable = lib.mkDefault true;
      extraPortals = lib.mkAfter [
        cfg.package
        pkgs.xdg-desktop-portal-gtk
      ];
      config.malaysia = lib.mkDefault {
        default = [ "gtk" ];
        "org.freedesktop.impl.portal.Settings" = [ "malaysia" ];
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
        ++ [
          "xdg-desktop-portal-malaysia.service"
          "graphical-session.target"
        ];
    };
  };
}
