# The Malaysia interface

Map of the visuals and the interaction; the details live in each document.

## Principles

1. **Tengen is the design** (ADR 0032, now in barrierkit): colors, radii, spacing and shadows come from `vow-design`; nobody invents values.
2. **Separate chrome, effects for all** (ADR 0020): GTK/Libadwaita keep their headerbar; the rest receive the compositor's chrome. Shadow, borders and radii belong to the compositor and apply to all windows.
3. **Native, separate shell** (ADR 0023/0025): one process per component.
4. **Skia only in clients** (ADR 0034): the shell uses BarrierKit (Freya + Skia); the compositor stays with GLES, `grade-graphics` and `cosmic-text`.

## Split

The shell draws its UI with BarrierKit; `malaysia-wallpaper` draws the background with wgpu/Vulkan; our own apps draw their UI (44 px CSD or 32 px SSD); the compositor composes and draws chrome, corners, shadows and animations.

## Layers

```
Overlay   launcher, screenshots, shell popups
Top       menubar (top) · dock (bottom) · notifications (future)
Windows   client + SSD titlebar + xdg popups
Bottom    layer-shell Bottom (future)
Background wallpaper
```

The exclusive zones of the menubar and the dock trim the workspace. The lock replaces everything: only its surfaces and the cursor are drawn.

## Design

Themes `light`/`dark`/`dark-tint`; radius 16, traffic lights 12 (radius 4), SSD titlebar 32 and CSD 44; Geist 13/600 and 13/400. `SHADOW`/`SHADOW_DARK` shadows by shader and clipping to radius 16. `appearance.window_color` (`neutral`/`accent`/`wallpaper`) and `traffic_lights` tint the titlebar; `window_tint_scope = "desktop"` tints the menubar and the dock. Background blur is in the catalog, but is not drawn yet.

## Shell

`malaysia-wallpaper` (Background), `malaysia-shell menubar` (Top, 28 px), `dock` (Top, bottom), `lock` (lock surfaces) and `launcher` (Overlay). They read `~/.config/malaysia/` and `~/.local/share/malaysia/`; they do not write configuration.

## Windows

- **Our own apps (ADR 0035):** `app_id` with the `io.github.hblanqueto.malaysia.` prefix; their decoration mode is honored before the first commit and `force_server`/`force_client` do not touch them.
- **SSD:** 32 px titlebar, traffic lights, centered title; close, minimize, maximize, drag and resize.
- **CSD:** GTK/Libadwaita, wezterm and apps that request it keep their chrome; `force_server` can force the bar (at the risk of duplicating it).
- **Frame:** 16 px corners, shadow, outer border and hairline for all; fullscreen removes it, maximized keeps it. Dialogs are centered over the parent.
- **Kit popups** (native app-id or shell layer, ADR 0035): the compositor gives them the same frame (outer border, hairline and shadow) with 12 px corners (`vow_design::POPUP`), clipped to their xdg geometry. The client only paints the background and the rows. Popups from other apps (GTK, Firefox, Qt) are drawn as they are.
