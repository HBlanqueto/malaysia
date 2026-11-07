# 0036 — Menú global

Estado: aceptado (2026-10-23); implementado en 14a–14e y 14f-1 (14f-2 pendiente). Superado en parte por los ADR 0040 (canal Infinity v2) y 0045 (GTK menus).

## Contexto

La menubar muestra la app activa y el reloj; macOS añade sus menús.

## Decisión

1. El menú usa `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`: el primero transporta servicio y ruta por superficie; el segundo es el árbol de items.
2. El compositor sirve `org_kde_kwin_appmenu`, guarda el par (servicio, ruta) por superficie y publica el de la ventana enfocada por `window.domain`. **Nunca habla dbusmenu ni dibuja menús.**
3. La menubar es el cliente dbusmenu: `zbus` y CelestialKit.
4. Las apps propias exportan por la misma vía; no hay atajo privado por Infinity.
5. Fuentes por app: dbusmenu (KDE), `org.gtk.Menus` vía `gtk_shell1` (ADR 0045), menú de atajos (ADR 0061) y respaldo `.desktop` (menú de la app, Ventana y Ayuda; Salir usa `zwlr_foreign_toplevel_handle_v1.close`, nunca SIGKILL). Sin ventana, la menubar muestra el menú propio «Yuki».

## Consecuencias

El runner del kit necesita `xdg_popup` con padre layer-shell; la menubar depende de `zbus`; el código portado de Otto obliga a actualizar `NOTICE`.

## Alternativas descartadas

- `com.canonical.AppMenu.Registrar`: depende de IDs X11.
