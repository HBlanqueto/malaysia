# Encargo a Claude — redraw como niri y menú global de Firefox

Lee antes de tocar código: `AGENTS.md`, `CLAUDE.md`,
`docs/auditoria-claude-2026-11-03.md` y
`docs/claude-2026-11-03-sesion-directa.md`.

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

### Referencia: niri

Estudia niri en el rev fijado de la auditoría (`a8562a1`, 2026-10-31):

- `RedrawState` y `queue_redraw` en `src/niri.rs` y su uso en el backend TTY.
- Cómo distingue petición de frame, frame en vuelo, vblank pendiente y
  animación.
- Cómo sirve frame callbacks a clientes que no adjuntan damage.
- Cómo calcula y aplica el damage sin repintar la salida entera.
- Cómo evita `queue_frame` pisando un frame pendiente.

Adapta las ideas, no copies ciegamente: el shell de Malaysia y Smithay tienen
sus propias particularidades.

### Plan

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
