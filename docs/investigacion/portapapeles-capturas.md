# Portapapeles de capturas: Otto, GNOME y macOS

Cómo ponen una captura en el portapapeles tres escritorios y qué se adoptó en
Yuki. Complementa [`../arquitectura/capturas.md`](../arquitectura/capturas.md)
(flujo y opciones) y [`../bloques/06-portapapeles.md`](../bloques/06-portapapeles.md)
(el portapapeles de texto entre clientes).

## El modelo Wayland

El portapapeles es **pull**: quien copia no envía los bytes a nadie, solo
publica una fuente (`wl_data_source`) con una lista de MIME types. Cuando otro
cliente pega, el compositor le da un `fd` y el dueño escribe ahí. Consecuencias:

- Los datos viven en el dueño de la selección. Si el dueño muere o la reemplaza,
  el portapapeles se pierde. Para persistencia hacen falta gestores externos vía
  `wlr-data-control`/`ext-data-control` (wl-clip-persist, cliphist, clipman).
- La selección se ofrece por asiento y sigue el **foco de teclado** del
  `wl_data_device`; un cliente sin foco no ve la selección.
- Una imagen se ofrece como `image/png` (algunos clientes piden además
  `image/bmp`, `image/tiff` o `text/uri-list`; `image/png` es el mínimo común).

En Smithay, un compositor puede ser el dueño sin crear un cliente:
`set_data_device_selection(display, seat, mime_types, user_data)` y
`SelectionHandler::send_selection`, que recibe el `fd` de cada pega. Es el mismo
camino que usa Yuki para las capturas.

## Otto

- `components/otto-kit/src/clipboard.rs` es un portapapeles **de cliente**
  (apps de Otto), no del compositor. Publica `(mime, bytes)` con un `serial`
  real, guarda la oferta mientras la posee, la limpia cuando el compositor
  cancela la fuente, y lee con límites (500 ms y 8 MiB) para no colgar la UI.
  Soporta texto, listas de ficheros y el formato de GNOME
  `x-special/gnome-copied-files`.
- Su portal de capturas (`xdg-desktop-portal-otto/src/portal/screenshot.rs`,
  `docs/developer/screenshot-plan.md`) ejecuta `grim` y **guarda un PNG**; el
  portal devuelve un `file://`. No copia al portapapeles: la app que llama
  decide si lo guarda o lo copia.
- La persistencia la delega en gestores externos con `wlr-data-control`
  (`docs/user/clipboard.md`: wl-clip-persist, cliphist, clipman).

Lección: el patrón de MIME y cancelación es correcto, pero para una captura
hecha por el compositor no hace falta un cliente que posea la selección: Yuki
ya tiene los bytes y puede ser el dueño.

## GNOME

- GNOME Shell, en `js/ui/screenshot.js`, **siempre** pone el PNG en el
  portapapeles (`St.Clipboard.set_content(CLIPBOARD, 'image/png', bytes)`) y
  además lo guarda en disco salvo que el lockdown lo impida. La notificación
  dice explícitamente que se puede pegar desde el portapapeles.
- `St.Clipboard` es la fachada de `MetaSelection` de Mutter: el dueño de la
  selección es el propio shell/compositor, con los bytes en su proceso; también
  puentea la selección a XWayland.
- El portal `Screenshot` de GNOME, cuando no se pide fichero, escribe el PNG en
  un stream y lo copia al portapapeles (`_onScreenshotComplete`).

Lección: el compositor como dueño funciona y es lo que hace GNOME. La
diferencia es de UX: GNOME copia **y** guarda; macOS copia **en vez de**
guardar cuando se lo pides.

## macOS

- Atajos: `Ctrl+Shift+Cmd+3/4` copia al portapapeles en vez de guardar; la app
  Captura (`Shift+Cmd+5`) tiene "Save to: **Clipboard**". `screencapture -c`
  hace lo mismo desde la terminal.
- El destino es `NSPasteboard.general` (el *general pasteboard*). Apple suele
  publicar **PNG** (formato por defecto de `screencapture`) y **TIFF** para
  compatibilidad; las apps leen con `NSPasteboard`/`NSImage`/`NSData`.
- El *pasteboard server* del sistema es el dueño real: el contenido sobrevive a
  la app que lo copió, al contrario que el modelo pull de Wayland. Hay
  *promised data* y sincronización con Universal Clipboard.
- `pbpaste` solo lee texto; para imágenes se usa `pngpaste` o
  `osascript -e 'the clipboard as «class PNGf»'`.
- Por defecto el puntero **no** sale en la captura; `screencapture -C` lo
  incluye.

Lección: la opción de destino y el modificador `Control` son parte de la UX de
macOS, no un detalle. Yuki adopta el destino configurable y deja `Control` para
la siguiente pasada.

## Decisión en Yuki

- `screenshot.domain` → `target = "desktop" | "clipboard"`; el default es
  Escritorio, como macOS.
- `target = "clipboard"` renderiza la captura igual que para fichero, la
  codifica en PNG y la publica como `image/png` con
  `set_data_device_selection`. **No escribe fichero.**
- El compositor es el dueño de la selección; los bytes se escriben al `fd` de
  cada pega en un hilo, para no bloquear el bucle de eventos.
- La selección dura hasta que otro cliente copie (no hay gestor de
  portapapeles todavía). `wl-paste --type image/png` la lee.
- `preview`, `mail`, `messages` y `other` siguen reservados y caen al
  Escritorio; `Control` para copiar al vuelo queda pendiente.

## Referencias

- Otto: [`components/otto-kit/src/clipboard.rs`](https://github.com/nongio/otto/blob/main/components/otto-kit/src/clipboard.rs),
  [`docs/user/clipboard.md`](https://github.com/nongio/otto/blob/main/docs/user/clipboard.md),
  [`portal/screenshot.rs`](https://github.com/nongio/otto/blob/main/components/xdg-desktop-portal-otto/src/portal/screenshot.rs).
- GNOME Shell: [`js/ui/screenshot.js`](https://gitlab.gnome.org/GNOME/gnome-shell/-/blob/main/js/ui/screenshot.js)
  (`_storeScreenshot` y `_onScreenshotComplete`).
- Mutter: `src/wayland/meta-wayland-data-device.c`, `MetaSelection`.
- macOS: [`screencapture(1)`](https://ss64.com/mac/screencapture.html),
  [Take a screenshot on Mac](https://support.apple.com/guide/mac-help/take-a-screenshot-mh26782/mac),
  [`NSPasteboard`](https://developer.apple.com/documentation/appkit/nspasteboard).


## Nota tras el ADR 0034

`freya-clipboard` (la dependencia que trae Freya) usa `copypasta`, que arrastra
`x11rb` y una segunda versión de SCTK/calloop. CelestialKit no lo usa: el
portapapeles del kit queda vacío hasta que un control lo necesite, y entonces
debe ir por `wl_data_device` propio. El guard
`scripts/check-client-only-deps.sh` lo avisa en cada `./dev.sh check`.
