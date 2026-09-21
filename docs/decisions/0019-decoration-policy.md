# 0019 — Decoration policy: honour what the client negotiated

Status: **accepted.** Supersedes ADR 0017, which configured server-side mode for
every client that bound `zxdg_decoration_manager_v1` and so put Neptune's
titlebar on top of a self-decorating client's own.

## Context

Neptune draws chrome for server-decorated windows (ADR 0014): a titlebar, a
hairline, an inner line, a shadow. A client only leaves that chrome to the
compositor if the two agree; otherwise both draw and the window shows two
titlebars. The agreement protocols are `zxdg_decoration_manager_v1` and the
older `org_kde_kwin_server_decoration_manager`. Clients differ in which they
speak and in whether they accept server-side mode at all.

### How other compositors decide

- **sway.** Creates both managers with the KDE default mode `server`
  (`sway/server.c:387-412`). `set_xdg_decoration_mode` defaults to
  `SERVER_SIDE`; for a **floating** view it honours the client's requested mode,
  and for a tiled view it keeps server-side and draws its own border
  (`sway/xdg_decoration.c:66-92`). A client with no xdg-decoration object is
  treated as client-decorated: `handle_map` falls back to the KDE object and
  `csd = !deco || deco->mode == CLIENT` (`sway/desktop/xdg_shell.c:479-495`);
  the KDE mode listener does the same (`sway/decoration.c:37-63`), and
  `view_update_csd_from_client` switches the container's border to `B_CSD`
  (`sway/tree/view.c:526-538`).
- **niri.** `new_decoration` and `unset_mode` choose server-side; `request_mode`
  stores **whatever the client asked for**, with a comment about an SDL2 bug
  where forcing a different mode breaks window creation
  (`src/handlers/xdg_shell.rs:978-1025`). The KDE handler replies with the
  client's mode and records whether it was server (`:1032-1067`). niri does not
  draw a titlebar at all: with `prefer-no-csd` it exposes the decoration globals
  so clients omit their CSD and draws a focus ring/border around them
  (`src/niri.rs:2396-2412`, `resources/default-config.kdl:150-154`).
- **KWin.** The window's default policy is `ClientPreference`
  (`src/xdgshellwindow.h:262`). `preferredDecorationMode`
  (`src/xdgshellwindow.cpp:1391-1443`) returns: client when no decoration plugin
  exists or the client has no decoration object at all; server when the client
  created an xdg-decoration object but never called `set_mode`; and otherwise
  the mode the client asked for over xdg-decoration, falling back to the KDE
  object. The chosen mode is sent back with the configure
  (`:1500-1530`).

All three treat *no negotiation* as self-decorating, and none of them forces
server-side against an explicit client-side request.

### What the toolkits do

- **Qt 5.15 and Qt 6** use xdg-decoration. A toplevel creates a decoration
  object unless it is frameless, then calls `unset_mode`, leaving the choice to
  the compositor (`qwaylandxdgshell.cpp:29-33,180-186`; the `requestMode` helper
  exists but is not called in 5.15 or 6.8). It suppresses client-side
  decorations while the decoration is unconfigured **or** once the compositor
  has configured server-side (`qwaylandxdgshell.cpp:113-120` in both). Without
  the global it falls back to its decoration plugin, which
  `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` disables
  (`qwaylanddisplay.cpp:888-903`). So the brief's "request server-side" is
  imprecise: Qt **defers** to the compositor and only draws CSD if told
  client-side or if the global is missing.
- **GTK3** never uses xdg-decoration. It creates an
  `org_kde_kwin_server_decoration` per toplevel and requests `client` when it
  draws CSD, `server` when it does not
  (`gdk/wayland/gdkwindow-wayland.c:2509-2535`). Which one it draws is decided
  at realize: a window with a `GtkHeaderBar` (set through
  `gtk_window_set_titlebar`) has `client_decorated` set
  (`gtk/gtkwindow.c:4198,4256`) and announces CSD; a plain window announces SSD
  (`gtk/gtkwindow.c:7592-7602`). GTK only *prefers* SSD if the manager's
  `default_mode` event said `server`
  (`gdk/wayland/gdkdisplay-wayland.c:366-393`,
  `gtk/gtkwindow.c:6132-6159`); otherwise `gtk_window_should_use_csd` is true
  and even a plain window draws CSD.
- **GTK4** is the same picture, not "always client-side": it also uses the KDE
  protocol and no xdg-decoration at all (the tree contains no
  `zxdg_decoration` reference). `gdk_wayland_toplevel_set_decorated` creates the
  KDE object and requests `server`/`client`
  (`gdk/wayland/gdktoplevel-wayland.c:1434-1456`), driven by
  `decorated && !client_decorated` (`gtk/gtkwindow.c:3264,4547`), with the same
  `default_mode` preference logic
  (`gdk/wayland/gdkdisplay-wayland.c:445-463,1273-1282`,
  `gtk/gtkwindow.c:3975-4006`). A plain GTK4 window therefore accepts server-side
  when the manager says `server`; a window with a headerbar keeps CSD.
- **libadwaita** windows always have a headerbar: `AdwWindow` and
  `AdwApplicationWindow` call `gtk_window_set_titlebar` with it
  (`src/adw-window.c:328`, `src/adw-application-window.c:280`) and error if an
  application calls `gtk_window_set_titlebar` itself (`:176`, `:128`). So
  Nautilus (GTK4 + libadwaita) is always client-decorated.
- **Frameless applications** such as OnlyOffice draw their own title area. A Qt
  frameless window never creates the decoration object
  (`qwaylandxdgshell.cpp:29-33`, guarded by `Qt::FramelessWindowHint`), so it
  never negotiates and falls under the no-negotiation rule below.

## Decision

1. A client that negotiates through xdg-decoration or the KDE protocol and
   accepts server-side mode gets compositor chrome.
2. A client that explicitly requests client-side mode, or that never negotiates
   decorations, is self-decorating: no compositor titlebar, hairline or inner
   line. **No compositor shadow either.** The client's surface shape is unknown,
   so a shadow would show through rounded or transparent corners; GTK and Qt
   draw their own shadows with their CSD.
3. The mode is settled before the first buffer attach. Every configure carries
   the mode, and the initial configure is sent when the toplevel is created. If
   the client negotiates after that, a new configure is sent, and the window does
   not map until the newest configure is acknowledged (`Windows::ack_configure`
   accepts only the latest serial).
4. If a client changes mode after mapping, the compositor recomputes the insets,
   sends a configure with the new usable size, and updates the stored mode in
   `neptune-scene` in the same step, so hit-testing and the move zone change with
   the chrome and no stale titlebar remains.

The xdg-decoration handlers answer `new_decoration` and `unset_mode` with
server-side (the client asked the compositor to choose), and `request_mode` with
the mode the client asked for. The KDE manager advertises `default_mode =
server`, so plain GTK3/GTK4 windows prefer server-side, and answers each request
with the requested mode; a client that creates the object but never requests a
mode is treated as self-decorating.

The mode is stored per window in `neptune-scene`, next to the geometry, and the
insets, hit-testing and edge zones read it from there.

`QT_WAYLAND_DISABLE_WINDOWDECORATION=1` is the fallback for Qt versions that
lack xdg-decoration; it is set only in the environment of clients Neptune
spawns, never in the session environment.

## Consequences

- ADR 0017 is reversed: server-side is the default and the answer to an
  unset mode, but an explicit client-side request is honoured.
- GTK3/GTK4 windows with a headerbar, libadwaita windows and frameless Qt
  windows keep their own chrome; plain GTK3/GTK4 and ordinary Qt windows get
  Neptune's.
- Neptune has no client-spawn path yet, so the Qt variable is not set anywhere
  today; the rule above takes effect when one exists. The test harness sets it
  for the Qt clients it launches and says so.
- A shadow for self-decorating windows is deliberately out of scope; if it is
  wanted later it needs the surface shape, which the protocol does not provide.

## References

- `docs/decisions/0017-xdg-decoration-is-server-side.md` — the superseded
  policy.
- Smithay `src/wayland/shell/xdg/decoration.rs`, `src/wayland/shell/kde/decoration.rs`.
- `crates/neptune/src/handlers.rs` — both decoration handlers.
- `crates/neptune/src/window.rs`, `crates/neptune-scene/src/scene.rs` — where
  the per-window mode lives and what reads it.
