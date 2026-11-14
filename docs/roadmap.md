# Roadmap

The compositor is built in blocks. **This file is the single source of
truth for the status**: what is done, what is missing and how it is tested. The why of
each decision lives in the ADRs; the open findings, in [`debt.md`](debt.md).

## Now (2026-11-07)

- **Global menu (Block 14).** 14f-2 accepted (2026-11-01); still to accept:
  14b-2 (system dialogs), the Alt mnemonics and the scrolling of long
  menus; fullscreen from the Window menu is still pending. 14d-1,
  14e and 14f-1 are accepted (see §14). Test: `./dev.sh demo`
  and the menus of a real app.
- **Spaces and focus mode (Block 17, ADR 0051).** 17a, 17b and 17c
  done: space row, slide with a black strip and focus mode with the
  green button, the top edge in focus (17d), and the black bar and the
  barrierkit traffic lights that appear with it (17e). The libadwaita focus
  mode (17f) is done in the fork; it still has to be accepted in a session.
- **Window capture with frame.** Window capture returns only the
  client's content; the Tengen titlebar and the effects are still missing.
- **Portal 12a.** `Settings` and the GTK/libadwaita bridge (ADR 0050) are
  done. Missing: `Screenshot`, `ScreenCast` with
  PipeWire and the permission dialogs. Test: `./dev.sh screencopy` and
  `./dev.sh screencopy-window`.
- **NixOS.** The module already installs the package, session, PAM, `hardware.graphics`,
  units and `xdg.portal` with GTK as fallback. Missing: polkit, keyring/AT-SPI and a VM.

## Blocks

### 0 — Scaffolding

Cargo workspace, Smithay pinned by revision (ADR 0028), `tracing` and coordinate
types. **Done.** Divergence: the types live in `malaysia-scene`, not
in the compositor (ADR 0001), and `ctdesign` is not empty. Test: `./dev.sh run`.

### 1 — A client connects

`CompositorState`, `ShmState`, `SeatState`, `XdgShellState`, Wayland socket and
globals (`wl_compositor`, `wl_shm`, `wl_output`, `wl_seat`, `xdg_wm_base`).
**Done.** Test: `./dev.sh run` and `wayland-info` lists all the globals.

### 2 — A window on screen

winit backend, double-buffered commit, initial configure, `wl_shm` import and
render. **Done.** Divergence: mapping happens on the first commit with a buffer,
not on `ack_configure`; windows are tracked in `window.rs` without
`desktop::Space` (ADR 0015). Test: `./dev.sh smoke`.

### 3 — Input

Keyboard focus on map, keymap/repeat, pointer hit-testing, `set_cursor` and
central serials. **Done.** Typing, hover and cursor pixels are
verified by hand. Test: `./dev.sh smoke`. `text-input-v3` and
`input-method-v2` done, not accepted: input method popups appear
below the text cursor, or above it if they do not fit. Test: `fcitx5` or `ibus` with
a GTK app. `virtual-keyboard` and `virtual-pointer` done, not accepted, offered
to every client outside a `security-context-v1` sandbox (`security::unrestricted`); their events take the same
path as libinput, with shortcuts, lock and idle. Test: a shell probe that
types into a focused `foot`.

### 4 — Manipulable windows

Move, resize by edges, close, stack and click-to-focus. **Done.**
Tests: `./dev.sh block4` (50 windows), `./dev.sh move` and
`./dev.sh decoration`; the drag feel is tested by hand.

### 5 — Popups

Geometry tree in `malaysia-scene`, surfaces in the compositor,
constraints against the output, configure before mapping and dismissal by click
outside/Esc/client death. **Done.** Layer-shell child popups
work since `ab8e0b5`; reactive repositioning is still pending. Test:
`cargo run -p sctk-probe --bin popup_probe`.

### 6 — Clipboard

`wl_data_device_manager` and primary selection, with focus tied to the keyboard.
**Done.** The selection is per seat; drag & drop goes through the same data device (`WaylandDndGrabHandler` in `handlers.rs`). Test: `wl-copy`
in one client and `wl-paste` in another.

### 7 — linux-dmabuf and explicit sync

`zwp_linux_dmabuf_v1` v5 with feedback, explicit and implicit sync, release
deferred by fence and shell over EGL (`wl_egl_window`, without `wl_shm`). **Done.**
No fallback to v3: without a render node, startup fails. Test: `vkcube`, `mpv`
(gpu-next), wezterm and Firefox accelerated in nested mode; `./dev.sh check`.

### 8 — Direct Session (DRM/KMS)

**8a done and accepted** on the real panel on 2026-10-21 (atomic DRM/KMS,
libseat, libinput, GBM/EGL/GLES, one primary plane; VT switching and lock).
Test: `./dev.sh drm-vkms`. **8b** (hotplug, multiple outputs, per-monitor
scale) pending. **8c**: direct scanout on the primary plane for
fullscreen windows, cursor plane and per-surface dmabuf feedback
with a scanout tranche, done, not accepted on the real panel. The
overlay planes exist behind `debug.overlay_planes`, off by default until
they are accepted on the real panel with a video playing; on i915 they stay off (S-02 in
[`debt.md`](debt.md)). Multi-GPU with `GpuManager` and `MultiRenderer` over
GLES, done, not accepted: with `--drm-device` on a GPU other than the rendering one, it
composites on the render GPU and copies to the scanout GPU's buffer; a
display-only device (vkms) still receives the render GPU's
buffers. Multiple outputs on multiple GPUs wait for 8b. Test: `./dev.sh drm-vkms`
and `--drm-device` with the second GPU of a hybrid laptop. Test: with `--log-level=trace`,
`mpv --fs` must land on the primary plane and moving the cursor must only touch the
cursor plane.

### 9 — Fractional scale

`State::scale`, persistent `display.scale`, `wp_fractional_scale_v1`, render,
damage, cursor and the shell's viewporter. **Done and accepted** on the real panel at
1.0/1.25/1.5 on 2026-10-21. Pending: per-output scale with 8b. Test:
`./dev.sh demo` with `scale = 1.25` in `display.domain`.

### 10 — Lock with authentication

PAM in the lock process, masked field, auto-lock via
`ext-idle-notify-v1`, recovery with backoff and `Ctrl+Alt+L` (locks or
relaunches, never unlocks). **Done and accepted** on the real panel on 2026-10-21.
The lock screen over the wallpaper and the transition from the desktop
(ADR 0074) are implemented (2026-11-11) and not yet tried on hardware.
Pending: repeat the unlock with the ThinkPad's real credentials.

### 11 — Everyday protocols

`wp_presentation`, `xdg-activation`, `wp_cursor_shape`, shake-to-locate,
pointer constraints, relative pointer, output/window capture and macOS
screenshots (`Shift+Super+3/4`, PrintScreen). **Accepted in nested mode on 2026-10-23.**
Pending: presentation on vkms/DRM, rotated capture and the
`Shift+Super+5` app (timer, thumbnail, `Control` to the clipboard, `Option`
without shadow). Tests: `./dev.sh presentation`, `activation`, `pointers`,
`screencopy` and `screencopy-window`.
**X11 apps (2026-11-03, ADR 0056):** xwayland-satellite with `-listenfd`; the
compositor exports `DISPLAY` to its children and to the user manager. Tested against
headless weston with `xdpyinfo` and `xeyes`; still to test in a session with OnlyOffice.

### 12 — Portals and services

**12a partial:** `xdg-desktop-portal-malaysia` serves `Settings`
(`org.freedesktop.appearance` from `appearance.domain`, `SettingChanged`,
`UseIn=Malaysia` and GTK as fallback) and `org.gnome.desktop.interface`
`color-scheme`. **GTK bridge done (2026-10-31, ADR 0050):** exact accent and
tint in GTK 3/4 and libadwaita through user CSS; dark GTK 3 through
`settings.ini`; the portal starts with `malaysia-session.target`. **Live
(2026-11-01, ADR 0055):** the portal publishes `org.malaysia.appearance`
(`gtk4-css`, `accent-color`) and the patched libadwaita changes accent and tint
without reopening the app; Flatpak and GTK 3 still need a reopen. **Folder
color (2026-11-14):** the portal writes `Malaysia-Tinted-<rrggbb>` and switches the GTK
icon theme to it ([`appearance.icon_color`](configuration/options.md)). Test:
`cargo test -p xdg-desktop-portal-malaysia`, and in a session open Nautilus with
`window_color = "accent"`. Missing: `Screenshot`/`ScreenCast` and the
permission dialogs. **12b:** notifications done (ADR 0068: server, history, banners, panel, Focus modes, Settings › Notifications and Focus, portal v2, reply, lock screen, Dock badges and progress, Live Activities for transfers, calls and printing; the panel has no widgets yet). Also pending: polkit, keyring/Secret
Service and AT-SPI. **12c phase 1 (2026-11-09, ADR 0069):** Settings ›
Wi-Fi, Bluetooth, Network, VPN and Energy on NetworkManager, BlueZ, UPower
and power-profiles-daemon. **Phase 2 menu bar (2026-11-09):** VPN, Bluetooth,
Wi-Fi and Battery menus and «Show in menu bar». Pending: the secret and pairing
agents in the shell (phase 2), idle timers and network details (phase 3).

### 13 — Chrome and launcher

Launcher `malaysia-shell launcher` + `Super+Space` **done (2026-10-23)**.
Unified frame **done (2026-10-24):** 16 px corners, compositor-side
shadow, inner border and integrated SSD titlebar; CSD windows receive
corners/shadow/border without a titlebar. Hover glyphs and double click also done.
Pending: window capture with frame (focus mode is Block 17). Test:
`./dev.sh titlebar` and `./dev.sh demo`.
The menu bar is solid, translucent or floating (default) over the blurred
wallpaper, 30 or 38 px high, chosen in System Settings › Desktop & Dock
(ADR 0075).
The Dock follows macOS: one icon per app with kept, open and recent apps,
dots, badges, name label, magnification, size fitted to the screen, left,
bottom or right, automatic hiding and launch bounce, all set in System
Settings › Desktop & Dock ([dock.md](architecture/dock.md)). Minimizing
runs the genie, scale or suck effect toward the Dock tile.

### 14 — Global menu

`org_kde_kwin_appmenu` + `com.canonical.dbusmenu` (ADR 0036/0040) and
`gtk_shell1` → `org.gtk.Menus` (ADR 0045). The status sources are the
block document merged in `fb02921` (`git show
fb02921^:docs/bloques/14-menu-global.md`) and the cited code.

**Done:**

- 14a (popups in BarrierKit): accepted 2026-10-24; sibling submenus
  re-accepted 2026-10-28.
- 14b-1 (desktop menu): 2026-10-24.
- 14c (per-app menu with `.desktop` fallback): 2026-10-25.
- 14d-1 (compositor, `org_kde_kwin_appmenu` + Infinity v2 `app_menu`
  event): implemented; `7c3bfd1` and `fb4005f` in the tag
  `archivo/historia-hasta-2026-10-26`, code in
  `crates/malaysia-compositor/src/appmenu.rs`.
- 14d-2/3 (dbusmenu client, full tree): 2026-10-26.
- 14d-5/6 (remembered titles, `MENU_GRACE`, separators): 2026-10-27.
- 14e (own apps export dbusmenu): accepted 2026-10-26 with the
  `contador` example, removed on 2026-11-02; Ondina tested it until barrierkit
  removed its demo apps (`35dba16`). The export is
  `crates/bk/src/platform/main_menu_export.rs` in barrierkit.
- 14f-1 (`gtk_shell1` v5 + Infinity v3 `gtk_menus` event): accepted
  2026-10-28 in a nested session (`crates/malaysia-compositor/src/gtk.rs`; the
  bar only records the address, `crates/malaysia-shell/src/menubar/bar.rs`).
- Keyboard in the popups (barrierkit `crates/bk/src/menu/mod.rs`,
  `on_global_key_down`): ↑↓, Home/End, → opens a submenu, ← closes the level,
  Enter/Space activate, ←→ in the root menu move to the neighboring title
  (`with_navigation`, used in `crates/malaysia-shell/src/menubar/menu.rs`).
  The compositor handles Esc (`State::dismiss_topmost_popup`).

**Pending:**

- 14f-2: accepted 2026-11-01 in a nested session with
  `gtk3-demo-application` (Preferences menu in the bar; `7a8199d`,
  `menubar/gtkmenus.rs`, ADR 0045 §5–6).
- 14b-2: implemented 2026-11-01, still to accept in nested mode.
  - Sleep runs directly; Restart…, Shut Down… and Log Out… open
    `malaysia-shell power <action>`: a macOS-style dialog (icon, question,
    60 s countdown that runs the action, full-width Cancel/action).
    logind: `Suspend`/`Reboot`/`PowerOff(true)` and `TerminateSession` with
    `XDG_SESSION_ID`, which the compositor imports into the user manager
    (`crates/malaysia-shell/src/power.rs`).
  - `MALAYSIA_POWER_DRY_RUN=1` (default in `./dev.sh`, forced in the
    nested compositor) only writes the call to the log.
  - ❄ menu: About This PC, System Settings… (with icons),
    Recent Items (focused apps in `recents.state` and documents from
    `recently-used.xbel`; «Clear Menu» hides the earlier ones), power,
    Lock Screen and «Log Out <name>…» (GECOS).
  - Force Quit… (⌥❖Esc, no longer in the ❄ menu): `malaysia-shell force-quit` lists
    one row per `zwlr_foreign_toplevel` app with its icon; the button requests
    `force_quit` (Infinity v4, barrierkit `8b96ae7`) and the compositor sends
    SIGKILL to the pid from the socket credentials. An X11 window is asked to
    close instead: that pid is `xwayland-satellite`, shared by every X11 app.
  - About This PC: `malaysia-shell about`, a fixed window with traffic lights
    like «About This Mac». Model from DMI (Lenovo stores «ThinkPad …» in
    `product_version`); drawing `about/thinkpad.svg` for ThinkPad and
    `about/generic.svg` for the rest. Subtitle with the panel's inches
    (EDID). Rows as on macOS: processor; graphics (the integrated GPU from
    the processor name or, otherwise, `pci.ids` from `hwdata` plus the VRAM);
    memory; startup disk («512 GB SSD», following LUKS/LVM); serial number if
    readable; and the system with its version (`NAME` and `VERSION` from
    `/etc/os-release`, e.g. «NixOS · 26.05 (Yarara)»). «More Info…» opens
    Settings (`crates/malaysia-shell/src/dialogs.rs`). It measures 280×547,
    as on Tahoe. The whole window is glass (`bk::WindowGlass`, ADR 0078), like
    «About <app>».
- Mnemonics, Space and scrolling: implemented 2026-11-01, still to accept in
  nested mode (barrierkit `bdbd583` and `a2394a6`, `menu.rs`).
  - dbusmenu and GTK turn `_` into `MNEMONIC_MARK` (U+2063, invisible and
    zero-width); with Alt held, the row underlines that letter and on release it returns to
    plain text. Title comparisons and search ignore it.
  - Space activates only without Ctrl or Alt (`inserts_text`): Ctrl+Space belongs to the
    input method and Alt+Space to the window menu. In the Help
    search, a space is typed into the query.
  - Menus taller than the output: the root menu requests `resize_y` without `slide_y`
    (it does not go up over the bar) and submenus `slide_y` + `resize_y`.
    `MenuView` shows ▲/▼ strips that scroll on hover, the wheel
    scrolls and the keyboard focus stays in view; submenus anchor to
    the visible row (`PopupHandle::scroll_shift`).
- «System Settings…» in the ❄ menu does not open the client with
  `MALAYSIA_TEST_INPUT` in the nested session.
- Acceptance on DRM of 14a, 14b-1 and 14c.
- 14f-4 (GTK 3 with `GtkMenuBar`): implemented 2026-11-04, pending acceptance
  with virt-manager. The NixOS module installs `appmenu-gtk-module`
  (`nix/appmenu-gtk-module.nix`) and loads it with `GTK_MODULES`; the bar
  resolves its `unity.*` actions on `menubar_path` (`menubar/gtkmenus.rs`).
  Accepted 2026-11-04 with virt-manager (patch
  `nix/patches/appmenu-gtk-module-wayland-realize.patch`).
- 14f-5 (Electron 43): implemented 2026-11-04, pending acceptance with
  Bitwarden. `programs.malaysia.globalMenu.electron.enable` applies
  electron/electron#54629 (`nix/patches/electron-wayland-appmenu.patch`) and
  adds `libdbusmenu` to the rpath. Chromium 150 already ships
  `org_kde_kwin_appmenu` (`ozone_platform_wayland.cc`); not tested yet.
- 14g (shortcut menus, ADR 0061): implemented 2026-11-07, pending acceptance
  with WPS and ONLYOFFICE. Catalog of 57 apps (office apps without a menu,
  IDEs, Electron and terminals) in `crates/malaysia-shell/shortcuts/` and
  Infinity v6 `send_shortcut`.

**Outside the block (noted):** HUD in the launcher and Registrar matching by
pid (XWayland).

Test: `./dev.sh demo` with real apps.

### 15 — Rebranding to Malaysia

Crates, binaries, D-Bus, `MALAYSIA_*` variables, paths `~/.config/malaysia` and
`~/.local/share/malaysia`, Nix, PAM, portal and session. **Done (2026-10-26).**
The old names (Yuki, CelestialKit, Neptunia) are kept only in the superseded ADRs
as a historical record (ADR 0043).

### 16 — Wallpaper on Vulkan

**16a in progress:** `malaysia-wallpaper` is an own client with wgpu/Vulkan
over layer-shell, fractional scale, mipmaps, zero-copy dmabuf and inotify reload
of `wallpaper-selection.state` (ADR 0049). **16b in progress:** the default wallpaper is
`collapse.heic` (decoded with libheif). **16c in progress (2026-11-11, ADR 0073):**
dynamic HEIC wallpapers follow the appearance (`apr`, `solar`) or the time of
day (`h24`); videos (GStreamer) and GLSL shaders animate at `WALLPAPER_FPS`
and wait for the frame callback; `tide.frag` ships as the first animated
wallpaper. Not tried in a real session yet. Test: `./dev.sh wallpaper-check`.

**16d in progress (2026-11-11, ADR 0072):** System Settings › Wallpaper
chooses among the installed images and solid colors (Malaysia Dream first),
sets «Scale to screen» (fill, fit, stretch, center) and frames an image by
hand («Custom framing…»: drag, click to center, zoom 100–400 %).
`malaysia-wallpaper` draws every mode. ADR 0073 adds a wallpaper per
desktop («Show on all desktops»), a crop per display, collections of folders
and images, the filter, shuffling of colors and collections, custom colors,
and the Dynamic and Animated sections, with previews written by
`malaysia-wallpaper`; the crop and collection sheets follow the mock-up.
«Show All» turns a long row into a grid and «Play» pauses an animated
wallpaper. Not tried in a real session yet.

### 17 — Spaces and focus mode

Concept and plan in ADR 0051.

- **17a — Space row. Done (2026-11-01), not accepted in a session.**
  `malaysia_scene::spaces` stores the row (up to 99 desktops; focus spaces
  do not count) and where each window lives. The compositor removes from the scene the
  windows of other spaces (`Windows::set_off_space`, separate from minimizing).
  `Ctrl+Alt+←/→` switches space and `Ctrl+Alt+N` adds a desktop. A
  new window goes to the shown desktop and a dialog goes with its parent.
  Focusing a window on another space (dock, activation) shows its space.
  Test: `./dev.sh demo`, open windows, `Ctrl+Alt+N`, open another one and switch
  with `Ctrl+Alt+←/→`; the log shows `space shown`.
- **17b — Slide. Done (2026-11-01), not accepted in a session.** When
  switching space the row slides (`SPACE_SLIDE_MS` 480 ms, curve
  `SPACE_SLIDE_EASE`) with a black strip of `SPACE_GAP` (48 px) between
  spaces; the tokens are in `ctdesign` (barrierkit `55614a3`). The top
  bar (Top panel anchored at the top with an exclusive zone) and the wallpaper are drawn
  inside each desktop and leave with it; the dock and the other panels
  stay fixed. The destination's windows receive input from the first
  frame and a second press reorients the slide from where it is.
  `appearance.reduced_motion` removes it. Test: `./dev.sh demo`, two
  desktops with windows and `Ctrl+Alt+←/→`.
- **17c — Focus mode. Done (2026-11-01), not accepted in a session.** The
  green traffic light (`green_button = "focus"`), a CSD app's own green button,
  F11 (`set_fullscreen`) and `Ctrl+Super+F` create a focus space to the
  right of the desktop. The window receives `fullscreen` and grows from its
  frame to the output, with the corner going from 16 to 0, while the row
  slides. The window is drawn above the dock, which does not move until
  it is covered. In the focus space the bar and the dock are not drawn and do not
  receive clicks, and the background is black. Exiting does the reverse and deletes the space
  at the end; closing the window deletes it instantly. During the animation
  `Ctrl+Alt+←/→` is ignored. When it finishes growing, the animation keeps
  drawing the window fullscreen until the client delivers its
  buffer at that size (300 ms at most), so that no black frame shows; the log
  `focus transition handed over` says how long it waited. Test: `./dev.sh demo`,
  green button of an SSD window, `Ctrl+Alt+←/→` between the desktop and the
  focus space, and green again. A fixed-width window (Settings) never enters
  focus: its green button stretches it vertically. barrierkit CSD apps request
  fullscreen with their green button (barrierkit `ec9ff5a`), and a `set_maximized` on
  a window in focus takes it out.
- **17d — Top edge. Done (2026-11-01), not accepted in a session.** In a
  still focus space, the pointer at the top edge lowers the bar over
  a black strip (`DURATION_BASE`, 220 ms). An SSD window gets back its
  Tengen titlebar below the bar, above the content and full width; its
  traffic lights work (green exits focus) and that strip does not reach the
  client. A CSD window (barrierkit, libadwaita) moves down entirely by the height of
  the bar, input included, and its own bar stays visible (since
  17g, only `infinity_focus_v1` v1 clients). The bar
  goes up when the pointer moves more than two titlebars below it, except
  with a menu open. The black bar with white text arrives with 17e. Test: `./dev.sh demo`, focus
  mode with an SSD window and with a CSD one, and the pointer at the top.
- **17e — Infinity. Done (2026-11-01), not accepted in a session.** Infinity v5 (barrierkit `ee5b141`) adds the `space` event:
  position, number of spaces, desktop and the state of the bar over a
  focus space (`focus_bar`: none, hidden, revealed). The compositor sends it on
  bind and on each change; the shell publishes it in `Toplevels::space`. Over
  a focus space the menubar is painted with `FOCUS_BAR_BACKGROUND` (black) and
  `FOCUS_BAR_TEXT` (white). For apps, the separate protocol
  `infinity_focus_v1` (barrierkit `c8965c3`) is open to any
  client: a surface's object receives `bar(1)` when the bar lowers
  over the CSD window in focus (and the window moves down with it) and `bar(0)` when it
  goes up. The kit hides the traffic lights of its titlebar and of its sidebar in
  fullscreen and shows them only with `bar(1)`, like Finder. Test: focus mode
  with a barrierkit app (no traffic lights), pointer at the top (they appear),
  pointer down (they go away).
- **17f — libadwaita. Done (2026-11-01), not accepted in a session.** The fork
  (`src/adw-focus-mode.c`, ADR 0053) binds `infinity_focus_manager_v1` if the
  compositor offers it and creates one `infinity_focus_surface_v1` per realized
  window. In fullscreen, the outer `AdwToolbarView`s (outside
  dialogs and popovers) hide their top bars; `bar(1)` shows them
  with their animation and `bar(0)` hides them. On leaving fullscreen they come back.
  A view whose `reveal-top-bars` the app manages is left alone.
  `ADW_DISABLE_FOCUS_MODE` disables it; without the global nothing changes. It arrives
  through `nix/patches/libadwaita-focus-mode.patch` in the
  `libadwaita-global-menu` package (dev shell and `system.replaceDependencies`). The
  compositor does not change. Tests in the fork (`tests/test-focus-mode.c`).
  Test: `./dev.sh demo`, gnome-calculator and gnome-text-editor launched
  from the dev shell; in focus without a headerbar, pointer at the top (it appears below the
  bar), pointer down (it goes away), leave focus (always visible). With
  `ADW_DISABLE_FOCUS_MODE=1`, upstream behavior.
- **17g — Overlay titlebar. Done (2026-11-01), not accepted in a session.**
  `infinity_focus_v1` v2 (barrierkit `3841a09`, ADR 0054): with
  `set_overlay` the compositor does not move the CSD window when the bar lowers and
  sends it `bar_height` before `bar(1)`. barrierkit takes the titlebar out of the
  layout in fullscreen and draws it over the content below the bar;
  libadwaita (fork `605edc1`) extends the content and shows its top
  bars on top, opaque. v1 clients keep sliding. Test:
  `./dev.sh demo`, focus mode with a barrierkit app and with
  gnome-calculator; pointer at the top: the titlebar appears below the black bar
  and the content neither moves nor loses its bottom part.
- **17h — Fullscreen bars and Rail in focus. Done (2026-11-03),
  not accepted in a session.** barrierkit `f7362c9` (ADR 0054, «Later
  changes»): the overlay titlebar is drawn plain (by default) or with
  glass that blurs what is below; `fullscreen_push` moves the content down instead of
  covering it; both options change live. With the Rail the titlebar
  stays in place, only the rail comes in from the left and the first row
  of the sidebar moves up to the titlebar's row. Test: `./dev.sh demo`,
  System Settings in focus; in Window › Fullscreen, each bar and the
  push with the pointer at the top; in Window › Titlebar, normal and
  unified Rail: nothing moves down with the bar and the rail comes in from the left.
- **14f-3 — Global menu for libadwaita apps.** Implemented: patch
  `nix/patches/libadwaita-global-menu.patch`, package
  `packages.<system>.libadwaita-global-menu` and NixOS option
  `programs.malaysia.globalMenu.libadwaita.enable` with
  `system.replaceDependencies` (ADR 0052). Still to accept in a nested session
  with a real app (`gnome-text-editor` or `gnome-calculator`).

### Cross-cutting — NixOS

`nixosModules.default` (`programs.malaysia.enable`) installs the package, session,
PAM, `hardware.graphics`, shell units and `xdg.portal` with GTK as fallback.
Pending: `services.graphical-desktop`, `security.polkit`, keyring/AT-SPI and a
VM that boots `malaysia-session.target`.

### Not in a block yet

Gaps found by the 2026-11-03 audit and checked again on 2026-11-08 by
searching `crates/malaysia-compositor/src`.

- **Protocols with a Smithay module, not wired:** tablet-v2 (the events are
  dropped), pointer-gestures, keyboard-shortcuts-inhibit, pointer-warp,
  xdg-toplevel-icon,
  xdg-toplevel-tag, xdg-foreign, xdg-system-bell, single-pixel-buffer,
  alpha-modifier, content-type, fifo, commit-timing, drm-lease,
  ext-data-control (no clipboard managers without it),
  ext-background-effect and xwayland-keyboard-grab.
- **Protocols without a Smithay module** (niri implements them):
  wlr-gamma-control (night light), wlr-output-management (kanshi,
  wdisplays), ext-workspace (ADR 0051 spaces) and tearing-control.
- **Configuration:** the `domain` command and the
  `org.malaysia.Compositor1` D-Bus API (ADR 0021) have no code; the options
  marked «pending» in [`options.md`](configuration/options.md) arrive with
  their effect.
- **Effects:** blur and vibrancy (`AGENTS.md`, design policy 3); plan in
  [ADR 0071](decisions/0071-background-blur-and-ext-background-effect.md).
  Phase 1 done (2026-11-11), not accepted in a session: `ext-background-effect-v1`
  and the wallpaper blur behind windows and layer surfaces that ask for it.
  The Dock's glass also blurs the windows behind it (`"behind"`, 2026-11-12).
  Pending: true transparency for the other surfaces (`"windows"`, pop-ups),
  general blur, the shell's own regions and their options in Settings.
  Sidebars of BarrierKit windows get a glass panel with the wallpaper's
  chroma and a line derived from their fill ([ADR 0078](decisions/0078-sidebar-glass-panels.md)),
  accepted 2026-11-13. The protocol, the kit and the compositor's material, edge, rim and island shadow are written, built and unit-tested, the shaders with a real GL context (2026-11-13), but not yet run on a screen; the values come from the owner's mockup and the dark unfocused state is not measured.
- **Shell:** supervision and overview (ADR 0023), notifications daemon,
  polkit agent, volume/network/battery/brightness
  indicators (network, Bluetooth and battery done, ADR 0069), OSD and
  media keys.

