# Ventanas y titlebar (Tengen)

> Mapa general: [`interfaz.md`](interfaz.md).

La titlebar SSD es del compositor (ADR 0020) y usa Tengen: barra arriba de 32 px lógicos, semáforos de 12 px (radio 4) con 8 px de separación y radio de ventana 16.

## Qué se dibuja

- Fondo de barra `titlebar_bg` (focused/unfocused) y hairline de 1 px al pie.
- Semáforos cerrar/minimizar/maximizar a la izquierda, con `traffic-close`/`traffic-min`/`traffic-zoom`; grises cuando la ventana no tiene foco.
- Título centrado, rasterizado con `cosmic-text` y coloreado con `title.focused`/`title.unfocused`.
- Marco para todas las ventanas: esquinas de 16, sombra (`shadow.frag`), borde exterior (`border.frag`) e inner hairline. Fullscreen elimina el marco; maximizado lo conserva (ADR 0037).
- Los diálogos usan el mismo marco y se centran sobre la ventana padre.

Solo las ventanas SSD reciben titlebar (ADR 0014/0019). GTK/Libadwaita y los clientes que piden CSD conservan su chrome; `window.decorations = "force_server"` puede forzar la barra en apps ajenas (con riesgo de duplicarla). Telegram (Qt) puede necesitar desactivar su marco en Ajustes › Avanzado; con `window.qt_server_decorations = true` (default) el compositor pide server-side.

## Apps propias y CSD (ADR 0035)

Una app de BarrierKit usa `app_id` con prefijo `io.github.hblanqueto.malaysia.` y pide su modo por `xdg-decoration` antes del primer commit:

- `Decorations::Client` (default): barra propia de 44 px con `TrafficLights` y hueco de toolbar; para apps con toolbar/sidebar (Ajustes).
- `Decorations::Server`: el compositor dibuja la barra de 32 px; para utilidades simples.

Para estas ventanas el compositor respeta el modo pedido; `force_server`/`force_client` solo afectan a apps ajenas. Si falta `xdg-decoration`, el kit dibuja su barra de 44 px. El prefijo no autentica nada.

## Resize por bordes

Las ventanas SSD normales (no maximizadas ni fullscreen) se redimensionan desde una banda exterior: `window.resize_border` (10 px) en los lados y una «L» de `window.resize_corner` (20 px) en las esquinas, con 2 px de tolerancia hacia dentro. El hit-test recorre la pila: la ventana superior gana y su contenido bloquea la banda de una inferior. Los semáforos se comprueban antes que el resize. Las CSD gestionan sus propios bordes.

## Color y semáforos

`appearance.window_color` tiñe la titlebar:

| Modo | Tinte | Semáforos `auto` |
|---|---|---|
| `neutral` | ninguno | estándar |
| `accent` | acento, 6 % claro / 8 % oscuro | derivados del acento |
| `wallpaper` | dominante, 7 % / 9 % | derivados del wallpaper |

El dominante lo calcula `malaysia-wallpaper` y lo escribe en `wallpaper.state`; sin color se usa el acento. `appearance.traffic_lights` fuerza `standard`, `accent` o `wallpaper`. `window_tint_scope = "desktop"` tiñe menubar y dock (20 %/26 %).

## Botones, hover y doble clic

- Cerrar → `xdg_toplevel.close`; minimizar → sale de la escena y queda en el dock; maximizar → guarda el rectángulo, ocupa el workspace y envía `maximized`; repetir restaura.
- Al entrar el puntero en el grupo, los tres semáforos se encienden y muestran su glifo (aspa, línea o dos triángulos); se escalan con la barra.
- Pulsar oscurece solo si el puntero sigue sobre ese botón. Un zoom no admitido (ventana de tamaño fijo) se dibuja gris y sin glifo.
- Doble clic en la barra (400 ms, 4 px) ejecuta `window.double_click_titlebar`: `zoom`/`fill` maximizan, `minimize` minimiza, `none` no hace nada.

## Límites actuales

- La titlebar no aparece en la captura de **ventana**; sí en la de salida/región. El blur de fondo aún no se dibuja. La captura con marco sigue pendiente (roadmap, Bloque 13).
