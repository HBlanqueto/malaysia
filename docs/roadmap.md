# Roadmap de bloques

El compositor se construye por bloques. Cada bloque es un incremento funcional con
una prueba de aceptación. Este archivo es la lista canónica de qué son los bloques;
los documentos por bloque en el directorio `bloques/` registran qué construyó de
hecho cada bloque, incluyendo cualquier divergencia deliberada con respecto al resumen.

## Bloque 0 — Andamiaje

- Workspace de Cargo con `yuki` y `yuki-design` (vacío)
- Smithay anclado a un `rev` fijo, con las features: `backend_winit`, `renderer_gl`,
  `wayland_frontend`, `desktop`
- `tracing` + `tracing-subscriber` con filtrado basado en variables de entorno
- Tipos de coordenadas: `Logical`, `Buffer`, `Physical` como tipos distintos desde el commit 1
- Compila y se ejecuta, sin hacer nada

## Bloque 1 — Conseguir que un cliente se conecte

- `State` que contiene `CompositorState`, `ShmState`, `SeatState`, `XdgShellState`,
  `OutputManagerState`, `DataDeviceState`
- Bucle de eventos calloop más un socket de Wayland, con `WAYLAND_DISPLAY` exportado al hijo
- `wl_compositor`, `wl_subcompositor`, `wl_shm` con `Argb8888`/`Xrgb8888`
- `wl_output` con `geometry`, `mode`, `scale`, `done`
- `wl_seat` con capabilities y un keymap xkb válido
- `xdg_wm_base` respondiendo `ping`/`pong`
- Aceptación: `weston-info` enumera todos los globals

## Bloque 2 — Una ventana en pantalla

- Backend winit y un bucle de renderizado básico
- `CompositorHandler::commit` aplicando correctamente el estado de doble buffer
- `XdgShellHandler::new_toplevel` → `configure` inicial antes de que el cliente adjunte un buffer
- `ack_configure` → mapeo de la ventana
- Un módulo dedicado que envuelve `desktop::Space` — ningún otro archivo lo llama directamente
- Importación y renderizado de buffers `wl_shm`
- Aceptación: `weston-simple-shm` muestra su patrón

## Bloque 3 — Entrada

- Foco de teclado al mapear, `wl_keyboard.enter`/`leave`
- Keymap, modificadores y repetición de teclas
- Foco de puntero mediante hit-testing, `motion`, `button`, `axis`
- Cursor: el `wl_pointer.set_cursor` del cliente y un cursor por defecto
- Contabilidad de serials — un único contador central, nunca serials fabricados
- Aceptación: `foot` se abre, muestra un prompt, acepta escritura y el layout es correcto

## Bloque 4 — Ventanas manipulables

- Arrastrar para mover y `xdg_toplevel.move`
- Redimensionado mediante `xdg_toplevel.resize` y sus bordes
- Cerrar: `close` y limpieza cuando el cliente muere
- Apilado y click-to-focus
- Aceptación: abrir y cerrar 50 ventanas sin superficies huérfanas y sin crecimiento de memoria

## Bloque 5 — Popups

- `XdgShellHandler::new_popup` y `PopupSurface` (`wayland_frontend`)
- Un árbol de popups propiedad de `yuki-scene`; el `desktop::PopupManager` de
  Smithay está detrás de la feature `desktop`, que el ADR 0015 mantiene desactivada
- Posicionamiento contra el rectángulo decorado del padre desde ese árbol
- Ajustes de restricciones (flip, slide, resize) contra el área utilizable del
  monitor, pasando el rectángulo objetivo relativo al padre a
  `PositionerState::get_unconstrained_geometry` (`wayland_frontend`); el
  `get_geometry` sin ajustar también está disponible
- `configure` de popup antes de mapear, portando la geometría ajustada
- Grabs de teclado y puntero propiedad del compositor, con validación de serials;
  el `PopupKeyboardGrab`/`PopupPointerGrab` de Smithay están detrás de `desktop`
- Un clic fuera cierra desde la hoja hasta la raíz; `Esc` cierra un nivel
- Submenús anidados
- Aceptación: un menú de `foot` contra el borde inferior se voltea hacia arriba; un submenú más un
  clic fuera cierra ambos; matar al cliente con un popup abierto no deja huérfanos

## Bloque 6 — Portapapeles

- `wl_data_device_manager` y selección primaria (`zwp_primary_selection_device_manager_v1`)
- El foco del portapapeles y de la selección primaria sigue al foco de teclado
  (`set_data_device_focus` / `set_primary_focus`)
- Aceptación: copiar en `foot` y pegar en otra instancia

## Bloque 7 — linux-dmabuf y explicit sync

- `zwp_linux_dmabuf_v1` (v4+, con feedback) en el compositor; el `main_device`
  del feedback es el render node del dispositivo del renderer GLES de winit, y
  los formatos son los que reporta el propio renderer. Sin `wl_drm`
- Explicit sync (`wp_linux_drm_syncobj_manager_v1`, solo si el dispositivo
  soporta `syncobj_eventfd`) más el sync implícito (un *blocker* pre-commit que
  espera a que el dmabuf sea legible); un solo camino de commit
- La shell presenta por EGL/dmabuf en lugar de copiar a `wl_shm`
- Aceptación: wezterm y Firefox acelerados dentro de Yuki anidado; la shell sin
  copias por CPU. Ver `bloques/07-linux-dmabuf-y-explicit-sync.md`.

## Bloque 8 — Direct Session (DRM/KMS)

- Backend DRM/KMS atómico + libseat + libinput, en paralelo a winit
- Cambio de VT, hotplug y direct scanout
- Aceptación: la sesión arranca sobre el panel real (Pendiente)

## Bloque 9 — Escala fraccional

- `wp_fractional_scale_v1` + viewporter validada en el panel real
- Resolver el `TODO(scale)` en `state.rs` (un valor por superficie, no un
  literal global)
- Aceptación: (Pendiente)

## Bloque 10 — Lock con autenticación

- Autenticación del lock con PAM y bloqueo por inactividad
  (`ext-idle-notify-v1`)
- Aceptación: (Pendiente)

## Bloque 11 — Protocolos de uso diario

- `wp_presentation`, `xdg-activation-v1`, `wp_cursor_shape_v1`,
  `zwp_idle_inhibit_manager_v1`, `xdg-output`
- Aceptación: (Pendiente)

## Después del Bloque 11

- D-Bus de configuración (ADR 0021) y la app de ajustes. XWayland sigue
  diferido.

## Dónde está este repositorio

- **Bloque 0 — hecho, con divergencias.** Los tipos de coordenadas viven en el
  crate `yuki-scene`, no en el compositor (ADR 0001). Smithay está anclado al
  release 0.7.0 de crates.io con `desktop` deliberadamente desactivado, no a un `rev`
  de git con `desktop` activado (ADR 0009, ADR 0010). El nivel de log se establece con `--log-level`,
  no con filtrado basado en variables de entorno. `yuki-design` no está vacío.
- **Bloque 1 — hecho.** `wayland-info` (el dev shell no tiene `weston-info`) enumera
  todos los globals. Véase `bloques/00-cimientos.md` y el registro a nivel de sesión
  `bloques/01-sesion.md`, que cubre los Bloques 1–3 juntos.
- **Bloque 2 — hecho, con divergencias.** El mapeo ocurre en el primer commit con
  buffer, no en `ack_configure` (`bloques/01-sesion.md`, Divergencias 1).
  Los toplevels se rastrean en `crates/yuki/src/window.rs` sin envoltorio de
  `desktop::Space` (ADR 0009; la elección es ahora una decisión escrita, ADR
  0015). El cliente de aceptación es `foot`, ya que weston no está instalado. Véase
  `bloques/02-ventana-en-pantalla.md`.
- **Bloque 3 — hecho, con lagunas verificadas a mano.** El foco de teclado, el
  keymap/repeat, el motion/button/axis del puntero, el soporte de cursor y la
  contabilidad central de serials están implementados. `./dev.sh smoke` comprueba que el compositor dibuja su
  puntero por defecto y verifica cualquier `set_cursor` del cliente contra el enter de puntero que
  responde. La escritura, la respuesta del puntero, la salida legible y los píxeles del
  cursor renderizado permanecen verificados a mano. Véase `bloques/03-entrada.md`.
- **Bloque 4 — hecho, con lagunas verificadas a mano.** El movimiento (arrastre de
  la titlebar y `xdg_toplevel.move`), el redimensionado (zonas de borde y `xdg_toplevel.resize`, todos
  los bordes, min/max respetados), cerrar, el apilado, el click-to-focus y los cursores de
  redimensionado de borde están implementados en `yuki-scene` y en el compositor. Los
  serials de `xdg_toplevel.move`/`resize` se validan contra la última pulsación del puntero. `./dev.sh block4` ejecuta
  tres tandas de 50 ventanas y comprueba que la escena vuelve a estar vacía (recuento de
  objetos vivos) sin crecimiento de memoria por tanda; `./dev.sh move` ejercita el
  move/resize iniciado por el cliente con serials válidos y fabricados. La sensación de arrastre, los
  visuales de redimensionado y la forma del cursor cambiando bajo el puntero se
  verifican a mano. Véase `bloques/04-ventanas-manipulables.md`. Dos seguimientos
  posteriores a la aceptación arreglaron el frame invertido verticalmente y reemplazaron la
  política de decoración siempre del lado del servidor (ADRs 0018 y 0019); `./dev.sh
  decoration` verifica la política contra aplicaciones reales.
- **Bloque 5 — hecho, con divergencias.** Los popups se rastrean en `popup.rs`
  (árbol de geometría en `yuki-scene`, superficies en el compositor), con
  ajuste de restricciones contra el output (`get_unconstrained_geometry`),
  configure antes de mapear, render encima de las ventanas, y cierre por clic
  fuera, Esc y muerte del cliente. Verificado en vivo con `popup_probe`
  (volteo del menú y sin huérfanos). Ver `bloques/05-popups.md`.
- **Bloque 6 — hecho.** Se anuncian `wl_data_device_manager` y
  `zwp_primary_selection_device_manager_v1`. El portapapeles y la selección primaria
  siguen el foco de teclado mediante `set_data_device_focus` y `set_primary_focus`
  (verificado: `wl-copy` → `wl-paste` entre clientes). La aceptación de copiar y
  pegar entre dos instancias de `foot` se comprueba a mano. Véase
  `bloques/06-portapapeles.md`.
- **Bloque 7 — hecho.** 7a (linux-dmabuf v5 con feedback) y 7b (explicit
  sync + sync implícito) hechos: `zwp_linux_dmabuf_v1` con el render node como
  `main_device`, `wp_linux_drm_syncobj_manager_v1` solo si el dispositivo lo
  soporta, un solo camino de blocker en el commit y release diferido por fence.
  7c hecho: la shell presenta por EGL (`wl_egl_window`), sin `wl_shm`. vkcube
  (RADV), mpv (gpu-next), wezterm y Firefox corren acelerados. Véase
  `bloques/07-linux-dmabuf-y-explicit-sync.md` y los ADR 0026/0027.

## Numeración de bloques

Este archivo es la única autoridad sobre los números de bloque. Dos cuerpos de
trabajo están deliberadamente nombrados y no numerados, para que no puedan confundirse
con un bloque del roadmap:

- **La sesión anidada** — el trabajo registrado en `bloques/01-sesion.md`
  (anidamiento de winit, mapeo de toplevels y entrada). Abarca los Bloques 1–3 de este
  roadmap, y las Partes 1–6 que usa el documento son Partes de esa sesión,
  nunca números de bloque.
- **Direct Session** — el hito de producción sobre DRM/KMS, que ahora es el
  **Bloque 8** del roadmap.

Ningún documento ni comentario de este repositorio usa "Bloque N" para algo distinto
del bloque N de este roadmap.