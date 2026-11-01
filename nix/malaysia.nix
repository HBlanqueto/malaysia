# NixOS integration for MalaysiaDreams. The flake exposes this as `nixosModules.default`.
#
# The module installs the compositor, registers its Wayland session with the
# display manager, creates the lock's PAM stack, and starts one shell process
# per component in `malaysia-session.target` (ADR 0023). The compositor imports
# `WAYLAND_DISPLAY` into the user manager and starts that target itself.
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

    # ADR 0052: replace the system libadwaita with a patched build that exports
    # the primary menu as an application menubar over org.gtk.Menus. This avoids
    # rebuilding every libadwaita application while still giving the shell a
    # global menu for them. The same build hides AdwToolbarView top bars in
    # focus mode (ADR 0053); ADW_DISABLE_FOCUS_MODE turns that off per app.
    system.replaceDependencies.replacements = lib.mkIf cfg.globalMenu.libadwaita.enable [
      {
        oldDependency = pkgs.libadwaita;
        newDependency = self.packages.${pkgs.stdenv.hostPlatform.system}.libadwaita-global-menu;
      }
    ];

    # The UI typeface and its symbol fallback. Geist has no ⌃⌥⇧❖ glyphs;
    # Inter provides all four from the same family.
    fonts.packages = [
      pkgs.geist-font
      pkgs.inter
    ];

    # The session file in the package makes MalaysiaDreams selectable in GDM, SDDM and
    # greetd.
    services.displayManager.sessionPackages = [ cfg.package ];

    # Both the compositor and the shell render through GBM/EGL.
    hardware.graphics.enable = lib.mkDefault true;

    # ADR 0031: MalaysiaDreams's backend serves Settings; GTK keeps FileChooser, Print
    # and Notification. `UseIn=Malaysia` ties the backend to this session.
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

    # The lock prefers this stack and falls back to `login` when it is absent.
    security.pam.services.malaysia-lock.text = ''
      auth include login
      account include login
      session include login
    '';

    # The package already ships the per-component units in
    # $out/lib/systemd/user with @out@ substituted. Install them system-wide
    # and let the target pull in the configured subset.
    systemd.packages = [ cfg.package ];

    systemd.user.targets.malaysia-session = {
      description = "MalaysiaDreams shell session";
      wants =
        map (component: "malaysia-shell@${component}.service") cfg.shellComponents
        ++ lib.optional cfg.wallpaper.enable "malaysia-wallpaper.service"
        # ADR 0050: the portal also keeps the GTK/libadwaita bridge, so it starts
        # with the session instead of waiting for the first app to activate it.
        ++ [ "xdg-desktop-portal-malaysia.service" ];
    };
  };
}
