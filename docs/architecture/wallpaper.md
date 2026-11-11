# The wallpaper (`malaysia-wallpaper`)

> Overview map: [`interface.md`](interface.md).

An independent Wayland client (ADR 0049) that draws the background on the `Background` layer with `wgpu` on Vulkan, without BarrierKit, Freya or Skia. It is an opaque surface with no widgets, text or accessibility.

## How it works

```
wallpaper-selection.state ─┐     ┌─► image (PNG/JPEG/HEIF) ─► wgpu texture ─┐
wallpaper.domain ──────────┼─► desktop ─► video (GStreamer) ─► texture ───┼─► one surface per
appearance.domain ─────────┘     └─► shader (GLSL) ─► drawn each frame ────┘   output and desktop
```

1. It opens a `wgpu` context on the `wl_display` with the Vulkan backend.
2. It is a `wlr-layer-shell` client: `Background` layer, anchored to the
   whole output, with `wp_fractional_scale_v1` and `wp_viewporter` for
   HiDPI. It presents via dmabuf (`zwp_linux_dmabuf_v1`).
3. It reads `wallpaper-selection.state` and makes one surface per output for
   the shared wallpaper and one more per desktop with its own. It tags the
   latter with `infinity_window_style_v1.set_desktop`; the compositor shows
   on each desktop its own surfaces or the untagged ones (ADR 0073).
4. It resolves each desktop's source: the built-in `collapse.heic`, a file,
   or a pick from the colors or a collection (`source.rs`), which it keeps
   in `wallpaper.state` until the next shuffle.
5. It draws images with the `scale` and crop of that desktop and output:
   `framing::uv_window` turns the mode into the window of the texture that
   spans the screen; outside the image it paints `DESKTOP_CLEAR` (ADR 0072).
   A dynamic HEIC shows the image its metadata gives for the appearance or
   the time (`dynamic.rs`).
6. Videos (`video.rs`) and shaders (`shader.rs`) redraw at most at
   `WALLPAPER_FPS`, each after the previous frame callback. With
   `reduced_motion` they stand still.
7. It watches the files with inotify and checks the shuffle and the time
   every minute. It computes the dominant color with
   `ctappearance::backdrop::dominant_from_pixels` (k-means in OKLab) and a
   grid of average colors per desktop with `WallpaperGrid::from_rgba8`, and
   writes them, the outputs and the shuffle to `wallpaper.state`.
8. In a background thread it writes the thumbnails System Settings shows
   (`thumbnails.rs`): HEIC, video and shader files of the system folder, the
   colors and the collections.
9. The compositor reads the grid of each window's desktop and sends the
   window the color of the wallpaper behind it (ADR 0065).

## Supervision

`malaysia-wallpaper.service` + `malaysia-session.target`; `Restart=on-failure`, `RestartSec=2`. In development, `./dev.sh demo` and `lab` launch it. Editing `wallpaper-selection.state` changes the background live; `kill -9` does not bring down the compositor; `scripts/check-client-only-deps.sh` checks that `wgpu` does not enter the compositor nor Freya/Skia the wallpaper.
