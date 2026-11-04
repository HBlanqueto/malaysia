# Encargo a Claude — redraw como niri y menú global de Firefox

Lee antes de tocar código: `AGENTS.md`, `CLAUDE.md`,
`docs/auditoria-claude-2026-11-03.md` y
`docs/claude-2026-11-03-sesion-directa.md`.

**Requisito duro:** estudia el código de niri a fondo, en la fuente y en el rev
exacto, antes de escribir una sola línea. No te quedes con el resumen de la
auditoría, no adivines, no saltes partes y no implementes solo el caso del
shell. Si no puedes explicar el flujo completo de niri (commit → petición →
vblank → frame callbacks → damage → present), sigue leyendo.

## Estado actual

- `cfcf7c0` eliminó el render incondicional por vblank siguiendo el
  `RedrawState` de niri: encola peticiones y solo renderiza en vblank si hay
  petición o animación. Es el modo idle correcto en teoría.
- `41b0661` hace que todo commit de superficie pida redraw.
- `fb45726` hace que cada movimiento del puntero pida redraw para que el cursor
  no vaya lento.
- `f020597` fuerza daño completo de la salida cuando llega un buffer nuevo sin
  damage.
- `22a1d49` es un puente temporal: el vblank y el timer de frame vacío vuelven
  a renderizar si no hay un frame en vuelo. Funciona, pero gasta GPU/batería en
  reposo. **Hay que eliminarlo cuando el modelo correcto esté implementado.**
- `17bf50a` y `9a7eeb2` intentaron limitar el daño del buffer sin damage a la
  superficie; se revirtieron en `38f8060`. Revisa por qué y no repitas el error.
- `1affa55` añade `graphical-session.target` a `malaysia-session.target`; es lo
  que hace funcionar el portal y el tinte de libadwaita. No lo rompas.

## 1. Redraw y frame callbacks

### Síntoma real

En la sesión DRM (ThinkPad L14 Gen2, AMD), la pantalla se congela hasta que el
usuario mueve el ratón o escribe. El shell (`barrierkit`/Freya 0.4.3) repinta
el árbol entero, pero **no adjunta damage**; además espera frame callbacks.
El compositor deja de renderizar/vblank en reposo, no llegan los callbacks y el
shell se queda esperando. El ratón genera daño y desbloquea el bucle.

No basta con pedir redraw en cada commit: hay que garantizar que los frame
callbacks se sirven aunque no haya daño, sin volver al bucle continuo.

### Objetivo

Implementa el modelo de niri completo y correcto:

- `RedrawState` con peticiones coalescidas: petición, frame en vuelo, vblank
  pendiente, animación y callbacks pendientes.
- Renderiza solo cuando haga falta: daño real, animación, cambio de cursor o
  frame callbacks pendientes.
- Sirve los frame callbacks en el momento correcto: un cliente que pide
  callback debe recibirlo aunque no haya daño; si no hay frame que presentar,
  programa un frame vacío al siguiente vblank. Nunca dejes a un cliente
  esperando indefinidamente.
- Un buffer nuevo sin damage se considera superficie completa dañada (no la
  salida completa, salvo que sea inevitable).
- Mantén el cursor fluido (`fb45726`) sin repintar toda la pantalla.
- Conserva el daño parcial (`18d90a5`) y los IDs/commits estables; evita
  repintados completos por elementos nuevos.
- Funciona igual en DRM/udev y en winit anidado.
- Elimina `force_full_damage` y el puente de `22a1d49` si el modelo nuevo los
  hace innecesarios; no dejes código muerto.

### Estudio obligatorio de niri

Antes de implementar, clona niri en `a8562a1` (2026-10-31) y lee el código real,
no solo la auditoría. Como mínimo, entiende y anota:

- `src/niri.rs`: `RedrawState`, `queue_redraw`, `redraw`, `take_redraw`, los
  flags de petición, frame en vuelo, vblank pendiente y animación, y **cada**
  sitio que llama a `queue_redraw`.
- Backend TTY (`src/backend/tty.rs` y módulos relacionados): cuándo programa el
  frame, cuándo hace `queue_frame`, cómo espera el vblank, cómo evita pisar un
  frame pendiente y cómo manda los frame callbacks.
- `src/handlers/compositor.rs` y el resto de handlers: qué commits piden
  redraw y qué pasa con buffers sin damage.
- Frame callbacks: dónde se guardan, cuándo se envían, qué pasa si no hay daño
  y cómo se programa un frame vacío.
- Damage: `OutputDamageTracker`, `damage_since`, IDs/commits estables, scanout
  directo, cursor plane, overlays y cómo se evita repintar toda la salida.
- Entrada: movimiento del puntero y botones, por qué piden redraw y cómo se
  coalescen.
- Winit/nested y cualquier otro backend, para no dejar caminos sin cubrir.

Escribe un resumen del flujo en el chat/commit antes de tocar Malaysia. Si algo
del código de niri no se entiende, consulta también Smithay en el rev fijado y
la documentación de Wayland; no supongas. Adapta las ideas a Malaysia, pero
implementa **todo** lo necesario: no dejes casos, clientes ni backends sin
cubrir. Si algo queda fuera, justifícalo con la fuente exacta y dilo en la
entrega.

### Plan

0. Clona niri en `a8562a1`, lee el flujo completo y escribe el resumen. No
   pases al paso 1 hasta poder explicar commit → petición → vblank → frame
   callbacks → damage → present de principio a fin.
1. Audita el flujo actual completo:
   - `udev.rs`: `after_input`, `queue_redraw`, `render_if_idle`, `on_vblank`,
     `render`, `schedule_repaint`, `dispatch_clients`, fuente del display fd.
   - `session.rs`: `draw`, `finish_empty_frame`, `send_frame_callbacks_for_state`,
     `OutputDamageTracker`.
   - `handlers.rs`: `CompositorHandler::commit`, `on_commit_buffer_handler`,
     `force_full_damage`.
   - `input.rs`: movimiento del puntero, botones, shortcuts.
   - `state.rs`: `needs_redraw`, `force_full_damage`, `animating`.
   - `effects.rs`, `chrome.rs`, `popup_fade.rs`, `theme_fade.rs`, `capture.rs`.
   - `barrierkit` (`SurfaceWindow::request_frame_callback`, presentación Vulkan)
     y Freya 0.4.3: confirma quién adjunta damage y quién espera callbacks.
2. Escribe el modelo de estado y las transiciones antes de tocar el render.
3. Implementa en DRM y winit con el mismo criterio.
4. Añade un frame vacío programado solo cuando haya callbacks pendientes, no
   un bucle continuo.
5. Trata buffer nuevo sin damage como daño de esa superficie. Si el cliente es
   el shell, que repinta todo su buffer, el daño será su superficie completa.
6. Prueba y mide:
   - `./dev.sh check`.
   - `./dev.sh lab` (winit).
   - `./dev.sh drm-vkms` para el path DRM sin tocar el panel ni el input real.
   - Sesión DRM real solo con el usuario delante.
   - Reloj/menú/animaciones sin tocar el ratón.
   - Shell Freya (sin damage), GTK4/Nautilus/libadwaita, Firefox, Marea/Vulkan,
     clientes wl_shm.
   - Cursor fluido; sin repintado completo por commit; CPU/GPU en reposo
     comparable a niri.
   - Frame callbacks: `WAYLAND_DEBUG=1` o un probe que cuente callbacks.
7. Cuando el modelo nuevo esté verde, borra el puente de `22a1d49` y el
   `force_full_damage` de `f020597` si ya no hacen falta. Actualiza el MD
   `docs/claude-2026-11-03-sesion-directa.md` con el resultado.

### Criterios de aceptación (redraw)

- La pantalla avanza sola (reloj, animaciones, commits de apps) sin input.
- No hay congelación que necesite mover el ratón.
- En reposo no hay bucle continuo ni repintado de toda la salida.
- Los frame callbacks se sirven siempre; ningún cliente se queda esperando.
- El daño parcial sigue funcionando; el shell sin damage no obliga a repintar
  toda la salida.
- Cursor fluido y sin regresión de `fb45726`.
- DRM y winit correctos.
- Sin workarounds, sin código muerto, sin `force_full_damage` si el modelo lo
  sustituye.
- La entrega demuestra el estudio de niri: archivos/funciones exactas,
  explicación del flujo y ninguna parte omitida sin justificar.

### Resultado (Claude, 2026-11-04, `aa98503`)

**Flujo de niri `a8562a1`** (Smithay `8a27023`):

1. **Commit.** `CompositorHandler::commit` (`src/handlers/compositor.rs:56`)
   aplica el buffer y pide `queue_redraw(output)` según la raíz: toplevel,
   popup, layer o lock. Para el cursor y el DnD pide `queue_redraw_all`.
   No mira el damage.
2. **Petición.** `RedrawState::queue_redraw` (`src/niri.rs:699`) solo cambia
   el estado: `Idle`, `Queued`, `WaitingForVBlank{redraw_needed}` o
   `WaitingForEstimatedVBlank[AndQueued]`.
3. **Fin de vuelta del bucle.** `src/main.rs:279` llama a
   `refresh_and_flush_clients` (`src/niri.rs:819`). Esa función llama a
   `redraw_queued_outputs` (`:3981`) y hace `flush_clients`.
4. **Redraw.** `Niri::redraw` (`:4888`) renderiza y siempre termina con
   `send_frame_callbacks` (`:5342`). Ese envío manda como mucho un callback
   por superficie y valor de `frame_callback_sequence`.
5. **Render en TTY** (`src/backend/tty.rs:1882`):
   - Con damage: `queue_frame`, `WaitingForVBlank` y la secuencia sube en 1
     (`:2014`).
   - Sin damage: `queue_estimated_vblank_timer` (`:3064`).
6. **VBlank** (`:1638`): presenta el feedback. Si hay petición o animación,
   pide otro redraw; si no, manda los callbacks.
7. **Timer estimado** (`:1826`): la secuencia sube en 1 y después pide
   redraw o manda los callbacks.
8. **Winit** (`src/backend/winit.rs:321`): presenta al momento y la
   secuencia sube en cada render.
9. **Input:** `on_pointer_motion` termina en `queue_redraw_all`
   (`src/input/mod.rs:2899`).

**Qué cambió en Malaysia:**

| Pieza | Antes | Ahora |
|---|---|---|
| Peticiones en udev | Solo las atendían display, input y apariencia; un commit tras soltarse un blocker de dmabuf quedaba sin render. | `redraw_queued_and_flush` al final de cada vuelta del bucle. |
| Flush | Solo tras despachar clientes o input; los callbacks del vblank esperaban. | Al final de cada vuelta, en udev y en winit. |
| Frame vacío | Callbacks al momento, más render continuo (`22a1d49`). | Timer al vblank estimado; un callback por ciclo. |
| Cursor | Sin frame callbacks. | Los recibe, también con la sesión bloqueada. |
| Presentación | Se recogía en el vblank. | Se toma al encolar el frame (udev) o al enviarlo (winit). |
| Buffer sin damage | `force_full_damage` repintaba la salida. | Daña su superficie entera (Smithay ignora el damage vacío, `renderer/utils/mod.rs:221-226`). |

**Por qué se congelaba:** Mesa adjunta damage completo en cada present
Vulkan (`wsi_common_wayland.c:3352-3375`, mesa-26.2.4). Lo que barrierkit
commitea sin damage es su petición de frame (`platform/surface.rs:98-104`).
`force_full_damage` no tocaba la causa.

**Casos fuera de este cambio, y por qué:**

- **Timer de 1 s para superficies invisibles** (`src/niri.rs:2708`,
  `:5432`). Malaysia no filtra por visibilidad: manda callbacks a todas las
  superficies mapeadas de su única salida. Con la sesión bloqueada solo los
  reciben el lock y el cursor; así era antes y la especificación lo permite.
- **Icono de DnD.** Malaysia no lo dibuja (`handlers.rs`, `dnd_requested`
  ignora `_icon`).
- **VRR, `VBlankThrottle` y varias salidas.** udev maneja un solo panel
  (`find_panel`) y no tiene VRR.
- **Feedback en un frame vacío.** Se descarta, como antes. niri lo deja
  pendiente, pero en reposo no llega otro frame y el cliente esperaría sin
  fin.
- **Cursor plane, scanout directo, overlays e IDs estables (`18d90a5`).** No
  cambian.

**Medido** con `./dev.sh frames` (winit sobre weston headless, llvmpipe,
60 Hz):

| Código | Callbacks/s sin damage | CPU en reposo | CPU con el probe (2 s) |
|---|---|---|---|
| `32fd711` | 0 (congelado) | 0 ticks/5 s | 1 tick |
| `aa98503` | 59,9 | 0 ticks/5 s | 9 ticks |

`./dev.sh check` no se pudo ejecutar entero porque el dev shell necesita un
tarball de Skia que el proxy bloquea. Se pasaron `cargo fmt --check`,
`cargo clippy -D warnings` (compositor y probes) y `cargo test -p
malaysia-compositor`: 265 + 15 tests.

En este entorno sin GPU, `a_failed_create_immed_is_drained_and_posts_the_protocol_error`
se cuelga y `screencopy-window` falla. Las dos cosas pasan igual en
`32fd711`.

Pasan los probes `presentation`, `smoke`, `activation`, `pointers` y `move`.

**DRM validado** el 2026-11-04 en la sesión real (ThinkPad L14 Gen2, Ryzen 5
PRO, amdgpu) con `3e6e899`: la pantalla avanza sin input. El compositor nuevo
solo se carga al iniciar una sesión nueva después del rebuild.

## 2. Menú global de Firefox

### Síntoma real

En `./dev.sh lab` el menú de Firefox se exporta; en la sesión DRM directa no.
El usuario tiene el menú global de Malaysia en la barra, pero para Firefox no
aparece.

### Lo que ya se hizo

- `dotfiles` escribe `/etc/firefox/policies/policies.json` con:
  - `widget.gtk.global-menu.enabled = true`
  - `widget.gtk.global-menu.wayland.enabled = true`
- `dev.sh` crea un perfil temporal de Firefox con esos mismos prefs y lo lanza
  con `MOZ_ENABLE_WAYLAND=1`, `GDK_BACKEND=wayland`, `QT_QPA_PLATFORM=wayland`
  y `XDG_CURRENT_DESKTOP=Malaysia`.
- En la sesión directa falta comprobar si Firefox corre en Wayland o XWayland y
  si el perfil real está leyendo las políticas.

### Qué hay que comprobar y arreglar

1. Firefox debe correr en **Wayland**, no XWayland:
   - `about:support` → Window Protocol debe decir `wayland`.
   - Si no, exporta `MOZ_ENABLE_WAYLAND=1` en la sesión/desktop file de Malaysia
     (o en el wrapper del paquete) y verifica que el `.desktop` no lo pise.
2. Prefs reales:
   - `about:config`: `widget.gtk.global-menu.enabled` y
     `widget.gtk.global-menu.wayland.enabled` en `true`.
   - `about:policies`: confirma que se leyó `/etc/firefox/policies/policies.json`
     y que no hay errores.
   - Si el perfil se creó antes de las políticas, reinicia Firefox; si hace
     falta, escribe un `user.js`/perfil gestionado desde home-manager.
3. Compara con lab: `dev.sh` usa perfil temporal y variables de entorno; la
   sesión directa no. Reproduce el entorno mínimo que hace falta.
4. Lado compositor/shell:
   - `crates/malaysia-compositor/src/appmenu.rs` y `gtk.rs` (`x-adw-app-menu`,
     `gtk_shell1`, `org_kde_kwin_appmenu`).
   - `crates/malaysia-shell/src/menubar/gtkmenus.rs` y `barrier.rs`.
   - Busca en el log `appmenu set`, `gtk_menus`, `x-adw-app-menu` al abrir
     Firefox. Si el compositor no recibe el appmenu, el problema es Firefox/GTK;
     si lo recibe y la barra no lo pinta, es la shell.
5. Comprueba si Firefox necesita `libdbusmenu-gtk3`, `GTK_MODULES` o el puente
   GTK del portal de Malaysia; documenta lo que falte.
6. Implementa el arreglo en el sitio correcto (entorno de sesión, wrapper de
   Firefox, módulo NixOS o shell), no en `dev.sh`, para que el laboratorio y la
   sesión directa usen el mismo camino.

### Criterios de aceptación (Firefox)

- Firefox exporta su menú a la barra de Malaysia en la sesión DRM directa, sin
  perfil temporal de `dev.sh`.
- El menú aparece y se actualiza al cambiar de ventana/pestaña.
- No se rompe el menú de otras apps GTK/libadwaita.
- La solución no depende de `dev.sh`.

### Resultado (Claude, 2026-11-04)

Firefox 158 (nixpkgs `31ecbaca`) se probó dentro de Malaysia anidado con el
entorno de la sesión directa:

- Con `WAYLAND_DISPLAY` y `DISPLAY`.
- Sin `MOZ_ENABLE_WAYLAND` ni `GDK_BACKEND`.
- Con un bus de sesión propio.

Resultados:

| Caso | Resultado |
|---|---|
| Prefs activos y el nombre `com.canonical.AppMenu.Registrar` con dueño | Firefox va por Wayland y el compositor registra `appmenu set window=0 service=:1.1 path=/com/canonical/menu/0`. |
| Mismo caso, sin dueño del Registrar | No llega ningún `appmenu`. |

Firefox exige estas condiciones, comprobadas en su código:

1. **Wayland.** `IsWaylandEnabled` (`toolkit/xre/nsAppRunner.cpp`) elige
   Wayland con GTK ≥ 3.24.30 aunque exista `DISPLAY`. Solo
   `MOZ_ENABLE_WAYLAND=0` o `GDK_BACKEND=x11` lo llevan a XWayland. Por
   XWayland no hay menú: el Registrar por XID no está implementado.
2. **Prefs.** `widget.gtk.global-menu.enabled` y
   `widget.gtk.global-menu.wayland.enabled` deben estar activos al abrir la
   ventana (`widget/gtk/NativeMenuSupport.cpp`).
3. **Política.** Firefox lee `/etc/firefox/policies/policies.json`
   (`xpcom/io/SpecialSystemDirectory.cpp`) antes que la de `distribution/`
   del paquete. La forma corta de `Preferences` vale para `widget.*` y deja
   el pref bloqueado (`Policies.sys.mjs`).
4. **Registrar.** `DBusMenuBar::OnNameOwnerChanged` (`widget/gtk/NativeMenuGtk.cpp`)
   no manda `org_kde_kwin_appmenu` hasta que el Registrar tiene dueño en
   su bus. En Malaysia lo toma la menubar (`menubar: Registrar acquired`).
5. **dbusmenu.** Hacen falta `libdbusmenu-glib.so.4` y
   `libdbusmenu-gtk3.so.4`. El wrapper de nixpkgs ya las añade
   (`libdbusmenu-gtk3`).

No hacen falta `GTK_MODULES` ni el puente GTK del portal.

El lab ya no escribe `user.js` ni fuerza `MOZ_ENABLE_WAYLAND` ni
`GDK_BACKEND`. Su Firefox recibe los prefs por la misma política del sistema
que la sesión, así que si falla en la sesión también falla en el lab.

No se pudo reproducir el fallo de la sesión fuera de la máquina. Para
encontrar cuál de las condiciones falla:

1. Comprueba el Registrar con
   `journalctl --user -b -u malaysia-shell@menubar | grep Registrar`. Debe
   decir `acquired`, no `already owned`.
2. En Firefox:
   - `about:support`, Window Protocol: `wayland`.
   - `about:policies`: sin errores.
   - `about:config`: los dos prefs en `true` y bloqueados.
   
   Si Firefox ya estaba abierto antes del `nixos-rebuild` que añadió la
   política, ciérralo del todo y ábrelo otra vez.
3. Mira el log del compositor al abrir Firefox. Si no aparece `appmenu set`,
   falla Firefox o el Registrar. Si aparece y la barra no lo pinta, falla
   la shell.

## Reglas

- `CLAUDE.md`: commits directos a `main`, `git pull --ff-only` antes de push,
  sin ramas ni PRs, nunca `--force`.
- `AGENTS.md`: diff mínimo, conventional commits, comentarios solo si explican
  un porqué que el código no puede expresar, documentación y comentarios en
  español.
- No rompas el portal: `graphical-session.target` en `malaysia-session.target`
  (`1affa55`) es necesario para libadwaita; verifica que sigue activo.
- El usuario está en la sesión DRM y el input va lento: no hagas builds ni
  pruebas pesadas sin avisar.

## Entrega

Commits pequeños en `main` con:
1. el modelo de redraw nuevo y el puente eliminado;
2. el arreglo del menú global de Firefox;
3. las mediciones y referencias exactas de niri.

Cierra con 1–3 líneas: qué cambió, qué se midió y qué queda.
