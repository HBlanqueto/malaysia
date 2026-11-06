# 0059 — Radius per bar kind and continuous corner

Status: accepted (2026-11-06). Supersedes 0058. Still to be tested in a session.

## Context

In macOS Tahoe a window's radius depends on its bar.
`-[NSThemeFrame _getCachedWindowCornerRadius]` on macOS 26.6.2 gives
([reverse-engineering notes](https://github.com/mansarip/mac-window-corners/blob/main/macos-tahoe-window-corner-radius.md)):

| AppKit window | Radius |
|---|---|
| Titlebar only; `expanded` or `preference` toolbar | 16 |
| `unifiedCompact` toolbar | 20 |
| `unified` or `automatic` toolbar; sheets and alerts | 26 |
| Panels and utility windows | 15 |

The shape is not an arc either. AppKit builds the mask with
`ceil(r × cornerCurveExpansionFactor)`, about 1.53 × r: Core Animation's
continuous curve. That curve starts 1.5287·R from the vertex and its
curvature grows from 0 ([PaintCode](https://www.paintcodeapp.com/news/code-for-ios-7-rounded-rectangles),
[NativeScript #11452](https://github.com/NativeScript/NativeScript/issues/11452)).
The compositor drew a circular arc (`superellipse-exponent` 2.0), whose
curvature jumps from 0 to 1/R at the edge.

ADR 0058 only told "with a toolbar" (26) from "without one" (16).

## Decision

1. **Protocol** `infinity_window_style_v1` (barrierkit): `set_toolbar`
   becomes `set_bar(wl_surface, bar)`, with the `bar` enum: `titlebar`,
   `compact`, `unified` and `utility`. A later request replaces the
   earlier one.
2. **Tokens** (`vow-design`): `components.window.corner` →
   `WINDOW_CORNER`, with the same names as the `bar` enum: `titlebar`
   (16), `compact` (20), `unified` (26) and `utility` (15), plus the
   continuous corner's `expansion` and `exponent`.
3. **Compositor**, in `window_style::corner_radius`:

   | Window | Radius |
   |---|---|
   | Sends `bar` | its bar's |
   | Dialog (toplevel with a parent) without `bar` | 26, like sheets and alerts |
   | CSD without `bar` (GTK, Electron) | 26: its headerbar is a unified bar |
   | SSD (compositor titlebar, 32 pt) | 16 |
   | Fullscreen | 0 |

4. **Continuous corner:** the frame, shadow, border and blur use a
   superellipse that starts at `WINDOW_CORNER.expansion` (1.52866) × R with
   `WINDOW_CORNER.exponent` 3.34. It deviates from Apple's curve by at most
   0.0076·R (0.2 pt at R = 26); the arc deviated 0.014·R. The shader caps
   the extent at half the shorter side.
5. **barrierkit:** a `close_only` `Titlebar` sends `titlebar`; with an
   island sidebar (52 pt), `unified`; the rest (40 pt), `compact`. If the
   sidebar style changes, it sends it again.
6. **User's fixed radius:** `window.corner_radius` in `window.domain`
   wins over the table and over `set_bar`, like macOS
   `NSConvolutionOverride1` ([options](../configuracion/opciones.en.md)).
7. **Shell alerts:** a layer with the `ALERT_LAYER_NAMESPACE` namespace
   (`malaysia-dialog`) gets the dialog frame from the compositor: `DIALOG`
   radius (26, like sheets and alerts), continuous corner, border and
   shadow. The shell draws the panel without corners; the namespace is the
   layer-shell purpose identifier.
8. **Popovers:** an xdg_popup marked with `set_popover` (protocol version 2)
   gets the `NSPopover` shape: the top band of `POPOVER.arrow_height` is the
   arrow and the body below it has the `POPOVER.radius` corner (20).
   `window_shape.glsl::frame_sdf` joins to the body a round-tipped triangle
   (`arrow_tip_radius`) with a fillet (`arrow_fillet`). The clip, the border
   and the shadow use that distance, so the arrow has a border and a shadow.
   The tip points at the centre of the anchor rectangle and stays clear of
   the corners (`popover_tip`). AppKit measurements on macOS 26.7 from the
   [`popover`](https://pub.dev/packages/popover) package.

## Consequences

- Every barrierkit window gets its bar's radius, as in Tahoe.
- GTK windows and dialogs go from 16 to 26.
- Compositor popups also use the continuous corner.
- A barrierkit app older than this change sends `set_toolbar`, which no
  longer exists, and the compositor disconnects it. All of them are built
  from the same rev.
- The sidebar island uses freya smoothing with `WINDOW_CORNER.expansion − 1` and
  stays concentric with the window. The other barrierkit controls keep a
  circular arc.

## Rejected alternatives

- **Keep `set_toolbar` and add `set_bar` in version 2:** no app uses it
  any more; it would be dead code.
- **Apple's exact curve with three cubics in the shader:** it has no
  closed-form SDF. The fitted superellipse stays under 0.3 pt.
- **Raise only the exponent without widening the extent:** with the extent
  at R, n > 2 makes the corner squarer, not softer.
