# Process model and compositor ↔ shell split

ADR 0023: each shell component is a process and its own Wayland client. The diagram is in [`malaysia-and-shell.svg`](malaysia-and-shell.svg); the UI map, in [`interface.md`](interface.md).

## Distribution

One process per component: `malaysia-wallpaper`, `malaysia-shell menubar`, `dock`, `lock` and `launcher`. None depends on another; there is no central daemon. `xdg-desktop-portal-malaysia` is a separate D-Bus backend that can crash without bringing down the session.

## Division of work

| Aspect | Who decides |
|---|---|
| Protocols and composition order | compositor |
| Geometry (position, size, anchors, exclusive zone) | compositor |
| Outline effects (shadow, border, radius, future blur) | compositor |
| Content of the shell surfaces | shell |
| Background | `malaysia-wallpaper` |
| SSD titlebar | compositor |
| Screenshots | compositor (no shell process) |

The shell **requests** placement; the compositor validates it and decides. Each one rules its own layer, like the Dock and WindowServer in macOS.

## Communication

Each component reads `~/.config/malaysia/` and `~/.local/share/malaysia/`; the compositor serves layer-shell, foreign-toplevel, session-lock and activation, and wraps the shell surfaces with the ADR 0020 effects.
