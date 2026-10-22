# Capturas de pantalla (`Shift+Super+3/4`, PrintScreen)

Las capturas son una capacidad del escritorio al mismo nivel que el
[wallpaper](wallpaper.md), la [menubar](menubar.md) o el [dock](dock.md), pero
**no son un proceso de la shell**: las hace el compositor. La shell no
participa; Yuki compone la escena, la vuelve a renderizar offscreen y guarda el
PNG. Es el equivalente a la app **Captura** de macOS y a
`com.apple.screencapture`, con los atajos de macOS.

## Atajos

`Command` de macOS es **Super** en Linux.

| Atajo | Efecto |
|---|---|
| `Shift+Super+3` | Captura la pantalla completa. |
| `Shift+Super+4` | El puntero pasa a cruz; arrastra para elegir una región y suelta para capturarla. |
| `Shift+Super+4`, luego `Espacio` | Modo ventana: al pasar el puntero se resalta la ventana y un clic la captura. |
| `Esc` | Cancela la selección en curso. |
| `PrintScreen` | Atajo extra: captura la ventana con foco (o la salida si no hay ninguna). |

Mientras hay una selección activa, el teclado y el puntero los consume el
compositor: los clientes no ven `Esc`, `Espacio`, el arrastre ni el clic.
`Shift+Super+3` funciona aunque no haya ninguna ventana enfocada.

## Flujo

```
teclado (input.rs)
   │  Shift+Super+3/4 / PrintScreen
   ▼
ScreenshotMode (state.rs) ──► overlay: exterior atenuado + borde blanco
   │  arrastre / Espacio / clic / Esc                 (session.rs)
   ▼
ScreenshotRequest { Output | Region | Window | Focused }
   │
   ▼
save_screenshot (session.rs)
   ├── Output/Region: build_output_elements(show_pointer) + render_and_read a la escala de salida
   ├── Window: surface del cliente + render_and_read
   └── PNG → fichero en el Escritorio XDG o portapapeles (capture.rs)
```

- **Selección**: el exterior se atenúa y la región lleva un borde blanco de
  1 px lógico. En modo ventana se resalta el rectángulo exterior de la ventana
  bajo el puntero. Los valores viven en `yuki-design`.
- **Región**: se normaliza (arrastrar hacia atrás) y se recorta al tamaño de la
  salida. Un arrastre de tamaño cero no captura: se puede reintentar o cancelar.
- **Render**: salida y región re-renderizan la escena completa en offscreen con
  el `Transform` del backend y la escala de la salida (no leen el framebuffer
  presentado). Ventana renderiza solo la superficie del cliente.
- **Guardado**: con `target = "desktop"` (default), `yuki::xdg::desktop_dir()`
  resuelve el Escritorio desde XDG user-dirs (nombre localizado: Escritorio,
  Desktop, Schreibtisch…), nunca un literal. El nombre es
  `Screenshot AAAA-MM-DD at HH.MM.SS.png`, con ` (2)`, ` (3)`… si coincide en
  el mismo segundo. Ventana: PNG con fondo transparente; salida/región: opaco.
- **Portapapeles**: con `target = "clipboard"`, el PNG no se escribe: se
  publica como `image/png` en la selección del asiento. Los bytes los sirve el
  compositor en cada pega, en un hilo, hasta que otro cliente copie. Ver
  [`../investigacion/portapapeles-capturas.md`](../investigacion/portapapeles-capturas.md).
- **Puntero**: `screenshot.show_pointer` decide si sale en salida/región. El
  default es `false`, como macOS. Las capturas de ventana nunca lo incluyen.

## Configuración

Dominio propio, como macOS: `~/.config/yuki/screenshot.domain`. El catálogo
canónico (tipos, valores y estado de cada opción) está en
[`../configuracion/opciones.md`](../configuracion/opciones.md#screenshot--capturas-app-captura-macos-comapplescreencapture).

Defaults macOS: puntero oculto, guardar en el Escritorio, PNG con fecha,
miniatura flotante sí, temporizador desactivado.

| Opción | Default | Estado |
|---|---|---|
| `show_pointer` | `false` | **activa** en salida/región |
| `show_thumbnail` | `true` | reservada (miniatura flotante) |
| `target` | `"desktop"` | **activa** para `desktop` y `clipboard`; `preview`, `mail`, `messages` y `other` reservadas |
| `location` | — | reservada (carpeta propia) |
| `type` | `"png"` | reservada (`jpg`, `heif`, `tiff`, `pdf`) |
| `include_date` | `true` | reservada (hoy siempre fecha) |
| `disable_shadow` | `false` | reservada (hoy la captura de ventana no dibuja sombra) |
| `timer` | `0` | reservada (`0`, `5`, `10` s) |
| `remember_selection` | `false` | reservada |

«Reservada» significa que la opción se lee, se valida y se documenta, pero el
flujo actual sigue guardando en el Escritorio, en PNG y con fecha. `target` ya
distingue Escritorio de portapapeles; el resto de destinos cae al Escritorio.

## Dónde vive

| Pieza | Archivo |
|---|---|
| Atajos, interceptación de teclado/puntero | `crates/yuki/src/input.rs` |
| `ScreenshotMode`, `ScreenshotRequest`, rect de región | `crates/yuki/src/state.rs` |
| Overlay, `save_screenshot`, captura de ventana/salida | `crates/yuki/src/session.rs` |
| Render offscreen, recorte, PNG, nombre y destino | `crates/yuki/src/capture.rs` |
| Escritorio XDG localizado | `crates/yuki/src/xdg.rs` |
| Opciones `screenshot.*` | `crates/yuki/src/config.rs` |
| Captura por protocolo (`ext-image-copy-capture-v1`) | `handlers.rs` + `capture.rs` |

## Capturas por protocolo

Los atajos son la vía del usuario. Las apps y el futuro portal usan
`ext-image-copy-capture-v1` con las fuentes de salida y de ventana
(`ext-output-image-capture-source-v1`,
`ext-foreign-toplevel-image-capture-source-v1`). Ambos caminos comparten el
render offscreen, pero el protocolo entrega el frame al cliente y **no** guarda
ficheros ni pasa por `screenshot.domain`. La integración con el portal
`Screenshot`/`ScreenCast` es el Bloque 12a.

## Límites y pendientes

- Miniatura flotante, temporizador, formatos, sombra de ventana y los destinos
  `preview`/`mail`/`messages`/`other`: opciones reservadas, aún sin efecto.
- `Control` para copiar al vuelo (como macOS) todavía no está; hoy se elige con
  `target = "clipboard"` en `screenshot.domain`.
- No hay gestor de portapapeles: la imagen dura hasta que otro cliente copie.
- La captura de ventana dibuja solo el contenido del cliente; el chrome
  server-side y su sombra todavía no forman parte de la imagen.
- Salida **rotada**: hoy la rotación se ignora en la captura.
- La ruta **dmabuf** de `ext-image-copy-capture-v1` está implementada pero no
  tiene un cliente de prueba (los probes usan SHM).
- HDR/HEIF/PDF no se escriben.
- El volcado de depuración por `SIGUSR1` es otra cosa: escribe en
  `$XDG_RUNTIME_DIR/yuki`, no en el Escritorio, y no es la vía de usuario.

## Verificación

```sh
./dev.sh demo                                  # sesión anidada con shell y ventanas
YUKI_TEST_SCREENSHOT=1 ./dev.sh smoke          # captura la ventana enfocada
YUKI_TEST_SCREENSHOT=region ./dev.sh smoke     # captura un rectángulo fijo (100,100 300x200)
YUKI_SCREENSHOT_DIR=/tmp/shots ./dev.sh smoke  # redirige el guardado
# target = "clipboard" en screenshot.domain; luego, dentro de la sesión:
WAYLAND_DISPLAY=wayland-N wl-paste --type image/png > /tmp/clip.png
```

`YUKI_TEST_SCREENSHOT=region` con `YUKI_TEST_INPUT=1` y `YUKI_TEST_PRESS_AT`
permite poner el puntero dentro del rectángulo y comparar las capturas con
`show_pointer = false` y `true`. Con `target = "clipboard"` no aparece ningún
fichero en el destino y `wl-paste --list-types` muestra `image/png`. La verificación del look del cursor ampliado
usa `YUKI_TEST_CURSOR_BOOST=3`. El histórico de la implementación y sus pruebas
está en [`../bloques/11-protocolos-de-uso-diario.md`](../bloques/11-protocolos-de-uso-diario.md).
