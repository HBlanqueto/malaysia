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
   - `solid` (Classic in System Settings): the theme's `shell-bar`, opaque
     and without blur, as in macOS Catalina: white or gray. The text follows
     the color scheme.
   - `translucent` (default): a light or dark glass over the blurred wallpaper. The
     shell reads the color of the wallpaper under the bar from the wallpaper
     grid in `wallpaper.state`, and `ctappearance::menubar::colors_over` picks
     dark ink and light glass over a light wallpaper, light ink and dark glass
     over a dark one, whatever the color scheme.
   - `floating`: the same ink as `translucent`, with no glass. Only the pill
     of an open menu title is blurred.
3. The blur shows only the wallpaper, never the windows. The bar asks for it
   with `set_blur_region`: `translucent` over its whole surface, `floating`
   over the open pill only. It stops asking while the screen locks. The
   compositor rounds a blur rectangle shorter than the bar with the pill's
   radius (`title-open.radius`).
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

- A new install shows the translucent bar over the blurred wallpaper.
- The ink follows the wallpaper of the shown desktop. Infinity v8 sends the
  bar the number of that desktop (`desktop` event of
  `infinity_window_manager_v1`), and the bar reads its grid from
  `wallpaper.state` with `grid_for`; a desktop without its own wallpaper, or
  a focus space, uses the shared grid.
- The pill behind an open menu title follows the ink too:
  `MenubarColors::selection` is a translucent dark pill over a light
  wallpaper and a light one over a dark wallpaper. Titles get no pill on
  hover, as in macOS Big Sur.
- Windows do not move when the height changes; the compositor applies the new
  exclusive zone as with any layer.

## Rejected alternatives

- Measuring the wallpaper in the compositor and sending the ink to the bar:
  the shell already reads `wallpaper.state`, and the grid is enough for a
  30–38 px band.
- Blurring the windows behind the bar: more energy for a band that windows
  only reach in full screen.
