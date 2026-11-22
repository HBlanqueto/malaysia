# Architecture

How the processes of Malaysia fit together: what each binary does, who draws
and who composes, and how they talk. Read it before you change a protocol, a
`.domain` file or the split between the compositor and a client. Every option
lives in [`configuration/options.md`](configuration/options.md).

## Processes

Each component is its own process and its own Wayland client. None depends on
another and there is no central daemon: a shell component, the wallpaper or the
portal can crash without bringing down the session.

| Binary | Crate | Role |
|---|---|---|
| `malaysia-compositor` | `crates/malaysia-compositor` | Wayland compositor on Smithay. `--drm` runs the direct session; `--winit` runs it nested in another session. |
| `malaysia-shell <component>` | `crates/malaysia-shell` | The shell: one binary, one process per component. |
| `malaysia-wallpaper` | `crates/malaysia-wallpaper` | Draws the background with wgpu on Vulkan, without BarrierKit. |
| `malaysia-settings` | `crates/malaysia-settings` | System Settings, a BarrierKit app. |
| `xdg-desktop-portal-malaysia` | `crates/xdg-desktop-portal-malaysia` | Portal backend: Settings, Notification and the GTK and Qt bridges. |

The library crates are `malaysia-scene` (window and space layout in logical
coordinates), `grade-graphics` (the compositor's shaders) and
`malaysia-services` (NetworkManager, BlueZ, UPower and power-profiles-daemon
for Settings and the menu bar).

### Shell components

| Component | Surface | Role |
|---|---|---|
| `menubar` | layer `Top`, top edge | Active app, global menu, status items, system menus and the clock. |
| `dock` | layer `Top`, bottom or side edge | Apps, open windows and minimized windows. |
| `notifications` | layer `Overlay` | Notification server: banners and Notification Center. |
| `lock` | `ext-session-lock-v1` surfaces | Lock screen and PAM authentication. |
| `launcher` | layer `Overlay` | App search. |
| `force-quit`, `power`, `empty-trash` | layer `Overlay` | Alerts. |
| `about`, `about-app` | window | About this computer and about an app. |

Under NixOS, `malaysia-session.target` starts one
`malaysia-shell@<component>.service` per entry of
`programs.malaysia.shellComponents`, plus `malaysia-wallpaper.service` and the
portal. The compositor itself launches `lock`, `launcher` and `force-quit`
(`spawn::shell_component`). In development, `./dev.sh demo` starts the
wallpaper, the menubar, the dock and System Settings; it runs on the host's
session bus, so it never starts `notifications`.

## Who draws and who composes

| Aspect | Who decides |
|---|---|
| Protocols and composition order | compositor |
| Geometry: position, size, anchors, exclusive zones | compositor |
| Window frame: corners, shadow, border, hairline | compositor, for every window, CSD or SSD |
| Rounded shapes | the compositor draws the frame of windows, sheets, popups and Dock and notification cards from `window_style.rs`; the shell draws its own with the kit's `continuous_corner`; both read the measured `CORNER_*` tokens |
| Blur and glass behind a surface, and under a BarrierKit window's chrome | compositor: one measured Gaussian pipeline, values from `ctdesign` ([ADR 0020](decisions.md)); the app declares the material |
| Server-side titlebar and traffic lights | compositor |
| Content of the shell surfaces | shell |
| Content of a window | its app |
| Background | `malaysia-wallpaper` |
| Screenshots and screen capture | compositor |

- A client requests its placement; the compositor validates it and decides.
- The shell and Malaysia's apps draw with BarrierKit: Freya and Skia on
  Vulkan. The compositor draws with GLES, `grade-graphics` and `cosmic-text`
  and never links Freya, Skia or wgpu. The wallpaper never links Freya or
  Skia. `scripts/check-client-only-deps.sh` enforces both.
- Design values (colors, radii, shadows, durations) come from `ctdesign`;
  `scripts/check-design-tokens.sh` rejects literals in the compositor.
- GTK and libadwaita keep their headerbar. Qt and the other apps get the
  compositor's 32 px titlebar. A BarrierKit app is the client that binds
  `infinity_window_style_v1`, whatever its `app_id`: it picks its own mode
  before the first commit, and `window.decorations` does not touch it.

Layers, from top to bottom:

```
Overlay     launcher, notifications, alerts
Top         menubar · dock
Windows     clients, SSD titlebars, xdg popups
Background  wallpaper
```

The exclusive zones of the menubar and the dock trim the work area. While the
session is locked, only the wallpaper, the lock surfaces, the menubar and the
cursor are drawn.

A BarrierKit window can put a material over its own content. The app draws its
chrome on a surface above (or at the root of) the content and sends
`set_backdrop` or `set_scroll_edge` for it; the compositor walks the window's
surface elements from the top, keeps the requester and what is above it in
front and draws the material right behind the requester, blurred from the
elements below it (`blur/within.rs`). A window capture includes that material;
a `behind_window` material, which blurs the wallpaper under the window, is left
out of it. The popups and layer surfaces of BarrierKit clients ask the same way;
for them `behind_window` blurs the windows and the wallpaper behind the surface.

## How they talk

| Channel | Between | Carries |
|---|---|---|
| Wayland: layer-shell, foreign-toplevel, session-lock, xdg-activation, image-copy-capture | compositor ↔ shell | placement, window list, lock, activation, capture |
| `infinity_window_manager_v1` | shell → compositor | arrange, restore, force quit, send a shortcut; app menu addresses, spaces and desktops back |
| `infinity_window_style_v1` | BarrierKit apps and the wallpaper → compositor | bar, panel, popover, glass, cards, plates, knobs, the desktop of a surface and, from version 10, a material over what lies below it (`set_backdrop`) and a scroll edge effect (`set_scroll_edge`) |
| `infinity_session_v1` | compositor → shell | `locking` and `unlocked` |
| `infinity_focus_manager_v1` | compositor ↔ patched libadwaita | focus mode bar, the wallpaper color behind a window and how light the glass under a popup is |
| `.domain` files | Settings and the shell write; every process reads | preferences, watched with inotify |
| `.state` files | each has one writer | wallpaper selection and colors, dock, Recent Items, notifications |
| `org.freedesktop.appearance`, `org.malaysia.appearance` | portal → apps | color scheme, accent, contrast and the live GTK colors |
| `org.freedesktop.Notifications`, `org.gtk.Notifications`, `org.malaysia.Notifications1` | apps, portal and compositor → `notifications` | notifications; the compositor posts «Screenshot Saved» |
| `org.freedesktop.Application.ActivateAction` | shell → Settings | opens a pane (`pane.<id>`) |

The compositor serves no D-Bus control interface: `org.malaysia.Compositor1`
does not exist. Settings changes the compositor by writing `.domain` files.
The `.domain` and `.state` files, their directories and their writers are in
[`options.md`](configuration/options.md).

Infinity's protocol crate, `infinity-protocol`, lives in
[barrierkit](https://github.com/HBlanqueto/barrierkit); the compositor
implements it in `crates/malaysia-compositor/src/infinity.rs`.

## Invariants

- Only a successful PAM call in the lock client unlocks, through
  `unlock_and_destroy`. If the lock client dies, the session stays locked and
  the compositor relaunches it (`lock.rs`).
- A privileged global (layer-shell, foreign-toplevel, session-lock, capture,
  virtual input, input method) takes the `security::unrestricted` filter, so a
  sandboxed client never sees it.
- Only `infinity_window_manager_v1` is limited to the trusted shell
  (`can_view`). Every client can bind the other Infinity globals, so they never
  carry control over another client's windows, geometry or content.
- The menubar names the active app from its `app_id`, never from the window
  title.
