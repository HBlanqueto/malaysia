# 0066 — Accessibility › Display

Status: accepted (2026-11-08).

## Context

macOS 26 groups the vision options in System Settings › Accessibility ›
Display (Mac User Guide, «Change Display settings for accessibility»:
<https://support.apple.com/guide/mac-help/unac089/26/mac/26>). Some change the
whole screen and some change how apps draw. Apps learn the second kind from
`NSWorkspace.accessibilityDisplayShould…`.

Malaysia had only `contrast`, `reduce_transparency` and `reduced_motion`, with
no page in System Settings. The user chose which macOS options to keep:

| Option | Decision |
|---|---|
| Invert colors, Increase contrast, Reduce transparency, Show borders, Differentiate without color, Display contrast | yes |
| Text size, Menu bar size | yes |
| Shake mouse pointer to locate, Pointer size | yes |
| Color filters | yes |
| Invert colors mode (Smart), Show window title icons, Pointer colors | later |
| Dim flashing lights, Prefer horizontal text | no |

## Decision

1. **One domain.** The keys live in `appearance.domain` next to `contrast`
   (`ctappearance::AppearanceConfig`; `options.md` lists them). The slider
   steps are `ctdesign` tokens: `DISPLAY_CONTRAST`, `TEXT_SIZES`,
   `POINTER_SIZES` and `COLOR_FILTER_INTENSITY`.
2. **Whole screen in the compositor.** `display_filter.rs` renders the scene
   into a texture and draws it with one shader. The shader applies a 3×3 color
   matrix, then inversion, then contrast around middle gray. The pass runs only
   while one of `invert_colors`, `display_contrast` or `color_filters` is set.
   Screenshots and screen capture keep the unfiltered image.
3. **Color filters.** Grayscale uses Rec. 709 luma. The three color-blindness
   filters daltonize: they add back what the Machado, Oliveira and Fernandes
   (2009) simulation loses, moved into the channels the viewer sees. Intensity
   blends the matrix with the identity.
4. **App drawing in the kit.** `ControlStyle::accessible` applies
   `show_borders`, `differentiate_without_color` and `text_size`. The
   compositor shows the symbols of its own traffic lights with
   `differentiate_without_color`. `contrast` also forces reduced transparency,
   as on macOS.
5. **Pointer in the compositor.** `shake_to_locate` gates the shake detector.
   `pointer_size` multiplies the Xcursor size of the cursors the compositor
   draws; it reloads them when the value changes.
6. **Menu bar in the shell.** `menu_bar_size = "large"` uses
   `MENUBAR_FONT_SIZE_LARGE` right away, without logging out.
7. **System Settings.** Accessibility comes before Appearance. Its page shows
   the section Vision with one row, Display, which opens a page
   (`Pane::page`). Invert colors and a color filter other than Grayscale turn
   each other off, as on macOS.

## Consequences

The filter pass costs one full-screen render per frame and turns off direct
scanout while it is on.

`pointer_size` reaches the cursors the compositor draws: its own and the ones
apps ask for by name (`wp_cursor_shape_v1`). A cursor that an app draws
itself, as a surface, keeps the app's size. This is intended: like a game
with its own cursor setting, that app owns its pointer.

Text size reaches only BarrierKit apps.

## Rejected alternatives

- The DRM color transformation matrix and gamma LUT: not every driver has them,
  and the nested session would lack the filters.
- A separate `accessibility.domain`: `contrast` and `reduce_transparency`
  already live in `appearance.domain`, and the kit reads one file.
