# 0031 — Portal backend propio (`xdg-desktop-portal-yuki`)

Estado: **aceptado.** `Settings` implementado; faltan `Screenshot` y
`ScreenCast`.

## Contexto

`xdg-desktop-portal` enruta a un backend `org.freedesktop.impl.portal.*`.
MalaysiaDreams ya tiene la apariencia (`appearance.domain`, ADR 0021) y la
captura (`ext-image-copy-capture-v1`, Bloque 11); falta exponerlo con las
interfaces estándar.

## Decisión

1. Backend propio `xdg-desktop-portal-yuki`, nombre
   `org.freedesktop.impl.portal.desktop.yuki`, objeto
   `/org/freedesktop/portal/desktop`. Proceso aparte, arrancado por activación
   D-Bus (ADR 0023).
2. Solo interfaces que dependen del compositor: `Settings` primero; después
   `Screenshot` y `ScreenCast`. FileChooser, AppChooser, Print, Notification y
   Access siguen en `xdg-desktop-portal-gtk`.
3. El portal **lee** `appearance.domain` (nunca escribe): `color-scheme`,
   `accent-color`, `contrast` y `reduced-motion` en
   `org.freedesktop.appearance`. `Config` sigue siendo el único esquema.
4. Cambios en vivo: vigila mtime (o inotify) y emite `SettingChanged`; interfaz
   versión 2.
5. Enrutado: `yuki.portal` con `UseIn=Yuki` y `xdg.portal.config.yuki`
   (`Settings` → `yuki`, `default` → `gtk`).
6. Sin diálogo de permiso para `Settings`; `Screenshot`/`ScreenCast` sí lo
   llevarán.
7. El destino por defecto de una captura de ventana es el Escritorio XDG
   resuelto por `xdg::desktop_dir()`.

## Consecuencias

- Una app GTK/Qt/Electron lee dark/light, accent y contraste sin hablar con el
  compositor.
- El portal puede caerse sin tumbar la sesión; D-Bus lo relanza.
- `Screenshot`/`ScreenCast` reutilizarán la captura del Bloque 11 y PipeWire; no
  se implementa `wlr-screencopy`.

## Alternativas descartadas

- Usar solo `xdg-desktop-portal-gtk`: no conoce la apariencia ni la captura del
  compositor.
- Implementar todas las interfaces: duplica diálogos genéricos que GTK ya
  resuelve.
- Portal dentro del compositor: lo agranda y mezcla responsabilidades.
