# The dock (`malaysia-shell dock`)

> Overview map: [`interfaz.en.md`](interfaz.en.md).

Native component (ADR 0023): a bottom dock that lists the open apps and activates them.

## How it works

```
ext-foreign-toplevel-list-v1 ──► window list (tiles)
zwlr_foreign_toplevel_management_v1 ──► activate / close / minimize; active state
dock.domain ──► tilesize, orientation
        ▼
RGBA frame (barrier-kit) → layer-shell Top → EGL/dmabuf
```

1. Surface on the `Top` layer, anchored at the bottom, height `tilesize + 2×padding` (64 px by default) and an equal `exclusive_zone`; windows do not go underneath.
2. `ext-foreign-toplevel-list-v1` gives one tile per mapped window. Each tile shows the title on Tengen's `ShellPalette` (`tile`, active `tile_active`), chosen by `appearance.color_scheme`.
3. A click sends `activate` through `zwlr_foreign_toplevel_management_v1`; the compositor restores the window if it was minimized, raises it and focuses it. `close`, `set_rectangle` and `set_minimized` are handled; `set_maximized` and `set_fullscreen` are recognized but not implemented.
4. The active window is highlighted from the `activated` state of `zwlr_foreign_toplevel_management_v1`.
5. A secondary click on a tile opens the Dock menu (the kit's `context_menu`): the `app_id` as a header, Minimize (or Show when minimized) and Close Window. Each command is a `zwlr_foreign_toplevel_management_v1` request.

## Compositor side

`ext-foreign-toplevel-list-v1` uses Smithay's state; `zwlr_foreign_toplevel_management_v1` is served by hand (Smithay 0.7 does not ship it), announces the existing windows on bind and each request acts on its window.

## Divergences

- `zwlr_foreign_toplevel_management_v1` by hand (ADR 0023 allows for it).
- `set_rectangle` is stored in `State::dock_rectangle`; the minimize animation (genie/scale) is still pending. The tiles show the title; the kit's SVG icons are not used yet.
- `activate` carries no press serial in this version of the XML, so it is not validated against the implicit grab.
