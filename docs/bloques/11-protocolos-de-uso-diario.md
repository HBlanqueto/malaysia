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
  `yuki-design`. Para que el aumento no se vea pixelado, el cursor se carga
  dos veces (tamaño normal y tamaño del shake) y el grande se muestrea en vez
  de ampliar el bitmap normal; `YUKI_TEST_CURSOR_BOOST=3` fija el aumento para
  verlo en una captura.
- `zwp_pointer_constraints_v1` v1: el global está anunciado y el handler activa
  la constraint en cuanto llega sobre la superficie con foco del puntero. Una
  constraint activa manda: `locked` deja el puntero quieto y solo alimenta
  `relative_motion`; `confined` limita el movimiento a la región de la
  superficie (deslizando por los bordes). Una constraint dormida se activa al
  entrar en su región, y Smithay la desactiva sola al perder el foco.
- `zwp_relative_pointer_manager_v1` v1: cada movimiento del puntero reenvía su
  delta a los objetos `zwp_relative_pointer_v1` del cliente con foco.
- **Captura de salida**: `ext-output-image-capture-source-v1` +
  `ext-image-copy-capture-v1`. El compositor anuncia las constraints de la
  sesión (tamaño del modo, `Argb8888`/`Xrgb8888` y, si el backend encontró un
  render node, los formatos dmabuf con ese nodo). `frame()` encola la petición
  (el handler no ve el renderer, ADR 0012) y el backend la completa en el
  siguiente frame: re-renderiza offscreen con `Transform::Flipped` (cancela el
  framebuffer GL bottom-up) y copia a un SHM por filas o renderiza directo al
  dmabuf del cliente. Probe: `./dev.sh screencopy`.
- **Captura de ventana**: `ext-foreign-toplevel-image-capture-source-v1`. La
  fuente guarda el `WindowId` en el `ext-foreign-toplevel-list` handle y el
  compositor dibuja solo la superficie del cliente, a su tamaño usable. Probe:
  `./dev.sh screencopy-window`.

- **Capturas al estilo macOS** (Command = Super en Linux):
  - `Shift+Super+3`: captura la **pantalla completa**.
  - `Shift+Super+4`: el puntero pasa a cruz; arrastra para seleccionar una
    **región** y suelta para capturarla. `Esc` cancela. La selección se ve con
    el exterior atenuado y un borde blanco.
  - `Shift+Super+4`, luego `Espacio`: modo **ventana**; al pasar el puntero se
    resalta la ventana y un clic la captura.
  - `PrintScreen`: atajo extra que captura la **ventana con foco** (o la salida
    si no hay ninguna).
  - El destino es el Escritorio del usuario vía `yuki::xdg::desktop_dir()`
    (XDG user-dirs, nombre localizado: Escritorio, Desktop, Schreibtisch…),
    nunca un literal. El archivo se llama
    `Screenshot AAAA-MM-DD at HH.MM.SS.png` (con ` (2)`, ` (3)` si coincide en
    el mismo segundo); fondo transparente en ventana, opaco en salida/región.
  - Para pruebas, `YUKI_TEST_SCREENSHOT=1` captura la ventana enfocada al
    mapear la primera ventana, `YUKI_TEST_SCREENSHOT=region` captura un
    rectángulo fijo (100,100 300×200) y `YUKI_SCREENSHOT_DIR` redirige el
    guardado (así la verificación no toca el Escritorio real).

## Pendiente

- El portal `Screenshot`/`ScreenCast` del Bloque 12 sobre esta captura.
- Del flujo macOS faltan: la app de `Shift+Super+5` (temporizador, opciones,
  destino, miniatura flotante), copiar al portapapeles con `Control`, mover la
  selección con Espacio mientras se arrastra, excluir la sombra con `Option` y
  que la captura de salida/región omita el puntero por defecto.
- Opcional: `keyboard-shortcuts-inhibit` para escritorio remoto.

## Aceptación pendiente

- Un cliente con `wp_presentation` recibe `presented` con un timestamp
  monótono y un `refresh` razonable: probado con `./dev.sh presentation`
  (timestamp monótono, `refresh` 16666000 ns, flag `Vsync`).
- Una app activa otra con un token válido y una ventana existente sube y
  recibe foco; un token viejo, sin foco o sin serial no activa: la vía positiva
  está probada con `./dev.sh activation` (dos ventanas del mismo cliente, token
  con el serial del press scriptado y foco vuelve a la primera); los rechazos
  tienen tests unitarios.
- Un cliente pide `cursor-shape` y el cursor cambia sin enviar un buffer;
  wezterm pide `Text` en el primer enter y Yuki lo aplica.
- Al agitar el ratón el cursor crece con animación suave hasta 3× y, al
  dejar de moverse, empieza a encogerse de inmediato.
- Un juego confina/bloquea el puntero y `relative-pointer` da deltas: probado
  con `./dev.sh pointers` (`pointer_lock` recibe `locked` sobre la superficie
  con foco; `relative_pointer` recibe un delta no nulo).
- La captura entrega frames: probado con `./dev.sh screencopy` (salida, SHM,
  píxel central igual al color de fondo y no en blanco) y
  `./dev.sh screencopy-window` (ventana, píxel central igual al contenido gris
  de la ventana). Las constraints anuncian dmabuf con el render node, pero
  falta un cliente que capture por dmabuf.
- Falta la captura con salida rotada (hoy se ignora la rotación).
- PrintScreen probado con `YUKI_TEST_SCREENSHOT=1` y `YUKI_SCREENSHOT_DIR`:
  PNG RGBA de la ventana (1248×800 con foot), contenido correcto y guardado en
  el directorio indicado.
- Región probada con `YUKI_TEST_SCREENSHOT=region`: PNG de 300×200, el
  rectángulo exacto pedido.

## Notas

- Referencias del shake-to-locate: macOS lo llama **Shake mouse pointer to
  locate**, crece el puntero a ~3× y lo encoge en cuanto el ratón se detiene.
  Hyprland **no** lo trae en el core; lo implementa el plugin
  `hypr-dynamic-cursors` (GPL-3.0, por eso no se copia código), con 4×,
  crecimiento mientras sigues agitando y 2 s de espera tras parar. Yuki sigue
  el comportamiento de macOS: 3× y encogido inmediato.
- `wp_presentation` se prueba con `./dev.sh presentation`: el probe pide
  feedback, commitea y comprueba que llega `presented` con timestamp monótono y
  `refresh`. No cubre `discarded` (haría falta un frame sin daño).
- `xdg-activation` se prueba con `./dev.sh activation`: dos ventanas del mismo
  cliente, token con el serial del press scriptado, y foco vuelve a la primera.
  El rechazo (sin foco, serial viejo o fabricado) lo cubren los tests unitarios
  de la política, no el probe.
- El gesto de agitar no se puede provocar con el input scriptado actual; el
  detector y sus umbrales tienen tests unitarios, pero falta confirmación
  visual en el panel real.
- La captura dmabuf renderiza directo al buffer del cliente con
  `renderer.bind(&mut dmabuf)`; no hay un cliente que la ejercite todavía, así
  que esa ruta está implementada pero sin probar. La de SHM sí pasa por
  `./dev.sh screencopy` y `./dev.sh screencopy-window`.
- El volcado PNG de depuración (`SIGUSR1`) sigue escribiendo en
  `$XDG_RUNTIME_DIR/yuki`; no es la vía de guardado del usuario. El portal
  (Bloque 12a) usará `yuki::xdg::desktop_dir()` para el destino por defecto de
  las capturas, que sale de XDG (localizado) y no de un nombre fijo.
