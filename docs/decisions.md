# Decisions

**Goal:** know why Malaysia is built this way before you change it.
**Reader:** anyone who changes the compositor, the session, the portal or the shell.

Each entry gives the decision as it holds today, with its ADR numbers. When a
decision changes, edit its entry; when it no longer holds, delete it. Git keeps
the history. Options are in [`options.md`](configuration/options.md); the
toolkit decisions are in
[BarrierKit's decisions](https://github.com/HBlanqueto/barrierkit/blob/main/docs/decisions.md).

## 0001 · 0002 · 0004 · 0012 · 0013 · 0015 — Crate structure and geometry
Decision:
- `malaysia-compositor` is a library (`lib.rs`) plus a thin binary (`main.rs`), which alone owns `cli` and `logging`.
- `State` holds protocol and scene state only. Each backend loop (`session.rs` for winit, `udev.rs` for DRM) owns the renderer, the damage tracker and the `Display`.
- `malaysia-scene` owns layout: windows, stacking, hit-testing and edges, in its own logical-only types. The compositor uses `smithay::utils` directly, owns the popup tree and does not use `desktop::Space`.
- Logical to physical rounds the position down and the size up (`to_i32_floor`, `to_physical_precise_up`), so adjacent surfaces share their edge pixel.
- Client dispatch polls a duplicated display fd (`try_clone_to_owned`) in a calloop `Generic`, without `unsafe`. A dispatch error is logged and the loop continues.
Why: Smithay's types make mixing coordinate spaces a compile error, and handlers stay testable without a graphics stack.
Rejected: `desktop::Space` with a side table of insets; rounding to nearest, which leaves seams at fractional scales.
Code: `crates/malaysia-scene/src/scene.rs`, `crates/malaysia-compositor/src/session.rs`, `crates/malaysia-compositor/src/udev.rs`

## 0003 — Bounded unsafe
Decision:
- `malaysia-scene` uses `#![forbid(unsafe_code)]`. `malaysia-compositor` (`lib.rs`, `main.rs`) and `xdg-desktop-portal-malaysia` use `#![deny(unsafe_code)]`.
- Each `unsafe` in those crates sits in a function marked `#[allow(unsafe_code)]`: the process environment at startup (`spawn.rs`), `pre_exec` for the XWayland fds (`xwayland.rs`), the shm pool in `capture.rs`, and test helpers.
- No `// SAFETY:` comment: the repo forbids comments, so the reason goes in the commit message.
Why: every exception is a named opt-in that a reviewer can find with `grep`.
Rejected: `forbid` in the compositor too; it forces edits to the crate root and then unreviewable relaxations.
Code: `crates/malaysia-compositor/src/lib.rs`, `crates/malaysia-scene/src/lib.rs`

## 0005 · 0006 · 0007 — Toolchain from the flake, builds only in the dev shell
Decision:
- `flake.nix` is the only toolchain source: the dev shell brings `cargo`, `rustc`, `clippy` and `rustfmt` from the pinned nixpkgs. There is no `rust-toolchain.toml`; `rust-version` in `Cargo.toml` is the MSRV floor.
- The dev shell links with mold through `RUSTFLAGS`. No linker is set in `.cargo/config.toml`.
- The dev shell exports `MALAYSIA_DEV_SHELL=1`. `build.rs` refuses to build without it, and `dev.sh` checks it early for build commands.
- The Nix package sets the marker itself and wraps the binaries with Mesa paths, keeping the previous values in `MALAYSIA_ORIG_*`.
- `dev.sh` signals only the saved PID after checking `/proc/<pid>/comm`. Its files live under `$XDG_RUNTIME_DIR/malaysia/`.
Why: a build outside the shell links differently and warms a different cache, and one toolchain source cannot drift.
Rejected: `rust-toolchain.toml` with rust-overlay or fenix; unconditional mold in `.cargo/config.toml`, which breaks `nix build`.
Code: `flake.nix`, `crates/malaysia-compositor/build.rs`, `dev.sh`

## 0009 · 0018 · 0029 · 0030 — Backends by flag
Decision:
- `--winit` (default) runs nested in a winit window with `GlesRenderer`. `--drm` runs a direct session through libseat, GBM, EGL and `GlesRenderer`. No autodetection, no fallback to winit.
- The winit `Output` stays `Normal`. The damage tracker gets a static `Flipped180` mode source, so only the projection flips.
- `--drm` drives one output: the eDP connector in its preferred mode, else the first connected one. With none, it fails.
- Primary and cursor planes may scan out; overlay planes only with `[debug] overlay_planes`. `--drm-device` and `--no-input` are dev-only and require `--drm`.
- A VT switch pauses and resumes DRM and libinput. `Ctrl+Alt+Backspace` exits, except while the session is locked.
Why: a failure on a TTY must not take the seat or the screen by surprise. A flipped `wl_output` would make clients pre-rotate their buffers.
Rejected: autodetection; `Flipped180` on the `Output`.
Code: `crates/malaysia-compositor/src/cli.rs`, `crates/malaysia-compositor/src/session.rs`, `crates/malaysia-compositor/src/udev.rs`

## 0014 · 0016 · 0019 · 0037 — Window decorations and frame
Decision:
- GTK clients (they bind `gtk_shell1`) and BarrierKit apps get the mode they request, client-side if they request none. Other clients get server-side unless they ask for client-side over xdg-decoration or KDE server-decoration. `window.decorations` can force either mode.
- The compositor exports `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` (`window.qt_server_decorations`, default `true`), so Qt leaves its frame to the compositor.
- Insets follow role, state and mode: only a server-side normal or maximized toplevel reserves the titlebar. They are recomputed on every configure.
- Every window, CSD or SSD, gets one frame element with rounded clip, shadow and border. Fullscreen drops the frame. Only SSD windows get the titlebar with the traffic lights.
- Move and resize are the compositor's own `Grab` state, checked against the pointer's held press (`has_grab(serial)`). Edges are measured on the decorated rectangle, and the opposite edge stays fixed.
Why: the compositor cannot redraw a toolkit's interior, so it honors CSD and unifies only what surrounds the window.
Rejected: always forcing server-side, which doubles the chrome; a Smithay `PointerGrab` per operation.
Code: `crates/malaysia-compositor/src/decoration.rs`, `crates/malaysia-compositor/src/handlers.rs`, `crates/malaysia-compositor/src/chrome.rs`

## 0020 · 0038 · 0065 · 0071 — Blur and wallpaper tinting
Decision:
- Blur is drawn only while `appearance.reduce_transparency` and `appearance.contrast` are `false` (both default `false`). Only then does `ext_background_effect_manager_v1` advertise `blur`.
- A surface gets blur only in the region it sends through `ext-background-effect-v1`. The shell and the kit use the same protocol.
- By default the blur shows the `Background` layer, blurred once per output into a cache. The Dock, notification cards and glass popups blur what is really behind them, each from its own cached render.
- The shaders are Dual Kawase in `grade-graphics`. `ctdesign::BACKGROUND_BLUR_PASSES` and `BACKGROUND_BLUR_OFFSET` set the strength; no user option does.
- `appearance.wallpaper_tinting` (default `true`, dark mode only unless `wallpaper_tinting_in_light`): the compositor averages the wallpaper grid cells behind each window and sends the color in the `wallpaper` event of `infinity_focus_surface_v1`. The kit, the patched libadwaita and the compositor's titlebar mix it in.
Why: no client sees other clients' pixels, so only the compositor can blur or sample what is behind a window. The [protocol](https://wayland.app/protocols/ext-background-effect-v1) leaves strength to the compositor.
Rejected: blurring every window by default (GPU cost); a blur request in Infinity; the compositor painting client backgrounds.
Code: `crates/malaysia-compositor/src/blur.rs`, `crates/malaysia-compositor/src/focus_notice.rs`

## 0021 — Configuration in `.domain` files
Decision:
- One TOML `.domain` file per domain: preferences in `~/.config/malaysia/`, state in `~/.local/share/malaysia/`. `MALAYSIA_CONFIG_DIR` and `MALAYSIA_DATA_DIR` override them.
- Options are typed with their defaults in `config.rs`; `appearance` comes from `ctappearance`. `docs/configuration/options.md` is the only catalog, with macOS names where they exist.
- System Settings writes the files. The compositor, the shell and the portal reload them through inotify (`ctappearance::watch::AppearanceWatch`).
Why: the settings app, the shell and the compositor share one store and one set of types.
Rejected: environment variables, which have no catalog, types or reload.
Code: `crates/malaysia-compositor/src/config.rs`

## 0023 · 0025 · 0026 — Native shell and GPU buffers
Decision:
- `malaysia-shell` is one binary with one process per component, each its own Wayland client drawn with BarrierKit. The wallpaper is `malaysia-wallpaper`. No external shell.
- `menubar`, `dock` and `notifications` run as `malaysia-shell@.service` units (`Restart=on-failure`) under `malaysia-session.target`. Under `--drm` the compositor imports `WAYLAND_DISPLAY` and `MALAYSIA_BIN_DIR` and starts and stops the target. It spawns `lock` and `launcher` on demand.
- `zwp_linux_dmabuf_v1` uses default feedback on the renderer's render node. A buffer that fails to import is rejected. No `wl_drm`.
- `wp_linux_drm_syncobj_manager_v1` exists only when the device supports `syncobj_eventfd`. Acquire points and implicit fences share one calloop blocker path, and buffers are released when the GPU fence signals.
Why: a shell crash must not end the session, and Mesa needs linux-dmabuf to run GPU clients accelerated.
Rejected: the shell inside the compositor; Quickshell; `wl_drm`.
Code: `crates/malaysia-shell/src/main.rs`, `crates/malaysia-compositor/src/shell_session.rs`, `crates/malaysia-compositor/src/blockers.rs`

## 0028 — Smithay pinned by revision
Decision: `Cargo.toml` takes Smithay from git `master` at a fixed `rev`, with `default-features = false` and only the features in use. `flake.nix` carries its `cargoLock.outputHashes` entry. `smithay-otto` is only a reference.
Why: the 0.7.0 release lacks `ext-image-copy-capture-v1`, `ext-image-capture-source-v1` and `ext-background-effect-v1`.
Rejected: staying on 0.7.0.
Code: `Cargo.toml`, `flake.nix`

## 0031 · 0044 — Own portal backend and appearance cross-fade
Decision:
- `xdg-desktop-portal-malaysia` (`org.freedesktop.impl.portal.desktop.malaysia`) is a separate process started by D-Bus activation. It serves `Settings` and `Notification`; every other interface goes to `xdg-desktop-portal-gtk`.
- `Settings` (version 2) reads `appearance.domain` and never writes it. It publishes `color-scheme`, `accent-color`, `contrast` and `reduced-motion` in `org.freedesktop.appearance` and emits `SettingChanged` on inotify changes.
- A new color scheme, accent or wallpaper cross-fades the whole output on the GPU. The compositor snapshots the output without the cursor, holds it opaque until surfaces commit or a timeout ends, then fades it out. The timings come from `ctdesign`.
- Any other appearance write applies at once. `appearance.reduced_motion` or a locked session skips the fade.
Why: apps read the appearance without talking to the compositor. Surfaces repaint on their own commits, so a plain switch flickers.
Rejected: only `xdg-desktop-portal-gtk`, which knows nothing of Malaysia's appearance; a per-client fade over a private protocol.
Code: `crates/xdg-desktop-portal-malaysia/src/settings.rs`, `crates/malaysia-compositor/src/theme_fade.rs`

## 0036 · 0040 · 0045 · 0050 · 0052 · 0055 · 0061 — GTK and libadwaita bridge, global menu
Decision:
- The bar draws every global menu. The compositor only publishes each window's menu address through Infinity (`app_menu`, `gtk_menus`) and never speaks D-Bus menus.
- Sources, first wins: dbusmenu via `org_kde_kwin_appmenu`, `org.gtk.Menus` via `gtk_shell1` v5, the app's shortcut catalog, then the `.desktop` fallback (app menu, Window, Help).
- `gtk_shell1` sends capabilities `3` only while `menubar.app_menus` is on, so GTK hides its own menubar only when the bar shows it.
- A patched libadwaita, swapped in with `system.replaceDependencies` (`programs.malaysia.globalMenu.libadwaita.enable`), publishes the ☰ menu as the menubar. The bar spreads it over the app menu, Help and a File or Options title.
- `xdg-desktop-portal-malaysia` writes the exact accent and tint as GTK user CSS (`malaysia.css`) and publishes it live in `org.malaysia.appearance` for the patched libadwaita. `appearance.gtk_bridge = false` turns it off.
- Apps without a menu get hand-written shortcut menus from `crates/malaysia-shell/shortcuts/`. The compositor types the chosen chord into the window (`send_shortcut`).
Why: GTK 4 loads no modules and libadwaita rounds the accent to its presets. Only a patched libadwaita and the portal give a real menu and exact, live colours.
Rejected: an own GTK theme (libadwaita ignores it); writing dconf (it changes every GNOME session).
Code: `crates/malaysia-compositor/src/gtk.rs`, `crates/malaysia-shell/src/menubar/gtkmenus.rs`, `crates/xdg-desktop-portal-malaysia/src/gtk.rs`

## 0049 · 0072 · 0073 — Wallpaper process, settings, desktops and animation
Decision:
- `malaysia-wallpaper` is its own Wayland client (SCTK + calloop) that draws with `wgpu` on Vulkan only. It links no Freya or Skia; the compositor and `malaysia-shell` link no `wgpu`.
- `ctappearance` owns the files: Settings writes `wallpaper-selection.state` and `wallpaper.domain` (`wallpaper.collections`); `malaysia-wallpaper` writes `wallpaper.state` (dominant color, tint grid per desktop, displays, shuffled pick).
- Sources are the embedded `collapse.heic`, image files, colors (`solid-colors/` and the user's 1×1 PNGs), `shuffle:colors` and `shuffle:collection:<name>`. Five scale modes; `custom` is a crop rectangle in image fractions, per desktop or per `"<desktop>/<output>"`.
- One layer surface per output and per desktop with its own wallpaper, tagged with `infinity_window_style_v1.set_desktop`. A desktop shows its tagged surfaces, else the untagged ones; key `default` means every desktop.
- Dynamic HEIC (`h24`, `apr`, `solar`) follows the time or the appearance. Videos (ferricast) and Shadertoy-style GLSL run at most at `WALLPAPER_FPS`, wait for frame callbacks and stop with `paused` or reduced motion.
- `malaysia-wallpaper` writes freedesktop thumbnails and light/dark stills; Settings shows those and decodes no HEIC, video or GLSL.
Why: the wallpaper has no widgets, text or accessibility, so the kit adds only weight; its decoders live in one process.
Rejected: keeping it in the kit; one surface for all desktops (the compositor slides desktops and could not split it); decoding in Settings.
Code: `crates/malaysia-wallpaper/src/wayland.rs`, `barrierkit:crates/ctappearance/src/wallpaper.rs`, `scripts/check-client-only-deps.sh`

## 0051 · 0053 · 0054 — Spaces and focus mode
Decision:
- Spaces form one row of desktops (at most 99) and focus spaces. `Ctrl+Alt+←/→` switches space and `Ctrl+Alt+N` adds a desktop.
- The green button (`window.green_button = "focus"`, the default), F11 and `Ctrl+Super+F` move the window to a new focus space on its right, with the xdg `fullscreen` state. Leaving focus or closing the window deletes the space.
- Browsers in `window.focus_titlebar_apps` get `maximized` instead, so they keep their tabs. While one owns the current focus space the compositor empties `button-layout`, so Firefox and Chromium drop their traffic lights and move their tabs to the left edge on their own; it resets the key when focus mode starts to end.
- With the pointer at the top edge, a black bar comes down. SSD windows and those browsers get the compositor titlebar over their content.
- CSD apps learn it through `infinity_focus_v1`, open to any client. With `set_overlay` they draw their own titlebar over the content at `bar_height`, as barrierkit and the libadwaita patch do; other CSD windows move down.
- Windows of hidden spaces leave the scene (`Windows::set_off_space`), so drawing, hit-testing and focus ignore them.
Why: `Ctrl+←/→` moves by words in apps. Only the app knows where its headerbar ends.
Rejected: hiding the headerbar from the compositor; an SSD titlebar over the libadwaita headerbar.
Code: `crates/malaysia-scene/src/spaces.rs`, `crates/malaysia-compositor/src/spaces.rs`, `crates/malaysia-compositor/src/focus_notice.rs`

## 0056 — X11 apps through xwayland-satellite
Decision: The compositor starts `xwayland-satellite` on the first free X display, hands it the listening sockets and restarts it unless it dies within 5 s. X11 windows arrive as plain `xdg_toplevel`s, and `spawn` adds `DISPLAY` to each child.
Why: satellite is the X11 window manager, so the compositor keeps a single kind of window.
Rejected: smithay's XWM (`xwayland` feature); a separate systemd unit (it cannot pick the free display).
Code: `crates/malaysia-compositor/src/xwayland.rs`, `crates/malaysia-compositor/src/spawn.rs`

## 0059 · 0075 — Window corners and menu bar styles
Decision:
- A window's radius follows the bar it sends with `infinity_window_style_v1.set_bar`: `titlebar` 16, `unified_compact` 20, `unified` 26, `utility` 15. Without it, CSD windows and dialogs get 26, SSD windows 16, fullscreen 0. `window.corner_radius` overrides them.
- Frames, shadows, borders and blur use a continuous corner: a superellipse that starts 1.52866 × R from the vertex, exponent 3.34 (`WINDOW_CORNER`).
- `menubar.style` is `translucent` (default: glass and ink picked from the wallpaper under the bar), `solid` (the theme bar at `MENUBAR_CLASSIC_ALPHA`) or `floating` (no glass; only the open title pill blurs).
- `menubar.bar_height` is `slim` (30 px, default) or `wide` (38 px). `MenubarConfig::height()` is the only mapping.
- `appearance.reduce_transparency` defaults to `false`. With it, or with Increase contrast, every style draws an opaque solid bar.
- The bar's blur shows only the wallpaper, never windows.
Why: the radius tells the window kind, as in macOS Tahoe, and the continuous curve has no curvature jump at the edge. The shell already reads the wallpaper grid, so it picks the ink itself.
Rejected: Apple's exact three-cubic curve (no closed-form SDF); blurring the windows behind the bar (energy for a band windows rarely reach).
Code: `crates/malaysia-compositor/src/window_style.rs`, `crates/grade-graphics/src/window_shape.glsl`, `barrierkit:crates/ctappearance/src/menubar.rs`

## 0066 · 0067 · 0070 · 0077 — Accessibility display, text size, own themes and font rendering
Decision:
- Every key lives in `appearance.domain` (`ctappearance::AppearanceConfig`); slider steps are `ctdesign` tokens. Settings › Accessibility holds Display, Text size and freedesktop Options.
- `invert_colors`, `display_contrast` and `color_filters` are one compositor shader pass: color matrix, inversion, contrast. Captures keep the unfiltered image.
- `show_borders`, `differentiate_without_color` and the text size apply in the kit; `pointer_size` and `shake_to_locate` in the compositor's cursor; `menu_bar_size` in the menubar.
- `text_sizes` sets one size per toolkit, else `text_size`: GTK 4 through the portal's `text-scaling-factor`, GTK 3 through `GDK_DPI_SCALE` on apps the shell opens, Qt 5/6 through qt5ct/qt6ct, whose Fusion style, palette and fonts the portal writes while `qt_bridge` is on.
- «Use your own themes» turns `gtk_bridge` and `qt_bridge` off together. Settings lists only installed themes and styles and writes where each toolkit reads (GSettings plus `settings.ini`; `qt6ct.conf` and `qt5ct.conf`), never a symlinked file.
- The compositor turns on FreeType stem darkening (`FREETYPE_PROPERTIES`) and points `FONTCONFIG_FILE` at `malaysia.conf` (sans-serif is Adwaita Sans), unless set. Weights are never remapped.
Why: each toolkit reads size and theme from its own source, so each gets it there; a GPU pass works on every driver and in the nested session.
Rejected: DRM color matrix and gamma LUT; `QT_FONT_DPI` (zooms the whole Qt UI); fontconfig `embolden`.
Code: `crates/malaysia-compositor/src/display_filter.rs`, `crates/xdg-desktop-portal-malaysia/src/qt.rs`, `crates/malaysia-settings/src/freedesktop.rs`

## 0068 — Notification Center
Decision:
- `malaysia-shell notifications` owns `org.freedesktop.Notifications`, `org.gtk.Notifications`, `org.malaysia.Notifications1` and KDE's job server, and draws banners and the panel. D-Bus starts it as `malaysia-shell@notifications.service`; the portal backend forwards to `org.gtk.Notifications`.
- Delivered notifications and the active Focus persist in `~/.local/share/malaysia/notifications.state`; per-app and Focus settings live in `notifications.domain`.
- The app id comes from GTK or the portal, else the sender's cgroup scope. The card icon is always the app's desktop-entry icon, so no notification borrows another app's.
- Urgency 2 is critical only for apps the user allows; otherwise it is active and never breaks through a Focus. The user, not the app, picks style and locations.
- Banners sit top right; the front app's notifications skip the banner. The panel opens from the clock or a two-finger swipe at the screen's right edge. Cards wear the Dock's glass, unlike macOS.
- Focus has four modes (Do Not Disturb, Sleep, Personal, Work), each with allowed apps and time-sensitive alerts; silenced notifications go quietly to the panel. A secondary click on the clock toggles Do Not Disturb.
- The lock screen lists apps allowed there and hides content unless previews are «Always»; it rereads `notifications.state` every 2 s.
- The Dock reads badges and progress from `com.canonical.Unity.LauncherEntry`. Live Activities (KDE jobs, `call.ongoing`, CUPS jobs) are Malaysia's own `Activities` property.
Why: one process with one persistent store lets the menubar, Dock and lock read a single model, and history survives restarts.
Code: `crates/malaysia-shell/src/notifications/server.rs`, `crates/malaysia-shell/src/notifications/store.rs`, `crates/xdg-desktop-portal-malaysia/src/notification.rs`

## 0074 — Lock screen over the wallpaper and the lock transition
Decision:
- While locked, the compositor draws the `malaysia-wallpaper` background under the lock surfaces and the `malaysia-menubar` layer above them, without input; with no wallpaper, `DESKTOP_CLEAR`.
- The lock surface is transparent. Only the password field asks for blur, drawn as a capsule over the blurred wallpaper; the field is light in every theme.
- `malaysia-shell lock` draws date and time at top center, then user, field, hint and notifications at bottom center. Keyboard layout and power menu sit in the slot (`LOCK_SLOT`) the menubar leaves free; only the menubar draws status icons.
- On `lock`, input and captures stop. The compositor sends `infinity_session_v1.locking`, fades the wallpaper in over the desktop while the menubar closes up, swaps in the lock surfaces and sends `locked` only after that frame is presented.
- After `LOCK_START_GRACE` (3 s) without lock buffers it swaps anyway. A lock before sleep skips the transition; reduced motion keeps only a `DURATION_BASE` fade.
- On unlock the client fades its content and sends `unlock_and_destroy`; the compositor sends `unlocked` and uncovers the desktop in `DURATION_BASE`.
Why: [`ext-session-lock-v1`](https://wayland.app/protocols/ext-session-lock-v1) allows shell UI while locked but forbids `locked` before normal content is hidden, so the menubar animates itself.
Rejected: the lock client drawing its own status row (two copies flashed at the swap); sending `locked` before the fade.
Code: `crates/malaysia-compositor/src/lock_transition.rs`, `crates/malaysia-shell/src/lock.rs`, `crates/malaysia-shell/src/menubar/lock_row.rs`
