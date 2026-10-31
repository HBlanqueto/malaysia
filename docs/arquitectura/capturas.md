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
- Salida y región se re-renderizan con la escala de la salida; ventana renderiza solo la superficie del cliente.
- `target = "desktop"` usa `xdg::desktop_dir()` (nombre localizado) y nombre `Screenshot AAAA-MM-DD at HH.MM.SS.png`, con ` (2)`… si coincide. Ventana: PNG transparente; salida/región: opaco.
- `target = "clipboard"` no escribe fichero: publica `image/png` en la selección; el compositor sirve los bytes en un hilo hasta que otro cliente copie.
- `show_pointer` decide si el puntero sale en salida/región (default `false`).

## Configuración

`screenshot.domain`; hoy solo están activos `show_pointer` y `target` (`desktop`/`clipboard`). Miniatura, `location`, `type`, `include_date`, `disable_shadow`, `timer` y `remember_selection` se leen y validan, pero el flujo guarda en el Escritorio, en PNG y con fecha.
## Protocolo y límites

Apps y el portal usan `ext-image-copy-capture-v1`; el frame va al cliente y no pasa por `screenshot.domain`. Faltan `Control` al vuelo, gestor de portapapeles, captura con marco y salida rotada; la ruta dmabuf está implementada pero los probes usan SHM.