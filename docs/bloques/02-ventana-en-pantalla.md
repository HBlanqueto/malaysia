# Bloque 2 — Una ventana en pantalla

> **Mapeo del roadmap.** Este es el registro detallado del **Bloque 2** del
> roadmap (Una ventana en pantalla). Son las Partes 4–6 de la **sesión anidada**,
> cuyo registro a nivel de sesión es `01-sesion.md`; las divergencias y los
> fallos de la Parte 5 citados aquí viven allí, y el Bloque 3 del roadmap es la
> otra mitad de la Parte 6 (`03-entrada.md`).

Estado: **completo, con divergencias.** La aceptación verificada por máquina es
`./dev.sh smoke`; si el contenido de un cliente es legible sigue siendo una
comprobación manual.

## Propósito

El Bloque 2 convierte un buffer comprometido en píxeles: el backend winit y el
renderizador GLES2 dibujan la superficie de cada ventana mapeada dentro de la
ventana anidada, y el ciclo de vida del toplevel decide cuándo se mapea la
superficie de un cliente. El seat y sus globales existen desde el Bloque 1, pero
solo se usan cuando una ventana se mapea, por eso el foco de teclado y el cursor
son del Bloque 3.

Dos viñetas del roadmap se implementan deliberadamente de forma distinta al
resumen, y ambas divergencias están registradas en este repositorio:

- **El mapeo ocurre en el primer commit con buffer, no en `ack_configure`.**
  El protocolo mapea una superficie cuando tiene un buffer no nulo comprometido;
  mapear en el ack mostraría una superficie vacía. `01-sesion.md`
  (Divergencias 1) es la fuente autoritativa y anula la redacción del resumen.
- **No existe un wrapper `desktop::Space`.** `desktop` sigue desactivado (ADR 0009) y
  el tracker está escrito a mano en `window.rs`. El ADR 0015 registra una razón
  para mantenerlo así; se escribió cuando el Bloque 4 hizo caro revertirlo.

## Qué se construyó

### `session.rs`

- El backend winit (`WinitEventLoop`, un `WinitGraphicsBackend` con un
  `GlesRenderer`) y el `OutputDamageTracker` viven en `Session` junto a
  `State`, porque son estado del backend y ningún manejador de protocolo puede
  alcanzarlos (ADR 0012).
- El bucle es bajo demanda, no continuo. Un manejador que cambia algo eleva
  `State::needs_redraw`; la sesión convierte eso en una solicitud de redibujado
  de winit, y `draw` se ejecuta en `WinitEvent::Redraw`.
- `draw` convierte cada ventana mapeada en elementos de renderizado
  (`render_elements_from_surface_tree`, `Kind::Unspecified`) en su offset de
  ventana en píxeles físicos, añade el cursor (Bloque 3), y los dibuja a través
  de `OutputDamageTracker::render_output` con el fondo de la sesión
  (`[0.06, 0.06, 0.09, 1.0]`) como color de limpieza, enviando solo el damage
  que el tracker reporta.
- Los callbacks de frame se envían después de dibujar el frame y solo desde esta
  ruta, así un cliente no puede acelerar el compositor más rápido que el refresh
  de la salida. `send_frame_callbacks` recorre el árbol de superficies porque el
  helper compartido está detrás de la feature `desktop`, que sigue desactivada.
- Un redimensionado de la ventana anfitriona (`WinitEvent::Resized`) actualiza el
  modo de la salida y reemplaza el tracker de damage, cuya fuente de modo es
  estática; un tracker nuevo reinicia el historial de damage, que es lo que toda
  la superficie necesita de todos modos.

### `window.rs`

- `Window`/`Windows` siguen los toplevels: la superficie, el tamaño exterior, la
  contabilidad de configure, los insets y el flag de mapeo. `Window` fue una
  vista de solo lectura desde el inicio, así que las rutas de dibujo y de entrada
  nunca tocaron los internos del tracker; el Bloque 4 reconstruyó el
  almacenamiento alrededor de `yuki-scene` y mantuvo la vista.
- `new_toplevel` envía el configure inicial inmediatamente, con el tamaño usable
  (el tamaño exterior menos los insets de decoración). Un toplevel que nunca
  recibe uno espera para siempre sin error.
- `ack_configure` acepta solo el serial del configure más recientemente enviado,
  así un ack obsoleto no puede marcar una ventana como lista (fallo de la Parte 5,
  regla 4).
- Una ventana se mapea en el primer commit que lleva un buffer *y* sigue a un ack.
  `handlers.rs::commit` llama primero a `on_commit_buffer_handler` — Smithay mueve
  el buffer pendiente al estado comprometido allí, y la comprobación antigua nunca
  podía verlo —, luego el tracker lee ese estado.
- `usable` resta los insets del tamaño exterior y nunca colapsa por debajo de un
  píxel; los insets se recalculan en cada commit, y un cambio de insets efectivos
  reconfigura el cliente.

### `decoration.rs`

- El tipo de insets se escribió antes de que la primera ventana se mapeara (Parte 4
  de la sesión): el configure que recibe un toplevel lleva su tamaño usable, así
  que el número se necesita antes de que exista cualquier chrome. Los insets son
  una función del rol y el estado *actuales* de la ventana, no del estado en el
  momento del mapeo (ADR 0014); `Role::Popup` y `Role::Dialog` no reservan nada,
  y el fullscreen y el chrome del lado del cliente no reservan nada.

### `state.rs` y `handlers.rs`

- `State` contiene los globales, el único seat, la salida y su tamaño actual, el
  tracker de ventanas, el estado del cursor y el flag de redibujado.
  `logical_output_size` es el único lugar donde el tamaño de modo de la salida se
  convierte en coordenadas lógicas (fallo de la Parte 5, regla 3), y
  `OUTPUT_SCALE` es la única literal de escala (ver Preguntas abiertas).
- `wl_shm` anuncia `Argb8888` y `Xrgb8888`; la ruta de renderizado importa el
  buffer del cliente a través del árbol de superficies.
- `Handlers` implementa `CompositorHandler`, `ShmHandler`, `SelectionHandler`
  y los callbacks de `xdg_shell`. `client_compositor_state` documenta el
  invariante de que cada cliente aceptado fue insertado con `ClientState` por el
  manejador del socket.

## Decisiones

- **ADR 0009** — el backend anidado es winit con el renderizador GLES2;
  `desktop` sigue desactivado, que es también por qué `send_frame_callbacks` está
  escrito a mano.
- **ADR 0010** — Smithay está anclado al release 0.7.0 de crates.io.
- **ADR 0012** — el backend vive junto al `State` del compositor.
- **ADR 0014** — los insets de decoración son una función del estado de la ventana.
- **ADR 0015** — no se adopta `desktop::Space`; el tracker sigue escrito a mano
  y `yuki-scene` crece hasta ser la autoridad del layout de ventanas
  (registrado cuando el Bloque 4 hizo difícil revertir la elección).
- La divergencia de mapeo es la **Divergencia 1** en `01-sesion.md`.

## Verificación

- **`./dev.sh smoke`** — lanza un compositor y `foot` con
  `WAYLAND_DEBUG=1`, captura el trace del cliente y verifica, en orden:
  `xdg_toplevel.configure` antes del primer attach no nulo; un serial de
  `ack_configure` coincidente antes de ese attach; un commit con buffer no nulo
  que el compositor reporta como mapeado; `wl_keyboard.enter` en esa superficie;
  y la finalización del id de callback que el cliente pidió con `wl_surface.frame`
  (un `wl_callback.done` simple, incluyendo un `wl_display.sync`, no se acepta).
  Observado:

  ```
  smoke: compositor is listening on wayland-1
  smoke: configure(serial 2) -> ack(2) -> attach/commit -> keyboard.enter -> frame callback #25 done at line 104; cursor surfaces set: 0
  smoke: PASS
  ```

  `cursor surfaces set: 0` es esperado: el harness no puede mover el puntero del
  anfitrión, así que ningún cliente pide un cursor (Bloque 3).
- **`./dev.sh check`** — lleva los tests unitarios de la aritmética y la
  contabilidad de este bloque: `usable_size_subtracts_the_top_inset`,
  `..._both_axes`, `..._is_the_window_when_nothing_is_reserved` y
  `..._never_collapses`; los tests de la cadena de errores de la sesión; y las
  conversiones de coordenadas de `yuki-scene` que usan los offsets de
  renderizado.
- **`cargo run -p sctk-probe --bin parent_probe`** — mapea dos toplevels y
  alterna un parent en el hijo, verificando que el hijo se reconfigura
  `1248×800 → 1280×800 → 1248×800` en una salida de 1280×800 (registrado en
  `01-sesion.md`); el README de la probe tiene el comando que encuentra el socket
  de un compositor en ejecución.
- **`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`** — la
  compilación de documentación está libre de warnings.

No verificado por un comando, y dejado a la comprobación manual: que el contenido
de un cliente real sea legible en la ventana anidada. El cliente de aceptación es
`foot`, porque weston no está instalado en esta máquina.

## Preguntas abiertas

> **Actualizado en el Bloque 9.** La constante `OUTPUT_SCALE` ya no existe:
> `State::scale` lleva la escala de la salida. Ver
> [`09-escala-fraccional.md`](09-escala-fraccional.md). Esta nota se conserva
> como registro de lo que estaba abierto al cerrar el Bloque 2.

1. **`OUTPUT_SCALE` es una literal.** Es `1` detrás de una única constante con
   nombre, y sus dos sitios de llamada — `State::logical_output_size` y la
   colocación de elementos en `session::draw` — cambian juntos cuando aterrice el
   escalado. Una salida HiDPI necesita `2`; el escalado fraccional necesita un
   valor por superficie en lugar de un único entero global. El `TODO(scale)` en
   `state.rs` nombra ambos sitios de llamada.
2. **Un contexto EGL perdido se registra, no se recupera.** Un fallo de dibujo se
   convierte en un `tracing::error!` y el bucle sigue corriendo para que un evento
   posterior pueda intentarlo de nuevo; nada recrea el renderizador ni el
   framebuffer.
3. **El historial de damage se descarta en un redimensionado del anfitrión.** La
   fuente de modo del tracker es estática, así que un resize construye un tracker
   nuevo en lugar de actualizar uno. Eso redibuja toda la superficie, que es lo
   que necesita un cambio de tamaño, pero no es una historia general de
   seguimiento de damage.

## Registros de decisión

- `../decisiones/0009-backend-winit-y-gles2.md` — el backend anidado es winit con
  el renderizador GLES2, `desktop` desactivado.
- `../decisiones/0010-anclaje-de-smithay.md` — Smithay 0.7.0 desde crates.io.
- `../decisiones/0012-backend-junto-al-state.md` — el estado del backend vive en
  `Session`, no en `State`.
- `../decisiones/0014-insets-de-decoracion.md` — los insets son una función del
  estado de la ventana.
- `../decisiones/0015-desktop-space-y-yuki-scene.md` — sin `desktop::Space`;
  `yuki-scene` es dueño del layout de ventanas.
- `01-sesion.md` — el registro a nivel de sesión: Partes, fallos de la Parte 5,
  Divergencias 1.