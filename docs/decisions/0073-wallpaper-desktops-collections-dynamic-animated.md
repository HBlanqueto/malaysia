# 0073 — Wallpaper per desktop and display, collections, dynamic and animated wallpapers

Status: accepted (2026-11-11).

## Goal and scope

This ADR finishes the wallpaper pane of the mock-up (block 16d) and the
dynamic and animated wallpapers (block 16c). It starts at System Settings ›
Wallpaper and ends at the surfaces the compositor shows on each desktop.
Readers: whoever works on `malaysia-settings`, `malaysia-wallpaper`, the
compositor's background layer or the wallpaper files.

## Context

1. ADR 0072 left one wallpaper and one crop for every desktop and display.
2. The compositor draws every `Background` layer surface on every desktop,
   also while it slides between two of them (ADR 0051).
3. Skia, which draws System Settings, decodes neither HEIF nor video nor
   GLSL; `malaysia-wallpaper` decodes all three.
4. `collapse.heic` holds a light and a dark image and Apple's
   `apple_desktop:apr` metadata; macOS also ships `h24` (time of day) and
   `solar` wallpapers.
5. Windows ask the compositor for the color of the wallpaper behind them
   (ADR 0065); with several wallpapers that color depends on the desktop.

## Decision

1. **The files.** `ctappearance` owns three files (barrierkit chapter 4,
   «The wallpaper files»): `wallpaper-selection.state` (what each desktop
   shows and how), `wallpaper.domain` (the user's collections) and
   `wallpaper.state` (what `malaysia-wallpaper` reports back). Settings
   writes the first two; `malaysia-wallpaper` writes the third.
2. **A desktop shows its own background surface.** `malaysia-wallpaper`
   makes one layer surface per output and per desktop that has its own
   wallpaper, and tags it with `infinity_window_style_v1.set_desktop`
   (version 5). A desktop shows the surfaces tagged with its number, or the
   untagged ones when it has none (`scene_elements::background_of`); the
   slide and the background blur (ADR 0071) use the same rule.
3. **«Show on all desktops» is macOS's «Show on all Spaces».** With it on,
   choosing a wallpaper sets every desktop (`set_source(None, _)`). Turning
   it off copies the shared choice to the current desktop (`make_own`);
   turning it on again makes that desktop's choice the shared one
   (`show_on_all_desktops`). Settings learns its desktop from the
   `desktop` event of `infinity_focus_surface_v1` (version 4).
4. **A crop per display.** A crop key `"<desktop>/<output>"` wins over the
   desktop's own. The crop sheet lists the displays from `wallpaper.state`
   (their size in pixels); «Same on all displays» saves one crop, otherwise
   one per display, each at that display's shape. The sheet follows the
   mock-up: the image with the frame, rule-of-thirds guides, corners that
   zoom and the pixels of the image inside the frame; zoom with − and +; a
   «Review in» picker for the light and dark stills of a dynamic wallpaper;
   and a side column with the desktop preview (menu bar and Dock optional),
   the sharpness of the result (`SETTINGS.wallpaper.crop_soft` and
   `crop_blurry`) and the display options.
5. **Collections are named lists of folders and images.** «Add…» offers a
   folder, images or an empty collection, through the file chooser portal;
   all open the same sheet («New Collection» or «Edit Collection»), which
   counts the images and lists each folder and image with its own remove
   button. A collection without a name takes the first
   folder's. Renaming or deleting one updates the desktops that shuffle it.
6. **Shuffle is a source.** `shuffle:colors` and `shuffle:collection:<name>`
   pick at random, never the image shown before, at login, every hour or
   every day (`shuffle`). `malaysia-wallpaper` records the pick in
   `wallpaper.state` (`shown`, `shuffled_at`) so the choice survives a
   restart and Settings can show it.
7. **Colors.** The colors are `share/wallpapers/solid-colors/` (with
   Malaysia Dream) and the user's own in `~/.local/share/malaysia/wallpapers/colors`.
   «+» adds a 1×1 PNG from a color picker; «Remove Color» in its context
   menu deletes it.
8. **Dynamic wallpapers.** A HEIC with `apple_desktop:h24` follows the time
   of day; one with `apr` or `solar` follows the appearance (solar needs a
   location, which Malaysia does not ask for). «Mode» keeps it
   Automatic, Light or Dark (`appearance`). `malaysia-wallpaper` decodes
   only the image it shows and checks the time every minute.
9. **Animated wallpapers.** Videos (MP4, MOV, WebM, MKV) play muted and in a
   loop through GStreamer (`decodebin3`, decoded on the CPU like bk's
   `Decoding::Cpu`) and accept every scale mode and crop. GLSL shaders use
   Shadertoy's single-pass contract (`mainImage`, `iResolution`, `iTime`) and
   fill the screen at its resolution, so they offer no framing. Both run at
   most at `WALLPAPER_FPS` and wait for the frame callback, so a hidden
   desktop costs nothing. With «Reduce motion» videos pause and shaders stop
   at time 0. `share/wallpapers/tide.frag` is the first animated wallpaper.
10. **Previews come from `malaysia-wallpaper`.** It writes freedesktop
    thumbnails (x-large, `Thumb::URI` and `Thumb::MTime`) of each HEIC,
    video and shader in the system folder, the colors and the collections,
    and a light and a dark still of each dynamic HEIC. Settings reads them,
    shows a dynamic HEIC under Dynamic when both stills exist, and waits for
    a missing thumbnail. Thumbnails and stills record the size of the
    original image (`Thumb::Image::Width` and `Height`), so the crop sheet
    shows real pixels. The built-in Collapse uses
    `share/wallpapers/previews/collapse-light.png` and `-dark.png`, which
    carry the same keys.
11. **The filter.** A search field with a scope (All, a kind or a
    collection) matches the name, the kind, the collection and the file name;
    while it filters, the sections merge into one grid of results.
12. **Each desktop tints its windows.** `wallpaper.state` keeps a grid per
    desktop with its own wallpaper; the compositor picks the grid of the
    window's desktop.

## Consequences

1. Without `infinity_window_style_v1` version 5 every desktop shows the
   shared wallpaper.
2. A video costs one CPU decode and one texture upload per frame; zero-copy
   decoding (bk's `GpuZeroCopy`) is not wired in the wallpaper yet.
3. Thumbnails appear a moment after a folder is added; a file
   `malaysia-wallpaper` cannot decode keeps an empty tile.
4. The 16d items of the mock-up left out: «Show all» grids for long rows
   and pausing an animated wallpaper by hand. The mock-up's sample folder
   and images only stand in for real files in a browser.

## Rejected alternatives

- Drawing every desktop's wallpaper in one surface: the compositor slides
  desktops and would need to know which part belongs to which.
- Decoding HEIC and video in Settings: it would link libheif and GStreamer
  into a second process and duplicate `malaysia-wallpaper`'s decoders.
- Storing a dynamic image by index: Settings cannot read the metadata that
  says which index is light or dark.
- `color:#rrggbb` sources for custom colors: a PNG is already a wallpaper
  every tool understands.
