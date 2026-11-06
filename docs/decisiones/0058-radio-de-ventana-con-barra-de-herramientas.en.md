# 0058 — Window radius with a toolbar

Status: superseded by 0059 (2026-11-06). Implemented in barrierkit (`de059fc`) and in
the compositor; still to be tested in a session.

## Context

In macOS Tahoe a window's radius depends on its bar. Measured on macOS 26
screenshots: Terminal (title only) has 16 pt corners and Finder or System
Settings (a toolbar with a sidebar), 26 pt. The sidebar island follows the
window corner: 26 − 8 of inset = 18.

The compositor rounds every window (ADR 0020) with `vow_design::WINDOW`
(16) and does not know which CSD windows have a toolbar.

## Decision

1. **Protocol** `infinity_window_style_v1` (barrierkit), a global without
   permissions with one request, `set_toolbar(wl_surface)`: the toplevel has
   a toolbar. It lasts as long as the `wl_surface`. Without the global, or
   without the request, the window has the titlebar corner.
2. **Tokens** (`vow-design`): `WINDOW_TOOLBAR` with radius 26
   (`components.window.toolbar-radius`); the island moves to radius 18
   (`size.sidebar-radius`).
3. **Compositor:** offers the global and stores the mark in the surface
   data. The frame (corner, shadow, border and blur) and the focus-mode
   transition use `WINDOW_TOOLBAR` for those windows and `WINDOW` for the
   rest. Dialogs keep `DIALOG` and fullscreen has no corners.
4. **barrierkit:** binds the global, and a `Titlebar` that is not
   `close_only` sends `set_toolbar` when it mounts. The 44 px bar (with or
   without a sidebar) is a toolbar; the 32 px bar of «About» is a titlebar.

## Consequences

- barrierkit apps with a toolbar get the Tahoe corner and their island is
  concentric with the window.
- SSD, GTK and Qt windows keep 16: they do not send the request.
- A barrierkit app older than `de059fc` keeps 16 and the 12 px island.

## Rejected alternatives

- **Radius 26 for every window:** Terminal and title-only dialogs would be
  rounder than in macOS.
- **A request on `infinity_focus_surface_v1`:** that object carries
  focus-mode notices; it would mix two reasons to change.
- **Letting the app send any radius:** the design value lives in
  `vow-design` (ADR 0011); the app only says what kind of window it is.
