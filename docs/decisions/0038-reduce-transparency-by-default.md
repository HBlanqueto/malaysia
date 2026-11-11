# 0038 — Reduce transparency by default

Status: accepted (2026-10-24); point 1 superseded by [ADR 0075](0075-menu-bar-styles-and-height.md) (2026-11-11): the default is now `false`.

## Context

The mockups show the bar, menus, dock and sidebar with solid backgrounds, but `shell-bar` was still translucent: over a strong wallpaper, the theme color was not visible.

## Decision

1. `appearance.reduce_transparency` is `true` if it is missing from `appearance.domain`; with that value, `Resolved` picks the opaque variants of the bar, menus, dock and sidebar.
2. The opacity is generated in `ctdesign`: `build.rs` composites each translucent token over `bg-window` and emits `*_OPAQUE`.
3. The choice lives in `Resolved`; the kit and the shell consume `resolved.shell`, `resolved.menu` and `resolved.sidebar`.
4. Blur is left to the compositor; with `false`, the current translucent tokens are kept.

## Consequences

The shell shows the solid color over any wallpaper; `false` does not enable blur yet, but it prepares the way.

## Rejected alternatives

- Keeping translucency by default: the theme color is not readable over strong backgrounds.
