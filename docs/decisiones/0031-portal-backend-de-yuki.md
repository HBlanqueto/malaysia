# 0031 — Portal backend propio (`xdg-desktop-portal-yuki`)

Estado: aceptado. `Settings` implementado; faltan `Screenshot` y `ScreenCast`.

## Contexto

`xdg-desktop-portal` enruta a un backend `org.freedesktop.impl.portal.*`.

## Decisión

1. Backend propio `xdg-desktop-portal-yuki`, nombre `org.freedesktop.impl.portal.desktop.yuki`, objeto `/org/freedesktop/portal/desktop`. Proceso aparte, arrancado por activación D-Bus (ADR 0023).
2. Solo interfaces que dependen del compositor: `Settings` primero; después `Screenshot` y `ScreenCast`. FileChooser, AppChooser, Print, Notification y Access siguen en `xdg-desktop-portal-gtk`.
3. El portal **lee** `appearance.domain` (nunca escribe): `color-scheme`, `accent-color`, `contrast` y `reduced-motion` en `org.freedesktop.appearance`.
4. Cambios en vivo: vigila mtime (o inotify) y emite `SettingChanged`; interfaz versión 2.
5. Enrutado: `yuki.portal` con `UseIn=Yuki` y `xdg.portal.config.yuki` (`Settings` → `yuki`, `default` → `gtk`).
6. Sin diálogo para `Settings`; `Screenshot`/`ScreenCast` sí lo llevarán.
7. El destino por defecto de una captura de ventana es el Escritorio XDG resuelto por `xdg::desktop_dir()`.

## Consecuencias

Una app GTK/Qt/Electron lee dark/light, accent y contraste sin hablar con el compositor; el portal puede caerse sin tumbar la sesión.

## Alternativas descartadas

- Solo `xdg-desktop-portal-gtk`: no conoce apariencia ni captura del compositor.
