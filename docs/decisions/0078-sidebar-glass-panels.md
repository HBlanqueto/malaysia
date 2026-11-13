# 0078 — Glass panels for BarrierKit sidebars

Status: accepted (2026-11-13). Nothing is implemented yet; the order is in «Plan».

## Goal and scope

This ADR fixes how the compositor draws the material of a BarrierKit sidebar:
a rounded panel of glass over the app's solid window. It covers the Infinity
request that describes the panel, the material, its rim, its states and what
it costs. It starts when a kit window commits a panel and ends when the
material reaches the frame. How the kit picks the colors and paints the rows is
[barrierkit ADR 0065](https://github.com/HBlanqueto/barrierkit/blob/main/docs/decisions/0065-sidebar-glass.md).
Readers: whoever works on `malaysia-compositor`, `grade-graphics` or the
Infinity protocol.

Labels: [S] specification or upstream text, [W] press or forum, [M] measured on
a capture, [O] observed by the project owner on macOS, [I] inferred.

## Context

### What macOS Tahoe does

1. Apple calls the sidebar a layer of glass that floats above the content
   of the app. Its curvature sits inside the window's corner. Each layer of the
   glass adapts to what is behind it, and the light that defines its silhouette
   responds to geometry ([WWDC25 219](https://developer.apple.com/videos/play/wwdc2025/219/),
   [WWDC25 310](https://developer.apple.com/videos/play/wwdc2025/310/)) [S].
2. The press disagrees on how much of the wallpaper shows. A reader in the
   [MacRumors review](https://www.macrumors.com/2025/08/01/macos-tahoe-review/)
   comments says «for the majority of windows in Tahoe it's just white
   underneath». [MacStories](https://www.macstories.net/stories/macos-26-tahoe-the-macstories-review/2/)
   says the wallpaper shines through more than in Sequoia [W].
3. One capture of the Finder in dark mode, on one wallpaper (2000×1250 px),
   gives these numbers. Lightness and chroma are Oklab [M]:

   | Part | Measured |
   |---|---|
   | Panel lightness | L 0.231 to 0.243 over the whole height. The ring around it is 0.252, the detail pane 0.263 and the wallpaper behind 0.20 to 0.22. |
   | Panel color | Chroma 0.008 to 0.013, hue 220° to 254°. The wallpaper has chroma 0.019 to 0.034, hue 241° to 253°. The ratio is 0.38 to 0.43 at both ends. |
   | Ring | About 8 px of the window's own color. |
   | Rim | A line of 1 to 2 px. Over the fill it adds 0.02 to 0.09 of lightness along the left edge, 0.02 to 0.045 along the top and 0.036 to 0.043 along the bottom. |
   | Rim and wallpaper | On the top and bottom edges, where the wallpaper beside the window is visible (24 points): rim − fill = 0.26 × wallpaper + 2.5, per channel, in 0 to 255. R² is 0.63 and the mean error 2.7. |

   The lightness comes from the app's solid layer. From the wallpaper comes only
   chroma, about 0.4 of the local one. The rim is not one tint: it is the fill
   plus a part of the wallpaper behind each stretch of the edge.

4. With the window not focused, the sidebar stops blurring and turns into a
   solid tone computed from the solid color and the wallpaper tint. In the
   island style the rim then disappears into the fill. «Reduce transparency»
   keeps the sidebar solid and neutral, focused or not [O].

Limits of the evidence: one dark capture and one wallpaper. Light mode, other
wallpapers and the unfocused state were not measured. The left and right edges
have no visible wallpaper to compare with, so the rim rule comes from the top
and the bottom.

### What Malaysia has today

1. The standard protocol takes a `wl_region`, which is a set of rectangles
   (ADR 0071). It cannot say a corner. The client draws over the blur, so it
   cannot take only the chroma of what is below. It does not see the wallpaper,
   so it cannot paint a rim that changes along the edge.
2. `set_cards` (Infinity, version 4) already lets a client name rectangles with
   a continuous corner and have the compositor draw the frame around them
   (ADR 0068, `window_style::card_corner`).
3. The wallpaper is blurred once per output and sampled by every surface
   (ADR 0071, step 6).
4. `take_chroma` keeps the lightness of a gray and moves its hue toward the
   wallpaper's. `wallpaper-tinting` gives it 0.15 in dark mode and 0.12 in
   light mode, times 2.0 for sidebars (ADR 0065).
5. `is_kit_client` tells a BarrierKit surface from any other (ADR 0035).

## Decision

1. **Only sidebars of BarrierKit windows.** The detail, the title bar and the
   inspector never get glass. Another client gets nothing: the compositor
   ignores `set_glass` from a surface that is not a kit toplevel. General blur
   stays a separate, optional mode (ADR 0071, step 4.1).
2. **A new request in `infinity_window_style_v1`, version 6.**
   `set_glass(surface, radius, panels, base, solid)`:
   1. `radius` is the corner in logical pixels, continuous as in ADR 0059.
   2. `panels` is an array of int32 quadruples x, y, width and height in
      surface-local logical pixels, clipped to the window geometry. At most 16.
   3. `base` and `solid` are `0xAARRGGBB`, alpha ignored. `base` is the solid
      tone with the app's lightness. `solid` is `base` with the wallpaper tint.
   4. It is double-buffered and applies on the next `wl_surface.commit`, like
      `set_blur_region`, so a panel moves with the content it belongs to. An
      empty array removes the panels. It lasts for the lifetime of the surface.
   5. The client leaves the panels transparent. The compositor fills them.
3. **The material of an active panel** is, for each point p of the panel:
   `take_chroma(base, B(p), chroma)`, where B is the blurred wallpaper of
   ADR 0071, step 6. It is always `"wallpaper"`: the windows behind add nothing
   measurable, so `appearance.blur_behind` does not apply to panels.
4. **The rim** is drawn above the surface along the edge of each panel, with
   additive blending: `fill + rim_weight × B(p) + rim_highlight`, `rim_width`
   wide, on the same corner. B is the same texture, so the rim changes color
   and brightness along the contour.
5. **States**, decided by the compositor:

   | State | Panel | Rim |
   |---|---|---|
   | Toplevel active | Material of step 3 | Rim of step 4 |
   | Toplevel not active | Flat `solid`, no blur | None. In the island style the rim disappears into the fill |
   | `reduce_transparency` or `contrast` | Nothing. The kit paints a neutral solid itself | None |

   The change between active and not active fades over `ctdesign::DURATION_BASE`.
   The flat sidebar's 1 px line is drawn by the client and stays in every state.
6. **Where it is drawn.** The material goes directly under the surface's
   elements, above the window's shadow. The rim goes directly above them and
   below the window's own border (ADR 0059). Both are part of the window, so
   offscreen fades, minimize and captures include them.
7. **Values live in `ctdesign`.** `chroma` is 0.40, `rim_weight` 0.26,
   `rim_highlight` 0.010 (2.5 of 255) and `rim_width` 2 logical pixels, from the
   measurements above. They are first values, to calibrate in phase 6.
8. **Without this request** (another compositor, or version below 6) the kit
   paints `solid` opaque. Nothing else changes.
9. **No new configuration.** `appearance.reduce_transparency`,
   `appearance.contrast` and `appearance.blur_all_windows` keep their meaning.

## Plan

1. **Protocol and kit binding.** Version 6 of the XML in barrierkit and
   `bk`'s request, with no drawing. Then the `rev` in `Cargo.toml` here.
2. **Kit.** Tokens, panels published on layout changes, fallback to `solid`
   (barrierkit ADR 0065).
3. **Material.** Parse and cache the panels, clip to the corner, fill with the
   flat `base`, the states and the fade.
4. **Chroma.** The shader in `grade-graphics` and the sampling of the cache.
5. **Rim.** Additive element and its shader.
6. **Calibration.** Measure light mode, other wallpapers and the unfocused
   state on a Mac, and fix the values of step 7.

Each phase updates [`roadmap.md`](../roadmap.md) and the chapter that
describes what it changes, in its own commit.

## Consequences

- A kit sidebar looks like a solid panel with the wallpaper's color. Only the
  compositor can draw that, because it is the only one that sees the wallpaper.
- Other compositors get the opaque `solid` panel, with the wallpaper's tint but
  without its local color.
- The cost is one textured quad per panel and its rim. The wallpaper is blurred
  once, as before.
- The Infinity protocol grows by one request. Only kit clients use it.
- A panel that moves needs a `set_glass` per frame. The kit sends it only when
  the rectangle changes.

## Rejected alternatives

- **Standard blur plus a translucent fill in the client.** It lets the
  wallpaper's lightness through and the mockup looked too luminous. It also
  cannot say a corner.
- **The client reads the wallpaper and draws everything.** Clients do not see
  other surfaces' pixels, and a thumbnail of the wallpaper would have to be
  kept in step with every output and scale.
- **Only the wallpaper's dominant color, on the client.** It is uniform: there
  is no local color and no rim that changes. It stays as the `solid` fallback.
- **`"behind"` for panels.** The windows behind add nothing measurable and
  cost a blur per frame.
- **A fixed rim in the kit.** It cannot follow the wallpaper stretch by
  stretch, and the measurement shows that it does.
