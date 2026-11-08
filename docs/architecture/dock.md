# The dock (`malaysia-shell dock`)

> Overview map: [`interface.md`](interface.md).

Native component (ADR 0023): a bottom dock that lists the open apps and activates them.

## How it works

```
ext-foreign-toplevel-list-v1 ──► window list (tiles)
zwlr_foreign_toplevel_management_v1 ──► activate / close / minimize; active state
        ▼
bk layer surface → layer-shell Top → Skia on Vulkan
```

1. Surface on the `Top` layer, anchored at the bottom, height `DOCK_BAR_HEIGHT` from `ctdesign` and an equal `exclusive_zone`; windows do not go underneath. `dock.domain` is not read yet: every key in [`options.md`](../configuration/options.md#dock--shell-dock-malaysia-shell-dock-macos-comappledock) is pending.
2. `ext-foreign-toplevel-list-v1` gives one tile per mapped window. Each tile shows the title on Tengen's `ShellPalette` (`tile`, active `tile_active`), chosen by `appearance.color_scheme`.
3. A click sends `activate` through `zwlr_foreign_toplevel_management_v1`; the compositor restores the window if it was minimized, raises it and focuses it. `close`, `set_minimized`, `set_maximized` and `set_fullscreen` (focus mode, ADR 0051) are handled; `set_rectangle` is ignored. Each handle carries `output_enter` for the outputs its client bound and, from v3, `parent`.
4. The active window is highlighted from the `activated` state of `zwlr_foreign_toplevel_management_v1`.
5. A secondary click on a tile opens the Dock menu (the kit's `context_menu`): the `app_id` as a header, Minimize (or Show when minimized) and Close Window. Each command is a `zwlr_foreign_toplevel_management_v1` request.

## Compositor side

`ext-foreign-toplevel-list-v1` uses Smithay's state; `zwlr_foreign_toplevel_management_v1` is served by hand (Smithay 0.7 does not ship it), announces the existing windows on bind and each request acts on its window.

## Divergences

- `zwlr_foreign_toplevel_management_v1` by hand (ADR 0023 allows for it).
- The minimize animation (genie/scale) is still pending, and so is the `set_rectangle` it would use. The tiles show the title; the kit's SVG icons are not used yet.
- `activate` carries no press serial in this version of the XML, so it is not validated against the implicit grab.
