# NixOS integration for Yuki. The flake exposes this as `nixosModules.default`.
#
# The module installs the compositor, registers its Wayland session with the
# display manager, creates the lock's PAM stack, and starts one shell process
# per component in `yuki-session.target` (ADR 0023). The compositor imports
# `WAYLAND_DISPLAY` into the user manager and starts that target itself.
{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.yuki;
in
{
  options.programs.yuki = {
    enable = lib.mkEnableOption "the Yuki Wayland compositor";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "self.packages.\${pkgs.stdenv.hostPlatform.system}.default";
      description = "The Yuki package to install.";
    };

    shellComponents = lib.mkOption {
      type = lib.types.listOf (lib.types.enum [ "wallpaper" "menubar" "dock" ]);
      default = [
        "wallpaper"
        "menubar"
        "dock"
      ];
      description = "The yuki-shell components started in yuki-session.target.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    # The session file in the package makes Yuki selectable in GDM, SDDM and
    # greetd.
    services.displayManager.sessionPackages = [ cfg.package ];

    # Both the compositor and the shell render through GBM/EGL.
    hardware.graphics.enable = lib.mkDefault true;

    # The lock prefers this stack and falls back to `login` when it is absent.
    security.pam.services.yuki-lock.text = ''
      auth include login
      account include login
      session include login
    '';

    systemd.user.targets.yuki-session = {
      description = "Yuki shell session";
      wants = map (component: "yuki-shell@${component}.service") cfg.shellComponents;
    };

    systemd.user.units."yuki-shell@.service".text =
      builtins.replaceStrings [ "@out@" ] [ "${cfg.package}" ]
        (builtins.readFile "${../share/systemd/user}/yuki-shell@.service");
  };
}
