# 0078 — Glass panels for BarrierKit sidebars

Status: accepted (2026-11-13). Phases 1 to 5 are written but were not built or run in a session. The fade between states and the calibration are pending; the order is in «Plan».

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
   | Rim | A line of 1 to 2 px, a cool white. Over the fill it adds on average 7, 8 and 12 levels of 255 in red, green and blue. A constant white alpha of 0.04 (0 to 0.08) explains it with a mean error of 2.9 of 255. |
   | Rim and wallpaper | Its chroma is 0.70 of the chroma of the wallpaper beside the window (0.65 along the wallpaper's direction), against 0.40 for the fill. Its hue is 17° from the wallpaper's and 10° from the fill's (24 points, top and bottom edges). Its lightness follows the wallpaper's brightness only slightly: R² 0.24, and an error of 2.5 against 2.9 for a constant white. Along the left edge it reaches +0.09 of lightness near the top and almost nothing at the bottom; the capture does not explain that. |

   The lightness comes from the app's solid layer. From the wallpaper comes only
   chroma, about 0.4 of the local one. The rim is a white line, like the inner
   border of the windows. It does not show what is behind it, but it takes more
   of the chroma of the wallpaper behind each stretch than the fill does.

4. With the window not focused, the sidebar stops blurring and turns into a
   solid tone computed from the solid color and the wallpaper tint. In the
   island style the rim then disappears into the fill. «Reduce transparency»
   keeps the sidebar solid and neutral, focused or not [O].

Limits of the evidence: one dark capture and one wallpaper. Light mode, other
wallpapers and the unfocused state were not measured. The left and right edges
have no visible wallpaper to compare with, so that comparison comes from the top
and the bottom.

### What Malaysia has today

1. The standard protocol takes a `wl_region`, which is a set of rectangles
   (ADR 0071). It cannot say a corner. The client draws over the blur, so it
   cannot take only the chroma of what is below. It does not see the wallpaper,
   so it cannot take its chroma either.
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
4. **The rim** is the inner border of the window frame, around each panel and
   on the same corner, with no shadow and no outer border. It is the line
   `set_cards` already gets (ADR 0068), drawn above the surface. It is a white
   line with `rim_chroma` of the chroma of B(p), the blurred wallpaper behind
   that stretch, so its tint changes along the contour. It shows nothing of
   what is behind it.
5. **States**, decided by the compositor:

   | State | Panel | Rim |
   |---|---|---|
   | Toplevel active | Material of step 3 | Rim of step 4 |
   | Toplevel not active | Flat `solid`, no blur | None. In the island style the rim disappears into the fill |
   | `reduce_transparency` or `contrast` | Nothing. The kit paints a neutral solid itself | None |

   The change between active and not active fades over `ctdesign::DURATION_BASE`.
   The flat sidebar's 1 px line is drawn by the client and stays in every state.
6. **Where it is drawn.** The material goes directly under the surface's
   elements, above the window's shadow, and grows 1 logical pixel past the panel
   under the opaque ring around it. Without that overlap the antialiased edge of
   the client's hole would let what is behind the window show through. The rim
   goes directly above the surface's elements and below the window's own border
   (ADR 0059). Both are part of the window, so offscreen fades, minimize and
   captures include them.
7. **Values live in `ctdesign`.** `chroma` is 0.40 and `rim_chroma` 0.65, from
   the measurements above. The rim's white and its width are the window
   frame's own tokens. They are first values, to calibrate in phase 6.
8. **Without this request** (another compositor, or version below 6) the kit
   keeps painting its opaque sidebar. Nothing else changes.
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
5. **Rim.** The window frame's inner border around each panel, tinted with the
   wallpaper's chroma.
6. **Calibration.** Measure light mode, other wallpapers and the unfocused
   state on a Mac, and fix the values of step 7.

Each phase updates [`roadmap.md`](../roadmap.md) and the chapter that
describes what it changes, in its own commit.

## Consequences

- A kit sidebar looks like a solid panel with the wallpaper's color. Only the
  compositor can draw that, because it is the only one that sees the wallpaper.
- Other compositors keep the opaque sidebar the kit paints today.
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
  is no local color. It is what the kit paints today and stays as the fallback.
- **`"behind"` for panels.** The windows behind add nothing measurable and
  cost a blur per frame.
- **A see-through rim.** It would show what is behind the sidebar. The measured
  rim is a line that takes the wallpaper's chroma, not its pixels.
- **The kit paints the rim.** The compositor already draws the inner border of
  cards, and one line is enough.
