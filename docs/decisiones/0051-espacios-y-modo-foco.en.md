# 0051 — Spaces and focus mode

Status: accepted (2026-11-01). Steps 1–6 implemented; step 6 is not yet
accepted in a session (ADR 0053).

## Context

Malaysia had no desktops: all windows lived on a single plane, and the
green button in `focus` mode (`window.green_button`) only logged the
request. The reference model is macOS: Spaces in a row and fullscreen
as its own space. The concept was fixed with an interactive mockup
(2026-11-01) that covers desktops, focus, the top edge and the three kinds of
app.

## Decision

1. **Spaces in a row.** There are two kinds: desktop and focus space. A
   new one is added at the end of the row, on the right. At most 99 desktops;
   focus spaces do not count. The row does not wrap around.
2. **Windows.** A new window goes to the shown desktop. A dialog goes
   with its parent. If a focus space is shown, the new window goes to its
   desktop and the view switches to it. Activating a window in another space (dock,
   xdg-activation) shows its space.
3. **Shortcuts.** `Ctrl+Alt+←/→` switches space and `Ctrl+Alt+N` adds a
   desktop and shows it. macOS uses `Ctrl+←/→`, but on Linux that shortcut moves
   the cursor by words in apps, so Alt is added.
4. **Sliding.** Spaces slide horizontally with a black vertical bar
   between them. Each desktop has its own top bar,
   which leaves with it. Only the dock stays fixed and comes along.
5. **Maximize = focus.** The green button (`window.green_button = "focus"`) creates
   a focus space right to the right of the window's desktop. In a
   single animation, the window grows to cover the output while the
   desktop slides; the dock does not move and ends up covered. The
   window gets the xdg `fullscreen` state, not `maximized`, so that
   apps with an immersive mode apply it. Leaving runs the reverse animation and
   deletes the space. Closing the window deletes it too.
6. **Top edge in focus.** With the pointer at the top edge, the
   panel appears in pure black with white text. What happens to the window depends
   on the kind of app:
   - **Third-party app with SSD** (Qt/KDE such as Kate, Firefox): the compositor lowers its
     Tengen titlebar over the content, below the panel.
   - **barrierkit app with CSD**: the app shows its titlebar and
     its traffic lights again, like Finder. The app learns about it through
     `infinity_focus_v1`. Since ADR 0054 the window does not move: the
     titlebar is drawn over the content, below the panel. Before (and with
     v1 clients) the compositor slides the whole window 28 px down.
   - **libadwaita/GTK**: the same as with barrierkit is used. The app keeps
     its headerbar (ADR 0020 §1). Apps with an immersive mode hide the
     headerbar themselves when they receive `fullscreen`. With the
     libadwaita fork (ADR 0053), the headerbar hides in focus and appears with the
     bar over the content (ADR 0054).
7. **Model without Smithay.** The row lives in `malaysia_scene::spaces` (ADR
   0001). The compositor removes from the scene the windows of spaces that are not
   shown with `Windows::set_off_space`, which is independent from minimizing,
   so drawing, hit-testing and focus ignore them without further checks.

## Plan

1. Spaces model and instant switching with shortcuts. **Done.**
2. Sliding with a black separator, one bar per desktop and a fixed dock.
   **Done.** The shell still has a single menubar: during the
   slide the compositor draws that surface inside each desktop,
   together with its wallpaper, so each one carries its bar and leaves with it. The
   top bar is the Top/Overlay panel anchored at the top, full width and with an
   exclusive zone. Duration, curve, width and color of the strip are
   `vow-design` tokens (`SPACE_*`).
3. Focus mode: focus space, `fullscreen` state and the combined animation.
   **Done.** F11 (`set_fullscreen`), the own green button
   of a CSD app with `green_button = "focus"` and `Ctrl+Super+F` also enter focus. During the
   animation the client surface is drawn clipped to a rounded
   rectangle that grows or shrinks, without the SSD bar; if the client has not yet
   redrawn at its new size, the rest of the rectangle shows black.
4. Top edge: black panel, SSD titlebar on top and a 28 px slide.
   **Done** in the compositor: the bar comes down over a black strip and the
   CSD window moves in the scene, not only in the drawing. The bar in pure
   black with white text needs the shell to know that it is over a focus space
   (step 5).
5. Infinity: current space for the shell and focus/panel notice for
   barrierkit. **The shell part is done:** Infinity v5 has the
   `space` event with the bar state (`focus_bar`), and over a focus space the
   menubar is painted in pure black with white text (`FOCUS_BAR_*` in
   `vow-design`). **The notice to apps is also done:** since Infinity
   is only bound by the trusted shell, it goes through a separate protocol,
   `infinity_focus_v1`, open to any client because it gives no geometry
   or permissions: `bar(1)`/`bar(0)` per surface when the bar comes down or goes up
   over a CSD window in focus. The kit hides its traffic lights in fullscreen and
   shows them with `bar(1)`.
6. Experiment with a libadwaita fork
   ([HBlanqueto/libadwaita](https://github.com/HBlanqueto/libadwaita)):
   `AdwToolbarView` hides its top bars in fullscreen and shows them again
   with the pointer at the top. It is installed with `system.replaceDependencies`
   so apps are not rebuilt. Flatpak apps do not get it. **Done**
   (ADR 0053): with the `infinity_focus_manager_v1` global, the outer
   `AdwToolbarView`s of a fullscreen window set
   `reveal-top-bars = FALSE` and return it to `TRUE` with `bar(1)` and when
   leaving fullscreen; those the app already manages are not touched. With ADR 0054 the
   window no longer moves down 28 px: the headerbar goes over the content. Patch
   `nix/patches/libadwaita-focus-mode.patch`, in the same package as the global
   menu. It still needs to be accepted in a session.

## Rejected alternatives

- **Hiding the headerbar from the compositor** (moving the window up 47 px):
  no protocol says where the headerbar ends, and with tabs, search
  bars or adaptive layouts content would be cut off.
- **SSD titlebar over the libadwaita headerbar**: it leaves two bars with the
  same title and covers the app's buttons.
- **`Ctrl+←/→` without Alt**: it breaks word movement in apps.
