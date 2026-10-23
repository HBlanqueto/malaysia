# Bloque 11 — Protocolos de uso diario

Estado: **aceptado en anidado el 2026-10-23** (winit y probes). Falta repetir
la presentación en DRM real; `drm-vkms` ya ejecuta el probe y exige `refresh`
no nulo y `Vsync`, pero no se pudo lanzar en esta máquina por falta de `sudo`.

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
  y guarda el flag `paint_cursors` de la sesión (el handler no ve el renderer,
  ADR 0012); el backend la completa en el siguiente frame: re-renderiza
  offscreen con `Transform::Flipped` y copia a un SHM por filas o renderiza
  directo al dmabuf del cliente, incluyendo o no el cursor según ese flag.
  Probe: `./dev.sh screencopy`, que alterna SHM/dmabuf y cursor activado/omitido.
- **Captura de ventana**: `ext-foreign-toplevel-image-capture-source-v1`. La
  fuente guarda el `WindowId` en el `ext-foreign-toplevel-list` handle y el
  compositor dibuja solo la superficie del cliente, a su tamaño usable. En una
  ventana SSD la titlebar de Yuki queda **fuera** de la captura: con una ventana
  de 1280×800 y titlebar de 32 px, el contenido configurado y el frame capturado
  miden 1280×768. Probe: `./dev.sh screencopy-window` (SHM y dmabuf).

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
  - Las opciones viven en `~/.config/yuki/screenshot.domain` con los nombres de
    la app Captura y `com.apple.screencapture` (`show_pointer`, `target`,
    `type`, `include_date`, `disable_shadow`, `timer`, `show_thumbnail`,
    `remember_selection`, `location`). Defaults macOS: puntero oculto,
    Escritorio, PNG, fecha. `show_pointer` ya se aplica en salida/región y
    `target = "clipboard"` copia `image/png` al portapapeles (el compositor es
    el dueño y sirve los bytes); las demás se leen y validan pero el flujo
    actual sigue guardando en el Escritorio en PNG con fecha.
  - Para pruebas, `YUKI_TEST_SCREENSHOT=1` captura la ventana enfocada al
    mapear la primera ventana, `YUKI_TEST_SCREENSHOT=region` captura un
    rectángulo fijo (100,100 300×200) y `YUKI_SCREENSHOT_DIR` redirige el
    guardado (así la verificación no toca el Escritorio real).

## Pendiente

- El portal `Screenshot`/`ScreenCast` del Bloque 12 sobre esta captura.
- Del flujo macOS faltan: la app de `Shift+Super+5` (temporizador, opciones,
  destino, miniatura flotante), copiar al portapapeles con `Control` al vuelo,
  mover la selección con Espacio mientras se arrastra y excluir la sombra con
  `Option`. El destino `clipboard` ya está en `screenshot.domain`.
  El puntero ya se puede omitir con `screenshot.show_pointer` (default `false`,
  como macOS) y las demás opciones de `screenshot.domain` quedan reservadas.
- Opcional: `keyboard-shortcuts-inhibit` para escritorio remoto.

## Aceptación (anidado, 2026-10-23)

- **`wp_presentation`**: `./dev.sh presentation` commitea tres frames (dos con
  daño y uno sin daño) y pide feedback en cada uno. En winit los tres llegan
  como `presented`, con timestamps crecientes (`0.163826178`, `0.167814207`,
  `0.185793123`), `refresh` 16666000 ns, secuencia creciente y `Vsync`. El
  frame sin daño también recibió respuesta (presented), que es una de las dos
  salidas aceptadas; nunca se quedó sin feedback.
- **`xdg-activation`**: `./dev.sh activation` usa dos procesos. A mapea
  primero, recibe el clic scriptado y pide cinco tokens con foco; B mapea
  después. B activa su propia ventana con el token válido y recibe el foco. Un
  token sin serial, un token pedido por la superficie sin foco de B y un token
  usado después de 31 s no mueven el foco a B.
- **Captura de salida**: `./dev.sh screencopy` pasa las cuatro combinaciones
  SHM/dmabuf × cursor incluido/omitido. La escena está vacía, así que el probe
  compara los píxeles con el fondo: sin `paint_cursors` hay 0 píxeles distintos;
  con el flag hay 185 (el cursor). La ruta dmabuf usa `/dev/udmabuf` con el
  formato y modificador lineales anunciados por la sesión.
- **Captura de ventana**: `./dev.sh screencopy-window` pasa por SHM y dmabuf y
  verifica el gris del contenido. Con `CAPTURE_SSD=1` la ventana pide
  decoración de servidor: el contenido se configura a 1280×768 y el frame
  capturado mide exactamente 1280×768, sin los 32 px de titlebar.
- **Pendiente en vkms/panel real**: `./dev.sh drm-vkms` compila y ejecuta
  `presentation` con `PRESENTATION_REQUIRE_VSYNC=1`, pero no se pudo lanzar aquí
  porque la máquina pide contraseña para `sudo`. Falta confirmar en el ThinkPad
  que el DRM real entrega `refresh` y `Vsync` en vblank.
- **Pendiente**: captura con salida rotada (hoy se ignora la rotación).

## Notas

- Referencias del shake-to-locate: macOS lo llama **Shake mouse pointer to
  locate**, crece el puntero a ~3× y lo encoge en cuanto el ratón se detiene.
  Hyprland **no** lo trae en el core; lo implementa el plugin
  `hypr-dynamic-cursors` (GPL-3.0, por eso no se copia código), con 4×,
  crecimiento mientras sigues agitando y 2 s de espera tras parar. Yuki sigue
  el comportamiento de macOS: 3× y encogido inmediato.
- `wp_presentation` se prueba con `./dev.sh presentation`: pide feedback en
  cada commit, imprime timestamp, refresh, secuencia y flags, y acepta
  `discarded` en el frame sin daño. En `drm-vkms` exige además refresh no nulo y
  `Vsync`.
- `xdg-activation` se prueba con `./dev.sh activation`: dos clientes distintos,
  token con el serial del press scriptado, y los tres rechazos (sin serial, sin
  foco y caducado) se ejercitan contra el compositor, no solo en tests puros.
- El gesto de agitar no se puede provocar con el input scriptado actual; el
  detector y sus umbrales tienen tests unitarios, pero falta confirmación
  visual en el panel real.
- La captura dmabuf renderiza directo al buffer del cliente con
  `renderer.bind(&mut dmabuf)`; `./dev.sh screencopy` y
  `./dev.sh screencopy-window` la ejercitan con un buffer lineal creado por
  `/dev/udmabuf`. El probe solo acepta modificador lineal; si el compositor no
  anuncia ninguno, falla con un mensaje explícito.
- El volcado PNG de depuración (`SIGUSR1`) sigue escribiendo en
  `$XDG_RUNTIME_DIR/yuki`; no es la vía de guardado del usuario. El portal
  (Bloque 12a) usará `yuki::xdg::desktop_dir()` para el destino por defecto de
  las capturas, que sale de XDG (localizado) y no de un nombre fijo.
