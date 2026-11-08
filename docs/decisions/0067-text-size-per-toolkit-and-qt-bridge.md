# 0067 — Text size per toolkit and Qt bridge

Status: accepted (2026-11-08).

## Context

macOS System Settings › Accessibility › Display › Text size opens a page with
a preferred reading size and a list of apps, each set to «Use Preferred
Reading Size» or a size of its own
(<https://support.apple.com/guide/mac-help/mchld786f2cd/26/mac/26>). Malaysia
runs three kinds of app, and each reads text size its own way:

- **GTK 4 and libadwaita** read `org.gnome.desktop.interface`
  `text-scaling-factor` through the Settings portal and turn it into
  `gtk-xft-dpi` (`gdk/wayland/gdksettings-wayland.c`). Since GTK 4.18 they use
  the portal outside a sandbox too; since 4.22 it is their only source.
- **GTK 3** reads GSettings outside a sandbox, so a `settings.ini` value loses.
  `GDK_DPI_SCALE` scales only its fonts (`gtk-3-24`,
  `gdk/wayland/gdkscreen-wayland.c`). GTK 4 does not read it.
- **Qt 6** has no font-only scale: `QT_FONT_DPI` scales the whole UI and Qt
  advises against it on desktops (<https://doc.qt.io/qt-6/highdpi.html>). The
  `xdgdesktopportal` theme reads only the color scheme and contrast. qt6ct sets
  the font and palette from `~/.config/qt6ct/qt6ct.conf` and reloads it when
  the folder changes (`qt6ctplatformtheme.cpp`).

## Decision

1. **One size per toolkit.** `appearance.text_sizes` holds an optional size
   for `barrierkit`, `gtk` and `qt`; a missing one uses `text_size`
   (`AppearanceConfig::text_size_for`).
2. **GTK 4.** The portal publishes `text-scaling-factor`. Open apps change
   right away.
3. **GTK 3.** The compositor sets `GDK_DPI_SCALE` on every app it starts,
   unless the session already has one. An app takes it when it opens again.
4. **Qt 6 through qt6ct.** The compositor sets `QT_QPA_PLATFORMTHEME=qt6ct` in
   its session, unless one is set. The portal's Qt bridge (`qt_bridge`,
   default `true`) writes, on every appearance change:
   - `colors/malaysia.conf`: the 22 `QPalette` roles from the resolved Tengen
     surfaces and inks, so Qt follows light, dark and the accent;
   - in `qt6ct.conf`, only `style = Fusion`, `custom_palette`,
     `color_scheme_path` and the `general` and `fixed` fonts (`BODY` and
     `MONO`, in points, times the Qt size). Other keys stay.
5. **NixOS.** The module installs qt6ct and enables `qt` so `QT_PLUGIN_PATH`
   finds it, without a global `QT_QPA_PLATFORMTHEME`.
6. **System Settings.** Accessibility › Display › Text size is a page: the
   preferred size slider and, under Apps, a menu per toolkit with «Use
   preferred reading size» or a size.

## Consequences

Qt 5 apps keep their default theme. Apps started outside the compositor (a
D-Bus activated service, another session) do not get `GDK_DPI_SCALE` or
`QT_QPA_PLATFORMTHEME`. A Qt style other than Fusion needs `qt_bridge = false`.

## Rejected alternatives

- `QT_FONT_DPI`: it zooms the whole Qt 6 UI.
- Writing `text-scaling-factor` to dconf for GTK 3: it changes a GNOME setting
  that other sessions share.
