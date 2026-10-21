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
- `xdg-activation-v1`: global v1. Un token solo se acepta si la superficie que
  lo pide pertenece al cliente con foco de teclado, lleva el serial del último
  teclado/botón pulsado y no tiene más de 30 s. Al activar, la ventana se
  restaura si estaba minimizada y sube con foco (`activate_window`).
- `wp_cursor_shape_manager_v1` v2: `TabletSeatHandler` completo con
  `ToolFocus = WlSurface`; los nombres del protocolo se mapean a los cursores
  del tema Xcursor y el compositor conserva el control visual (ADR 0020).
  `CursorIcon` cubre las 35 formas del estándar y los nombres legacy del tema.
- El tema del cursor se resuelve con `XCURSOR_THEME` y, si no está, con
  `Adwaita`, `breeze` y `default` en ese orden; antes un `XCURSOR_THEME` vacío
  caía al triángulo integrado y el cursor parecía recortado.
- **Shake to locate**: al agitar el ratón en horizontal (tres cambios de
  dirección, cuatro vaivenes de al menos 24 px en menos de 1 s) el puntero
  crece con animación suave hasta 3× y empieza a encogerse en cuanto el ratón
  se queda quieto (180 ms de crecimiento y 220 ms de vuelta, a ~60 Hz).
  Es el gesto de macOS Accessibility > Pointer Control. Los valores viven en
  `yuki-design`.

## Pendiente

- `zwp_pointer_constraints_v1`: estado/global y activación por foco; hoy solo
  existe el handler vacío que exige `PointerTarget`.
- `ext-image-copy-capture-v1` + `ext-image-capture-source-v1`: Smithay ya
  implementa los protocolos; falta integrar sources, constraints, render a
  dmabuf/SHM y el portal del Bloque 12.
- Opcional: `relative-pointer` y `keyboard-shortcuts-inhibit` para juegos y
  escritorio remoto.

## Aceptación pendiente

- Un cliente con `wp_presentation` recibe `presented` con un timestamp
  monótono y un `refresh` razonable; al cerrar la superficie recibe
  `discarded` o deja de esperar.
- Una app activa otra con un token válido y una ventana existente sube y
  recibe foco; un token viejo, sin foco o sin serial no activa.
- Un cliente pide `cursor-shape` y el cursor cambia sin enviar un buffer;
  wezterm pide `Text` en el primer enter y Yuki lo aplica.
- Al agitar el ratón el cursor crece con animación suave hasta 3× y, al
  dejar de moverse, empieza a encogerse de inmediato.
- Un juego confina/bloquea el puntero y `relative-pointer` da deltas.
- La captura entrega frames por dmabuf y la app puede elegir monitor/ventana.

## Notas

- Referencias del shake-to-locate: macOS lo llama **Shake mouse pointer to
  locate**, crece el puntero a ~3× y lo encoge en cuanto el ratón se detiene.
  Hyprland **no** lo trae en el core; lo implementa el plugin
  `hypr-dynamic-cursors` (GPL-3.0, por eso no se copia código), con 4×,
  crecimiento mientras sigues agitando y 2 s de espera tras parar. Yuki sigue
  el comportamiento de macOS: 3× y encogido inmediato.
- `wp_presentation` no se ha probado todavía con un cliente que pida feedback;
  `wayland-info` solo verifica el global. Falta un probe pequeño para el evento
  `presented`/`discarded`.
- `xdg-activation` no tiene todavía un probe automatizado de dos ventanas. La
  política es deliberadamente estricta: tokens sin superficie, sin serial o con
  un serial que no sea el último teclado/botón se descartan; si una app usa un
  serial de foco en vez de input, no activará. Es el límite documentado.
- El gesto de agitar no se puede provocar con el input scriptado actual; el
  detector y sus umbrales tienen tests unitarios, pero falta confirmación
  visual en el panel real.
