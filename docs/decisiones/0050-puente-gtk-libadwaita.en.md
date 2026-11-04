# 0050 — Appearance bridge for GTK and libadwaita

Status: accepted (2026-10-31).

## Context

The portal (ADR 0031) publishes `color-scheme` and `accent-color`. With that,
libadwaita follows light/dark, but it rounds the accent to the closest of its
nine presets and does not know the window tint (`window_color` = `accent` or
`wallpaper`). barrier-kit apps do receive the exact accent and the tint
because they read `appearance.domain` and `wallpaper.state` with `vow-appearance`.
GTK 3 does not read the portal for dark mode.

The GNOME extension [ChromaLeon](https://github.com/Fabito02/ChromaLeon)
solves the same problem by writing GTK user CSS. We take the idea; we do not
copy code or templates (ChromaLeon is GPL-3.0, Malaysia is MIT).

## Decision

1. `xdg-desktop-portal-malaysia` maintains the bridge. On startup and on each
   change of `appearance.domain` or `wallpaper.state` it writes
   `$XDG_CONFIG_HOME/gtk-{3,4}.0/malaysia.css` and leaves at the top of each
   `gtk.css` a block marked `malaysia:gtk-bridge` with
   `@import url("malaysia.css")`. The user's rules stay below it and still
   win.
2. Colors come from `vow_appearance::resolve`, the same rules as the kit: the
   accent always (`accent_bg_color`, `accent_fg_color`, `accent_color`); the
   Adwaita backgrounds (window, headerbar, view, sidebars, cards,
   dialogs, popovers) only if there is a tint, mixed towards it with the
   Tengen factor and the layers (`LAYER_FACTOR_*`). It respects `tint_parts`. With
   `contrast = true` only the accent is written. Without a tint, Adwaita stays as
   it is (ADR 0020 §1).
3. GTK 4: CSS variables (`--accent-bg-color`, libadwaita ≥ 1.6) and
   `@define-color` (earlier versions), with one
   `@media (prefers-color-scheme: light|dark)` block per scheme. This way light/dark
   changes live, also in apps that force a scheme.
4. GTK 3: only the current scheme, plus the `theme_*` names, and
   `gtk-application-prefer-dark-theme` in `gtk-3.0/settings.ini`.
5. The portal also publishes `org.gnome.desktop.interface` `color-scheme`
   (`default` / `prefer-dark`) for apps that only read that key.
6. `malaysia-session.target` starts the portal
   (`xdg-desktop-portal-malaysia.service`, `Type=dbus`; D-Bus activation
   uses the same unit with `SystemdService=`). On `SIGTERM`/`SIGINT`
   it removes the blocks and the `malaysia.css` files, so that another session (GNOME) does not
   inherit them. `settings.ini` keeps its last value.
7. `appearance.gtk_bridge = false` disables the bridge and cleans it up. A
   `gtk.css` or `settings.ini` that is a symbolic link (home-manager) is not touched.
8. **Traffic lights.** The portal publishes `org.gnome.desktop.wm.preferences`
   `button-layout` in the order of `vow_design::TITLEBAR.buttons`
   (`close,minimize,maximize:`, on the left), also with the bridge off.
   GTK 4 and libadwaita read it from the portal as `gtk-decoration-layout`
   (`gdksettings-wayland.c`, `gdk_display_should_use_portal`). GTK 3 outside
   Flatpak reads it from GSettings, not the portal, and keeps its order.
9. The buttons are drawn like the kit's traffic lights: size, spacing,
   radius and shape (`traffic_light_shape`) from `TITLEBAR.buttons`; fills and
   glyph from `vow_appearance::traffic::visual` for focused, unfocused
   (`:backdrop`), pointer over the group (GTK 4) or the button (GTK 3) and
   pressed. The GTK icon shows only under the pointer. In GTK 4 the light is
   drawn on the button's image, which GTK centres (the button fills the
   headerbar's height); the controls start `margin_start` from the edge, less
   the libadwaita headerbar padding, and libadwaita's unfocused dimming
   (`filter: opacity(0.5)`) does not reach them. They go in
   `malaysia.css` and, in GTK 4, also in `gtk4-css` (ADR 0055), inside each
   `@media` block.

## Consequences

- libadwaita apps see the same accent and tint as barrier-kit apps.
- GTK does not reread `gtk.css`: an accent or tint change shows when the app is reopened.
  **Revised by ADR 0055:** the patched libadwaita receives it live through
  the portal; the file remains for Flatpak, GTK 3 and the unpatched libadwaita.
  Light/dark is live in GTK 4. Meanwhile, libadwaita still
  receives the rounded accent from the portal, but the bridge variable
  wins.
- The portal writes outside `~/.config/malaysia`, only in `gtk-3.0` and
  `gtk-4.0` and only inside its block and its files.

## Rejected alternatives

- Own GTK theme (`GTK_THEME`): libadwaita ignores it and it breaks GTK 3 apps.
- Writing to dconf (`org.gnome.desktop.interface`): it changes the GNOME
  configuration for all sessions.
- Forcing the reload by toggling high contrast (like ChromaLeon): it flickers in
  every app.
