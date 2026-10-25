# 0036 — Menú global

Estado: **aceptado** (2026-10-23). Documenta el Bloque 14; no hay código todavía.

## Contexto

La menubar de Yuki (`yuki-shell menubar`) muestra hoy el nombre de la app activa
y el reloj. macOS pone además los menús de la app enfocada en la barra superior.
Wayland no tiene un protocolo de menú global de freedesktop: lo que existe es
`org_kde_kwin_appmenu` (Wayland) más `com.canonical.dbusmenu` (D-Bus). Lo usan
Firefox, el plugin de JetBrains y un PR de menú global de Zed; Qt 6 lo lista
entre sus protocolos Wayland (ver
[la investigación](../investigacion/global-menu.md)).

Hace falta decidir **quién habla cada protocolo**. El compositor conoce la
ventana enfocada y sirve protocolos Wayland; la shell dibuja y no debe depender de
que el compositor sepa dibujar menús (ADR 0023/0024). Reconstruir menús por
accesibilidad es frágil y fija ventanas en pantalla.

## Decisión

1. **El menú global usa `org_kde_kwin_appmenu` (Wayland) + `com.canonical.dbusmenu`
   (D-Bus).** `org_kde_kwin_appmenu` solo transporta por superficie el servicio
   y la ruta D-Bus del menú; `com.canonical.dbusmenu` es la interfaz D-Bus que
   la shell lee para obtener el árbol de items.
2. **El compositor sirve `org_kde_kwin_appmenu`.** Guarda por superficie el par
   (servicio D-Bus, ruta) y publica el de la ventana enfocada por el canal que ya
   usa la menubar: el estado `window.domain` (`crates/yuki/src/active.rs`), hasta
   que exista el D-Bus del [ADR 0021](0021-superficie-de-configuracion-y-control.md).
   **El compositor nunca habla dbusmenu ni dibuja menús.**
3. **La menubar es el cliente dbusmenu.** Usa `zbus`, que ya está en el
   workspace, y dibuja con CelestialKit.
4. **Las apps de CelestialKit exportan su menú por la misma vía dbusmenu.** No
   hay un atajo privado por Infinity: una app propia se comporta como cualquier
   cliente para el menú global.

## Reparto del trabajo

```
app ──org_kde_kwin_appmenu──► compositor ──window.domain──► menubar
                                                              │
                                              com.canonical.dbusmenu
                                                              ▼
                                                     popups CelestialKit
```

- **Compositor:** sirve el global, valida la superficie, guarda el par y lo
  publica al enfocar. No abre conexiones D-Bus de menú ni conoce el árbol de
  items.
- **Menubar:** al cambiar la ventana activa, resuelve el servicio y la ruta,
  pide el layout a dbusmenu, escucha sus señales y dibuja. Es también quien
  cierra los popups.
- **Cliente:** exporta su menú por dbusmenu y llama a `set_address` en
  `org_kde_kwin_appmenu` para su `wl_surface`.

## Orden de fuentes por app enfocada

1. **dbusmenu (appmenu de KDE).** Fuente principal.
2. **`org.gtk.Menus` vía `gtk_shell1`.** A investigar, sin compromiso: GTK puede
   exportar su menú en Wayland, pero el camino no está validado en Yuki.
3. **Menú de atajos por app.** Opcional y futuro: mapeos propios que traducen
   acciones a combinaciones de teclas.
4. **Respaldo por `.desktop`.** Si no hay menú: nombre y acciones del `.desktop`,
   un menú **Ventana** con los controles reales (minimizar, maximizar, cerrar) y
   **Salir**. Salir llama a `zwlr_foreign_toplevel_handle_v1.close` desde la
   menubar; el compositor responde enviando `xdg_toplevel.close`, **nunca con
   SIGKILL**.

## Menú «Yuki» sin ventana enfocada

Cuando no hay ventana activa, la menubar muestra el menú «Yuki» (los ocho menús,
sus entradas y las medidas están en
[`arquitectura/menubar.md`](../arquitectura/menubar.md#menú-yuki-aprobado-sin-ventana-enfocada)).
Es contenido propio de la shell: no depende de D-Bus ni de una app.

## Alternativas descartadas

| Alternativa | Motivo |
|---|---|
| `com.canonical.AppMenu.Registrar` | Depende de IDs de ventana X11; no hay equivalente limpio en Wayland. |
| AT-SPI en v1 | Menús perezosos, Firefox solo los expone si se abren visualmente y se quedan fijados; además exige `QT_ACCESSIBILITY=1`. |
| `xdg-dbus-annotation` | No aterrizó upstream; Firefox lo abandonó por el protocolo de KDE. |
| Leer los menús desde el compositor | Rompe el [ADR 0024](0024-stack-de-render-del-toolkit.md) y el reparto compositor ↔ shell de los ADR 0023/0025. |

## Consecuencias

- CelestialKit necesita **`xdg_popup` con padre layer-shell** en su runner: los
  menús de la barra no son superficies layer-shell hijas, son popups.
- La menubar pasa a depender de **zbus**. Es una dependencia de cliente, no del
  compositor.
- El código portado de Otto (`kde_appmenu.rs` del compositor y `app_menu.rs` del
  kit) obliga a actualizar `NOTICE` en el mismo commit.
- El canal `window.domain` transporta ahora también el par appmenu. Sigue siendo
  un estado de fichero hasta el D-Bus del ADR 0021.
- La aceptación real del Bloque 14d depende de Kate (menú dbusmenu) y de Firefox
  con sus dos preferencias. Si Firefox en Wayland necesita además
  `appmenu-gtk-module`, se anota como requisito de la sesión, no del repo.

## Medidas de los menús

El ancho de un menú sigue el comportamiento de macOS, no un máximo fijo de
480 px:

- El ancho = contenido (icono + label + atajo + márgenes) entre un mínimo de
  220 px y un máximo de `min(640, ancho lógico de la salida − 32)`.
- Solo por encima del máximo se recorta el label por el medio con «…».
- Los atajos y los iconos nunca se recortan.
- El token `components.shell.menu.panel.max-width` en Neptunia es 640 px;
  la salida lógica se aplica en runtime.

## Referencias

- [`0020`](0020-politica-de-diseno-y-unificacion.md) — chrome y efectos.
- [`0021`](0021-superficie-de-configuracion-y-control.md) — superficie de
  configuración y control (D-Bus futuro).
- [`0023`](0023-los-componentes-del-shell-corren-como-procesos-separados.md) —
  un proceso por componente.
- [`0024`](0024-stack-de-render-del-toolkit.md) — stack de render del toolkit.
- [`0032`](0032-neptunia-como-fuente-de-diseno.md) — Neptunia como fuente.
- [`0034`](0034-celestialkit-sobre-freya.md) — CelestialKit sobre Freya.
- [`0035`](0035-identificacion-de-apps-propias-y-csd.md) — apps propias y CSD.
- [`investigacion/global-menu.md`](../investigacion/global-menu.md) — los
  proyectos de referencia.
- [`arquitectura/menubar.md`](../arquitectura/menubar.md) — el componente y el
  menú «Yuki».
- [`roadmap.md`](../roadmap.md#bloque-14--menú-global) — las entregas 14a–14e.
