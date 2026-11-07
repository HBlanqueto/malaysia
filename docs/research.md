# Research: conclusions

Summary of what was decided after studying macOS, GNOME and Otto. The full text lives in git, in the commit before `docs/investigacion/` was deleted.

## How macOS renders

- Each app draws its window and **WindowServer** composes, animates and routes events.
- Core Animation separates the model, presentation and render tree; the render tree lives in WindowServer, which composes on the GPU with Metal.
- We take «the app paints, the compositor blends», Smithay's render elements + damage, compositor effects and, with DRM, direct scanout and dma-buf. Apple's private protocol is not copied.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/como-renderiza-macos.md`

## macOS versus Wayland

- They share the split and the optimizations (GPU buffers, fences, damage, vsync, direct scanout).
- WindowServer retains a render tree per window; in Wayland the compositor only has textures.
- We take Smithay's pipeline + compositor-side effects + direct scanout; nothing couples to a toolkit and Apple's protocol is not copied.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/wayland-vs-macos.md`

## macOS scaling

- It separates points, backing store (1×/2×) and panel pixels; it composes at 2× and downscales to the panel.
- We take the `Logical`/`Physical`/`Buffer` split and per-screen scale (8b). We do not fake 1.5 nor render everything at 2× without measuring.
- Global supersampling remains an optional future item after 8b/9/11/12 and with real measurements.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/escalado-macos.md`

## Otto and its forks

- `smithay-otto` is not used: its patches are Vulkan/Intel and Malaysia is GLES/AMD.
- From `layers` only the spring is taken; `laye-rs` has no license.
- From `otto-kit` we port states and protocols, not drawing: loop, key repeat, scale, menus, DBusMenu, portal, desktop entries, icons and accessibility. Otto is read for udev/input, fractional scale, PAM/lock, xdg-activation and appmenu; render, planes, backdrop and screencopy are not ported.
- If an MIT line is copied, the notice goes into `NOTICE` in the same commit; ADR 0034 settled BarrierKit on Freya.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/otto-y-forks.md`

## GNOME as a reference

- GNOME is a reference, not a dependency: Mutter/Shell are GPL and the portals LGPL; we reimplement in Rust.
- From Mutter we take models for 8b/8c/9/11; from GNOME Shell, the UX and the state model of the bar, dock, lock, notifications and OSD.
- Our own portal only for Settings, Screenshot and ScreenCast; GTK for FileChooser/Print/Notification.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/gnome.md`

## Global menu

- We choose `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`: the compositor routes and the shell consumes the tree.
- From Otto/Noctalia we take the protocol direction and the dbusmenu client; the drawing belongs to BarrierKit.
- We discard AT-SPI, Canonical's Registrar, `xdg-dbus-annotation` and reading menus in the compositor. GPL sources are read as behavior; MIT/Apache ones, with a notice in `NOTICE`.
- Qt 6 publishes its menu through `org_kde_kwin_appmenu` on Wayland ([`qwaylandappmenu.cpp`](https://codebrowser.dev/qt6/qtbase/src/plugins/platforms/wayland/qwaylandappmenu.cpp.html)); since 6.9.1 it skips the Registrar on Wayland ([qtbase fa1fab99](https://github.com/qt/qtbase/commit/fa1fab994e76c19a08903e9b641732ce388a30bd)).
- No menu through any path: WPS and ONLYOFFICE (their own ribbon, also on X11), JetBrains since 2024 (IJPL-156149) and VS Code with its own title bar (`"window.titleBarStyle": "native"` turns it on). For them, the shortcut menus of ADR 0061.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/global-menu.md`

## Screenshot clipboard

- Wayland uses *pull*: the owner publishes MIME types and the compositor hands over an fd per paste; the selection follows focus.
- Malaysia owns the capture: PNG as `image/png`, no file if `target = "clipboard"`; default Desktop.
- From GNOME we take that the compositor is the owner; from macOS, the configurable destination and `Control` (pending). Writing to the fd runs in a thread; there is no clipboard manager. `freya-clipboard` still drags in duplicate SCTK/calloop; the replacement lives in barrierkit.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/portapapeles-capturas.md`

## Edge resize and cursors

- Implemented: 10 px outer band, 20 px corner «L» (2 px inward), button priority, SSD only and no maximized/fullscreen.
- GNOME, Otto and macOS agree on an outer band and per-side cursors; resize by inner squares is discarded. Pending: shadow/CSS in the grab area, rounded corners and excluding tiled windows.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/resize-bordes-y-cursores.md`

## ChromaLeon and accents

- We take the HSL `L` lightness adjustment for WCAG contrast and raising saturation if the accent ends up flat.
- We do not take the first pixel as the representative nor its gray detection; Malaysia uses a chroma-weighted histogram and a 3×3×3 neighborhood.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/chromaleon.md`

## Wallpaper accent and tint

- The accent and tint derive from the background's dominant color with WCAG contrast against the surfaces.
- It touches barrierkit (color helpers, `vow-appearance`, tokens) and malaysia (3×3×3 in `dominant()` and visual integration).
- Limit: a pure yellow with `L = 0.65` cannot meet both window contrast ≥3 and white text ≥3; the window takes priority. The visual test only covered a yellow background; the logic has unit tests.

Full text: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/informe-wallpaper-accent.md`
