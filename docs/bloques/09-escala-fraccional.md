# Bloque 9 — Escala fraccional

Estado: **parcial.** La base de escala ya no es un literal global; falta la
aceptación en el panel real y la escala por salida de 8b.

## Implementado

- `State::scale` es un `smithay::output::Scale`, no un entero global. Expone la
  parte entera a `wl_output` y la fraccionaria a `wp_fractional_scale_v1`.
- `display.scale` es la opción persistente en `~/.config/yuki/display.domain`
  (`1.0..=4.0`, ADR 0021). `YUKI_SCALE` sigue siendo el override de desarrollo y
  gana cuando está definido; sin ninguno de los dos, escala 1.
- `State::logical_output_size`, la colocación y escala de los elementos de
  render, el damage tracker, el cursor y el foco usan la escala de la salida.
- La shell usa `wp_viewport` para la parte fraccionaria y mantiene
  `wl_surface.set_buffer_scale(1)` cuando hay viewport. Antes combinaba
  `buffer_scale` con viewport y a 1.5 el source quedaba fuera del buffer
  (`wp_viewport: out_of_buffer`).
- `FractionalScaleHandler` envía `preferred_scale` exacto a cada superficie.
- Test unitario de parseo/clamp de `display.scale` y del override `YUKI_SCALE`.
- Las capturas offscreen (debug `SIGUSR1`, guardado a Escritorio y
  `ext-image-copy-capture-v1`) usan la escala de la salida en su damage
  tracker. Antes estaba fijada a 1.0 y a 1.5 el frame solo ocupaba la esquina
  lógica superior izquierda.
- El cursor se carga a `ceil(scale)` y su hotspot se convierte de píxeles de la
  imagen a coordenadas lógicas; antes a 2× el hotspot del tema se restaba dos
  veces.

## Pendiente

- Aceptación en el panel real a 1.25 y 1.5 con `wezterm`, shell y puntero.
- Escala por salida y por monitor cuando 8b introduzca varios outputs.
- Probar clientes que no negocian fractional-scale y clientes con
  `viewporter` propio.

## Hecho después de la primera pasada

- `display.scale` persistente en `~/.config/yuki/display.domain`; `YUKI_SCALE`
  queda como override de desarrollo (ADR 0021).
- Redondeo asimétrico (floor posición, ceil tamaño) en `yuki-scene`, ADR 0004
  aceptado e implementado.

## Cómo probar

```sh
YUKI_SCALE=1.5 ./dev.sh demo   # wallpaper, menubar, dock y dos ventanas
YUKI_SCALE=1.5 ./dev.sh run
# o, persistente, sin YUKI_SCALE:
#   printf 'scale = 1.5\n' > "${XDG_CONFIG_HOME:-$HOME/.config}/yuki/display.domain"
#   ./dev.sh run
# en otra terminal, con el WAYLAND_DISPLAY que imprima Yuki:
WAYLAND_DISPLAY=wayland-N yuki-shell wallpaper
WAYLAND_DISPLAY=wayland-N yuki-shell menubar
WAYLAND_DISPLAY=wayland-N yuki-shell dock
WAYLAND_DISPLAY=wayland-N wezterm
```

No debe aparecer `wp_viewport: out_of_buffer` ni `bad_size` en los logs de la
shell. El puntero debe caer en el mismo píxel que se ve y la captura por
`SIGUSR1` debe mostrar el wallpaper, la menubar y el dock completos.

## Verificación en anidado (2026-10-21)

- `YUKI_SCALE=1.25`, `1.5` y `2.0` con `yuki-shell wallpaper/menubar/dock`:
  sin `wp_viewport` ni error de protocolo; los tres procesos siguen vivos.
- `YUKI_SCALE=1.5 ./dev.sh smoke`, `screencopy` y `screencopy-window`: PASS.
- Captura `SIGUSR1` a 1.5 con `foot` (cliente sin fractional-scale) y
  `yuki-shell wallpaper/menubar` (con viewporter): frame completo de 1280×800,
  la menubar cruza todo el ancho y el wallpaper ya no queda recortado.
- Región a 1.5 (`YUKI_TEST_SCREENSHOT=region`): PNG de 450×300 (300×200
  lógicos), el rectángulo correcto.
- `smoke`, `block4`, `move` y `decoration` a escala 1: PASS.
- `./dev.sh check` y `nix build .#default`: PASS.

Falta la comprobación visual en el panel real (nitidez, cursor y alineación).

## Arreglado al verificar la aceptación (2026-10-21)

- `SIGUSR1` y `SIGTERM` no llegaban al manejador: calloop bloquea las señales
  solo en el hilo principal y los hilos del renderer las tenían
  desbloqueadas, así que el kernel se las entregaba a ellos (muerte directa y
  captura perdida). `main` las bloquea antes de crear ningún hilo, de modo que
  todos heredan la máscara.
- Las capturas a escala fraccional renderizaban los elementos a 1× (tracker
  con escala 1.0): ahora usan la escala real.

## Comparación con macOS

macOS no usa escala fraccional por app: renderiza el escritorio a 2× de la
resolución lógica y reduce el framebuffer virtual al panel. Es supersampling
global. Yuki sigue el modelo Wayland (escala por salida/superficie); ver
[`../investigacion/escalado-macos.md`](../investigacion/escalado-macos.md) para
el análisis y las diferencias. El supersampling tipo macOS queda como **futuro
opcional**, solo cuando el escritorio base sea competente; no forma parte de
este bloque.

## Notas

- `wl_output.scale` es entero; para 1.5 se anuncia `advertised_integer = 1` y
  los clientes que lo soportan usan `wp_fractional_scale_v1`.
- El cursor se carga a la escala entera **hacia arriba** (`ceil`) para no
  quedar borroso, y el hotspot del tema se pasa de píxeles de la imagen a
  coordenadas lógicas (antes a 2× quedaba desplazado).
