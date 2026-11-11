# 0065 — Wallpaper tinting per window

Status: accepted (2026-11-08), amended 2026-11-11 (steps 8 to 10). The
amounts are provisional until they are measured on macOS (step 4 of
«Calibration»).

## Context

macOS tints each window background with the wallpaper behind it. Apple
describes it in WWDC 2018, session 210 («Introducing Dark Mode»): the system
samples the part of the desktop behind the window, takes its average color and
blends it with a gray; that gray is the window background. The color follows
the window as it moves, and the app never sees it in its `NSColor` values. The
Graphite accent turns it off. That session says window, under-page and content
backgrounds take it and the sidebar does not. macOS 26 does the opposite: two
screenshots of System Settings, with the switch off and on and the window in
the same place, show the sidebar going from `#141414` to `#131418`, while the
window background (`#232323`) and the groups (`#2a2a2a`) do not change. macOS 11 added the switch «Allow wallpaper tinting in
windows»; macOS 26 names it «Tint window background with wallpaper color». It
shows in dark mode only.

Malaysia had `window_color = "wallpaper"`: one dominant color of the whole
wallpaper tinted every window, the menu bar and the Dock, and became the
accent. It did not depend on the window's position.

Apple does not publish the blend amount. A screenshot of macOS 26 with the
switch off gives a neutral `#232323` window and `#2a2a2a` groups, the same as
`ctdesign`'s dark `bg-window` and `bg-surface`.

## Decision

1. **A switch, like macOS.** `appearance.wallpaper_tinting` (default `true`)
   replaces `window_color = "wallpaper"`. An old `"wallpaper"` reads as
   `"neutral"`. `tint_strength_wallpaper`, the wallpaper accent and the
   wallpaper tint of the menu bar and the Dock go away.
2. **Dark mode only.** `wallpaper_tinting_in_light` (default `false`, System
   Settings › Accessibility) extends it to light mode.
3. **Per window.** `malaysia-wallpaper` writes a grid of average colors
   (`WALLPAPER_TINTING_GRID` cells per side) to `wallpaper.state`. The
   compositor averages the cells behind each window, with the wallpaper
   scaled to cover the output, and sends the color with the `wallpaper` event
   of `infinity_focus_surface_v1` version 3. It sends it again only when it
   changes.
4. **Blend in the kit.** `Resolved::behind_window(color)` mixes the color into
   `sidebar.bg` by `WALLPAPER_TINTING` (light 0.07, dark 0.09), as macOS 26
   does. The window background, groups, controls and menus stay as they are.
5. **When it applies.** With the switch on, a neutral `window_color` and any
   accent but graphite. The accent window color wins over it.
6. **One tint control (2026-11-09).** System Settings joins both tints:
   «Tint windows» goes from Neutral (both off) to Intense and sets
   `tint_strength`, which also multiplies `WALLPAPER_TINTING` (×0.5 to ×2);
   «Tint color» picks the accent or the wallpaper behind each window. The retired
   key `tint_strength_accent` is no longer read; a write removes it.
7. **The wallpaper accent comes back as a swatch (2026-11-09).** The first
   accent swatch, a rainbow like macOS «Multicolor», sets `wallpaper_accent`:
   the accent is the dominant color of `wallpaper.state`. The traffic-light
   style `wallpaper` goes away and reads as `accent`, which follows it.
8. **The rainbow swatch is «Automatic» (2026-11-11).** It replaces step 7:
   `wallpaper_accent` is retired and «Automatic» is `accent_color` left
   out. The accent is the theme's blue, and an area with its own accent
   (`Accent::Own`, `Accent::FromBackdrop`) or an app reading the portal keeps
   it; the portal reports no accent preference. A chosen color is forced on
   every app.
9. **The wallpaper tints like the accent (2026-11-11).** It replaces steps 4
   and 5: `behind_window` tints every part of `tint_parts`, with the same
   `LAYER_FACTOR_*` as the accent tint, and any accent allows it, graphite
   included. The sidebar-only blend tinted too little to see, and users chose
   the parts anyway.
10. **The bars are a part (2026-11-11).** `tint_parts` gains `bars`, the menu
    bar and the Dock, and `window_tint_scope` is retired. With the wallpaper
    as tint color they take the wallpaper's dominant color.

## Calibration

1. On a Mac, choose a solid color wallpaper (System Settings › Wallpaper ›
   Colors), for example pure blue `#0000ff`.
2. Open System Settings in dark mode over the wallpaper.
3. Take a screenshot with «Tint window background with wallpaper color» on,
   then another with it off, without moving the window.
4. Read the same sidebar pixel in both, `T` (on) and `G` (off), and the
   wallpaper color `W`. The amount is `(T − G) / (W − G)` per channel.
5. Repeat with a red and a green wallpaper. If the three amounts match, the
   model of step 4 of «Decision» holds; write the value to
   `appearance.wallpaper-tinting` in barrierkit's `components.json`.

## Consequences

Only BarrierKit windows show the tint; the compositor's server-side titlebars
and GTK apps do not. The grid covers the one output Malaysia draws today.

## Rejected alternatives

- Keep the global dominant: it does not follow the window, unlike macOS.
- Let the compositor draw the tinted background: the client draws its own
  background (ADR 0034), and the compositor never uses Skia.
