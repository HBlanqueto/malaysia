# 0036 — Menú global

Estado: aceptado (2026-10-23); implementado en 14a–14e y 14f-1 (14f-2 pendiente). Superado en parte por los ADR 0040 (canal Infinity v2) y 0045 (GTK menus).

## Contexto

La menubar muestra la app activa y el reloj; macOS añade sus menús. Wayland no tiene menú global de freedesktop: existen `org_kde_kwin_appmenu` (Wayland) y `com.canonical.dbusmenu` (D-Bus). Hay que decidir quién habla cada protocolo.

## Decisión

1. El menú usa `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`: el primero transporta servicio y ruta por superficie; el segundo es el árbol de items.
2. El compositor sirve `org_kde_kwin_appmenu`, guarda el par (servicio, ruta) por superficie y publica el de la ventana enfocada por `window.domain`. **Nunca habla dbusmenu ni dibuja menús.**
3. La menubar es el cliente dbusmenu: `zbus` y CelestialKit.
4. Las apps propias exportan por la misma vía; no hay atajo privado por Infinity.

## Reparto

La app anuncia el par al compositor; este lo publica; la menubar pide el layout, escucha señales, dibuja y cierra los popups.

## Orden de fuentes

1. dbusmenu (appmenu de KDE).
2. `org.gtk.Menus` vía `gtk_shell1` (ADR 0045).
3. Menú de atajos por app, opcional.
4. Respaldo por `.desktop`: Ventana (minimizar/maximizar/cerrar) y Salir; Salir usa `zwlr_foreign_toplevel_handle_v1.close`, nunca SIGKILL.

Sin ventana, la menubar muestra el menú propio «Yuki».

## Alternativas descartadas

- `com.canonical.AppMenu.Registrar`: depende de IDs X11.
- AT-SPI: menús perezosos, Firefox se queda fijado y exige `QT_ACCESSIBILITY=1`.
- `xdg-dbus-annotation`: no aterrizó.
- Leer menús en el compositor: rompe el reparto de los ADR 0023/0024/0025.

## Consecuencias

El runner del kit necesita `xdg_popup` con padre layer-shell; la menubar depende de `zbus`; el código portado de Otto obliga a actualizar `NOTICE`. `window.domain` transporta el par hasta el D-Bus del ADR 0021. Ancho de menú = contenido entre 220 px y `min(640, salida lógica − 32)`; solo se recorta el label por encima del máximo.
