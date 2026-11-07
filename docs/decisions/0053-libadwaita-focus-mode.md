# 0053 — Focus mode in libadwaita

Status: accepted (2026-11-01). Implemented in the fork
[`HBlanqueto/libadwaita`](https://github.com/HBlanqueto/libadwaita) (branch
`claude/optimistic-bell-p2v9or`, `f2ce550`) and in the
`libadwaita-global-menu` package; it still needs to be accepted in a session with real apps. It is
step 6 of ADR 0051 (17f).

## Context

In focus mode (ADR 0051 §5–§6) a libadwaita app keeps its headerbar: the
window is fullscreen and the headerbar takes the top part all the time.
In macOS, a fullscreen app hides its toolbar and
shows it only with the menu bar. barrierkit apps already do this with
`infinity_focus_v1` (17e). Hiding the headerbar from the compositor is
rejected (ADR 0051): nobody knows where it ends.

Option C is for the app to do it: `AdwToolbarView` already has
`reveal-top-bars`, with its animation, and libadwaita documents it for this.

## Decision

1. **In the fork** (`src/adw-focus-mode.c`), hooked into
   `AdwToolbarView::root`/`unroot`. No new public API.
2. **Only with the global.** The first view of a window binds
   `infinity_focus_manager_v1` through `wl_registry` (once per `GdkDisplay`).
   Each realized window creates an `infinity_focus_surface_v1` with its
   `wl_surface` and destroys it when unrealized. Without the global, without the GTK
   Wayland backend or with `ADW_DISABLE_FOCUS_MODE`, nothing changes.
3. **What it does.** With the window in fullscreen, the taken views have
   `reveal-top-bars = FALSE`; with `bar(1)` they switch to `TRUE` and with `bar(0)`
   back to `FALSE`. When leaving fullscreen they return to `TRUE`. The `bar`
   notices are applied in an idle, so a `bar(0)` and leaving fullscreen
   in the same batch do not hide the bar only to show it right away.
4. **Which views.** The window's `AdwToolbarView`s, on its own
   surface (not in a popover), outside an `AdwDialog`, with some visible top
   bar and without an `AdwToolbarView` ancestor with top
   bars. This way the outer headerbar goes and, in a split view, both
   go, but not the one of a view nested in another view's content.
5. **Apps that already manage `reveal-top-bars`** (viewers, players):
   - Views are taken in an idle after `notify::fullscreened`, when the
     app's handlers and bindings on `fullscreened` have already run. If
     a view has its bars hidden at that point, it belongs to the app during that
     fullscreen.
   - If the property changes on a taken view and the module did not change it, the
     app has taken it: it is no longer touched and is not restored on exit.
   - When leaving fullscreen everything is forgotten and the next one looks again.
6. **The compositor does not change.** It still lowers the CSD window 28 px with the
   bar: that is what leaves the newly shown headerbar below the panel, the same
   as barrierkit's traffic lights. **Revised by ADR 0054:** with
   version 2 of the protocol the window does not move down and the bars go over the
   content, at `bar_height`.
7. **Nix package.** `nix/patches/libadwaita-focus-mode.patch` is applied
   after the global menu one on top of the `1.9.3` tag, with `wayland-scanner` and
   `wayland` added to the package. It uses only GTK <= 4.22 API and goes in through the
   same `system.replaceDependencies` (ADR 0052 §5). The package keeps the
   name `libadwaita-global-menu` so the module is not touched.
8. The protocol XML is copied from barrierkit
   (`crates/infinity-protocol/protocols/infinity-focus-v1.xml`) to
   `src/protocols/` in the fork, with its MIT notice and a comment with its
   origin. If the protocol changes, it must be copied again.

## Consequences

- libadwaita apps in focus use the whole screen and show the headerbar
  with the pointer at the top, like barrierkit and like macOS.
- The headerbar reappears by resizing the content (it does not overlap):
  that is what `reveal-top-bars` does without `extend-content-to-top-edge`.
  Only with v1 compositors; with v2 it overlaps (ADR 0054).
- When leaving focus with the bar down, the compositor sends `bar(0)` when
  the animation starts and fullscreen ends at the end: the headerbar
  hides during the shrink and comes back afterwards. The same happens with
  barrierkit's traffic lights.
- Flatpak apps, with the libadwaita of their runtime, do not get it.
- A view in a bottom panel without an ancestor with bars would also be
  taken; this has not been seen in real apps.

## Rejected alternatives

- **Doing it in `AdwApplicationWindow`/`AdwWindow`:** it leaves out
  `GtkWindow`s with `AdwToolbarView` and forces walking the tree on each
  notice. Views already know when they enter and leave a window.
- **Deciding by geometry** (views stuck to the top edge): the position
  is not allocated at root time and changes when bars hide.
- **Marking the app's own views at root time:** an app that hides the
  bars when entering fullscreen does so later; it only shows at that moment.
