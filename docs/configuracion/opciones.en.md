# Malaysia options catalog

Canonical catalog of the compositor options (ADR 0021) and of its NixOS
module. **Domain names and values are identical to macOS's**
whenever an equivalent exists: the sections mirror System Settings
(`appearance`, `window`, `dock`, `menubar`) and the keys mirror `com.apple.dock`
and the rest (`autohide`, `orientation`, `tilesize`, `mineffect`,
`show-process-indicators`, ...).

Full visual map: [`../arquitectura/interfaz.md`](../arquitectura/interfaz.md).

The NixOS options (`programs.malaysia.*`) are documented in their own
section at the end; they are not part of the `.domain` domains.

**Like macOS, the configuration is stored in one file per domain** (extension
**`.domain`**, Malaysia's own format, as `.plist` is macOS's) in two
directories — preferences and state:

```
~/.config/malaysia/            ← ~/Library/Preferences
├── appearance.domain
├── display.domain
├── window.domain
├── screenshot.domain
├── dock.domain
├── menubar.domain
├── lock.domain
├── effects.domain
└── debug.domain

~/.local/share/malaysia/       ← ~/Library/Application Support
├── wallpaper.domain       ← catalog + current selection ([desktops]), like
│                            com.apple.wallpaper
├── wallpaper.state        ← dominant wallpaper color (written by malaysia-wallpaper)
├── recents.domain         ← Recent Items of the ❄ menu: `apps` (app ids) and
│                            `cleared_at` (unix; «Clear Menu»), written by the menubar
└── window.domain          ← state: the active app, published by the compositor
```

Each file is optional; missing files use their default (like macOS's
`com.apple.dock`, `com.apple.menubar`... domains). The same catalog feeds the
`domain` command and the future settings app (both over D-Bus, interface
`org.malaysia.Compositor1`).

Status of each option:
- **active** — read at runtime and changes the behavior.
- **data** — lives in `Config`, is documented and validated, but nothing consumes it yet.
- **pending** — defined in the catalog; it is implemented together with its effect.

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
| `Ctrl+Alt+←` / `Ctrl+Alt+→` | shows the space to the left / right, without wrapping around (ADR 0051); not while the session is locked |
| `Ctrl+Alt+N` | adds a desktop at the end of the row and shows it; at most 99 (ADR 0051) |
| `Ctrl+Super+F` | enters or leaves focus mode with the focused window (ADR 0051) |

---

## `appearance` — appearance (System Settings → Appearance)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `appearance.color_scheme` | `"auto"` \| `"dark"` \| `"dark_tint"` \| `"light"` | `"auto"` | **active** | portal `org.freedesktop.appearance`, shell, **System Settings** |
| `appearance.accent_color` | `[r,g,b]` in `[0,1]` or `null` | `null` | **active** | portal, shell, effects, **System Settings** |
| `appearance.contrast` | bool | `false` | **active** | portal (`org.freedesktop.appearance` `contrast`) |
| `appearance.reduced_motion` | bool | `false` | **active** | portal (`reduced-motion`) |
| `appearance.scroll_bars` | `"when_scrolling"` \| `"always"` | `"when_scrolling"` | **active** | the kit's `ScrollView`: the bar shows only while scrolling or always, **System Settings** |
| `appearance.reduce_transparency` | bool | `true` | **active** | shell and kit: solid backgrounds; `false` will enable the compositor blur (ADR 0038) |
| `appearance.window_color` | `"neutral"` \| `"accent"` \| `"wallpaper"` | `"neutral"` | **active** | compositor titlebar/chrome, **System Settings** |
| `appearance.window_tint_scope` | `"windows"` \| `"desktop"` | `"windows"` | **active** | `desktop` also tints menubar/dock (20 % / 26 %), **System Settings** |
| `appearance.traffic_lights` | `"auto"` \| `"standard"` \| `"accent"` \| `"wallpaper"` | `"auto"` | **active** | titlebar traffic lights, **System Settings** |
| `appearance.sidebar_style` | `"island"` \| `"flat"` | `"island"` | **active** | style of the kit's sidebar |
| `appearance.gtk_bridge` | bool | `true` | **active** | portal: exact accent and tint in GTK 3/4 and libadwaita (`~/.config/gtk-{3,4}.0`, ADR 0050) and live through `org.malaysia.appearance` (ADR 0055). Only the portal reads it |

macOS: "Appearance: **Light / Dark / Auto**" and "**Accent color**"; "Increase
contrast" and "Reduce motion" live in Accessibility and are published in the same
portal namespace.

`color_scheme` reaches every app: barrier-kit reads it from the domain; GTK 4,
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
`vow-appearance::write_appearance`. The write is atomic (temporary file +
`rename`) and keeps unknown keys and the user's comments, so
editing the file by hand and using the app are compatible.

`sidebar_style` selects the kit's sidebar: `"island"` (floating, with
vibrancy, border and shadow) or `"flat"` (flat list blended into the window
background). With `reduce_transparency = true` the island stays opaque, but keeps
its radius, outline and shadow.

`window_color` is Tengen's idea "Settings › Appearance › Window
color": `neutral` does not tint, `accent` mixes the accent with the titlebar
background (6 % in light, 8 % in dark) and `wallpaper` uses the dominant color of the
wallpaper (7 % / 9 %). Traffic lights in `auto` follow the mode (standard in neutral,
derived from the accent/wallpaper in the others).

## `display` — output scale (System Settings → Displays)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `display.scale` | float `1.0..=4.0` | `1.0` | **active** | `State::scale`, `wl_output`, `wp_fractional_scale_v1` |

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
| `window.titlebar` | bool | `true` | data | decoration renderer |
| `window.double_click_titlebar` | `"fill"` \| `"minimize"` \| `"zoom"` \| `"none"` | `"zoom"` | **active** | input |
| `window.green_button` | `"focus"` \| `"zoom"` \| `"fullscreen"` \| `"none"` | `"focus"` | **active** | SSD and CSD titlebar |
| `window.focus` | `"click"` \| `"sloppy"` | `"click"` | data | input |
| `window.resize_border` | integer (logical px) | `10` | **active** | width of the outer resize band on the sides |
| `window.resize_corner` | integer (logical px) | `20` | **active** | length of the outer resize «L» at the corners |
| `window.qt_server_decorations` | bool | `true` | **active** | `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` in clients and session |

`window.resize_border` is a band **outside** the side; the «L» of
`window.resize_corner` is outside and may reach 2 px in. The hit-test is only
offered on normal SSD windows (not maximized or fullscreen).

`"default"` respects the negotiation (GTK/Libadwaita keep their headerbar; Qt and the
rest get Malaysia's chrome). `"force_server"` draws Malaysia's chrome on every
window; `"force_client"` draws none.

`double_click_titlebar` uses macOS's words: "**Fill / Minimize / Zoom / No
Action**".

`green_button` decides what the green traffic light does. As in macOS, the default
value is **`focus`**: the focus mode of ADR 0051. The window gets its own
space to the right of its desktop and grows to cover the screen
while the desktop slides away. It gets the xdg `fullscreen` state; the bar
and the dock hide. Repeating leaves focus. With `focus`, a CSD app's own green
traffic light (`set_maximized`) does the same. `zoom` maximizes
keeping the frame; `fullscreen` still only records the request; `none`
does nothing. An app that requests fullscreen (F11) and `Ctrl+Super+F` always enter
focus mode. Double-clicking the bar is still governed by `double_click_titlebar`.

`qt_server_decorations` exports `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` for the
clients that `malaysia_compositor::spawn` launches and in the session environment, so that Qt
uses the `xdg-decoration` protocol and gets Malaysia's frame. Setting it to `false`
lets Qt draw its own bar (as Telegram does; see
[`../arquitectura/ventanas.md`](../arquitectura/ventanas.md)).

## `screenshot` — screenshots (Screenshot app; macOS `com.apple.screencapture`)

Flow, shortcuts and overlay: [`../arquitectura/capturas.md`](../arquitectura/capturas.md).

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `screenshot.show_pointer` | bool | `false` | **active** | screen/region capture (window captures never include it) |
| `screenshot.show_thumbnail` | bool | `true` | pending | floating thumbnail |
| `screenshot.target` | `"desktop"` \| `"clipboard"` \| `"preview"` \| `"mail"` \| `"messages"` \| `"other"` | `"desktop"` | **active** (`desktop`/`clipboard`); the rest fall back to the Desktop | target: file or `image/png` on the clipboard |
| `screenshot.location` | string (path) or absent | — | data | custom folder for `target = "other"` |
| `screenshot.type` | `"png"` \| `"jpg"` \| `"heif"` \| `"tiff"` \| `"pdf"` | `"png"` | data | format (always PNG today) |
| `screenshot.include_date` | bool | `true` | data | date and time in the name |
| `screenshot.disable_shadow` | bool | `false` | data | window shadow in the capture |
| `screenshot.timer` | integer (seconds) | `0` | data | timer (0 = none, 5, 10) |
| `screenshot.remember_selection` | bool | `false` | data | remember the last region/window |

The defaults are macOS's: pointer **hidden**, save to the **Desktop** as
**PNG** with date, floating thumbnail **yes** and window shadow **yes**. The names
follow the Screenshot app ("Show Mouse Pointer", "Show Floating Thumbnail", "Save to",
"Timer", "Remember Last Selection") and `com.apple.screencapture` (`type`,
`location`, `include-date`, `disable-shadow`).

Status: `show_pointer` and `target = "clipboard"` already apply. The rest is read
and validated, but the current flow still saves to the Desktop, as PNG and with
date; `preview`/`mail`/`messages`/`other`, thumbnail, timer, formats and
shadow get wired in the next passes. Window captures never
include the pointer. The clipboard is `image/png` and the compositor serves it; context in
[`../investigacion.md`](../investigacion.md).

## `wallpaper` — system wallpapers (System Settings → Wallpaper)

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `wallpaper.dir` | string (path) | `/run/current-system/sw/share/wallpapers/solid-colors` | data | `malaysia-wallpaper`, settings app |
| `wallpaper.scale` | `"fill"` \| `"stretch"` \| `"fit"` \| `"center"` | `"fill"` | data | `malaysia-wallpaper` |
| `wallpaper.shuffle` | bool | `false` | data | `malaysia-wallpaper` |

**Malaysia's wallpapers live in the project itself** (`share/wallpapers/solid-colors/`,
the macOS "Solid Colors" set, vendored in the repo) and the flake installs them as system
data in `/run/current-system/sw/share/wallpapers/`, just as GNOME installs
its own in `/usr/share/backgrounds`. `wallpaper.dir` points there by default; a
development setup can point it at the repo or another folder. The
**`malaysia-wallpaper`** component draws the wallpaper on the `Background` layer with the
images from that folder (ADR 0049; **Vulkan**/wgpu render presented through dmabuf).

**The default wallpaper is `bandura.jpg`**: a 1280×959 sRGB JPEG
embedded in the `malaysia-wallpaper` binary.
It is the one shown when the store picks none, does not exist yet, or the
selection points to a file that does not exist or does not decode. If `wallpaper.dir`
points to another folder and there is no selection, `bandura.jpg` still shows.

**Like macOS, the current selection lives in a separate store**:
`~/.local/share/malaysia/wallpaper.domain` (the equivalent of `~/Library/Application
Support`, where macOS keeps `com.apple.wallpaper`) — per desktop, with a `file://`
to the image:

```toml
# ~/.local/share/malaysia/wallpaper.domain  (current selection, per desktop)
[desktops]
default = "file:///home/.../Solid Colors/Solid Mint.png"   # optional; without this line, bandura.jpg
```

Editing that file changes the wallpaper **live** (`malaysia-wallpaper` watches the
store and repaints). The catalog (which wallpapers exist) and the current selection (what
is shown) are together in the `wallpaper.domain` domain; the catalog is in the
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
| `lock.idle_timeout_seconds` | integer (seconds) | `0` | **active** | `ext-idle-notify-v1` / auto-lock; also locks before suspend |

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
| `effects.corner_radius` | float (px) | `0.0` | data | surface render |
| `effects.inner_border.enabled` | bool | `false` | data | decoration render |
| `effects.inner_border.width` | float (px) | `1.0` | data | same |
| `effects.inner_border.color` | `[r,g,b]` | `[0.5,0.5,0.5]` | data | same |
| `effects.shadow.enabled` | bool | `false` | data | shadow render |
| `effects.shadow.radius` | float (px) | `24.0` | data | same |
| `effects.shadow.offset` | `[x,y]` (px) | `[0,6]` | data | same |
| `effects.shadow.opacity` | float `[0,1]` | `0.35` | data | same |
| `effects.blur.enabled` | bool | `false` | data | blur render |
| `effects.blur.radius` | float (px) | `20.0` | data | same |

Once a **transparent theme** exists, the app paints with alpha and `effects.blur.*`
makes the compositor blur what is behind (macOS-style vibrancy).

## `debug` — development

| Option | Type | Default | Status | Consumed by |
|---|---|---|---|---|
| `debug.damage_highlight` | bool | `false` | data | render (highlight damage per frame) |
| `debug.overlay_planes` | bool | `false` | active | DRM: overlay planes and their dmabuf feedback tranche |

`debug.overlay_planes` is read when the DRM session starts; changing it requires
a restart. It is off, like niri's `debug.enable-overlay-planes`, until
it is accepted with `./dev.sh video` on the real panel. On Intel i915 it must stay
off: see S-02 in [`deuda.en.md`](../deuda.en.md).

`MALAYSIA_POWER_DRY_RUN=1` (environment variable, not a domain one): Suspend,
Restart, Shut Down and Log Out only write to the log the logind call they
would make. `./dev.sh` sets it by default (`0` disables it) and the nested
compositor forces it for everything it launches.

---

## The `domain` command

The compositor's control command, with the same structure as macOS's
`defaults`. It talks to the compositor over D-Bus (`org.malaysia.Compositor1`); the settings
app and the shell use the same channel. The domain of each option is its section:
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
window_color = "neutral"        # neutral | accent | wallpaper
window_tint_scope = "windows"   # windows | desktop (tints menubar/dock)
traffic_lights = "auto"         # auto | standard | accent | wallpaper
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
green_button = "focus"          # focus | zoom | fullscreen | none
focus = "click"                 # click | sloppy
resize_border = 10
resize_corner = 20
qt_server_decorations = true    # false for Qt's native bar
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
# ~/.local/share/malaysia/wallpaper.domain
dir = "/run/current-system/sw/share/wallpapers/solid-colors"
scale = "fill"                  # fill | stretch | fit | center (macOS words)
shuffle = false

# Current selection per desktop (like com.apple.wallpaper):
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

`programs.malaysia.globalMenu.libadwaita.enable` can be disabled to avoid
the patched fork; libadwaita apps keep working, but they do not
publish their ☰ menu in Malaysia's menu bar.
