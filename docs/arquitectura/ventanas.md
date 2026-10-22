# Ventanas y titlebar (Neptunia)

> Mapa general de la interfaz: [`interfaz.md`](interfaz.md).

La titlebar de Yuki es **compositor-side** (ADR 0020) y usa el diseño de
**Neptunia** (ADR 0032): barra **arriba** de **32 px lógicos** para las ventanas
SSD de terceros, semáforos de 12 px
(**cuadrados redondeados** de radio 4) con 8 px de separación y radio de
ventana de 16 px.

## Qué se dibuja hoy

- Fondo de la barra con `titlebar_bg` (focused/unfocused) y una hairline de
  1 px al pie.
- Tres semáforos a la izquierda, en orden **cerrar / minimizar / maximizar**,
  como cuadrados redondeados de 12 px (radio 4), con los colores
  `traffic-close`, `traffic-min` y `traffic-zoom`; grises de
  `traffic_lights_inactive` cuando la ventana no tiene foco.
- El título de la ventana, centrado horizontal y verticalmente (la baseline se
  calcula desde la altura de línea), rasterizado con `cosmic-text` (misma
  fuente que el resto de la UI) y coloreado con `title.focused` /
  `title.unfocused`.

Solo reciben titlebar las ventanas **server-side** que reservan inset (ADR
0014/0019). Las apps GTK/Libadwaita y las que piden CSD conservan su propio
chrome; los diálogos, los popups y el fullscreen no llevan chrome. Wezterm, por
ejemplo, pide CSD y por eso no lleva la barra de Yuki; para forzarla está
`window.decorations = "force_server"`.

## Resize por bordes

Las ventanas **SSD normales** (no maximizadas ni fullscreen) se redimensionan
desde una banda **exterior** al rectángulo de la ventana: `window.resize_border`
(10 px por defecto) en los lados y una «L» de `window.resize_corner` (20 px) en
las esquinas, que puede entrar 2 px. No hay cuadrados grandes dentro del
contenido. El hit-test recorre la pila: la ventana de arriba gana y el contenido
de una ventana superior bloquea la banda exterior de otra inferior. Los
semáforos y la barra de título se comprueban antes que el resize.

Las ventanas **CSD** gestionan sus propios bordes; cuando llegue el pintor de
efectos (ADR 0020) podrán usar las mismas bandas exteriores. Ver
[`../investigacion/resize-bordes-y-cursores.md`](../investigacion/resize-bordes-y-cursores.md).

## Cómo funciona

```
WindowMap::iter (topmost → bottom)
  insets.top > 0  ──►  ChromeCache::element(id, título, foco, ancho, escala)
                          │
                          ├── ¿cambió la clave? → rasteriza RGBA (CPU) y sube MemoryRenderBuffer
                          └── MemoryRenderBufferRenderElement en la posición física
  superficie del cliente en (outer + insets)
```

- La caché es por ventana y se invalida al cambiar título, foco, ancho o
  escala; no se re-rasteriza por frame. Un `MemoryRenderBuffer` con formato
  `Abgr8888` lleva el RGBA ya en el orden que espera la textura.
- El orden de render de ventanas es **topmost primero**; la titlebar se añade
  justo antes de la superficie de su ventana para que el apilado sea correcto.
- `Windows::surface_under` solo devuelve la superficie del cliente cuando el
  punto cae dentro del **rectángulo útil** (outer menos insets). La banda de la
  titlebar no es superficie del cliente: así el foco de puntero no se va al
  cliente con coordenadas negativas y los clics de los botones llegan al
  compositor.
- El texto de la titlebar es un motor pequeño dentro del compositor
  (`crates/yuki/src/chrome.rs`); no usa `celestial-kit`, que es el toolkit de
  la shell (ADR 0024).
- La ventana se eleva al recibir foco; el foco y el orden de apilado no se
  separan.

## Color de las ventanas (Neptunia)

`appearance.window_color` elige qué tiñe la titlebar, con los factores de
Neptunia:

| Modo | Tinte | Semáforos en `auto` |
|---|---|---|
| `neutral` | ninguno | macOS fijos |
| `accent` | el acento, 6 % en claro / 8 % en oscuro | derivados del acento |
| `wallpaper` | el color dominante del wallpaper, 7 % / 9 % | derivados del wallpaper |

El color dominante lo calcula `yuki-shell wallpaper` al decodificar la imagen:
muestreo en rejilla, histograma de 4 bits ponderado por croma, descarte de grises
y normalización de luminosidad HSL a 45–65 %. Lo escribe en `wallpaper.state`;
si no hay color, se usa el acento. El compositor vigila ese archivo y
`appearance.domain` para reconstruir el tema en caliente.
`appearance.traffic_lights` permite forzar `macos`, `accent` o `wallpaper` en
vez de `auto`.

`appearance.window_tint_scope = "desktop"` tiñe menubar y dock con el dominante
del wallpaper (20 % claro / 26 % oscuro). El tema se reconstruye en vivo cuando
cambian `appearance.domain` o `wallpaper.state`.

## Botones

| Botón | Acción |
|---|---|
| Cerrar | `xdg_toplevel.close` |
| Minimizar | sale de la escena y queda en el dock (`State::set_minimized`) |
| Maximizar | guarda el rectángulo, ocupa el workspace, envía `maximized`; al repetir, restaura |

El hit-testing está en `chrome::button_at` y se comprueba antes de iniciar un
move; un clic en el resto de la barra mueve la ventana (el comportamiento de
siempre). Los botones solo actúan con el botón primario.

## Límites actuales

- No se dibujan aún las **esquinas redondeadas de la ventana** (16 px) ni la
  **sombra**; el radio y los valores de sombra ya están en `yuki-design`. Los
  semáforos sí son cuadrados redondeados.
- Los semáforos no muestran los glifos de macOS al pasar el puntero.
- No hay doble clic en la barra para maximizar/zoom.
- La titlebar no aparece en la captura de **ventana** (`build_window_elements`
  sigue capturando solo el contenido del cliente); sí aparece en la captura de
  salida/región.
- El cambio de tema en vivo (light/dark) aún no reconstruye la caché de chrome
  al editar `appearance.domain`; se aplica al arrancar.
- Los botones no respetan aún las variantes `accent`/`wallpaper` de Neptunia.

## Verificación

- `./dev.sh demo`: titlebar clara con título centrado y semáforos de color en la
  ventana con foco; la de atrás queda gris.
- `./dev.sh titlebar` lanza un `foot`, pulsa el centro exacto de cada botón
  (`YUKI_TEST_TITLEBAR_BUTTON`) y comprueba en el log `window removed`,
  `window minimized` y `window maximized`. Pasa a 1.0 y con
  `YUKI_SCALE=1.5` (los centros se calculan en lógico y el hit-test usa la
  misma geometría).
- `YUKI_SCALE=1.5 ./dev.sh demo`: la barra y el texto se rasterizan a la escala
  física.
- `./dev.sh check` pasa (tests de `button_at` incluidos).
