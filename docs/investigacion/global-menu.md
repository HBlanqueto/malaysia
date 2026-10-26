# Menú global: proyectos de referencia

Estado: **investigación, no dependencia.** Este documento reúne los proyectos que
resuelven el menú global en Linux y qué toma Yuki de cada uno. La decisión está en
el [ADR 0036](../decisiones/0036-menu-global.md); el plan, en el
[Bloque 14](../roadmap.md#bloque-14--menú-global) y en la
[menubar](../arquitectura/menubar.md#menú-global-planeado-bloque-14).

Revisado el 2026-10-23 contra la documentación pública de cada proyecto. No se
clonaron los repositorios: se leyeron sus README, su licencia y sus notas de
soporte. La única excepción es Otto, ya analizado archivo a archivo en
[Otto y los forks de HBlanqueto](otto-y-forks.md).

## Regla de licencias

| Proyecto | Licencia | Se puede copiar a Yuki (MIT) |
|---|---|---|
| `yolo-labz/noctalia-appmenu` | Apache-2.0 | Sí, con aviso en `NOTICE` |
| `helloSystem/Menu` | GPL-3.0 | No |
| `gonzaarcr/fildem` | GPL-3.0 | No |
| `Unmade760/orbit-global-menu` | GPL-3.0-or-later | No |
| `ShiroOSL/global-menu-for-gnome` | GPL-3.0 | No |
| Firefox (global menu) | MPL-2.0 | No |
| Otto | MIT | Sí, con aviso en `NOTICE` |

De los proyectos GPL se toman **ideas y comportamiento**, nunca código ni
estructuras copiadas. Si algún día se copia código Apache-2.0 o MIT, el aviso
correspondiente entra en `NOTICE` en el mismo commit.

## 1. `yolo-labz/noctalia-appmenu` — Apache-2.0

**Qué es.** Un bridge en Rust para el compositor niri. Convierte el menú de la
ventana enfocada en algo que la barra de Noctalia pueda dibujar.

**Cómo obtiene los menús.** Por AT-SPI: recorre el árbol de accesibilidad del
cliente y localiza la barra de menús y sus elementos. No habla `dbusmenu` ni
`org_kde_kwin_appmenu`.

**X11 / Wayland.** Es Wayland puro (niri). AT-SPI funciona en los dos, pero el
resultado depende de lo que el cliente exponga.

**Límites que documenta.** Firefox solo publica su menú si este se abre
visualmente, y la propia barra de menús de Firefox queda fijada en pantalla.
Su propio README dice que la solución limpia es que el compositor anuncie
`org_kde_kwin_appmenu_manager` y que el cliente entregue el menú por D-Bus, en
vez de reconstruirlo por accesibilidad.

**Qué toma Yuki.** La idea del **menú de respaldo**: cuando la app no entrega
menú, mostrar el nombre y las acciones del `.desktop`, los controles reales de la
ventana y Salir. Se descarta AT-SPI como fuente principal por los dos límites
anteriores (menús perezosos y ventanas fijadas).

## 2. `helloSystem/Menu` — GPL-3.0

**Qué es.** El menú global de helloSystem (FreeBSD), escrito en Qt5.

**Cómo obtiene los menús.** Es un cliente X11 clásico: lee la barra de menús de
las apps X11 y la pinta en su propia ventana global. No es un camino Wayland.

**X11 / Wayland.** X11 (y FreeBSD). No sirve como implementación en Wayland.

**Qué toma Yuki.** Solo **referencia de UX**: jerarquía de primer nivel, orden de
los menús de aplicación y el menú de la propia shell cuando no hay ventana
enfocada. Nada de código (GPL-3.0).

## 3. `gonzaarcr/fildem` — GPL-3.0

**Qué es.** Un menú global para GNOME en Python. Su autor lo describe como
**prototipo**, no como producto terminado.

**Cómo obtiene los menús.** Usa `appmenu-gtk-module` con su registrar: el módulo
GTK exporta el menú y fildem lo consume.

**X11 / Wayland.** En la práctica funciona **solo en X11**; en Wayland el camino
del módulo y el registrar no está resuelto.

**Qué toma Yuki.** Dos ideas, sin código: el **HUD** (buscar entre los elementos
del menú de la app enfocada) como evolución futura ligada al launcher, y la
separación entre «exportar el menú» y «dibujarlo» que también usa el ADR 0036.

## 4. `Unmade760/orbit-global-menu` — GPL-3.0-or-later

**Qué es.** Un daemon en Python para GNOME 49/50 que pone el menú de la app
enfocada en el panel.

**Cómo obtiene los menús.** En GTK 3/4 usa `gtk_application_set_menubar`, que sí
funciona en Wayland. Para Qt y Electron depende de X11 bajo GNOME.

**X11 / Wayland.** Mixto: GTK nativo en Wayland; Qt/Electron solo en X11.

**Qué toma Yuki.** La constatación de que **GTK sí puede exportar su menú en
Wayland** (aunque por una vía distinta a dbusmenu, ver `org.gtk.Menus` en el ADR
0036) y la idea de una **capa de menús de atajos por app**: mapeos propios que
traducen una acción del menú a una combinación de teclas. Es opcional y futuro.
Los mapeos se escribirían desde cero; no se copian los suyos (GPL-3.0-or-later).

## 5. `ShiroOSL/global-menu-for-gnome` — GPL-3.0

**Qué es.** Una extensión de GNOME que dibuja menús globales **genéricos**.

**Cómo obtiene los menús.** No lee el menú real de la app: muestra entradas con
atajos y comandos configurados a mano.

**X11 / Wayland.** GNOME Shell, sin depender de X11.

**Qué toma Yuki.** Ideas para el **menú del sistema** («Yuki» sin ventana
enfocada): acciones como Forzar salida y Monitor del sistema. No se copia el
código (GPL-3.0) ni se adopta su modelo de menús genéricos como sustituto del
menú real.

## 6. Firefox nativo — MPL-2.0

**Qué es.** Soporte de menú global dentro de Firefox. Está **apagado por
defecto** y se activa con dos preferencias:
`widget.gtk.global-menu.enabled` y `widget.gtk.global-menu.wayland.enabled`.

**Cómo obtiene los menús.** Firefox 126 lo implementó con
`xdg-dbus-annotation`, un mecanismo que nunca aterrizó upstream; posteriormente
(cambio D243251, disponible desde aproximadamente la versión 138) pasó al
**protocolo appmenu de KDE** (`org_kde_kwin_appmenu` para anunciar el servicio,
`com.canonical.dbusmenu` para el contenido).

**X11 / Wayland.** El soporte existe para los dos. **Verificado (2026-10-26,
Firefox 158.0 de nixpkgs en sesión anidada):** en Wayland las dos preferencias
bastan; no hace falta `appmenu-gtk-module`. Firefox publica su menú por
`org_kde_kwin_appmenu` y el contenido por `com.canonical.dbusmenu`, como Kate.
El binario hace `dlopen` de `libdbusmenu-glib.so.4` y `libdbusmenu-gtk3.so.4`:
el wrapper de nixpkgs pone `libdbusmenu-gtk3-16.04.0` en `LD_LIBRARY_PATH` y
las encuentra; el log del lab no muestra ningún error de carga. La primera
publicación llega unos segundos después del map (primer `GetLayout` con 0
títulos; el árbol real llega por `LayoutUpdated`, ya con `File`, `Edit`, etc.).

**Qué toma Yuki.** Nada de código. Las dos preferencias son **configuración del
usuario** (`~/nixdots`), no del repositorio de Yuki: el compositor y la menubar
no las tocan. El perfil temporal de `dev.sh lab` las escribe en su `user.js`
solo como fixture de pruebas. Firefox es el cliente de aceptación del menú real
en Wayland.

## 7. El ecosistema: el protocolo appmenu de KDE

El par `org_kde_kwin_appmenu` (Wayland) + `com.canonical.dbusmenu` (D-Bus) es
el **estándar de facto** del menú global en Wayland:

- Firefox pasó de `xdg-dbus-annotation` a este protocolo de KDE (cambio
  D243251, disponible desde aproximadamente la versión 138).
- El plugin de JetBrains lo implementa.
- Un PR de menú global de Zed lo adopta.
- Qt 6 lo lista entre sus protocolos Wayland.

Eso no lo convierte en un protocolo de freedesktop, pero sí en la única vía con
clientes reales hoy. Yuki lo elige por eso (ADR 0036), no por afinidad con KDE.

## 8. Otto — MIT

Otto ya está analizado en [Otto y los forks de HBlanqueto](otto-y-forks.md). Dos
piezas tocan este bloque:

| Pieza | Dónde | Tamaño | Qué aporta |
|---|---|---|---|
| `src/state/kde_appmenu.rs` | compositor de Otto | 143 líneas | `org_kde_kwin_appmenu` para el menú global. |
| `app_menu.rs` de `otto-kit` | kit de cliente | 816 líneas | `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`: dirección del menú global y consumo del árbol D-Bus. |

Ambas son **portables** (cero referencias a Skia o layers). Si se copian, el
`NOTICE` se actualiza en el mismo commit, como exige la regla de terceros.
El ADR 0034 ya dejó `otto-kit` como referencia de lógica; el código que se
reutilice aquí es el del compositor y el del cliente D-Bus, no el dibujo.

## Qué se descarta y por qué

| Fuente | Motivo |
|---|---|
| AT-SPI | Menús perezosos, Firefox se queda fijado y exige `QT_ACCESSIBILITY=1`. |
| `com.canonical.AppMenu.Registrar` | Depende de IDs de ventana X11. |
| `xdg-dbus-annotation` | No aterrizó upstream. |
| Leer menús en el compositor | Rompe el ADR 0024 y el reparto compositor ↔ shell. |

El detalle de las alternativas está en el [ADR 0036](../decisiones/0036-menu-global.md).
