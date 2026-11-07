# 0054 — Overlaid titlebar in focus

Status: accepted (2026-11-01). Implemented in barrierkit (`ce12d36`,
`3841a09`), in the compositor and in the libadwaita fork (`605edc1`); it still
needs to be accepted in a session. It changes ADR 0051 §6 for CSD apps.

## Context

In focus mode, with the pointer at the top, the compositor lowered the CSD
window (barrierkit, libadwaita) by 28 px so that its bar sat below the black panel.
The whole window slid and lost 28 px at the bottom; in libadwaita
(ADR 0053) the headerbar also pushed the content when it appeared. In macOS
the window does not move: the toolbar comes down **over** the
content, as the Tengen titlebar of an SSD app already does here.

## Decision

1. **Protocol** `infinity_focus_v1` **version 2** (barrierkit):
   - `infinity_focus_surface_v1.set_overlay`: the app draws its titlebar
     over its content, below the bar; the compositor does not move the
     window.
   - Event `bar_height(height)`, sent right before each `bar(1)` to
     whoever requested `set_overlay`: the height the bar covers, in
     surface logical pixels.
   - A v1 client, or one without `set_overlay`, behaves as in ADR 0051: the
     window moves down.
2. **Compositor:** it offers v2. When the bar comes down over a CSD window
   in focus, it only shifts it (scene and drawing) if it did not request `set_overlay`.
   `FocusReveal` separates the notified window (`notified`) from whether it was shifted
   (`shifted`, which is kept while the bar goes up for drawing).
3. **barrierkit:** binds up to v2 and calls `set_overlay`. In fullscreen
   the `Titlebar` leaves the layout (the content takes the whole window); with
   the bar down it is drawn over the content at `bar_height`, in a
   layer in front of the content and behind menus and popups. The
   sidebar traffic lights move down by the same amount.
4. **libadwaita (fork):** the `AdwToolbarView`s controlled by focus mode
   (ADR 0053) extend their content to the top edge during the whole
   fullscreen and, with the bar down, show their top bars
   on top, at `bar_height`, with the `raised` style (opaque, with a shadow). It is a
   private mode of the view: it does not change `extend-content-to-top-edge` or
   `top-bar-style`. Views managed by the app are not touched.

## Consequences

- Neither barrierkit apps nor libadwaita apps slide: the content
  stays still and the titlebar covers its top part while the bar
  is down, like the SSD titlebar.
- A barrierkit app in focus no longer shows its titlebar without the bar
  (before, it kept it, only without traffic lights).
- A libadwaita view that the app manages by itself (a viewer that shows its
  headerbar when the mouse moves) can end up below the black bar with the bar
  down, because the window no longer moves down.
- Apps with version 1 (earlier barrierkit or libadwaita, other apps)
  keep sliding: the compositor keeps that path.

## Rejected alternatives

- **Keeping the slide only for barrierkit:** two behaviors
  in the same focus mode.
- **Letting the app guess the bar height:** `MENUBAR_HEIGHT` exists in
  `ctdesign`, but libadwaita does not know it and the bar may measure something
  else; the compositor sends it.
- **Public `top-bar-style = raised` and `extend-content-to-top-edge`:**
  they notify the app and overwrite values the app may watch.

## Later changes

Changes in barrierkit since this ADR was accepted. The details of each case are
in [chapter 9 of its documentation](https://github.com/HBlanqueto/barrierkit/blob/main/docs/barrierkit/09-titlebars.md#maximized-full-screen-and-focus-mode).

- **Fullscreen bar** (`FullscreenBar`, barrierkit ADR 0057): the
  overlaid titlebar is drawn plain (`Solid`, the default) or with glass that
  blurs what is below (`Glass`, `ProgressiveGlass`).
- **Pushing the content** (`WindowOptions::fullscreen_push`, `4de4079`): with
  the bar down, the content moves down by the height of the bar and the titlebar instead
  of being covered. It combines with any `FullscreenBar`.
- **Live** (`d07d84e`): the bar and the push are states of
  `WindowControls`; the app changes them without reopening the window.
- **Rail** (`bcc1ecd`): with `TitlebarLayout::Rail` the top titlebar does not
  leave the layout or overlap, and nothing moves down with the bar. Only the rail comes in
  from the left, with its traffic lights below the bar.
- **Previews** (`d07d84e`): a `Titlebar::preview` inside the content
  does not read the window state, so it does not overlap in fullscreen.
