# 0031 — Own portal backend (`xdg-desktop-portal-malaysia`)

Status: accepted. `Settings` is implemented; `Screenshot` and `ScreenCast` are missing.

## Context

`xdg-desktop-portal` routes to an `org.freedesktop.impl.portal.*` backend.

## Decision

1. Own backend `xdg-desktop-portal-malaysia`, name `org.freedesktop.impl.portal.desktop.malaysia`, object `/org/freedesktop/portal/desktop`. A separate process, started by D-Bus activation (ADR 0023).
2. Only interfaces that depend on the compositor: `Settings` first; then `Screenshot` and `ScreenCast`. FileChooser, AppChooser, Print, Notification and Access stay in `xdg-desktop-portal-gtk`.
3. The portal **reads** `appearance.domain` (it never writes): `color-scheme`, `accent-color`, `contrast` and `reduced-motion` in `org.freedesktop.appearance`.
4. Live changes: it watches the mtime (or inotify) and emits `SettingChanged`; interface version 2.
5. Routing: `malaysia.portal` with `UseIn=Malaysia` and `xdg.portal.config.malaysia` (`Settings` → `malaysia`, `default` → `gtk`).
6. No dialog for `Settings`; `Screenshot`/`ScreenCast` will have one.
7. The default destination of a window capture is the XDG Desktop resolved by `xdg::desktop_dir()`.

## Consequences

A GTK/Qt/Electron app reads dark/light, accent and contrast without talking to the compositor; the portal can crash without bringing down the session.

## Rejected alternatives

- Only `xdg-desktop-portal-gtk`: it knows neither the compositor's appearance nor its capture.
