# Bloque 3 — Entrada

> **Mapeo del roadmap.** Este es el registro detallado del **Bloque 3** del
> roadmap (Entrada), la otra mitad de la Parte 6 de la **sesión anidada**
> (`01-sesion.md`, Parte 6, entrada; el Bloque 2 del roadmap es la mitad de
> renderizado de esa Parte, `02-ventana-en-pantalla.md`). El soporte de cursor fue
> la última pieza, añadida después de las demás partes de la sesión.

Estado: **completo, con huecos de comprobación manual.** `./dev.sh smoke` comprueba
las partes visibles por máquina; escribir con el layout correcto, la respuesta del
puntero y los píxeles del cursor se comprueban a mano.

## Propósito

El Bloque 3 hace real el seat: una ventana mapeada toma el foco de teclado y
recibe `wl_keyboard.enter`; las teclas le llegan a través del keymap y el repeat
configurados en la creación del seat; el puntero impacta en la ventana que tiene
debajo y entrega motion, botones y scroll; y el puntero se dibuja — la imagen
propia del cliente cuando él pone una, la flecha del compositor en caso contrario.
Cada serial proviene del contador único de Smithay; ninguno se inventa.

## Qué se construyó

### `input.rs`

- `process_input_event` traduce la forma unificada `InputEvent` de Smithay a
  eventos de seat, así el módulo es agnóstico del backend y sigue funcionando
  cuando el hito Direct Session reemplace a winit:
  - la entrada de teclado pasa por `Seat::get_keyboard` y `keyboard.input` con
    `FilterResult::Forward`, llevando un serial del contador central;
  - el motion absoluto del puntero se convierte con `State::logical_output_size`
    (el backend winit lo reporta como una fracción de la ventana);
  - los botones del puntero se entregan al pointer; la propia lógica de pulsación
    del compositor se ejecuta antes del evento de protocolo;
  - `PointerAxis` se convierte en un `AxisFrame` con `value`, `v120` donde el
    backend lo reporta, y un `stop` con fuente de dedo para un eje en cero.
- El foco del puntero es por hit-testing de la ventana bajo el punto, y el motion
  actualiza tanto el foco como el cursor del borde. Los dos lugares que construyen
  una tupla de foco de puntero — `pointer_motion` y la pulsación scripteada que
  inyecta el backend de test — le dan a Smithay el *origen* de la superficie que
  espera, no el punto local a la superficie; la pulsación scripteada pasa por
  `pointer_motion`, así que los dos no pueden divergir.
- El foco de teclado existe desde el primer mapeo: `handlers.rs::commit` llama a
  `focus_window` cuando una ventana se mapea, que envía `wl_keyboard.enter`. Cuando
  una ventana se destruye, `forget_window` pasa el foco a la nueva ventana más alta
  o lo limpia (`clear_keyboard_focus`).
- El Bloque 4 extendió la ruta de botones con las grabs propias del compositor
  (resize de borde, move de la banda decorada, click-to-focus) y validó los
  seriales de `xdg_toplevel.move`/`resize` contra la pulsación mantenida del
  puntero (ADR 0016).

### `cursor.rs` y la ruta de dibujo

- Un cliente elige su puntero con `wl_pointer.set_cursor`. Smithay maneja la
  solicitud y, antes de llamar a `SeatHandler::cursor_image`, valida su serial
  contra el **último serial de enter** del puntero, que este compositor generó a
  partir del contador único en `input.rs`; una solicitud con cualquier otro serial
  nunca llega a `SeatHandler::cursor_image`. `handlers.rs::cursor_image` guarda el
  estado y pide un redibujado.
- `CursorImageStatus` es una de tres cosas en la ruta de dibujo:
  - una superficie de cliente, dibujada en la posición del puntero menos su
    hotspot;
  - `Hidden` (una superficie nula), que no dibuja nada;
  - un cursor con nombre — la flecha por defecto — cargado una vez desde el tema
    Xcursor (`XCURSOR_THEME`, `XCURSOR_SIZE`), con un triángulo delineado
    integrado cuando no se puede leer ningún tema. La carga es la única dependencia
    nueva, `xcursor`, que Smithay no proporciona.
- `session.rs::append_cursor` dibuja el árbol de superficies del cliente o la
  forma del compositor sobre las ventanas; `cursor_origin` coloca ambos en el
  puntero menos el hotspot, así las dos rutas se mantienen en sincronía. Una
  superficie de cursor de un cliente puede morir mientras sigue siendo la imagen
  del puntero; `reset_dead_cursor` vuelve a la flecha del compositor en lugar de
  dibujar una superficie muerta.
- El Bloque 4 añadió las formas de resize de borde a la misma tabla.

### Seat y keymap

- El seat se crea en `State::new` y el teclado se añade antes de que el seat sea
  usable, porque un seat sin keymap es peor que ningún seat: los clientes que
  aceptaron la capability no reciben keymap y se comportan mal. `add_keyboard` se
  llama con `XkbConfig::default()` y un delay de repeat de 200 ms a 25/s; un fallo
  es fatal para el arranque. La sesión registra el keymap cargado
  (`English (US)`).

## Decisiones

- **Un único contador de seriales.** Los seriales provienen de `SERIAL_COUNTER`
  de Smithay; el ADR 0016 extiende la regla con la validación de grabs mantenidas
  de las solicitudes de cliente.
- **ADR 0012** — el seat y el estado de entrada viven en `State`; el backend y el
  renderizador no.
- **ADR 0013** — las solicitudes de los clientes se despachan haciendo poll de un
  descriptor de display duplicado; los eventos de entrada llegan en la fuente de
  winit y se reenvían al mismo `State`.
- La comprobación del serial en `set_cursor` del cursor no es una política de este
  repositorio: es la de Smithay, contra el enter de puntero que él generó. No
  existe un segundo contador ni una comprobación paralela.

## Verificación

- **`./dev.sh smoke`** — ejecuta el compositor a nivel de debug y verifica la
  mitad visible por entrada de la secuencia de mapeo: `wl_keyboard.enter` llega a
  la superficie mapeada después de su commit, y cualquier `wl_pointer.set_cursor`
  en el trace usó un serial de un `wl_pointer.enter` que el cliente ya había
  recibido. También verifica que el compositor dibujó su propio puntero por
  defecto. Observado:

  ```
  smoke: compositor is listening on wayland-1
  smoke: configure(serial 2) -> ack(2) -> attach/commit -> keyboard.enter -> frame callback #25 done at line 104; cursor surfaces set: 0
  smoke: PASS
  ```

  `cursor surfaces set: 0` no es un hueco de la comprobación: el harness no puede
  mover el puntero del anfitrión, así que ningún cursor de cliente se solicita; el
  puntero por defecto del compositor se dibuja y el trace no tiene nada que
  emparejar.
- **`./dev.sh check`** — lleva los tests unitarios de los que depende la ruta de
  este bloque: `input::tests::host_pointer_corners_map_to_logical_corners` y
  `host_pointer_moves_down_in_logical_space` manejan el `process_input_event`
  real a través de un backend de test construido con el contrato de winit; los
  tests del cursor de respaldo (`the_fallback_image_is_a_full_rgba_buffer`,
  `the_fallback_hotspot_is_the_tip`, `the_fallback_arrow_is_visible`); el test de
  píxeles fuera de pantalla `the_cursor_lands_at_the_hotspot_adjusted_position`;
  y los tests del parser del smoke que aceptan un `set_cursor` que coincide con
  un enter precedente y rechazan uno cuyo serial ningún enter llevó, uno antes de
  cualquier enter, y un cursor nulo (hide).
- **`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`** — la
  compilación de documentación está libre de warnings.

No verificado por un comando, y dejado a la comprobación manual: que la escritura
llegue al cliente con el layout correcto, que el puntero responda donde apunta,
que la salida del cliente sea legible, y que los píxeles del cursor dibujado se
vean bien. El harness de smoke no puede mover el puntero del anfitrión, así que
nunca puede hacer que un cliente ponga un cursor en vivo; comprueba el puntero por
defecto del compositor y el emparejamiento de serial `set_cursor`/enter, no los
píxeles finales.

## Preguntas abiertas

1. **El harness de smoke no puede mover el puntero del anfitrión.** El
   `set_cursor` de cliente en vivo, el feedback de hover y los píxeles del cursor
   siguen comprobándose a mano.
2. **El foco de teclado no sigue al puntero.** Cambia solo al hacer click y al
   destruirse la ventana enfocada; el hover no eleva ni enfoca.
3. **Los eventos táctiles y de gestos se ignoran.** `process_input_event` reenvía
   teclado, motion de puntero, botones y axis, y descarta el resto; el backend
   winit los reporta, así que añadirlos es un manejador, no una fuente nueva.
4. **La política `deny(unsafe_code)` debe redefinirse antes del hito Direct
   Session.** La sesión anidada ya la dobló dos veces (la fuente de dispatch y la
   exportación de `WAYLAND_DISPLAY`); la pregunta en sí es a nivel de sesión y
   está registrada en `01-sesion.md`.

## Registros de decisión

- `../decisiones/0009-backend-winit-y-gles2.md` — el backend anidado y el conjunto
  de features en el que corre esta ruta de entrada (winit, GLES2,
  `wayland_frontend`; `desktop` desactivado).
- `../decisiones/0012-backend-junto-al-state.md` — `State` es dueño del seat; el
  backend vive en `Session`.
- `../decisiones/0013-dispatch-consume-el-fd-duplicado.md` — cómo las solicitudes
  de los clientes y los eventos de entrada llegan al mismo `State`.
- `../decisiones/0016-movimiento-y-redimensionado-interactivo.md` — la validación
  de serial de grabs mantenidas que el Bloque 4 añadió sobre el contador único.
- `01-sesion.md` — el registro a nivel de sesión: Partes, fallos de la Parte 5,
  la pregunta abierta de la política de unsafe.