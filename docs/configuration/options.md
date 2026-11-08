# Malaysia options catalog

Canonical catalog of the compositor options (ADR 0021) and of its NixOS
module. **Domain names and values are identical to macOS's**
whenever an equivalent exists: the sections mirror System Settings
(`appearance`, `window`, `dock`, `menubar`) and the keys mirror `com.apple.dock`
and the rest (`autohide`, `orientation`, `tilesize`, `mineffect`,
`show-process-indicators`, ...).

Full visual map: [`../architecture/interface.md`](../architecture/interface.md).

The NixOS options (`programs.malaysia.*`) are documented in their own
section at the end; they are not part of the `.domain` domains.

**Like macOS, the configuration is stored in one file per domain** (extension
**`.domain`**, Malaysia's own format, as `.plist` is macOS's) in two
directories — preferences and state:

```
~/.config/malaysia/            ← ~/Library/Preferences
├── appearance.domain
├── language.domain
├── display.domain
├── window.domain
├── screenshot.domain
├── dock.domain
├── menubar.domain
├── lock.domain
├── effects.domain
└── debug.domain

~/.local/share/malaysia/       ← ~/Library/Application Support
├── wallpaper-selection.state ← current wallpaper per desktop ([desktops]), like
│                            com.apple.wallpaper
├── wallpaper.state        ← dominant wallpaper color (written by malaysia-wallpaper)
└── recents.state          ← Recent Items of the ❄ menu: `apps` (app ids) and
                             `cleared_at` (unix; «Clear Menu»), written by the menubar
```

Each file is optional; missing files use their default (like macOS's
`com.apple.dock`, `com.apple.menubar`... domains). Every reader watches the
directory with inotify and applies changes live, unless its row says otherwise.

`MALAYSIA_CONFIG_DIR` and `MALAYSIA_DATA_DIR` replace the two directories.

Environment variables that change behavior without a domain:

| Variable | Read by | Effect | Default |
|---|---|---|---|
| `MALAYSIA_SCREENSHOT_DIR` | compositor | folder for screenshots | the XDG Desktop folder |
| `MALAYSIA_RESIZE_CONFIGURE_TIMEOUT_MS` | compositor | how long an interactive resize waits for the client's configure ack | 100 |
| `MALAYSIA_SHELL_EXE` | compositor | one more `malaysia-shell` binary that Infinity trusts | none |
| `MALAYSIA_TERMINAL` | launcher | terminal for `.desktop` entries with `Terminal=true`; then `TERMINAL` | `wezterm` |
| `MALAYSIA_ICON_THEME` | launcher | icon theme searched before `hicolor` | `Adwaita` |
| `MALAYSIA_SCALE` | compositor | output scale; wins over `display.scale` | none |
| `MALAYSIA_POWER_DRY_RUN` | compositor, shell | `1`: Suspend, Restart, Shut Down and Log Out only log the logind call | none (`./dev.sh` sets `1`) |
| `XDG_DESKTOP_DIR` | compositor | Desktop folder for screenshots; wins over `user-dirs.dirs` | `user-dirs.dirs` |

`MALAYSIA_WALLPAPER_CAPTURE` points `./dev.sh wallpaper-check` at a capture to
compare with the reference. The `MALAYSIA_TEST_*` variables drive the synthetic input of the probes in
`tools/sctk-probe` and `dev.sh`; `MALAYSIA_BIN_DIR` and `MALAYSIA_ORIG_*`
are set by the session and the Nix wrappers, not by hand.

Status of each option:
- **active** — read at runtime and changes the behavior.
- **pending** — defined in the catalog; not parsed or consumed yet, and it arrives with its effect.

---

## Fixed compositor shortcuts

They are not domain options; they are fixed and documented here:

| Shortcut | Action |
|---|---|
| `Super+Space` | opens or closes `malaysia-shell launcher`; blocked while the session is locked |
| `Ctrl+Alt+Backspace` | emergency exit (blocked while the session is locked) |
| `Ctrl+Alt+L` | locks or relaunches the lock |
| `Ctrl+Alt+F1..F12` | switches VT |
| `Shift+Super+3` / `Shift+Super+4` | screen / region screenshot |
| `PrintScreen` | captures the focused window |
| `Alt+Super+Escape` | opens `malaysia-shell force-quit` («Force Quit…»); not while the session is locked |
| `Super+H` / `Alt+Super+H` | minimizes every window of the focused app / of the other apps; not while the session is locked |
| `Super+Q` | asks every window of the focused app to close (`xdg_toplevel.close`); not while the session is locked |
| `Super+M` / `Super+W` | minimizes / asks the focused window to close; not while the session is locked |
| `Ctrl+Alt+←` / `Ctrl+Alt+→` | shows the space to the left / right, without wrapping around (ADR 0051); not while the session is locked |
| `Ctrl+Alt+N` | adds a desktop at the end of the row and shows it; at most 99 (ADR 0051) |
| `Ctrl+Super+F` | enters or leaves focus mode with the focused window (ADR 0051) |
| `Ctrl+Super+←/→/↑/↓` | moves the focused window to the left, right, top or bottom half (Infinity `arrange`); not while the session is locked |
| `Shift+Ctrl+Super+←/→/↑/↓` | tiles the two front windows side by side or stacked, focused one first on the arrow's side (`arrange_pair`) |
| `Ctrl+Super+Return` / `Ctrl+Super+C` / `Ctrl+Super+R` | fills the screen / centers / restores the focused window |

---

## `appearance` — appearance (System Settings → Appearance)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `appearance.color_scheme` | `"auto"` \| `"dark"` \| `"dark_tint"` \| `"light"` | `"auto"` | **active** | portal `org.freedesktop.appearance`, shell, **System Settings** |
| `appearance.accent_color` | `[r,g,b]` in `[0,1]` or `null` | `null` | **active** | portal, shell, effects, **System Settings** |
| `appearance.contrast` | bool | `false` | **active** | portal (`org.freedesktop.appearance` `contrast`), kit (`ctappearance::resolve`; also reduces transparency), **System Settings › Accessibility › Display** |
| `appearance.reduced_motion` | bool | `false` | **active** | portal (`reduced-motion`), compositor (cross-fade, spaces, popups), kit (`bk::animation`) |
| `appearance.scroll_bars` | `"when_scrolling"` \| `"always"` | `"when_scrolling"` | **active** | the kit's `ScrollView`: the bar shows only while scrolling or always, **System Settings** |
| `appearance.reduce_transparency` | bool | `true` | **active** | shell and kit: solid backgrounds; `false` will enable the compositor blur (ADR 0038) |
| `appearance.window_color` | `"neutral"` \| `"accent"` | `"neutral"` | **active** | compositor titlebar/chrome, **System Settings**; an old `"wallpaper"` reads as `"neutral"` |
| `appearance.window_tint_scope` | `"windows"` \| `"desktop"` | `"windows"` | **active** | `desktop` also tints the menu bar and the Dock with the accent tint, **System Settings** |
| `appearance.traffic_lights` | `"auto"` \| `"standard"` \| `"accent"` \| `"wallpaper"` | `"auto"` | **active** | titlebar traffic lights, **System Settings** |
| `appearance.traffic_light_shape` | `"square"` \| `"round"` | `"square"` | **active** | traffic light shape (radius 4 or circle), **System Settings** |
| `appearance.tint_strength_accent` | `"subtle"` \| `"normal"` \| `"strong"` \| `"intense"` | `"normal"` | **active** | strength of `window_color = "accent"` (×0.5, ×1, ×1.5, ×2, `TINT_STRENGTH`), **System Settings** |
| `appearance.tint_parts` | list of `"sidebars"`, `"window"`, `"groups"`, `"controls"`, `"menus"` | all five | **active** | which surfaces the tint reaches, **System Settings** |
| `appearance.sidebar_style` | `"island"` \| `"flat"` | `"island"` | **active** | style of the kit's sidebar |
| `appearance.wallpaper_tinting` | bool | `true` | **active** | each BarrierKit window mixes the wallpaper behind it into its sidebar, in dark mode and with a neutral window color (ADR 0065), **System Settings › Appearance** |
| `appearance.wallpaper_tinting_in_light` | bool | `false` | **active** | `wallpaper_tinting` also in light mode, **System Settings › Accessibility › Display** |
| `appearance.invert_colors` | bool | `false` | **active** | compositor: inverts the screen (ADR 0066), **System Settings › Accessibility › Display** |
| `appearance.show_borders` | bool | `false` | **active** | kit: a border on icon buttons, tool groups and search fields, **System Settings › Accessibility › Display** |
| `appearance.differentiate_without_color` | bool | `false` | **active** | kit and compositor: the traffic lights keep their symbols in the focused window, **System Settings › Accessibility › Display** |
| `appearance.display_contrast` | a value of `DISPLAY_CONTRAST` | `1.0` | **active** | compositor: contrast of the whole screen around middle gray, **System Settings › Accessibility › Display** |
| `appearance.text_size` | a value of `TEXT_SIZES` | `1.0` | **active** | the preferred reading size of every toolkit without its own, **System Settings › Accessibility › Display › Text size** |
| `appearance.text_sizes` | table: `barrierkit`, `gtk`, `qt`, each a value of `TEXT_SIZES` | empty | **active** | kit (`Text`, sidebar items), portal (`text-scaling-factor` for GTK 4), compositor (`GDK_DPI_SCALE` for GTK 3) and the Qt bridge (qt6ct font); a missing key uses `text_size` (ADR 0067) |
| `appearance.menu_bar_size` | `"default"` \| `"large"` | `"default"` | **active** | shell: menu bar text size, **System Settings › Accessibility › Display** |
| `appearance.shake_to_locate` | bool | `true` | **active** | compositor: shaking the pointer enlarges it, **System Settings › Accessibility › Display** |
| `appearance.pointer_size` | a value of `POINTER_SIZES` | `1.0` | **active** | compositor: size of the cursors it draws; a cursor an app draws itself keeps the app's size (ADR 0066), **System Settings › Accessibility › Display** |
| `appearance.color_filters` | bool | `false` | **active** | compositor: applies `color_filter` to the screen, **System Settings › Accessibility › Display** |
| `appearance.color_filter` | `"grayscale"` \| `"red_green"` \| `"green_red"` \| `"blue_yellow"` | `"grayscale"` | **active** | compositor: grayscale or a color-blindness filter (ADR 0066) |
| `appearance.color_filter_intensity` | a value of `COLOR_FILTER_INTENSITY` | `1.0` | **active** | compositor: blends the filter with the original colors |
| `appearance.qt_bridge` | bool | `true` | **active** | portal: writes `qt6ct.conf` and `qt5ct.conf` in `~/.config/qt6ct` and `~/.config/qt5ct` (Fusion, the Malaysia palette in `colors/malaysia.conf`, the Qt text size); other keys stay (ADR 0067). Only the portal reads it |
| `appearance.gtk_bridge` | bool | `true` | **active** | portal: exact accent and tint in GTK 3/4 and libadwaita (`~/.config/gtk-{3,4}.0`, ADR 0050) and live through `org.malaysia.appearance` (ADR 0055). Only the portal reads it |

macOS: "Appearance: **Light / Dark / Auto**" and "**Accent color**"; "Increase
contrast" and "Reduce motion" live in Accessibility and are published in the same
portal namespace.

`color_scheme` reaches every app: bk reads it from the domain; GTK 4,
libadwaita, Qt with the portal theme, Firefox, Chromium and Electron read it from
the portal (`org.freedesktop.appearance`, and `org.gnome.desktop.interface`
`color-scheme` for those that only read the GNOME key); GTK 3 reads it from
`gtk-3.0/settings.ini`, which the GTK bridge writes (ADR 0050). With
`gtk_bridge = true` the portal also writes the exact accent and, if
`window_color` tints, the tinted Adwaita backgrounds; GTK applies them when the
app opens. Malaysia's patched libadwaita also receives them live through the
portal's `org.malaysia.appearance` namespace (ADR 0055); with
`gtk_bridge = true` those colors win over a `gtk.css` rule that
redefines them.

The **System Settings** app reads and writes `appearance.domain` through
`ctappearance::write_appearance`. The write is atomic (temporary file +
`rename`) and keeps unknown keys and the user's comments, so
editing the file by hand and using the app are compatible.

`sidebar_style` selects the kit's sidebar: `"island"` (floating, with
vibrancy, border and shadow) or `"flat"` (flat list blended into the window
background). With `reduce_transparency = true` the island stays opaque, but keeps
its radius, outline and shadow.

`window_color` is Tengen's idea "Settings › Appearance › Window
color": `neutral` does not tint and `accent` mixes the accent with the titlebar
background (6 % in light, 8 % in dark, at `normal` strength). Traffic lights in
`auto` follow the mode (standard in neutral, derived from the accent in accent).

`wallpaper_tinting` is macOS's "Tint window background with wallpaper color".
The compositor sends each BarrierKit window the average color of the wallpaper
behind it, and the window mixes it into its sidebar (9 % in dark, 7 % in
light with `wallpaper_tinting_in_light`; provisional values, ADR 0065). It
needs a neutral `window_color` and an accent other than graphite.

## `language` — language (System Settings → General → Language & Region)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `language.language` | `"es"` \| `"en"` | absent | **active** | kit apps (`ctappearance::language`), **System Settings**, shell (its text and the localised `.desktop` names) |

When absent or empty, the language comes from `LANGUAGE`, `LC_ALL`,
`LC_MESSAGES` and `LANG`, in gettext order.

The "Language" row of **System Settings** (Appearance) writes it. Each app
reads it at startup; the change shows when the app is reopened.

The `language` key in a kit app's own domain (`<name>.domain`)
wins over this domain. The "Language" row of its preferences writes it. The
full order is in barrierkit's
[`17-languages.md`](https://github.com/HBlanqueto/barrierkit/blob/main/docs/barrierkit/17-languages.md).

The compositor and the shell pass the language in `LANGUAGE` to the apps they launch, so GTK and Qt follow it too (gettext ignores it with the `C` locale).

## `display` — output scale (System Settings → Displays)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `display.scale` | float `1.0..=4.0` | `1.0` | **active**, read when the compositor starts | `State::scale`, `wl_output`, `wp_fractional_scale_v1` |

It has no exact macOS equivalent: macOS picks a **display mode** per
monitor, not a fractional scale. Malaysia follows the Wayland model and publishes the
integer part through `wl_output` and the fractional part through `wp_fractional_scale_v1`.
`MALAYSIA_SCALE` still exists as a **development override** and wins when it is
set; it serves `dev.sh` without writing configuration. With 8b this option
becomes per output.

## `window` — window chrome and behavior (System Settings → Windows & Apps)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `window.decorations` | `"default"` \| `"force_server"` \| `"force_client"` | `"default"` | **active** | decoration handlers (ADR 0019/0020) |
| `window.titlebar` | bool | `true` | pending | decoration renderer |
| `window.double_click_titlebar` | `"fill"` \| `"minimize"` \| `"zoom"` \| `"none"` | `"zoom"` | **active** | input |
| `window.green_button` | `"focus"` \| `"zoom"` \| `"fullscreen"` \| `"none"` | `"focus"` | **active** | SSD and CSD titlebar |
| `window.focus` | `"click"` \| `"sloppy"` | `"click"` | pending | input |
| `window.resize_border` | integer (logical px) | `10` | **active** | width of the outer resize band on the sides |
| `window.resize_corner` | integer (logical px) | `20` | **active** | length of the outer resize «L» at the corners |
| `window.corner_radius` | integer (logical pt), optional | unset | **active** | radius of every window; unset, its bar's (ADR 0059) |
| `window.qt_server_decorations` | bool | `true` | **active** | `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` in clients and session |
| `window.focus_titlebar_apps` | list of `app_id` | `["firefox", "brave-browser", "chromium-browser"]` | **active** | focus mode of CSD apps with tabs |

`window.resize_border` is a band **outside** the side; the «L» of
`window.resize_corner` is outside and may reach 2 px in. The hit-test is only
offered on normal SSD windows (not maximized or fullscreen).

`window.corner_radius` sets one radius for every window, whatever the app's
`set_bar` or its decoration. It matches macOS `NSConvolutionOverride1`.
Without the key, each window gets its bar's radius: 16, 20, 26 or 15
(ADR 0059). Fullscreen still has no corners.

`"default"` respects the negotiation (GTK/Libadwaita keep their headerbar; Qt and the
rest get Malaysia's chrome). `"force_server"` draws Malaysia's chrome on every
window; `"force_client"` draws none.

`double_click_titlebar` uses macOS's words: "**Fill / Minimize / Zoom / No
Action**". `fill` and `zoom` maximize and never enter focus mode; in focus mode,
they leave it. This holds for every window: SSD, GTK (`gtk_surface.titlebar_gesture`)
and the other CSD apps, whose `set_maximized` right after a double click counts as
a double click.

`green_button` decides what the green traffic light does. As in macOS, the default
value is **`focus`**: the focus mode of ADR 0051. The window gets its own
space to the right of its desktop and grows to cover the screen
while the desktop slides away. It gets the xdg `fullscreen` state; the bar
and the dock hide. Repeating leaves focus. With `focus`, a CSD app's own green
traffic light (`set_maximized`) does the same. A fixed-width window is
maximized. `fullscreen` is an alias of `focus`. `zoom` maximizes
keeping the frame; `none` does nothing. An app that requests fullscreen (F11) and `Ctrl+Super+F` always enter
focus mode. Double-clicking the bar is still governed by `double_click_titlebar`.

`focus_titlebar_apps` lists the CSD apps that focus mode treats like an SSD
window: they get `maximized` instead of `fullscreen`, so they keep their tabs
and address bar, and when the menu bar comes down the compositor draws its
32 px titlebar over them. A browser that gets `fullscreen` enters its F11 mode
and hides its tabs. If the app asks for fullscreen itself (F11, a video), it
gets `fullscreen` as always.

`qt_server_decorations` exports `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` for the
clients that `malaysia_compositor::spawn` launches and in the session environment, so that Qt
uses the `xdg-decoration` protocol and gets Malaysia's frame. Setting it to `false`
lets Qt draw its own bar (as Telegram does; see
[`../architecture/windows.md`](../architecture/windows.md)).

## `screenshot` — screenshots (Screenshot app; macOS `com.apple.screencapture`)

Flow, shortcuts and overlay: [`../architecture/captures.md`](../architecture/captures.md).

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `screenshot.show_pointer` | bool | `false` | **active** | screen/region capture (window captures never include it) |
| `screenshot.show_thumbnail` | bool | `true` | pending | floating thumbnail |
| `screenshot.target` | `"desktop"` \| `"clipboard"` \| `"preview"` \| `"mail"` \| `"messages"` \| `"other"` | `"desktop"` | **active** (`desktop`/`clipboard`); the rest fall back to the Desktop | target: file or `image/png` on the clipboard |
| `screenshot.location` | string (path) or absent | — | pending | custom folder for `target = "other"` |
| `screenshot.type` | `"png"` \| `"jpg"` \| `"heif"` \| `"tiff"` \| `"pdf"` | `"png"` | pending | format (always PNG today) |
| `screenshot.include_date` | bool | `true` | pending | date and time in the name |
| `screenshot.disable_shadow` | bool | `false` | pending | window shadow in the capture |
| `screenshot.timer` | integer (seconds) | `0` | pending | timer (0 = none, 5, 10) |
| `screenshot.remember_selection` | bool | `false` | pending | remember the last region/window |

The defaults are macOS's: pointer **hidden**, save to the **Desktop** as
**PNG** with date, floating thumbnail **yes** and window shadow **yes**. The names
follow the Screenshot app ("Show Mouse Pointer", "Show Floating Thumbnail", "Save to",
"Timer", "Remember Last Selection") and `com.apple.screencapture` (`type`,
`location`, `include-date`, `disable-shadow`).

Status: `show_pointer` and `target = "clipboard"` already apply. The rest is not
parsed yet; the current flow saves to the Desktop, as PNG and with
date; `preview`/`mail`/`messages`/`other`, thumbnail, timer, formats and
shadow get wired in the next passes. Window captures never
include the pointer. The clipboard is `image/png` and the compositor serves it; context in
[`../research.md`](../research.md).

## `wallpaper` — system wallpapers (System Settings → Wallpaper)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `wallpaper.dir` | string (path) | `/run/current-system/sw/share/wallpapers/solid-colors` | pending | `malaysia-wallpaper`, settings app |
| `wallpaper.scale` | `"fill"` \| `"stretch"` \| `"fit"` \| `"center"` | `"fill"` | pending | `malaysia-wallpaper` |
| `wallpaper.shuffle` | bool | `false` | pending | `malaysia-wallpaper` |

**Malaysia's wallpapers live in the project itself** (`share/wallpapers/solid-colors/`,
the macOS "Solid Colors" set, vendored in the repo) and the flake installs them as system
data in `/run/current-system/sw/share/wallpapers/`, just as GNOME installs
its own in `/usr/share/backgrounds`. `wallpaper.dir` points there by default; a
development setup can point it at the repo or another folder. The
**`malaysia-wallpaper`** component draws the wallpaper on the `Background` layer with the
images from that folder (ADR 0049; **Vulkan**/wgpu render presented through dmabuf).

**The default wallpaper is `malaysia-dream.heic`**: a square 6016×6016
lossless HEIC of a single salmon colour (`#e16e79`), embedded in the
`malaysia-wallpaper` binary. A HEIC can hold several images; future colours go
in that same file. `malaysia-wallpaper` decodes PNG, JPEG and HEIF (libheif).
It is the one shown when the store picks none, does not exist yet, or the
selection points to a file that does not exist or does not decode. If `wallpaper.dir`
points to another folder and there is no selection, `malaysia-dream.heic` still shows.

**Like macOS, the current selection lives in a separate store**:
`~/.local/share/malaysia/wallpaper-selection.state` (the equivalent of `~/Library/Application
Support`, where macOS keeps `com.apple.wallpaper`) — per desktop, with a `file://`
to the image:

```toml
# ~/.local/share/malaysia/wallpaper-selection.state  (current selection, per desktop)
[desktops]
default = "file:///home/.../Solid Colors/Solid Mint.png"   # optional; without this line, malaysia-dream.heic
```

Editing that file changes the wallpaper **live** (`malaysia-wallpaper` watches the
store and repaints). The preferences above (`dir`, `scale`, `shuffle`) go in
`~/.config/malaysia/wallpaper.domain` once they are read; the catalog is in the
system (`share/wallpapers/`). On a wallpaper change, the client writes the dominant
color to `wallpaper.state` (`dominant = [r, g, b]`) for the window tint
and the portal's `accent-color`.

## `dock` — shell dock (`malaysia-shell dock`; macOS `com.apple.dock`)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `dock.autohide` | bool | `false` | pending | shell (dock) |
| `dock.orientation` | `"bottom"` \| `"left"` \| `"right"` | `"bottom"` | pending | shell |
| `dock.magnification` | bool | `false` | pending | shell |
| `dock.tilesize` | integer (px) | `48` | pending | shell |
| `dock.largesize` | integer (px) | `64` | pending | shell |
| `dock.mineffect` | `"genie"` \| `"scale"` \| `"suck"` | `"genie"` | pending | shell |
| `dock.show-process-indicators` | bool | `true` | pending | shell |
| `dock.static-only` | bool | `false` | pending | shell |
| `dock.minimize-to-application` | bool | `false` | pending | shell |
| `dock.show-recents` | bool | `true` | pending | shell |

`mineffect` uses macOS's names: "**Minimize windows using: Genie / Scale**".
The hyphenated keys are **exactly** those of `defaults write com.apple.dock ...`.

## `lock` — session lock

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `lock.idle_timeout_seconds` | integer (seconds) | `0` | **active**, read when the compositor starts | `ext-idle-notify-v1` / auto-lock; also locks before suspend |

`0` disables automatic locking. When the timeout expires without input, the
compositor launches `malaysia-shell lock`. A `zwp_idle_inhibit_manager_v1` inhibitor
(fullscreen video) stops the clock.

## `menubar` — shell menu bar (macOS "Menu Bar")

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `menubar.autohide` | bool | `false` | pending | shell |
| `menubar.app_menus` | bool | `true` | **active** | shell (dbusmenu and `org.gtk.Menus`) and compositor (`gtk_shell1` `capabilities` 3/0, ADR 0045) |

## `effects` — compositor surface effects (ADR 0020)

They apply to **every** window, client- or server-decorated.

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `effects.corner_radius` | float (px) | `0.0` | pending | surface render |
| `effects.inner_border.enabled` | bool | `false` | pending | decoration render |
| `effects.inner_border.width` | float (px) | `1.0` | pending | same |
| `effects.inner_border.color` | `[r,g,b]` | `[0.5,0.5,0.5]` | pending | same |
| `effects.shadow.enabled` | bool | `false` | pending | shadow render |
| `effects.shadow.radius` | float (px) | `24.0` | pending | same |
| `effects.shadow.offset` | `[x,y]` (px) | `[0,6]` | pending | same |
| `effects.shadow.opacity` | float `[0,1]` | `0.35` | pending | same |
| `effects.blur.enabled` | bool | `false` | pending | blur render |
| `effects.blur.radius` | float (px) | `20.0` | pending | same |

Once a **transparent theme** exists, the app paints with alpha and `effects.blur.*`
makes the compositor blur what is behind (macOS-style vibrancy).

## `debug` — development

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `debug.damage_highlight` | bool | `false` | pending | render (highlight damage per frame) |
| `debug.overlay_planes` | bool | `false` | active | DRM: overlay planes and their dmabuf feedback tranche |

`debug.overlay_planes` is read when the DRM session starts; changing it requires
a restart. It is off, like niri's `debug.enable-overlay-planes`, until
it is accepted on the real panel with a video playing in a window. On Intel i915 it must stay
off: see S-02 in [`debt.md`](../debt.md).

`MALAYSIA_POWER_DRY_RUN=1` (environment variable, not a domain one): Suspend,
Restart, Shut Down and Log Out only write to the log the logind call they
would make. `./dev.sh` sets it by default (`0` disables it) and the nested
compositor forces it for everything it launches.

---

## The `domain` command

**Pending (ADR 0021): no code yet.** The planned control command, with the same
structure as macOS's `defaults`. Today System Settings writes `appearance.domain`
directly and every reader picks the change up through inotify. The domain of each option is its section:
`appearance`, `display`, `window`, `screenshot`, `wallpaper`, `dock`,
`menubar`, `lock`, `effects`, `debug`.

```
domain read   <domain> <key>
domain write  <domain> <key> <valor> [-bool | -int | -float | -string]
domain delete <domain> <key>
domain rename <domain> <old> <new>
domain find   <palabra>
domain domains
```

Examples (same syntax as `defaults`):

```
domain write effects.blur.enabled -bool true        # or "read effects.blur.enabled"
domain write appearance.color_scheme -string dark
domain write dock.autohide -bool true
domain write window.decorations -string force_server
domain read  effects.shadow.radius
domain delete effects.blur.enabled                  # back to the default
domain find  blur
```

## Example files (`.domain` extension, one file per domain)

Like macOS, one file per domain in `~/.config/malaysia/`:

```toml
# ~/.config/malaysia/appearance.domain
color_scheme = "dark"           # auto | dark | dark_tint | light
accent_color = [0.208, 0.518, 0.894]
contrast = false                # Increase contrast
reduced_motion = false          # Reduce motion
reduce_transparency = true      # Reduce transparency (solid backgrounds; false = future blur)
scroll_bars = "when_scrolling"  # when_scrolling | always
window_color = "neutral"        # neutral | accent
window_tint_scope = "windows"   # windows | desktop (tints menubar/dock)
tint_strength_accent = "normal"     # subtle | normal | strong | intense
tint_parts = ["sidebars", "window", "groups", "controls", "menus"]
traffic_lights = "auto"         # auto | standard | accent | wallpaper
traffic_light_shape = "square"  # square | round
wallpaper_tinting = true        # tint windows with the wallpaper behind them
wallpaper_tinting_in_light = false
invert_colors = false
show_borders = false
differentiate_without_color = false
display_contrast = 1.0
text_size = 1.0
menu_bar_size = "default"
shake_to_locate = true
pointer_size = 1.0
color_filters = false
color_filter = "grayscale"
color_filter_intensity = 1.0

[text_sizes]                    # optional; a missing key uses text_size
qt = 1.3                        # barrierkit | gtk | qt
```

```toml
# ~/.config/malaysia/language.domain
language = "es"                 # es | en; without the key, follows LANG
```

```toml
# ~/.config/malaysia/display.domain
scale = 1.5                     # 1.0 .. 4.0
```

```toml
# ~/.config/malaysia/window.domain
decorations = "default"         # default | force_server | force_client
titlebar = true
double_click_titlebar = "zoom"  # fill | minimize | zoom | none
green_button = "focus"          # focus | zoom | none
focus = "click"                 # click | sloppy
resize_border = 10
resize_corner = 20
# corner_radius = 16            # without the key: radius by bar
qt_server_decorations = true    # false for Qt's native bar
focus_titlebar_apps = ["firefox", "brave-browser", "chromium-browser"]
```

```toml
# ~/.config/malaysia/screenshot.domain
show_pointer = false            # macOS hides it by default
show_thumbnail = true
target = "desktop"              # desktop | clipboard | preview | mail | messages | other
# location = "/home/yo/Capturas"  # for target = "other"
type = "png"                    # png | jpg | heif | tiff | pdf
include_date = true
disable_shadow = false
timer = 0                       # 0, 5 or 10
remember_selection = false
```

```toml
# ~/.config/malaysia/lock.domain
idle_timeout_seconds = 300      # 0 disables auto-lock
```

```toml
# ~/.config/malaysia/dock.domain
autohide = false
orientation = "bottom"          # bottom | left | right
magnification = false
tilesize = 48
largesize = 64
mineffect = "genie"             # genie | scale | suck
show-process-indicators = true
static-only = false
minimize-to-application = false
show-recents = true
```

```toml
# ~/.config/malaysia/wallpaper.domain  (pending: not read yet)
dir = "/run/current-system/sw/share/wallpapers/solid-colors"
scale = "fill"                  # fill | stretch | fit | center (macOS words)
shuffle = false
```

```toml
# ~/.local/share/malaysia/wallpaper-selection.state  (current selection per desktop, like com.apple.wallpaper)
[desktops]
default = "file:///run/current-system/sw/share/wallpapers/solid-colors/Solid Mint.png"
```

```toml
# ~/.config/malaysia/menubar.domain
autohide = false
app_menus = true                 # global menu of the focused app (dbusmenu)
```

```toml
# ~/.config/malaysia/effects.domain
corner_radius = 12.0
[inner_border]
enabled = true
width = 1.0
color = [1.0, 1.0, 1.0]
[shadow]
enabled = true
radius = 24.0
offset = [0.0, 6.0]
opacity = 0.35
[blur]
enabled = false
radius = 20.0
```

```toml
# ~/.config/malaysia/debug.domain
damage_highlight = false
overlay_planes = false
```

> Note: inside each domain file the sections are written **without the
> domain prefix** (`[inner_border]` in `effects.domain`, not
> `[effects.inner_border]`); the file *is* the domain.

The catalog lives in `crates/malaysia-compositor/src/config.rs` as typed `serde` types; any
change in this document must be reflected there and vice versa (ADR 0021).

---

## NixOS options (`programs.malaysia`)

These options configure the system integration; they go in
`configuration.nix`, not in a `.domain` domain.

| Option | Type | Default | Description |
|---|---|---|---|
| `programs.malaysia.enable` | bool | `false` | Enables Malaysia: installs the package, registers the Wayland session, PAM, units and the portal. |
| `programs.malaysia.globalMenu.libadwaita.enable` | bool | `true` | Replaces the system libadwaita with a patched version that exports the primary menu as a global menubar (`org.gtk.Menus`). It uses `system.replaceDependencies`, so apps are not rebuilt (ADR 0052). |
| `programs.malaysia.globalMenu.electron.enable` | bool | `false` | Replaces `electron_43` with a build carrying electron/electron#54629, which publishes Electron apps' menus on Wayland through `org_kde_kwin_appmenu`. It uses `system.replaceDependencies`; Electron builds from source (hours, no cache). |
| `programs.malaysia.globalMenu.gtk3.enable` | bool | `true` | Installs `appmenu-gtk-module` and loads it with `GTK_MODULES`, so GTK 3 apps with an in-window `GtkMenuBar` publish their menu through `gtk_shell1`/`org.gtk.Menus`. |

`programs.malaysia.globalMenu.libadwaita.enable` can be disabled to avoid
the patched fork; libadwaita apps keep working, but they do not
publish their ☰ menu in Malaysia's menu bar.
