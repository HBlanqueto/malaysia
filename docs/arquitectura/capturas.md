# Capturas de pantalla

> Mapa general: [`interfaz.md`](interfaz.md).

Capacidad del compositor, no proceso de shell: compone la escena, la renderiza offscreen y guarda el PNG o lo publica en el portapapeles.

## Atajos

`Command` de macOS es **Super**.

| Atajo | Efecto |
|---|---|
| `Shift+Super+3` | Pantalla completa. |
| `Shift+Super+4` | Región: cruz, arrastre y suelta. |
| `Shift+Super+4`, `Espacio` | Modo ventana; un clic captura la resaltada. |
| `Esc` | Cancela. |
| `PrintScreen` | Ventana enfocada (o la salida si no hay ninguna). |

Mientras hay selección, teclado y puntero los consume el compositor.

## Flujo

```
input.rs → ScreenshotMode (state.rs) → overlay (session.rs)
  → Output | Region | Window | Focused
  → render offscreen o superficie del cliente
  → PNG en el Escritorio XDG o portapapeles (capture.rs)
```

- La región se normaliza y se recorta a la salida; un arrastre de tamaño cero no captura.
- Salida y región se re-renderizan con la escala de la salida, no leen el framebuffer presentado. Ventana renderiza solo la superficie del cliente.
- `target = "desktop"` usa `xdg::desktop_dir()` (nombre localizado). El nombre es `Screenshot AAAA-MM-DD at HH.MM.SS.png`, con ` (2)`, ` (3)`… si coincide. Ventana: PNG transparente; salida/región: opaco.
- `target = "clipboard"` no escribe fichero: publica `image/png` en la selección del asiento; el compositor sirve los bytes en un hilo hasta que otro cliente copie.
- `show_pointer` decide si el puntero sale en salida/región (default `false`); la ventana nunca lo incluye.
## Configuración

`~/.config/malaysia/screenshot.domain`; catálogo en [`../configuracion/opciones.md`](../configuracion/opciones.md). Hoy están activas `show_pointer` y `target` (`desktop`/`clipboard`). `show_thumbnail`, `location`, `type`, `include_date`, `disable_shadow`, `timer` y `remember_selection` se leen y validan, pero el flujo actual guarda en el Escritorio, en PNG y con fecha.

La lógica vive en `input.rs` (atajos), `state.rs` (modo y petición), `session.rs` (overlay y guardado), `capture.rs` (render, PNG y destino), `xdg.rs` (Escritorio), `config.rs` (opciones) y `handlers.rs` (protocolo).
## Capturas por protocolo

Apps y el futuro portal usan `ext-image-copy-capture-v1` con fuente de salida o de ventana. Comparten el render offscreen, pero el frame va al cliente y no pasa por `screenshot.domain`. El portal es el Bloque 12a.
## Límites

- Miniatura, temporizador, formatos, sombra de ventana y destinos `preview`/`mail`/`messages`/`other` están reservados.
- `Control` para copiar al vuelo aún no existe; se usa `target = "clipboard"`.
- No hay gestor de portapapeles: la imagen dura hasta que otro cliente copie.
- La captura de ventana no incluye chrome ni sombra; la salida rotada se ignora.
- La ruta dmabuf del protocolo está implementada, pero los probes usan SHM.
- El volcado `SIGUSR1` escribe en `$XDG_RUNTIME_DIR/malaysia` y no es la vía de usuario.
## Verificación

```sh
./dev.sh demo
MALAYSIA_TEST_SCREENSHOT=region ./dev.sh smoke   # o =1 para ventana
MALAYSIA_SCREENSHOT_DIR=/tmp/shots ./dev.sh smoke
WAYLAND_DISPLAY=wayland-N wl-paste --type image/png > /tmp/clip.png
```

Contexto: [`../investigacion.md`](../investigacion.md).
