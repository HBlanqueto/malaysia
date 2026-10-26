# 0040 — Menú global real: arquitectura propia

Estado: **aceptado** (2026-10-25). Solo diseño: 14d no tiene código todavía.

## Contexto

14a (popups), 14b-1 (menús de escritorio) y 14c (menús de la app enfocada con
respaldo de `.desktop`) están implementados. El [ADR 0036](0036-menu-global.md)
fijó el par `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`, pero dejó abiertos
el canal hasta la barra, el cliente D-Bus, la carga del árbol y la composición
de títulos. Esta decisión los cierra para 14d.

Las referencias se han estudiado como comportamiento, nunca como código: Otto
(`src/state/kde_appmenu.rs`, `components/otto-bar/src/{appmenu,dbusmenu}.rs`),
noctalia-appmenu, Fildem, Orbit, helloSystem y ShiroOSL. La implementación de
Yuki se escribe desde las especificaciones —el XML de
`org_kde_kwin_appmenu` de plasma-wayland-protocols (LGPL-2.1) y la interfaz
`com.canonical.dbusmenu`— y desde este ADR. **No se copia código de ninguna
referencia**, así que el `NOTICE` no cambia en 14d.

## Decisión

### 1. Fuentes del menú, por prioridad

1. **`org_kde_kwin_appmenu`** (Wayland). El compositor anuncia
   `org_kde_kwin_appmenu_manager` y guarda **por superficie** el par (servicio
   D-Bus, ruta). Es la vía de Qt/KDE (Kate, Dolphin), Firefox, el plugin de
   JetBrains y el PR de menú global de Zed. El compositor **nunca** habla
   dbusmenu ni dibuja menús (ADR 0023/0025/0034).
2. **`com.canonical.AppMenu.Registrar`**, servido por la barra. Es la vía de GTK
   con `appmenu-gtk-module` y de apps XWayland futuras. La barra es dueña del
   nombre en la sesión y casa la ventana enfocada **por pid**, no por el id X11
   que trae el registro. Es la fase 14d-3.
3. **`gtk_shell1` → `org.gtk.Menus`**, GTK 3/4 nativo por Wayland. Queda para
   una fase posterior; no entra en 14d.
4. **Respaldo de 14c** (`focused.rs`), sin cambios: si no hay menú real, la
   consulta vence o el servicio desaparece, la barra sigue mostrando
   {App} · Ventana · Ayuda.

**Sin AT-SPI.** No es una API de menús sino el árbol de accesibilidad: exige
`QT_ACCESSIBILITY=1` en Qt, en Firefox el menú solo se publica cuando se abre
visualmente y además fija su barra en pantalla, y los menús perezosos solo
existen mientras están abiertos. Es frágil y no sirve como fuente principal.

### 2. De la app a la barra: Infinity v2

`infinity_window_manager_v1` sube a **versión 2** con la misma restricción a
clientes de confianza del [ADR 0039](0039-infinity-v1-gestion-de-ventanas-para-la-shell.md)
(`can_view` por `/proc/<pid>/exe`). Añade:

```xml
<event name="app_menu" since="2">
  <arg name="toplevel" type="object"
       interface="zwlr_foreign_toplevel_handle_v1"/>
  <arg name="service" type="string"/>
  <arg name="path" type="string"/>
</event>
```

`service` y `path` vacíos significan que esa ventana ya no tiene menú; la barra
suelta el que tuviera. El evento se emite:

- cuando cambia el par de la superficie (incluido el `release` del
  `org_kde_kwin_appmenu`);
- al crear el handle de un toplevel, y al bindear el manager, para que la barra
  reciba el estado de las ventanas que ya existían.

No se emite al cambiar el foco: la barra ya conoce la ventana activa por el
`State` de `zwlr_foreign_toplevel_management_v1`; solo necesita el par de cada
ventana. Se envía únicamente a los clientes que hayan bindeado la versión 2.

**Objeto de otro protocolo.** Pasar un `zwlr_foreign_toplevel_handle_v1` es
legal: es un objeto que el receptor ya posee. `wayland-scanner` resuelve la
referencia contra el módulo hermano del protocolo generado, y los `use
...::server::*` / `...::client::*` de `infinity.rs` ya traen el módulo de wlr
(servidor en `crates/yuki`, cliente en `celestial-kit`). Solo hay que respetar
la vida del handle: se envía mientras `is_alive()` y no se guarda como
identidad del menú, solo se usa en el envío. Si en 14d-1 apareciera un problema
real de generación o de orden de bind, la alternativa sería publicar el par en
`window.domain` junto al `app_id`; se descarta por ahora porque dos ventanas de
la misma app comparten `app_id` y el fichero no distingue cuál tiene el foco.

### 3. Proceso y concurrencia

- `zbus = "5"` entra en `yuki-shell` con `zbus.workspace = true`. Sus features
  por defecto son `async-io`; **no se añade tokio**.
- Un **hilo dedicado** posee **una única conexión de sesión**. Ese hilo corre
  `async_io::block_on` (ya es dependencia de `yuki-shell`) y atiende una cola
  FIFO de peticiones. Ninguna llamada D-Bus ocurre en el hilo del render.
- La barra manda direcciones, aperturas y activaciones por un canal; el hilo le
  devuelve **snapshots inmutables** (`Arc<MenuSnapshot>`) por otro.
- El estado de Freya guarda el último `Arc`; un `Timers::after` (que ya hace
  ping al bucle calloop) drena el canal y actualiza el estado. El render solo
  lee el `Arc`: nunca espera a D-Bus.
- Un contador de generación descarta respuestas que llegan después de que el
  foco haya cambiado.
- El módulo nuevo es `crates/yuki-shell/src/menubar/dbusmenu.rs`. `appmenu.rs`
  ya existe y es el menú del escritorio; no se renombra.

```
render (calloop) ── comandos ──► hilo D-Bus (zbus, 1 conexión)
      ▲                                  │
      └──── Arc<Snapshot> por canal ◄────┘
```

### 4. Carga perezosa y caché

La caché es por ventana (`service`, `path`, `WindowId`) y guarda la **revisión**
que devolvió dbusmenu, el primer nivel y los subárboles ya pedidos.

| Cuándo | Llamada | Efecto |
|---|---|---|
| Cambia el foco a una ventana con menú | `GetLayout(0, 1, [])` | Solo el primer nivel; si ya está en caché, no se llama. |
| Se abre un submenú | `AboutToShow(id)` y, si devuelve `true` o no está en caché, `GetLayout(id, 1, [])` | Subárbol de ese id. |
| El puntero pasa por un título | prefetch del primer submenú de ese título | Deja listo el caso más probable. |
| `LayoutUpdated(rev, parent)` | — | Invalida solo el subárbol `parent` si `rev` es mayor que la cacheada. |
| `ItemsPropertiesUpdated` | — | Actualiza los items en sitio, sin refetch. |
| Se abre Ayuda | una sola `GetLayout(0, -1, [])` en segundo plano | Árbol completo cacheado por revisión para el buscador. |

- **Debounce de 50 ms** para ráfagas de señales y cambios de foco: se agrupan
  en una sola consulta.
- **Timeout de 500 ms por llamada.** Si vence, se muestra el respaldo de 14c y
  se reintenta perezosamente en la siguiente apertura. Un submenú que vence
  conserva lo cacheado; sin caché se abre con «No disponible» deshabilitado.
- La petición de Ayuda usa el mismo timeout y no bloquea el teclado: si tarda,
  el buscador sigue con lo que ya tiene y los resultados de dbusmenu aparecen
  cuando llegan.

### 5. Mapeo dbusmenu → `celestial_kit::menu::MenuItem`

El `id` numérico de cada item se conserva en `Action.id` (decimal) y en el
`Submenu.id` que añade 14d-2; sin él no se puede llamar a `AboutToShow`.

| Propiedad dbusmenu | Campo de `MenuItem` | Regla |
|---|---|---|
| `label` | `label` | Se quita el `_` de mnemónico; `__` se convierte en `_`. No se pinta subrayado por ahora. |
| `enabled` | `enabled` | Por defecto `true` si falta. |
| `visible` | — | `false` omite el item y su subárbol. |
| `type = separator` | `MenuItem::Separator` | |
| `children-display = submenu` | `MenuItem::Submenu` | Los hijos se piden al abrir. |
| `toggle-type` + `toggle-state` | `check` | `checkmark` → `Checkbox`, `radio` → `Radio`; activo si `toggle-state == 1`. |
| `shortcut` (`aas`) | `shortcut` | Solo la primera combinación, con `⌃⌥⇧❖` y **nunca `⌘`**; el formateo lo hace el kit. |
| `icon-name` | `icon` | Solo si el nombre está en el conjunto estático de iconos de la barra; si no, `debug!`. |
| `icon-data` | `icon` | ARGB32. Requiere que 14d-2 amplíe el icono del kit a mapa de bits; hasta entonces, `debug!`. |
| resto | — | Se ignora con un `debug!` (una vez por propiedad), incluidos `disposition`, `child-display` y las combinaciones de atajo adicionales. |

Las propiedades no soportadas no rompen el menú: el item se dibuja con lo que
sí se entendió.

### 6. Composición de la barra

Con ventana enfocada la barra queda:

```
❄ · {App} · menús de la app · Ventana · Ayuda
```

`{App}` es el menú de app del 14c; **Ventana** y **Ayuda** son nuestros. Los
títulos de la app se normalizan (minúsculas, sin acentos) para decidir:

- **Ventana / Window:** sus entradas van al final de **nuestro Ventana**, tras
  un separador.
- **Ayuda / Help:** sus entradas van inmediatamente debajo del buscador de
  **nuestro Ayuda**; también entran en el índice del buscador.
- **El nombre de la app:** sus entradas se añaden al final de nuestro menú
  `{App}`, tras un separador.
- **Cualquier otro título:** se inserta en su orden, después de `{App}` y antes
  de **Ventana**.

Nunca hay dos títulos iguales en la barra. Los títulos se muestran completos
(decisión ya tomada); si aun así no caben, la barra no los trunca: el exceso se
recorta por el borde derecho y puede dejar fuera el reloj. No hay menú de
desbordamiento en 14d; es un límite conocido, no una solución.

### 7. Activación

`Event(id, "clicked", variant vacío con un `i32` 0, timestamp)` sobre el
servicio y la ruta del snapshot que abrió el menú, **no** sobre el que esté enfocado
al pulsar: el foco puede cambiar con un popup abierto. Después se cierran los
menús y el foco sigue en la app (el compositor ya lo devuelve al cerrar el
popup, 14a). Los atajos solo se muestran: los teclea la app; la barra nunca
sintetiza teclas.

### 8. Desactivación

`menubar.app_menus = true|false` en la configuración de la shell; por defecto
`true`. Con `false` la barra no consulta dbusmenu ni pide layouts y, desde
14d-3, suelta `com.canonical.AppMenu.Registrar` para que las apps GTK vuelvan a
pintar su propia barra. El catálogo canónico y el lector tipado se actualizan en
14d-2: hoy `yuki-shell` no tiene lector de configuración y `menubar.autohide`
está en el catálogo sin implementar.

### 9. Rendimiento medible (aceptación de 14d-2)

| Criterio | Umbral |
|---|---|
| Abrir un menú de primer nivel desde la caché | < 16 ms |
| Primera apertura en frío, app local | < 100 ms |
| Llamadas D-Bus en el hilo de render | 0 |

El tercero se comprueba con un contador por sesión y una aserción en debug; el
contador se registra al cerrar la barra.

### 10. Fases

- **14d-1 — Compositor.** `org_kde_kwin_appmenu` (XML tal cual, LGPL-2.1) e
  Infinity v2. La barra no cambia salvo loguear el evento `app_menu`.
- **14d-2 — Cliente y barra.** Hilo D-Bus con zbus, caché por ventana, carga
  perezosa, fusión de títulos y buscador de Ayuda. Incluye las extensiones
  mínimas del kit que necesitan el mapeo (id de submenú, icono de mapa de bits
  y callback de apertura).
- **14d-3 — Registrar y aceptación.** `com.canonical.AppMenu.Registrar`
  servido por la barra y prueba con Kate y Firefox. Las dos preferencias de
  Firefox viven en `~/nixdots`, nunca en el repo.
- **14e — Apps propias.** CelestialKit exporta dbusmenu por la misma vía, con
  la app Contador de los mockups 14 y 15.

### 11. Qué tomamos de cada referencia

| Referencia | Licencia | Idea, sin código copiado |
|---|---|---|
| Otto | MIT | Dirección por superficie, refresco cuando el menú llega después del foco, una conexión compartida y soltar el Registrar al desactivar. |
| noctalia-appmenu | Apache-2.0 | El menú de respaldo cuando la app no entrega menú (ya en 14c). |
| Fildem | GPL-3.0 | Separar «exportar el menú» de «dibujarlo» y el HUD como evolución futura. |
| Orbit | GPL-3.0-or-later | GTK puede exportar su menú en Wayland; los mapeos de atajos por app como idea futura. |
| helloSystem | GPL-3.0 | Orden y jerarquía de los títulos de primer nivel. |
| ShiroOSL | GPL-3.0 | Ideas para el menú del sistema sin ventana enfocada. |

Ninguna fila implica código, estructuras ni ficheros copiados: solo
comportamiento y decisiones de diseño.

## Enmienda (14d-1)

Verificadas las fuentes de Qt 6 y Firefox, ambos exigen que
`com.canonical.AppMenu.Registrar` tenga dueño para exportar el menú, también en
Wayland (`QGenericUnixTheme::isDBusGlobalMenuAvailable` y
`DBusMenuBar::OnNameOwnerChanged`). Por eso la barra es dueña del nombre
**desde 14d-2**, con `menubar.app_menus = true`; 14d-3 conserva el casado por
pid de `RegisterWindow` para GTK. El stub de 14d-1 solo existe para la
aceptación.

## Consecuencias

- **El ADR 0036 queda superado en dos puntos.** El par appmenu viaja por
  Infinity v2, no por `window.domain`, y la implementación es propia: no se
  porta código de Otto, así que no hay actualización de `NOTICE` por 14d. El
  resto del 0036 (protocolo, reparto compositor ↔ shell, respaldo) sigue vivo.
- **`ToplevelInfo` gana el par del menú** (`Option<AppMenuAddress>`), que la
  barra lee de la entrada `active`; el compositor lo rellena desde el estado por
  superficie. No se adivina por `app_id`.
- **14d-2 toca tres costuras del kit/barra:** `MenuItem::Submenu` necesita `id`
  y `MenuView` un gancho para pedir los hijos al abrir; `MenuItem::icon` hoy es
  `Option<&'static str>` de SVG y necesita admitir un mapa de bits para
  `icon-data`; `MenuTitleSpec::on_activate` es un `fn(String)` sin contexto y
  pasa a `Rc<dyn Fn(String)>` para que el título sepa a qué servicio y ruta
  activar. Además `MAX_TITLES = 8` se queda corto con menús reales.
- **14d-3 necesita el pid del toplevel enfocado.** Ni `ToplevelInfo` ni
  `app_menu` lo llevan; se añadirá al estado de foco (`window.domain`), que la
  barra ya vigila, para casar el registro del `AppMenu.Registrar` por pid.
- **La configuración de la shell es nueva.** `menubar.app_menus` exige un
  lector de `menubar.domain` en `yuki-shell` y su entrada en el catálogo
  canónico; ambas cosas entran en 14d-2.
- **Sin privilegios nuevos.** El global sigue siendo solo para clientes de
  confianza, pero el par (servicio, ruta) lo elige la app: puede apuntar a otro
  servicio cualquiera del usuario. No es una frontera de seguridad; es una
  sesión de usuario.
- **Fallo del servicio:** si la app muere o su nombre desaparece, se evicta su
  caché, se suelta el snapshot y la barra vuelve al respaldo de 14c sin
  bloquearse.
- **14e usa el mismo camino.** Una app de CelestialKit exporta dbusmenu y se
  comporta como cualquier cliente; no hay atajo privado por Infinity.

## Referencias

- [`0036`](0036-menu-global.md) — menú global: protocolo y reparto (superado en
  el canal y en el port de Otto).
- [`0039`](0039-infinity-v1-gestion-de-ventanas-para-la-shell.md) — Infinity v1,
  confianza y handles de wlr.
- [`0021`](0021-superficie-de-configuracion-y-control.md) — configuración y
  control por D-Bus.
- [`0023`](0023-los-componentes-del-shell-corren-como-procesos-separados.md) —
  un proceso por componente.
- [`0024`](0024-stack-de-render-del-toolkit.md) — stack de render del toolkit.
- [`0034`](0034-celestialkit-sobre-freya.md) — CelestialKit sobre Freya.
- [`investigacion/global-menu.md`](../investigacion/global-menu.md) — los
  proyectos de referencia.
- [`bloques/14-menu-global.md`](../bloques/14-menu-global.md) — estado de 14a a
  14e.
- [`arquitectura/menubar.md`](../arquitectura/menubar.md) — el componente.
- `protocols/infinity-window-manager-v1.xml` — el protocolo privado.
