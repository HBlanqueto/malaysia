# 0056 — X11 apps with xwayland-satellite

Status: accepted (2026-11-03). Tested against headless weston; still pending in a session
with real apps (OnlyOffice).

## Context

Malaysia did not run X11 apps: smithay is built without the `xwayland` feature
and the window model (`window.rs`) only knows `xdg_toplevel`. Integrating
Xwayland requires writing an X11 window manager and a second kind of
window in focus, placement, decoration and effects.

[xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite) acts
as the X11 manager and presents each X11 window as an `xdg_toplevel`. It only needs
`xdg_shell` and `wp_viewporter`, which Malaysia already serves. niri integrates it the same way
since 25.08.

## Decision

1. `xwayland.rs` starts satellite when the Wayland socket opens, in a session
   (`udev.rs`) and nested (`session.rs`):
   1. It reserves the first free display with `/tmp/.X{n}-lock` (PID in 10
      columns). A lock whose PID no longer exists is deleted.
   2. It listens on `/tmp/.X11-unix/X{n}` and on the abstract socket with the same
      name. The directory must belong to root or to the user.
   3. It launches `xwayland-satellite :{n} -listenfd <fd> -listenfd <fd>` through
      `spawn`. An X11 app that connects before Xwayland is ready waits
      on the socket instead of failing.
   4. If satellite dies after 5 s or more, it relaunches it; if it dies earlier, it gives up and
      logs it.
2. `DISPLAY` is not written into the process environment: `spawn` adds it to each
   child that does not set `DISPLAY` itself, and the session imports it into the user
   manager (`systemctl --user import-environment`).
3. The package puts `xwayland-satellite` in the `PATH` of the compositor and of the dev
   shell. Without it, the compositor keeps running and warns that there is no X11.
4. X11 windows arrive as `xdg_toplevel`: they get focus, effects and
   decoration like the rest (ADR 0019/0020). A frameless X11 window
   (`_MOTIF_WM_HINTS`) stays CSD.

## Consequences

- X11-only apps, or those that force `QT_QPA_PLATFORM=xcb`, open.
- Xwayland runs for the whole session, even if there are no X11 apps.
- satellite applies the scale with `wp_viewporter`.
- The global menu of X11 apps (`com.canonical.AppMenu.Registrar`) still does not
  match windows (block 14).

## Rejected alternatives

- **smithay's XWM (`xwayland` feature):** a second kind of window throughout the
  compositor.
- **On-demand startup:** it saves memory, but it requires watching the sockets in
  the event loop; it can be added on top of this base.
- **Separate systemd unit:** it does not know which display is free and does not export it.
