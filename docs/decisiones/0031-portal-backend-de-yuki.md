# 0031 — Portal backend propio (`xdg-desktop-portal-yuki`)

Estado: **aceptado.** Implementación en curso (Bloque 12a).

## Contexto

`xdg-desktop-portal` es el intermediario entre apps (sobre todo sandboxed) y el
escritorio. El compositor no habla ese D-Bus directamente: implementa un
**backend** (`org.freedesktop.impl.portal.*`) y el frontend enruta las
interfaces. GNOME, KDE y wlroots tienen el suyo; Otto también.

Yuki ya tiene la fuente de verdad de apariencia (`appearance.domain`, ADR 0021)
y la captura (`ext-image-copy-capture-v1`, Bloque 11). Lo que falta es exponerlo
a las apps con las interfaces estándar.

## Decisión

1. **Un backend propio**, binario `xdg-desktop-portal-yuki`, con nombre D-Bus
   `org.freedesktop.impl.portal.desktop.yuki` y objeto
   `/org/freedesktop/portal/desktop`. Es un proceso aparte, no vive dentro del
   compositor, igual que los componentes de la shell (ADR 0023). Lo arranca
   `xdg-desktop-portal` por activación D-Bus.
2. **Solo las interfaces que dependen del compositor.** La primera entrega es
   `org.freedesktop.impl.portal.Settings`; después `Screenshot` y `ScreenCast`.
   FileChooser, AppChooser, Print, Notification y Access siguen en
   `xdg-desktop-portal-gtk`.
3. **La configuración sigue en `.domain`.** El portal **lee**
   `appearance.domain` (nunca escribe): `color-scheme`, `accent-color`,
   `contrast` y `reduced-motion`, publicados en `org.freedesktop.appearance`.
   `Config` sigue siendo el único esquema (ADR 0021); el portal no duplica
   tipos.
4. **Cambios en vivo.** El portal vigila `appearance.domain` (sondeo de mtime
   cada 250 ms por ahora; inotify si hace falta) y emite `SettingChanged`.
   Version 2 de la interfaz.
5. **Enrutado.** El paquete instala `share/xdg-desktop-portal/portals/yuki.portal`
   con `UseIn=Yuki`, y el módulo NixOS escribe la configuración
   `xdg.portal.config.yuki`: `Settings` → `yuki`, `default` → `gtk`. La sesión
   ya exporta `XDG_CURRENT_DESKTOP=Yuki`.
6. **Sin diálogo de permiso para Settings.** Es lectura de preferencias, no
   acceso a datos del usuario. `Screenshot`/`ScreenCast` sí llevarán diálogo y
   concesión por app en su entrega.
7. **El destino por defecto de una captura de ventana** sigue siendo el
   Escritorio XDG resuelto por `yuki::xdg::desktop_dir()`; el portal no fija
   rutas.

## Consecuencias

- Una app GTK/Qt/Electron puede preguntar dark/light, accent y contraste sin
  hablar con Yuki.
- El portal es un proceso más que puede caerse sin tumbar el compositor; D-Bus
  lo relanza.
- El sondeo es simple y suficiente para preferencias que cambian a mano; si la
  app de ajustes empieza a escribir mucho, se cambia a inotify sin tocar la
  interfaz.
- `Screenshot`/`ScreenCast` reutilizarán `ext-image-copy-capture-v1` y PipeWire;
  no se implementa `wlr-screencopy` (Otto lo usa, Yuki no lo necesita).

## Referencias

- [`../roadmap.md`](../roadmap.md) Bloque 12a.
- [`../arquitectura/capturas.md`](../arquitectura/capturas.md).
- [`../configuracion/opciones.md`](../configuracion/opciones.md).
- `org.freedesktop.impl.portal.Settings` (xdg-desktop-portal).
