# The wallpaper (`malaysia-wallpaper`)

> Overview map: [`interface.md`](interface.md).

An independent Wayland client (ADR 0049) that draws the background on the `Background` layer with `wgpu` on Vulkan, without BarrierKit, Freya or Skia. It is an opaque surface with no widgets, text or accessibility.

## How it works

```
wallpaper-selection.state ──► decode PNG/JPEG ──► wgpu texture
 (~/.local/share/malaysia)                    │
                                         Vulkan (fill)
                                              ▼
                              compositor Background layer
```

1. It opens a `wgpu` context on the `wl_display` with the Vulkan backend and draws with «fill» scaling (covers and crops).
2. It is a `wlr-layer-shell` client: `Background` layer, anchored to the whole output, with `wp_fractional_scale_v1` and `wp_viewporter` for HiDPI.
3. It presents via dmabuf (`zwp_linux_dmabuf_v1`) with no readback or CPU copies.
4. It watches `wallpaper-selection.state` with inotify and repaints on change. It computes the dominant color with `ctappearance::backdrop::dominant_from_pixels` (k-means in OKLab, the same algorithm as the kit) and a grid of average colors with `WallpaperGrid::from_rgba8`, and writes both with `write_wallpaper_state` to `wallpaper.state` (`dominant` and `grid`). The compositor no longer decodes images.
5. The compositor reads the grid and sends each BarrierKit window the color of the wallpaper behind it (ADR 0065).

## Supervision

`malaysia-wallpaper.service` + `malaysia-session.target`; `Restart=on-failure`, `RestartSec=2`. In development, `./dev.sh demo` and `lab` launch it. Editing `wallpaper-selection.state` changes the background live; `kill -9` does not bring down the compositor; `scripts/check-client-only-deps.sh` checks that `wgpu` does not enter the compositor nor Freya/Skia the wallpaper.
