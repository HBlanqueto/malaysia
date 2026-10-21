# Bloque 11 — Protocolos de uso diario

Estado: **parcial.** Empezado por `wp_presentation`; faltan el resto de
protocolos y la captura.

## Implementado

- `wp_presentation` v2 con `CLOCK_MONOTONIC` (clock id 1).
- Los feedbacks se recogen del árbol de superficies dibujado y se contestan
  después de presentar el frame:
  - `presented` con timestamp, refresh fijo de 60 Hz, secuencia y flag
    `Vsync`;
  - `discarded` cuando el frame no se presentó por no tener daño.
- En el backend DRM se contestan en `vblank`, con el contador de vblank como
  secuencia.
- El global está anunciado: `wayland-info` muestra
  `wp_presentation`, versión 2, `presentation clock id: 1`.

## Pendiente

- `xdg-activation-v1`: tokens, activación de una ventana existente y
  restricciones por antigüedad.
- `wp_cursor_shape_v1`: requiere completar el `TabletSeatHandler` que Smithay
  exige para el `Dispatch2` del dispositivo de cursor.
- `zwp_pointer_constraints_v1`: estado/global y activación por foco; hoy solo
  existe el handler vacío que exige `PointerTarget`.
- `ext-image-copy-capture-v1` + `ext-image-capture-source-v1`: captura por
  monitor/ventana con dmabuf, base del portal del Bloque 12.
- Opcional: `relative-pointer` y `keyboard-shortcuts-inhibit` para juegos y
  escritorio remoto.

## Aceptación pendiente

- Un cliente con `wp_presentation` recibe `presented` con un timestamp
  monótono y un `refresh` razonable; al cerrar la superficie recibe
  `discarded` o deja de esperar.
- Una app activa otra con un token válido y una ventana existente sube y
  recibe foco; un token viejo no activa.
- Un cliente pide `cursor-shape` y el cursor cambia sin enviar un buffer.
- Un juego confina/bloquea el puntero y `relative-pointer` da deltas.
- La captura entrega frames por dmabuf y la app puede elegir monitor/ventana.

## Notas

- `wp_presentation` no se ha probado todavía con un cliente que pida feedback;
  `wayland-info` solo verifica el global. Falta un probe pequeño para el evento
  `presented`/`discarded`.
