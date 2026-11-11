# The menubar (`malaysia-shell menubar`)

> Overview map: [`interface.md`](interface.md).

Native component (ADR 0023/0034): a top bar, 30 or 38 px high, with the active app, the clock and the menus.

## How it works

```
foreign-toplevel ──► active app (app_id)
        ▼
bk layer surface → layer-shell Top (30 or 38 px) → Skia on Vulkan
```

1. `Top` layer, anchored top/left/right, with `exclusive_zone` equal to its height (`MenubarConfig::height()`); windows do not go underneath.
2. The bar is a `bk` layer surface (`run_layer`); the kit draws it with Skia on Vulkan ([barrierkit ADR 0056](https://github.com/HBlanqueto/barrierkit/blob/main/docs/decisions/0056-barrierkit-on-vulkan.md)).
3. The clock shows `%H:%M` and commits at the minute boundary.
4. The active app is the activated toplevel in foreign-toplevel; while a shell dialog (Force Quit, About) is active, the bar keeps the previous app. Each new active app is recorded in Recent Items. The menubar uses the **app_id**, never the window title.
5. Shell dialogs («Acerca de este PC» and an app's «Acerca de») do not change the active app: the bar keeps the last app that had focus, as on macOS.

## The app name

1. A `.desktop` whose id or `StartupWMClass` matches → its localized `Name` (`foot` → «Foot», `org.wezfurlong.wezterm` → «WezTerm»).
2. Without a `.desktop`, the last segment with an initial capital (`org.example.notes` → «Notes»).
3. Without an app or window, «Malaysia».

It is bold (`MENUBAR_APP_NAME_WEIGHT`, 700), color `ink-strong`, 13 px, with the `ctdesign` sans (Geist); the clock uses the regular weight.

## Style and height

`menubar.style` and `menubar.bar_height` ([`options.md`](../configuration/options.md#menubar--shell-menu-bar-macos-menu-bar), ADR 0075) choose how the bar looks. System Settings › Desktop & Dock writes them.

1. The bar reads `menubar.domain` and `wallpaper.state` and redraws when either changes.
2. It takes the wallpaper color under the bar from the wallpaper grid (`WallpaperGrid::behind` over its own rectangle).
3. `solid` paints `shell-bar` with the theme's text. `translucent` paints the glass and ink of `colors_over` for that color. `floating` uses the same ink with no background.
4. `solid` and `translucent` ask the compositor to blur the wallpaper under the whole bar (`set_blur_region`). The blur stops while the screen locks.
5. With `reduce_transparency`, every style is an opaque `solid` bar without blur.
6. When the height changes, the bar sends its new size and exclusive zone together (`set_layer_geometry`). The lock screen row, the menus and the live activities read the same height.

## Exclusive zone

The compositor computes the workspace as the output minus the exclusive zones of `Top`/`Overlay` layers anchored to the edges (`Layers::exclusive_top`/`exclusive_bottom`). New windows are placed there: origin `(0, top)` and size `(w, h − top − bottom)`. A layer surface with `exclusive_zone = 0` that is anchored to an edge is moved out of those zones too, as wlr-layer-shell asks: notification banners start under the menubar. The wallpaper sets `-1` and covers the whole output.

## Global menu

The bar embeds `MenuView`: without a window it shows ❄ · Escritorio · Archivo · Edición · Visualización · Ir · Ventana · Ayuda; with a window, ❄ · {App} · real menus · Ventana · Ayuda. The real menu uses `org_kde_kwin_appmenu` + dbusmenu (ADR 0036/0040); if there is none, the `.desktop` fallback (14c). GTK uses `gtk_shell1`/`org.gtk.Menus` (14f-1 registers; 14f-2 draws); a GTK 3 `GtkMenuBar` arrives the same way with `appmenu-gtk-module` and `unity.*` actions (14f-4). Chromium, patched Electron (14f-5) and native Qt 6 (`qwaylandappmenu.cpp`) use `org_kde_kwin_appmenu`. When the app publishes no menu and is in the `crates/malaysia-shell/shortcuts/` catalog, its shortcut menus go between the app menu and Window; each entry types the chord through Infinity's `send_shortcut` (ADR 0061). Pending: 14b-2, 14f-2, F10/Alt keyboard and overflow.

Each title measures its text plus `title-padding-x` (10) per side. Its pill (`title-open`, 24 high, radius 10) overhangs by `bleed` (3) per side and drops by `text-lift` (1) to sit centred in the bar; it is painted with `title-open.bg` while the menu is open and with `hover-bg` under the pointer. The panel opens shifted to the left so its text (`panel.padding` 5 + `item.padding-x` 13) falls in the column of the title text, as in Tahoe.

«Acerca de {App}» takes the app's «Acerca de…»/«About…» entry from whichever menu holds it (almost always Help) and removes it there (`remote::take_about`). If the app has none, it opens `malaysia-shell about-app <app_id>`: icon, name and `GenericName` from the `.desktop`.

## Status icons

App icons (`NSStatusItem`, the system tray) go on the right, before the clock. The bar is the `org.kde.StatusNotifierWatcher` and the host of the [StatusNotifierItem](https://specifications.freedesktop.org/status-notifier-item/latest/) protocol, like plasmashell; the logic lives in `bk::shell::use_status_items` and the bar only draws them (`menubar/status.rs`).

1. If another watcher already runs, the bar does not replace it: it follows its signals.
2. The icon comes from the theme (`IconName`, searched in `status`, `panel`, `devices` and `apps`, or in KDE's `IconThemePath`); without one, `IconPixmap` is painted. With `Status = NeedsAttention` the attention ones are used.
3. An icon with a menu opens it on press, like the bar menus and like an `NSStatusItem` with a menu; a click on an item sends `Event("clicked")` over dbusmenu. An icon without a menu sends `Activate`.
4. Every signal from the icon or its menu (`NewIcon`, `NewTitle`, `NewStatus`, `LayoutUpdated`…) reads it again; if its name leaves the bus, it is removed.

## System status menus

After the app icons come VPN, Bluetooth, Wi-Fi and Battery (`menubar/extras.rs`, ADR 0069). They read NetworkManager, BlueZ, UPower and power-profiles-daemon through `malaysia-services`, the same workers as System Settings.

1. Each menu shows only if its daemon answers and its `show_*` key in `menubar.domain` is on ([`options.md`](../configuration/options.md#menubar--shell-menu-bar-macos-menu-bar)). VPN shows only with a VPN profile or a tunnel from another VPN app; its symbol is dimmed until one is up, and the other apps are listed under their own header. Battery shows only with a battery.
2. The icons take the bar text color: the Wi-Fi arcs show the signal of the current network, the battery fills to its level, green while charging and red from 20 % down.
3. Wi-Fi turns the radio on or off and lists known and other networks; opening it requests a scan. A known or open network connects from the menu; one that needs a password opens Settings › Wi-Fi.
4. Bluetooth turns the radio on or off and connects or disconnects paired devices, with their battery.
5. Battery shows the level, the power source and the time left, and toggles Low Power Mode.
6. «… Settings…» opens that pane: the bar sends `ActivateAction("pane.<id>")` to a running System Settings, or starts it with the pane id as argument.
