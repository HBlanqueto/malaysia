# 0070 — Your own themes for GTK and Qt

Status: accepted (2026-11-10).

## Goal and scope

This ADR fixes how System Settings lets the user stop the GTK and Qt
bridges (ADR 0050 and 0067) and pick themes that are already on the
system. It starts at Accessibility › freedesktop Options and ends at the
files and GSettings keys that GTK and Qt read. Readers: whoever works on
`malaysia-settings` or `xdg-desktop-portal-malaysia`.

## Context

1. The portal tints GTK, libadwaita and Qt with the accent and the window
   tint. `appearance.gtk_bridge` and `appearance.qt_bridge` turn that off,
   but only by editing `appearance.domain`.
2. GTK 3 and GTK 4 on Wayland read the theme, the icon theme and the cursor
   from GSettings (`org.gnome.desktop.interface`) when its schemas are
   installed, and from `gtk-3.0/settings.ini` otherwise. Their GSettings
   defaults are `Adwaita`.
3. libadwaita has no themes; it reads only `~/.config/gtk-4.0/gtk.css`.
4. qt5ct and qt6ct read `style`, `custom_palette`, `color_scheme_path` and
   `icon_theme` under `[Appearance]`. With `style` unset they use Fusion.

## Decision

1. **One switch.** «Use your own themes» sets `gtk_bridge` and `qt_bridge`
   together, after an alert that warns the accent will no longer match.
   `ctappearance::AppearanceConfig` carries both keys, so Settings and the
   portal read the same field.
2. **The portal cleans up.** With the GTK bridge off it removes its
   `gtk.css` block and `malaysia.css`, as before. With the Qt bridge off it
   removes `malaysia-colors.conf` and the `custom_palette` and
   `color_scheme_path` keys that point at it; the style, icons and fonts
   stay.
3. **What is installed, never a fixed list.** Settings lists:
   1. GTK 3 themes: folders with `gtk-3.0/gtk.css` in
      `$XDG_DATA_HOME/themes`, `~/.themes` and each `$XDG_DATA_DIRS/themes`,
      plus the three themes GTK 3 builds in. Key themes (Emacs) carry only
      `gtk-keys.css` and are left out.
   2. Icon themes: folders in `icons` and `~/.icons` whose `index.theme`
      lists `Directories` and is not `Hidden`, without `hicolor` and
      `default`.
   3. Qt styles: Fusion and Windows, built into Qt, and the plugins in each
      `styles` folder of `QT_PLUGIN_PATH`, `/run/current-system/sw/lib/qt-6/plugins`
      and `/usr/lib/qt6/plugins`; `lib<key><major>.so` gives the key.
   4. Qt palettes: the `.conf` schemes in `qt6ct/colors` under the config,
      data and system data folders, after «The style's».
4. **The current value is the real one.** GTK values come from `gsettings
   get`, then `settings.ini`, then `Adwaita`; Qt values from `qt6ct.conf`,
   then Fusion. A value that no folder lists is still shown and selected.
5. **Writes go where the toolkit reads.** GTK writes both GSettings and
   `settings.ini`; Qt writes `qt6ct.conf` and `qt5ct.conf`, through
   `ctappearance::ini`. A symlinked file (home-manager) is not changed, and
   Settings says so.
6. **Imports.** A `.css` replaces `gtk-4.0/gtk.css` and keeps the old one
   as `gtk.css.bak`; a `.conf` palette is copied to `qt6ct/colors` and
   chosen. GTK 3 themes are added by copying them to the folder that
   «Themes Folder › Open» shows.
7. **Back to Malaysia.** It resets `gtk-theme` and `icon-theme`, removes
   the Qt `style` and `icon_theme`, and turns both bridges on. The user's
   `gtk.css` stays.

## Consequences

1. Light or dark mode, text size and the traffic light order still reach
   GTK and Qt through the portal with the bridges off.
2. The cursor is not offered: apps that use `cursor-shape-v1` show the
   compositor's cursor, so a GTK cursor theme would change only some apps.
3. Flatpak apps read GSettings through the GNOME portal backend, which
   Malaysia does not run; they keep Adwaita.

## Rejected alternatives

- A themes table in `appearance.domain` that the portal applies: the
  toolkit files would then hold two truths, and a theme set outside
  Settings would not be recognized.
- Writing only `settings.ini`: GTK on Wayland prefers GSettings when its
  schemas exist, so the choice would not take effect.
