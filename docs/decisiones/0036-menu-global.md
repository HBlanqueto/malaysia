# 0036 — Menú global

Estado: **aceptado (2026-10-23); implementado en 14a–14f.** Superado en parte por
los ADR 0040 (canal Infinity v2) y 0045 (GTK menus).

## Contexto

La menubar muestra el nombre de la app activa y el reloj. macOS añade los menús
de la app enfocada. Wayland no tiene un protocolo de menú global de freedesktop:
existen `org_kde_kwin_appmenu` (Wayland) y `com.canonical.dbusmenu` (D-Bus).
Hace falta decidir quién habla cada protocolo.

## Decisión

1. El menú global usa `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`: el
   primero transporta el servicio y la ruta D-Bus por superficie; el segundo es
   el árbol de items.
2. El compositor sirve `org_kde_kwin_appmenu`, guarda el par (servicio, ruta)
   por superficie y publica el de la ventana enfocada por `window.domain`.
   **Nunca habla dbusmenu ni dibuja menús.**
3. La menubar es el cliente dbusmenu: usa `zbus` y dibuja con BarrierKit.
4. Las apps propias exportan su menú por la misma vía dbusmenu; no hay atajo
   privado por Infinity.

## Reparto

```
app ──org_kde_kwin_appmenu──► compositor ──window.domain──► menubar
                                                              │
                                              com.canonical.dbusmenu
                                                              ▼
                                                     popups BarrierKit
```

- **Compositor:** sirve el global, valida la superficie, guarda el par y lo
  publica al enfocar.
- **Menubar:** resuelve el servicio/ruta, pide el layout, escucha señales y
  dibuja; también cierra los popups.
- **Cliente:** exporta por dbusmenu y llama a `set_address`.

## Orden de fuentes por app

1. dbusmenu (appmenu de KDE).
2. `org.gtk.Menus` vía `gtk_shell1` (ADR 0045).
3. Menú de atajos por app, opcional.
4. Respaldo por `.desktop`: Ventana (minimizar/maximizar/cerrar) y Salir;
   Salir usa `zwlr_foreign_toplevel_handle_v1.close`, nunca SIGKILL.

Sin ventana enfocada, la menubar muestra el menú propio «MalaysiaDreams».

## Alternativas descartadas

- `com.canonical.AppMenu.Registrar`: depende de IDs de ventana X11.
- AT-SPI: menús perezosos, Firefox se queda fijado y exige `QT_ACCESSIBILITY=1`.
- `xdg-dbus-annotation`: no aterrizó upstream.
- Leer menús en el compositor: rompe el reparto de los ADR 0023/0024/0025.

## Consecuencias

- El runner del kit necesita `xdg_popup` con padre layer-shell.
- La menubar depende de `zbus` (cliente, no compositor).
- El código portado de Otto obliga a actualizar `NOTICE` en el mismo commit.
- `window.domain` transporta el par appmenu hasta que exista el D-Bus del ADR
  0021.

## Medidas

Ancho = contenido entre 220 px y `min(640, salida lógica − 32)`; solo por encima
del máximo se recorta el label. Atajos e iconos nunca se recortan.
