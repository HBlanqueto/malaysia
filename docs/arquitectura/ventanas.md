# Ventanas y titlebar (Tengen)

> Mapa general: [`interfaz.md`](interfaz.md).

La titlebar SSD es del compositor (ADR 0020) y usa Tengen: barra arriba de 32 px lógicos, semáforos de 14 px (radio 4) con 9 px de separación y radio de ventana 16.

## Qué se dibuja

- Fondo de barra `titlebar_bg` (focused/unfocused) y hairline de 1 px al pie.
- Semáforos cerrar/minimizar/maximizar a la izquierda, con `traffic-close`/`traffic-min`/`traffic-zoom`; grises cuando la ventana no tiene foco.
- Título centrado, rasterizado con `cosmic-text` y coloreado con `title.focused`/`title.unfocused`.
- Marco para todas las ventanas: esquinas continuas con el radio de su barra: 16 (titlebar o SSD), 20 (compacta), 26 (unificada, CSD sin aviso y diálogos) o 15 (utilitaria) (ADR 0059), también en las alertas de la shell (capa `malaysia-dialog`), sombra (`shadow.frag`), borde exterior (`border.frag`) e inner hairline. Fullscreen elimina el marco; maximizado lo conserva (ADR 0037).
- Los diálogos usan el mismo marco y se centran sobre la ventana padre.
- Una ventana hija (`set_parent`) queda siempre encima de su madre y de sus demás ancestros, como pide [xdg-shell](https://wayland.app/protocols/xdg-shell#xdg_toplevel:request:set_parent): al subir la madre suben con ella sus hijas, en el orden que tenían (`raise_order`).
- En oscuro, todas las ventanas, diálogos y popups llevan el borde interior de Tahoe: `panel-inner-edge` (22 %) arriba y abajo, que se funde con `panel-inner` (9 %) en los lados a lo largo del radio de la esquina.
- Los semáforos son cuadrados (radio 4) o redondos según `appearance.traffic_light_shape`.

Solo las ventanas SSD reciben titlebar (ADR 0014/0019). GTK/Libadwaita y los clientes que piden CSD conservan su chrome; `window.decorations = "force_server"` puede forzar la barra en apps ajenas (con riesgo de duplicarla). Telegram (Qt) puede necesitar desactivar su marco en Ajustes › Avanzado; con `window.qt_server_decorations = true` (default) el compositor pide server-side.

## Apps propias

Una app de BarrierKit usa `app_id` con prefijo `io.github.hblanqueto.malaysia.` y pide su modo por `xdg-decoration` antes del primer commit: `Client` dibuja su barra de 44 px (apps con toolbar/sidebar); `Server` deja la barra de 32 px al compositor (utilidades simples). `force_server`/`force_client` solo afectan a apps ajenas.
## Mover

Al arrastrar, la ventana no pasa por detrás de la barra de menús: su borde superior se detiene en el área de trabajo (`State::workspace`, que descuenta las zonas exclusivas de arriba), como en macOS. En una CSD cuenta el borde visible: se descuenta la sombra que la app dibuja dentro de su superficie (`window_geometry`). Los lados y el borde inferior pueden salir de la pantalla.

## Resize por bordes

Las ventanas SSD normales (no maximizadas ni fullscreen) se redimensionan desde una banda exterior: `window.resize_border` (10 px) en los lados y una «L» de `window.resize_corner` (20 px) en las esquinas, con 2 px de tolerancia hacia dentro. El hit-test recorre la pila: la ventana superior gana y su contenido bloquea la banda de una inferior. Los semáforos se comprueban antes que el resize. Las CSD gestionan sus propios bordes.

## Color

`appearance.window_color` tiñe la titlebar: `neutral` no tiñe, `accent` usa el acento (6 %/8 %) y `wallpaper` el dominante (7 %/9 %). `traffic_lights` fuerza `standard`, `accent` o `wallpaper`; `window_tint_scope = "desktop"` tiñe menubar y dock (20 %/26 %).
## Botones y doble clic

Cerrar cierra, minimizar deja el tile en el dock, maximizar guarda el rectángulo y envía `maximized`; repetir restaura. Los semáforos muestran su glifo al pasar el puntero y oscurecen solo el botón pulsado. Doble clic (400 ms, 4 px) ejecuta `window.double_click_titlebar`.

## Paneles

Una app declara panel una ventana con `infinity_window_style_v1.set_panel` (versión 3), como hacen las ventanas auxiliares de barrierkit. El compositor reproduce el `NSPanel` de AppKit:

1. **Nunca es la ventana principal.** Mientras el panel tiene el teclado, la ventana más alta de su app que no es panel sigue `activated` y es la que se publica en `window.domain` y como activa en foreign-toplevel, así que la barra de menús sigue con sus menús (`shown_active`).
2. **`hides_on_deactivate`:** el panel se oculta mientras la ventana activa es de otro cliente o no hay ninguna, y vuelve en el mismo sitio y orden cuando se activa una ventana de su app. Es un motivo de ocultación propio, separado de minimizar (`set_deactivated`). El foco en una capa de la shell no cuenta como cambio de app.
3. **`floating`:** el panel queda encima de todas las ventanas que no flotan (`raise_order`).
4. **`key_only_if_needed`:** ni al mapearse ni al pulsarlo recibe el teclado; solo sube. La app lo pide con un token de xdg-activation hecho con ese clic, que se acepta porque el teclado sigue en su mismo cliente.
5. Al cerrarse la ventana con el teclado, el foco pasa a la ventana más alta que no es panel (`focus_fallback`).

Fuentes: [`hidesOnDeactivate`](https://developer.apple.com/documentation/appkit/nswindow/hidesondeactivate), [`isFloatingPanel`](https://developer.apple.com/documentation/appkit/nspanel/isfloatingpanel), [`becomesKeyOnlyIfNeeded`](https://developer.apple.com/documentation/appkit/nspanel/becomeskeyonlyifneeded) y el estado `activated` de [xdg-shell](https://wayland.app/protocols/xdg-shell#xdg_toplevel:enum:state:entry:activated), que no implica tener el teclado.
