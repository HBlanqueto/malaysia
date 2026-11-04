# 0023 — Shell components run as separate processes

Status: accepted. Supervision remains a proposal; the location of the overview is still open.

## Context

Dock, menubar, notifications, launcher and lock must survive each other.

## Decision

1. Each component is its own process and Wayland client. None depends on another; each tolerates the absence of the others.
2. Shared code lives in a toolkit crate that reads the design tokens; our own apps use the same toolkit.
3. One binary with subcommands (`yuki-shell dock`, `menubar`, ...), many processes. `panic = "abort"` in release.
4. Supervision by systemd user units (`Restart=on-failure`, short `RestartSec`, retry limit, logs and `yuki-session.target` after importing `WAYLAND_DISPLAY`). In `--drm` the compositor starts and stops the target; it does not supervise processes. The units start the binary in `MALAYSIA_BIN_DIR`, which the compositor imports along with `WAYLAND_DISPLAY`: the shell is always the running compositor's, the only one Infinity trusts. If the system installs the units in `/etc/systemd/user`, before starting the target the compositor removes the links in `~/.config/systemd/user` with their names that point to another package or nowhere, and reloads the user manager.
5. The compositor serves layer-shell, `ext-session-lock-v1`, foreign-toplevel list/management, xdg-activation and idle-notify. System state goes over D-Bus directly.

## Consequences

Isolation per component; the only unrecoverable crash is the compositor's.

## Rejected alternatives

- Shell inside the compositor: a shell crash brings down the session.
