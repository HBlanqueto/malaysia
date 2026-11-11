# 0075 — Menu bar styles, height and blur by default

Status: accepted and implemented (2026-11-11). Replaces point 1 of
[ADR 0038](0038-reduce-transparency-by-default.md).

## Goal and scope

This ADR fixes how the menu bar draws its background and how tall it is. It
covers `menubar.style`, `menubar.bar_height`, the default of
`appearance.reduce_transparency` and the System Settings › Desktop & Dock
pane. The Dock options of that pane are out of scope.

## Context

The bar had one look: the theme's `shell-bar`, opaque by default (ADR 0038).
The mockup «Panel superior — 3 variantes» draws three: solid (Catalina),
translucent (Big Sur) and floating (no bar). The compositor already blurs the
wallpaper alone and shows it behind any surface that asks for it with
`ext-background-effect-v1` (ADR 0071).

## Decision

1. `appearance.reduce_transparency` is `false` when it is missing. The bar,
   menus, Dock and sidebar use their translucent tokens, and the compositor
   draws the blur.
2. `menubar.style` chooses the background:
   - `solid`: the theme's `shell-bar` over the blurred wallpaper, as in macOS
     Catalina. The text follows the color scheme.
   - `translucent`: a light or dark glass over the blurred wallpaper. The
     shell reads the color of the wallpaper under the bar from the wallpaper
     grid in `wallpaper.state`, and `ctappearance::menubar::colors_over` picks
     dark ink and light glass over a light wallpaper, light ink and dark glass
     over a dark one, whatever the color scheme.
   - `floating` (default): the same ink as `translucent`, with no glass and
     no blur.
3. The blur shows only the wallpaper, never the windows: the bar asks for it
   over its whole surface with `set_blur_region`, and stops asking while the
   screen locks.
4. With `reduce_transparency` (or Increase contrast) every style draws as an
   opaque `solid` bar without blur.
5. `menubar.bar_height` is `slim` (`MENUBAR_HEIGHT`, 30 px, default) or
   `wide` (`MENUBAR_HEIGHT_WIDE`, 38 px), like the taller bar of a Mac with a
   notch. `MenubarConfig::height()` is the only place that maps it. The bar
   changes its size and exclusive zone at once with `set_layer_geometry`; the
   lock screen, the menus and the live activities use the same height.
6. System Settings shows both options in a Desktop & Dock pane, below
   Appearance, with the `menubar.dock.rectangle` symbol.

## Consequences

- A new install shows the floating bar over the wallpaper with the blur on.
- The ink follows the shared wallpaper grid, not the grid of the current
  desktop: the shell does not know the desktop number of the bar yet.
- Windows do not move when the height changes; the compositor applies the new
  exclusive zone as with any layer.

## Rejected alternatives

- Measuring the wallpaper in the compositor and sending the ink to the bar:
  the shell already reads `wallpaper.state`, and the grid is enough for a
  30–38 px band.
- Blurring the windows behind the bar: more energy for a band that windows
  only reach in full screen.
