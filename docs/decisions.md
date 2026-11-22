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

## 0020 · 0038 · 0065 · 0071 · 0091 · 0092 · 0093 — Blur and wallpaper tinting
Decision:
- Blur is drawn only while `appearance.reduce_transparency` and `appearance.contrast` are `false` (both default `false`), except two cases. The backdrop of a window is the `under_window_background` material of `ctdesign` and follows what macOS 27.2.1 measures: `reduce_transparency` draws that material's solid, `reduce_transparency` with `contrast` draws the other solid, and `contrast` alone keeps the blur. The glass of kit menus survives `reduce_transparency`, as on macOS, and stops only with `contrast`; `ext_background_effect_manager_v1` advertises `blur` while that glass is drawn. How the desktop surfaces change with either setting is not measured, so they keep their old gate: while either is on, the Dock and the notification cards draw their solid and the layer surfaces and the sidebars' glass draw no blur.
- The compositor composes every blur: behind a window, behind a layer surface and inside a BarrierKit window. A client declares a material and never draws a backdrop blur.
- Behind a window, a surface gets blur in the region it sends through `ext-background-effect-v1`. A BarrierKit client may send `infinity_window_style_v1.set_backdrop` with `behind_window` instead, to get the radius of a material; that request wins over the region.
- Inside a window, a BarrierKit client sends `set_backdrop` with `within_window` or `set_scroll_edge` (`infinity_window_style_v1`, version 10) on the surface that draws over the content, and the compositor blurs what the toplevel's surfaces draw below it: the parent, the sub-surfaces under the requester and their children. It never reads the region of `ext-background-effect-v1` for this, so the wallpaper blur of the root surface is not touched. Served since 2026-11-22 (M2), in `blur/within.rs`: each request is a `Cacheable` state of its surface (`WindowBackdrop`: blending, material, radius and at most 16 rectangles; `ScrollEdge`: edge, style, depth), so it applies with the parent's commit on a synchronized sub-surface. A value version 10 does not define makes the request ignored and the earlier one stays; an empty array or a depth of 0 removes it.
- The window path (`append_window_frame`, `build_window_elements`) walks the surface elements from the top. The requester's own element and everything above it stay in front; the material goes right behind the requester's element and the elements below are the source. The requester may be a sub-surface above or below its parent, or the root with a sub-surface placed under it. With nothing below a rectangle the request has no effect there. A request on the root surface wins over the `ext-background-effect-v1` region of the same surface (`blur_region` returns nothing for it); a region on another surface of the tree is not touched. A requester over another one sees the other's material in its source: the lower requesters are painted first, from the bottom up, and their material elements sit in the source behind their own surface elements, so two glass chromes that overlap blur each other; the cache slots are shared, so the second paint of a frame finds nothing to redraw.
- Each rectangle has its own cache (`Behind`, the one the Dock glass uses): the source is rendered into a texture of the rectangle's size, damage-tracked, and the measured pipeline runs again only when that texture or the look changes, so a window that scrolls under its chrome blurs every frame and a still one never. A rectangle blurs only the pixels inside its own bounds, as a region does on macOS. A solid look (Reduce Transparency, an inactive window, the materials that do not blur) is drawn without rendering the source.
- Popups (xdg popups of BarrierKit clients, menus, popovers and tooltips) and layer surfaces of BarrierKit clients serve the same requests, in `append_popups` and `append_layers`. A popup's material is clipped to the shape it already has (the popover's corner and arrow, or the corner radius of the request) and a request on the popup suppresses the menu glass of `popup_has_glass`; a layer surface's request suppresses its `ext-background-effect-v1` region. For these two, `behind_window` blurs what is really behind the surface (the windows and the wallpaper, `behind_elements`) and not the wallpaper alone; for a window it stays the wallpaper, as before.
- `behind_window` blurs the `Background` layer under the rectangle with the material's look, from the same per-rectangle cache, and is drawn behind the window's surfaces like the wallpaper blur; `within_window` is not. A capture of a window (`build_window_elements`: screencopy of a window and the window screenshot) holds the `within_window` material, and never the `behind_window` one.
- The material is one of the 14 `NSVisualEffectView` materials of `ctappearance::material` in the look of the window's state: active for the focused window, inactive (a solid) for the others, `reduced` with Reduce Transparency, `reduced_contrast` with both settings, and `contrast` alone keeps the active look (measured on macOS 27.2.1, `macos-materials.md` 10.4). `glass_regular` and `glass_clear` take the Liquid Glass tokens of `ctappearance::liquid_glass` for the state (`look::glass_look`, with the slider of 0093): the Gaussian of the deep-inside share of the blur radius (`2 x blur x blur_opacity[0]`, adapted to the rectangle's short side and capped) and the face of the model in the post shader. Not implemented for the glass, and so not identical to macOS where they matter: the lens and its refraction (`lens_offset`), the rim, ring and shadow of the 2-D table (`rim_levels`, which also draws outside the shape), the blur that varies with depth (`blur_sigma(depth)` through the 13 sigma levels), the interaction and the merging of neighbours. They need a pass that reads several blurred copies of the backdrop and the shape in one shader, which `draw_pass` cannot do with one texture; the reference is `ctappearance::liquid_glass::render` and the shader description is section 9 of `macos-liquid-glass.md`.
- `set_scroll_edge` blurs a strip of `depth` logical pixels along the surface's own `edge` (`top`, `leading`, `bottom`, `trailing` are physical sides) in the same pipeline. `hard` is uniform and ends in an edge; `soft` and `automatic` fade the blur to nothing across the depth with the stops of `FULLSCREEN_GLASS.mask` (`edge_fade.frag`). What macOS builds for it is observed only as a layer tree (`macos-materials.md` 16): the same tree for the three styles, a pocket of 52 pt or 88 pt, a variable blur of radius 2 and a Gaussian of radius 6 and 20, with the pixels not fitted. So the radius (`FULLSCREEN_GLASS.blur` for soft and `automatic`, `FULLSCREEN_BAND.blur` for hard, the values the kit draws today), the stops, the single blur level instead of the kit's five, the absence of a tint and the resolution of `automatic` to `soft` are **not measured**; under Reduce Transparency the strip is the solid of the `titlebar` material, also **not measured** for a scroll edge.
- A desynchronized sub-surface applies its request at its own commit and a frame can show the material over the content of the frame before or after (the phase 0 control tore 23 of 23 frames in between; the synchronized one 0).
- The compositor decides radius, saturation, tint and lens per material and appearance from `ctdesign`. The radius is in logical pixels: at a larger output scale it widens the Gaussian to reach the same distance.
- The compositor advertises version 10 of `infinity_window_style_v1` only while it serves these requests, and serves them only for BarrierKit clients (any client that binds the global) and for the surfaces of a toplevel's tree. It advertises 10 since M2; a client that binds 9 gets every older request as before and cannot send the new ones.
- One pipeline blurs everything, the one macOS 27.2.1 builds. `blur/look.rs` decides each surface's radius, saturation, veil and cap or tone from the tokens; `blur/gaussian.rs` and `grade-graphics` run it: the backdrop of a region is reduced by 2, 4 or 8 with a box filter, blurred by a Gaussian of the remaining sigma, renormalised at the region's border (a region sees only itself, as `inputNormalizeEdges` does) and upsampled linearly, which stays within 1.4 levels of the exact Gaussian; then a material saturates about the Rec. 709 luma, mixes its veil, takes its per-channel cap or floor and applies its gain and bias (`ctappearance::material::over`), and a shell glass applies its luma tone and chroma gain after mixing its wide halo at its weight (`ctappearance::shell_glass::over`). All of it acts on the output's own values, never on sRGB conversions of macOS's result.
- The values and what each consumer takes:

| Consumer | Backdrop | Token | Gate |
|---|---|---|---|
| Windows with `ext-background-effect-v1` | the wallpaper, whole output | `MATERIAL_UNDER_WINDOW_BACKGROUND_*` (Gaussian 27.6 px, saturation, veil, cap or floor) | solid with `reduce_transparency`; `contrast` alone keeps it |
| Dialogs and alerts | the wallpaper | the same material, its sigma times `Resolved::dock.soften`, which is 1 | as windows |
| Layer surfaces (the bar's open pill, the lock field) | the wallpaper | not measured: the Gaussian that today's Dual Kawase spreads like, no tone | no blur with either setting |
| The sidebars' glass (`set_glass`) | the wallpaper | not measured: the Gaussian that today's passes spread like, the kit's own fill | no blur with either setting |
| The Dock, in dark | what is behind it | `SHELL_GLASS_DOCK_DARK` | solid with either setting |
| Menus and popovers, in dark | what is behind them | `SHELL_GLASS_MENU_DARK` | stops with `contrast` only |
| Notification cards, in dark | what is behind them | `SHELL_GLASS_NOTIFICATION_CARD_DARK` with the panel open, `SHELL_GLASS_MENU_DARK` for a banner | solid with either setting |
| The Dock, menus and notifications, in light | what is behind them | not measured: the softened backdrop and tone of before | as above |
| The panel veil | what is behind it | not measured: unchanged | as the cards |
| `set_backdrop`, `within_window` | what the toplevel's surfaces draw below the requester | the material's token: `MATERIAL_<NAME>_*`, or `LIQUID_GLASS_{REGULAR,CLEAR}_*` for the glass without its lens | solid with `reduce_transparency`; `contrast` alone keeps it; an inactive window is its solid |
| `set_backdrop`, `behind_window` | the `Background` layer | the same | as `within_window`; left out of a window capture |
| `set_scroll_edge` | what the toplevel's surfaces draw below the requester | `FULLSCREEN_GLASS` and `FULLSCREEN_BAND` blur and mask: not measured on macOS | the `titlebar` solid with `reduce_transparency` |

- By default the blur of a window, a dialog, a layer surface and a sidebar shows the `Background` layer, blurred once per output into a cache of its own. The Dock, notification cards and glass popups blur what is really behind them, each from its own cached render, normalised to the blur region it asked for; the wide halo of a menu or a card needs the backdrop up to three sigma around it.
- `ctdesign::BACKGROUND_BLUR_PASSES` and `BACKGROUND_BLUR_OFFSET` no longer set a strength: the compositor takes the Gaussian they spread like only for the surfaces whose blur is not measured. no user option sets a strength: `appearance.glass_blur`, which widened every glass and the dialogs by `DockSurface::soften`, is retired (0092), so `soften` is the constant of each surface.
- `appearance.wallpaper_tinting` (default `true`, dark mode only unless `wallpaper_tinting_in_light`): the compositor averages the wallpaper grid cells behind each window and sends the color in the `wallpaper` event of `infinity_focus_surface_v1`. The kit, the patched libadwaita and the compositor's titlebar mix it in.
Why: macOS composes every blur outside the app, in both blending modes, and the app declares the material; one place then decides radius and accessibility. No client sees other clients' pixels, so only the compositor can blur or sample what is behind a window, and it already samples a client's own pixels for `set_knobs`. The [protocol](https://wayland.app/protocols/ext-background-effect-v1) leaves strength to the compositor and has no material. `set_glass`, `set_plates` and `set_knobs` already paint for clients. A Dual Kawase of 3 passes at offset 3.0 spreads like a Gaussian of 16.8 px where macOS measures 27.6, has no halo and does not renormalise at a region's border; at the offsets that fit 27.6 it stays within 2.0 levels but over the rms tolerance for the low-veil materials. A Gaussian on a backdrop reduced by 8 costs a sixty-fourth of the pixels and stays within the tolerance of the parity procedure (`max <= 2.0`, `rms <= 0.8` per channel).
Rejected: blurring every window by default (GPU cost); a blur drawn by the client as the shipping path (two places for Reduce Transparency, two blur algorithms); `ext-background-effect-v1` on sub-surfaces (KWin, Hyprland and niri ignore their regions); the compositor painting client backgrounds the client did not ask it to paint; the Dual Kawase.
Pending: the frame time of the in-window pass on a real GPU, which is not measured and which the owner's criterion gates (median of at most 1.0 ms at 2x for a 96 px band to pass, over 2.5 ms to stop, no frame of 40 at 8.33 ms or more); M2 and K3 were started before it on the owner's word of 2026-11-22, accepting the risk. On llvmpipe, labeled software rendering, a band of 96 logical rows in a 1280x800 window takes a median of 3.1 to 3.5 ms at 1x and 8.5 to 9.9 ms at 2x in three runs (`cargo test -p malaysia-compositor --lib -- within the_band_blur --ignored --nocapture --test-threads=1`); that says nothing about a GPU. Light mode, Reduce Transparency, Increase Contrast and scale 2 of every desktop surface, the banner's kernel (the banner takes the menu's) and the lock screen (none measured on macOS); a window's blur sees the whole output at its border instead of only its region; scale 2 and a see-through backdrop of the in-window materials (the alpha where the toplevel draws nothing below a rectangle is not measured on macOS, so nothing is drawn there); at most 4 surfaces of one toplevel are requesters; the popup and layer paths are not exercised by the headless tests (no layer-shell or popup client there), only the source they share; the glass materials without the lens, the rim and the depth profile of the blur, and the scroll edge's look, as written above; light mode of the Dock, menus, notifications and banners, and the launcher, Control Center and spaces bar (the `SHELL_GLASS_*` tokens of the kit are dark only, and Malaysia's launcher draws no blur); the in-window blur needs `state.blur`, so it is absent when the blur shaders fail to compile; no real session or kit client has run it, only the client of the headless tests.
Code: `crates/malaysia-compositor/src/blur.rs`, `crates/malaysia-compositor/src/blur/look.rs`, `crates/malaysia-compositor/src/blur/gaussian.rs`, `crates/malaysia-compositor/src/blur/within.rs`, `crates/grade-graphics/src/blur_reduce.frag`, `crates/grade-graphics/src/edge_fade.frag`, `crates/malaysia-compositor/src/window_style.rs`, `crates/malaysia-compositor/src/scene_elements.rs`, `crates/malaysia-compositor/src/capture.rs`, `crates/malaysia-compositor/src/focus_notice.rs`

0092 (2026-11-22): `appearance.glass_blur` is retired like `traffic_light_shape`: a file that has it still loads and the next write drops it. macOS 27.2.1 has no blur setting: Appearance has a Liquid Glass tint slider (`NSGlassTintAmount`) whose per-process effect is measured in the kit (the regular glass goes from a blur of 6.5 px dark or 8.2 px light to 15.5 or 17.5 px and to a tinted panel, the clear glass from none to 16.5 px), and Accessibility › Display has two independent switches. So the blur strength comes only from the measured tokens, the System Settings row of Reduce transparency shows the `reduce_transparency` key alone, enabled and not tied to Increase contrast, and the page Appearance has no Liquid Glass row until a key and a model that interpolates the glass inputs exist. Reversing it means restoring the key in `ctappearance` and the row in `malaysia-settings`; the commit that retired it lists both. The kit side is its ADR 0096. Pending: confirmation by the owner.

0093 (2026-11-22): the Liquid Glass slider is back as `appearance.liquid_glass_tint` (float 0 to 1, default 0; the key is in `ctappearance`, kit ADR 0099), the second row of the first card of Appearance, with the controls of macOS: a slider of 220 px named `Liquid Glass Tint Amount` (es-419 `Cantidad de tinte de Liquid Glass`), three tick marks, and the icons `Capsule Shape On A Rectangle With A Liquid Glass Effect` on the left and `Capsule Shape On A Rectangle` on the right. The compositor applies it to the glass a window requests (`look::glass_look`): in its active look the kit's `tinted` look gives the sigma, the face and the blur fill, which is the `halo` of the pipeline (sigma 16 px at the weight of its opacity: the mix `(1 - n) blur + n fill`). The post shader now draws the face of the model for requested glass (`Post::Glass`: saturation, `black` to `white`, the cap by luma, the fill and the nine-point table of `tone_delta`), which before was only a veil; at 0 the blur is the one of 0092. Against the macOS captures (llvmpipe, `blur::parity`, ignored test `the_glass_a_window_requests_against_the_macos_captures_at_each_slider_position`) the tone over gray 128 and the ramp is within 2 levels (rms 1.3) at the five positions for regular and clear, the 25 to 75 % width of a step within 2 px except regular dark at 0 (8.0 against 6.5: the pipeline uses one sigma, the deepest share, where macOS has a depth profile) and regular light at 0 (10.2 against 8.2), and the contrast within 0.05 of its share at 0; the face of the pipeline is within 0.5 level of `liquid_glass::face`. The Dock, the menu bar, Control Center and the notifications keep their measured looks (not measured under the slider), the inactive, Reduce Transparency and Increase Contrast looks do not follow it (inputs read, pixels not captured), and the sidebar glass of `set_glass` is the wallpaper-tinted shader of 0020 and has no model path. The `reach` factor and the dialogs family of `blur.rs` went: both were constants since 0092. The es-419 names of the two icons are translations; the English ones are macOS's. Pending: confirmation by the owner of the key name `appearance.liquid_glass_tint` and of the row's position; the depth profile of the regular blur in the pipeline; the surfaces of the desktop under the slider (the batch `lgtint-desktop.sh` for `global-states.sh` is prepared in `~/src/probes`); two tests of `window_style` (`each_bar_gets_the_tahoe_radius` and the one of the bar-less titlebar) fail against the kit's new corner tokens (kit ADR 0097) and belong to the corners work.

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

## 0059 · 0075 · 0097 — Window corners and menu bar styles
Decision:
- A window's radius is read from `WINDOW_CORNER` by the bar it sends with `infinity_window_style_v1.set_bar`, and macOS 27.2.1 gives every window kind 16 (the kit's `cae20f79`, ADR 0097 there): `titlebar`, `unified_compact`, `unified` and `utility`, CSD windows and dialogs alike; fullscreen is 0. `window.corner_radius` overrides it.
- The silhouette drawn for a window and a sheet is `CORNER_WINDOW.fitted`, 16.8, which is 0.8 larger than the API radius the content has: its extent is `fitted x expansion` = 25.7, exponent 3.34 (`window_extent`); a forced `window.corner_radius` is taken as given. A menu is `CORNER_MENU.fitted` 12.5 and a popover `CORNER_POPOVER.fitted` 19.7, unrounded (`popover_corner`, `menu_extent`); the alert frame keeps `DIALOG.corner_radius`, 26. The notification radius (20) and the launcher's (24.5) come from their tokens in the shell. Not applied, because the shell draws them with bk's `continuous_corner` or needs a real session to compare: the Dock, the tooltip (5.2, circular), Notification Center widgets, Control Center modules and the concentric rule of `ctappearance::corner` for inner shapes; the 112 x 60 popover whose radius shrinks with its body is not measured as a rule.
- Frames, shadows, borders and blur use a continuous corner: a superellipse that starts 1.52866 × R from the vertex, exponent 3.34 (`WINDOW_CORNER`).
- `menubar.style` is `floating` (default: a clear bar, ink picked from the wallpaper under it, and only the open title pill blurs), `translucent` (glass and ink picked from the wallpaper) or `solid` (the theme bar at `MENUBAR_CLASSIC_ALPHA`).
- `menubar.bar_height` is `slim` (30 px, default) or `wide` (38 px). `MenubarConfig::height()` is the only mapping.
- `appearance.reduce_transparency` defaults to `false`. With it, or with Increase contrast, every style draws an opaque solid bar.
- The bar's blur shows only the wallpaper, never windows.
Why: macOS 27.2.1 measures one radius for every window kind and a silhouette larger than it, and the continuous curve has no curvature jump at the edge. The shell already reads the wallpaper grid, so it picks the ink itself.
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
- Banners sit top right; the front app's notifications skip the banner. The panel opens from the clock or a two-finger swipe at the screen's right edge. Cards wear the menu's veil and tone, with their own light veil, in the Dock's frame.
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
