# Resize por bordes y cursores: GNOME, Otto y macOS

Nota de investigación para la tarea pendiente de mejorar el hit-testing de
bordes de Yuki. Hoy `yuki-scene` usa una banda uniforme de 8 px
(`RESIZE_BORDER`) y `input.rs` elige `NsResize`/`EwResize`/`NwseResize`/
`NeswResize`.

## GNOME

GNOME separa dos responsabilidades:

1. **Detección del borde y cursor en CSD:** la hace GTK4 en `GtkWindow`.
   `get_edge_for_coordinates()` calcula el área de agarre con:
   - borde CSS + padding del tema,
   - sombra de cliente si existe,
   - `RESIZE_HANDLE_SIZE = 12` para los lados,
   - `RESIZE_HANDLE_CORNER_SIZE = 24` para agrandar las esquinas,
   - comprobación de esquinas redondeadas con
     `gsk_rounded_rect_corner_box_contains_point()`.
   Solo se activa si la ventana es `client_decorated`, `resizable`, no está
   maximizada ni fullscreen y respeta `edge_constraints` (tiled).
   `gtk_window_capture_motion()` traduce el borde a `nw-resize`, `n-resize`,
   `ne-resize`, `w-resize`, `e-resize`, `sw-resize`, `s-resize` o `se-resize`.
   Al pulsar, `click_gesture_pressed_cb()` llama a `gdk_toplevel_begin_resize()`.
   Código:
   <https://gitlab.gnome.org/GNOME/gtk/-/blob/main/gtk/gtkwindow.c#L1533>
   y <https://gitlab.gnome.org/GNOME/gtk/-/blob/main/gtk/gtkwindow.c#L1745>.
2. **Grab en Mutter:** `MetaGrabOp` codifica las direcciones con bits
   (`META_GRAB_OP_WINDOW_DIR_NORTH/SOUTH/EAST/WEST`) y sus combinaciones
   `META_GRAB_OP_RESIZING_*`. `meta_cursor_for_grab_op()` mapea el grab a
   `CLUTTER_CURSOR_*_RESIZE` y `meta_window_drag_update_cursor()` lo aplica al
   actor del grab. Código:
   <https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/meta/meta-enums.h#L70>
   y
   <https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/compositor/meta-window-drag.c#L313>.
3. **X11:** Mutter traduce `_NET_WM_MOVERESIZE` a `META_GRAB_OP_RESIZING_*`
   (`src/x11/window-x11.c`). `MetaFrame` es un `GtkWindow`, así que las ventanas
   X11 con decoración del servidor reutilizan la lógica de GTK.
4. **Resistencia de bordes** (`src/compositor/edge-resistance.c`) es snapping
   durante el arrastre, no selección de cursor.

## Otto

Otto lo hace en `src/workspaces/window_view/resize_view.rs`:

- `RESIZE_MARGIN = 6.0` px, una banda **dentro** del rectángulo de la ventana.
- Solo para ventanas decoradas por el servidor y no minimizadas, maximizadas ni
  fullscreen (`src/input/pointer.rs`, alrededor de `resize_edges_at`).
- `resize_edges_at()` compone bits `ResizeEdge::{LEFT,RIGHT,TOP,BOTTOM}`; las
  esquinas salen de combinar dos bits.
- `cursor_for()` mapea cada combinación a `CursorIcon::{N,S,E,W,NE,NW,SE,SW}Resize`.
- `WindowResizeView` es un target de input: al entrar/moverse fija el cursor, al
  pulsar con el botón izquierdo lanza `resize_request_ssd`.
- Tiene en cuenta el tiling: un borde exterior del árbol no es arrastrable y deja
  el cursor por defecto.
- No usa sombra/CSS ni esquinas redondeadas: es una banda geométrica simple.

## macOS (teoría)

macOS es cerrado; lo público es AppKit y el comportamiento de WindowServer:

- `NSWindow` con `NSWindowStyleMaskResizable` delega el resize de la ventana
  titulada en WindowServer/AppKit. El borde de la ventana es una región de
  tracking del sistema, no una `NSTrackingArea` de la app.
- El cursor de marco se expresa con `NSCursor.frameResize(position:directions:)`
  (macOS Sequoia) y, antes, con `resizeLeftRight`, `resizeUpDown`,
  `resizeUpLeftDownRight` y `resizeUpRightDownLeft`.
- En ventanas sin decoración del sistema, la app dibuja su propio borde y debe
  instalar `NSTrackingArea` + `cursorUpdate` y llamar a `NSCursor.set()`; no hay
  un `WM_NCHITTEST` público equivalente.
- La teoría es la misma que GNOME/Otto: zona de agarre alrededor del frame,
  esquinas con prioridad sobre los lados, cursor según la combinación de
  direcciones y resize al pulsar.

## Estado en Yuki

Implementado:

- `window.resize_border = 10` (banda exterior del lado) y
  `window.resize_corner = 20` (longitud de la «L» exterior de esquina), ambos
  configurables.
- La banda vive **fuera** de `outer`; solo las esquinas pueden entrar 2 px.
  No hay cuadrados grandes dentro del contenido: un punto 30 px dentro de una
  esquina es cliente.
- `Scene::edge_at()` recorre la pila de arriba abajo. Si el punto está dentro
  del `outer` de una ventana pero fuera de su tolerancia de esquina, es
  contenido y detiene la búsqueda; la banda exterior de una ventana inferior no
  roba input al contenido de una superior. La banda de la ventana superior sí
  gana sobre el contenido de una inferior.
- Los botones de la titlebar se comprueban antes que el resize.
- Solo las ventanas con decoración del servidor ofrecen resize del compositor;
  las CSD gestionan sus propios bordes. Cuando llegue el pintor de efectos
  (ADR 0020), las CSD podrán usar las mismas bandas exteriores.
- No se ofrece resize en maximizado ni fullscreen. Minimizado queda fuera de la
  escena.
- Tests: 1 px dentro del borde derecho → cliente; 6 px fuera → lado; 6 px fuera
  en diagonal → esquina; 30 px dentro de una esquina → cliente; además,
  contenido superior bloquea banda inferior y banda superior gana a contenido
  inferior.

Pendiente: sumar la sombra/CSS al área de agarre, comprobar esquinas
redondeadas y excluir tiled cuando exista tiling en Yuki.

## Traspaso para la siguiente sesión

### Archivos tocados

- `crates/yuki-scene/src/scene.rs`: `Scene::edge_at(point, side, corner)` y
  `edge_in(...)`; la banda vive fuera de `outer`, las esquinas son una «L» con
  2 px de tolerancia hacia dentro y la búsqueda respeta la pila.
- `crates/yuki/src/window.rs`: `Windows::edge_under(point, side, corner)`
  filtra por `Decoration::Server`, `!maximized` y `WindowState::Normal`.
- `crates/yuki/src/input.rs`: `pointer_press()` atiende la banda exterior
  fuera de `outer`, comprueba los botones de la titlebar antes del resize y
  `update_edge_cursor()` lee la config.
- `crates/yuki/src/config.rs`: `window.resize_border = 10` y
  `window.resize_corner = 20`.
- `docs/configuracion/opciones.md`, `docs/por-hacer.md` y este documento.

### Decisiones tomadas

- **Botones primero:** la «L» solo entra 2 px, pero el orden se mantiene
  explícito para que close/minimize/maximize nunca pierdan contra el resize.
- **Solo SSD:** las ventanas CSD conservan sus propios bordes y cursor. El
  compositor no impone resize encima.
- **Sin maximizado/fullscreen:** no hay borde libre que arrastrar.
- **Sin tiling todavía:** Yuki aún no tiene estado de tiling; cuando lo tenga,
  hay que añadir la exclusión (como hace Otto con `tiling_edge_is_draggable`).

### Siguiente paso sugerido

1. Sumar la sombra/efectos al área de agarre si se implementa el pintor de
   efectos (hoy la banda se mide sobre el rectángulo exterior).
2. Comprobar esquinas redondeadas solo si el hit-testing de la escena pasa a
   usar la forma redondeada; hoy el rectángulo exterior es suficiente.
3. Cuando exista tiling, excluir los bordes exteriores del árbol.
4. Tests pendientes: prioridad botón/esquina en `input.rs` (requiere montar un
   `State`), CSD no redimensionable y maximizado/fullscreen. Los tests de
   geometría de `yuki-scene` ya cubren lado, esquina y ventana pequeña.

### Comandos de verificación

```sh
./dev.sh check
cargo test -p yuki-scene scene::tests::edge_zones_are_measured_against_the_decorated_bounds
cargo test -p yuki --lib config::tests::the_resize_band_defaults_are_10_and_20
```
