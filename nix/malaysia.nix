# NixOS module for MalaysiaDreams, exposed as `nixosModules.default`.
{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.malaysia;
in
{
  options.programs.malaysia = {
    enable = lib.mkEnableOption "the MalaysiaDreams Wayland compositor";

    globalMenu.libadwaita.enable = lib.mkEnableOption "global menu support for libadwaita applications" // {
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
      enable = lib.mkEnableOption "the MalaysiaDreams Vulkan wallpaper client" // {
        default = true;
      };
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    system.replaceDependencies.replacements = lib.mkIf cfg.globalMenu.libadwaita.enable [
      {
        oldDependency = pkgs.libadwaita;
        newDependency = self.packages.${pkgs.stdenv.hostPlatform.system}.libadwaita-global-menu;
      }
    ];

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
      description = "MalaysiaDreams shell session";
      wants =
        map (component: "malaysia-shell@${component}.service") cfg.shellComponents
        ++ lib.optional cfg.wallpaper.enable "malaysia-wallpaper.service"
        ++ [ "xdg-desktop-portal-malaysia.service" ];
    };
  };
}
