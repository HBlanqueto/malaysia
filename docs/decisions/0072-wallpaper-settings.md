# 0072 — Wallpaper in System Settings

Status: accepted (2026-11-11). [0073](0073-wallpaper-desktops-collections-dynamic-animated.md)
replaces points 5 and 6 and builds what consequence 2 leaves out.

## Goal and scope

This ADR fixes how System Settings chooses the wallpaper and how much of it
the screen shows. It starts at Settings › Wallpaper and ends at the pixels
`malaysia-wallpaper` draws. Readers: whoever works on `malaysia-settings`,
`malaysia-wallpaper` or the selection file.

## Context

1. `malaysia-wallpaper` (ADR 0049) reads `wallpaper-selection.state` and
   draws it with «fill». It parsed the file itself; nothing wrote it but a
   person.
2. The file already carried `scale`, ignored except for a warning.
3. The default wallpaper is `collapse.heic`, inside the binary; it replaced
   `malaysia-dream.heic`, one salmon color (`#e16e79`), which is now installed
   like the others.
4. The system wallpapers live in `$XDG_DATA_DIRS/wallpapers`: `bandura.jpg`,
   `malaysia-dream.heic` and the `solid-colors/` set.
5. Skia, which draws Settings, has no HEIF decoder.

## Decision

1. **One reader and writer.** `ctappearance::WallpaperSelection` is the
   file: `scale`, `[desktops]` and `[crop]`, with `ALL_DESKTOPS`
   (`"default"`) as the key for every desktop. Settings writes it with
   `write_wallpaper_selection`; the wallpaper reads it with
   `read_wallpaper_selection`.
2. **Five scale modes.** `fill`, `fit`, `stretch`, `center` and `custom`.
   The wallpaper turns each mode into the window of the texture that spans
   the screen (`framing::uv_window`); outside the image it paints
   `DESKTOP_CLEAR`.
3. **Custom framing is a rectangle of the image.** `[crop]` holds
   `[x, y, width, height]` as fractions of the image. The screen covers the
   rectangle like «fill», so one crop serves outputs of any shape. Settings
   draws the frame at `SETTINGS.wallpaper.crop_aspect` (16:10) and zooms
   from 100 % to 400 % in steps of 25 %.
4. **Collapse first, Malaysia Dream as a color.** Collapse is the first tile
   of Images; choosing it removes the desktop's line, so the built-in
   wallpaper shows. Malaysia Dream is the first dot of Colors and is chosen
   by its file.
5. **The pane lists what is installed.** Images are the image files of the
   first `$XDG_DATA_DIRS/wallpapers`; Colors are `malaysia-dream.heic` and
   the files of `solid-colors/`. Each row scrolls sideways.
6. **HEIC is previewed by a JPEG.** Settings shows `previews/<name>.jpg` for
   each HEIC, and `previews/collapse.jpg` for the built-in one. Names come
   from the file: `malaysia-dream` reads «Malaysia Dream».
7. **A new image resets the framing.** Choosing a wallpaper removes its
   crop and turns `custom` into `fill`, because a crop belongs to one image.
   Colors hide «Scale to screen»: every mode looks the same on one color.

## Consequences

1. Editing the file by hand still works and changes the desktop live.
2. One wallpaper and one crop serve every desktop and output; the
   per-desktop switch, collections, filtering, dynamic and animated
   wallpapers and custom colors from the mock-up are not built yet
   (`roadmap.md`, block 16).
3. «Center» shows one image pixel per logical pixel.

## Rejected alternatives

- A crop per output: the selection does not know the outputs, and a
  rectangle covered like «fill» already adapts to their shape.
- Writing the color of Malaysia Dream as `color:#rrggbb`: the wallpaper
  would need a second source type for a color it already embeds.
