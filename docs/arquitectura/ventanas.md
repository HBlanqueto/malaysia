# Ventanas y titlebar (Neptunia)

La titlebar de Yuki es **compositor-side** (ADR 0020) y usa el diseño de
**Neptunia** (ADR 0032): barra **arriba** de 44 px lógicos, semáforos de 12 px
con 8 px de separación y radio de ventana de 16 px.

## Qué se dibuja hoy

- Fondo de la barra con `titlebar_bg` (focused/unfocused) y una hairline de
  1 px al pie.
- Tres semáforos a la izquierda, en orden **cerrar / minimizar / maximizar**,
  con los colores `traffic-close`, `traffic-min` y `traffic-zoom`; grises de
  `traffic_lights_inactive` cuando la ventana no tiene foco.
- El título de la ventana, centrado y en una línea, rasterizado con
  `cosmic-text` (misma fuente que el resto de la UI) y coloreado con
  `title.focused` / `title.unfocused`.

Solo reciben titlebar las ventanas **server-side** que reservan inset (ADR
0014/0019). Las apps GTK/Libadwaita conservan su headerbar; los diálogos, los
popups y el fullscreen no llevan chrome.

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
- El texto de la titlebar es un motor pequeño dentro del compositor
  (`crates/yuki/src/chrome.rs`); no usa `celestial-kit`, que es el toolkit de
  la shell (ADR 0024).
- La ventana se eleva al recibir foco; el foco y el orden de apilado no se
  separan.

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

- No se dibujan **esquinas redondeadas** (16 px) ni **sombra**; el radio y los
  valores de sombra ya están en `yuki-design`.
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
- `YUKI_TEST_INPUT=1 YUKI_TEST_PRESS_AT` sobre las fracciones de los botones
  (`~0.0172`, `~0.0328`, `~0.0484` del ancho) cierra, minimiza y maximiza; los
  logs muestran `window removed`, `window minimized` y `window maximized`.
- `YUKI_SCALE=1.5 ./dev.sh demo`: la barra y el texto se rasterizan a la escala
  física.
- `./dev.sh check` pasa (tests de `button_at` incluidos).
