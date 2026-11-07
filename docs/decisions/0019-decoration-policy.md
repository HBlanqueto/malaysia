# 0019 — Decoration policy: honor what the client negotiated

Status: accepted. Supersedes ADR 0017.

## Context

The compositor draws chrome for SSD windows (ADR 0014).

## Decision

1. A client that negotiates through xdg-decoration or KDE and accepts server-side gets the compositor's chrome.
2. A client that asks for client-side, or never negotiates, decorates itself: no titlebar, hairline or inner line. ADR 0020 later adds a shadow for all of them.
3. The mode is decided before the first attach; every configure carries it. If it changes later, the insets are recomputed, a configure is sent and the scene is updated.
4. `new_decoration`/`unset_mode` answer server-side; `request_mode` answers what was requested. The KDE manager advertises `default_mode = server`, which makes plain GTK3/GTK4 prefer SSD. A KDE object without a request is treated as self-decorating.
5. GTK4 requests the mode before its toplevel exists; it is remembered and applied when the window is created. GTK/libadwaita with a headerbar and frameless Qt keep CSD; ordinary Qt defers to the compositor.
6. The mode is stored per window in `malaysia-scene`. `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` is the fallback for Qt without xdg-decoration, only in clients launched by the compositor.

## Consequences

ADR 0017 is reverted: server-side is the default, but explicit client-side is honored.

## Rejected alternatives

- Always forcing server-side (ADR 0017): it duplicates chrome in self-decorated apps.
