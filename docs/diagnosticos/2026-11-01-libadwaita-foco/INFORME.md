# Informe — modo foco de libadwaita (17f, ADR 0053)

Fecha: 2026-11-01. Sesión en la nube, sin pantalla ni GPU.

## Qué se probó

**No se pudo probar con Malaysia.** El dev shell no se construye en la
sesión: la derivación de Skia de barrierkit descarga
`github.com/rust-skia/skia/archive/...` y la red de la sesión lo rechaza
(403). Sin dev shell no se compila el compositor (ADR 0007).

En su lugar, un banco con un compositor Wayland real:

- **weston 16 headless** (pixman) con un módulo de prueba,
  [`banco/focus-sim.c`](banco/focus-sim.c), que ofrece
  `infinity_focus_manager_v1` con el XML de barrierkit `c8965c3` y envía a
  cada `infinity_focus_surface_v1` los `bar` de `FOCUS_SIM_BARS`
  (`ms:valor,...` desde su creación). No desplaza la ventana 28 px como
  Malaysia.
- La libadwaita del paquete `libadwaita-global-menu` del flake, cargada por
  `LD_LIBRARY_PATH` (`/nix/store/lnsj2h1l…-libadwaita-1.9.3`, comprobado en
  `/proc/<pid>/maps`).
- Un probe en Python ([`banco/probe.py`](banco/probe.py)): una
  `AdwApplicationWindow` con dos `AdwToolbarView` lado a lado (`side`,
  `main`, como una vista partida) y una anidada en el contenido de `main`
  (`inner`, con una barra de búsqueda). Entra en fullscreen a los 2 s, sale
  a los 13 s y escribe `fullscreened` y `reveal-top-bars` en cada cambio.
- gnome-text-editor 50.1 y gnome-calculator 50.0 de nixpkgs en kiosk-shell
  (fullscreen desde el inicio), con capturas por `weston-screenshooter` a
  3,5 s, 6,5 s y 9,5 s; `bar(1)` a los 5 s y `bar(0)` a los 8 s.

## Resultados

| Caso | Resultado |
|---|---|
| Probe con el global ([`probe.txt`](banco/probe.txt)) | Al entrar en fullscreen se esconden `side` y `main`; `inner` no (está dentro de una vista con barras). `bar(1)` las muestra, `bar(0)` las esconde (dos ciclos). Al salir vuelven las dos. El `bar(1)` de los 14 s, ya fuera de fullscreen, no hace nada. |
| Probe sin el global ([`noglobal.txt`](banco/noglobal.txt)) | Nada cambia: upstream. |
| Probe con el global y `ADW_DISABLE_FOCUS_MODE=1` ([`disabled.txt`](banco/disabled.txt)) | Ni siquiera liga el global; nada cambia. |
| Probe que esconde `main` él mismo en fullscreen ([`appmanages.txt`](banco/appmanages.txt)) | `main` queda en manos de la app (sigue oculta con `bar(1)` y la app la muestra al salir); `side` sí sigue a la barra. |
| gnome-text-editor ([captura](capturas/gnome-text-editor.png)) | Arriba, con el global: sin headerbar, con headerbar tras `bar(1)`, sin ella tras `bar(0)`. Abajo, sin el global: headerbar siempre. |
| gnome-calculator ([captura](capturas/gnome-calculator.png)) | Igual: sin headerbar, con ella, sin ella. |

Tests del fork: `tests/test-focus-mode.c` (10 casos) pasa, y la suite
entera del paquete (71 tests) pasa en `nix build .#libadwaita-global-menu`.
La suite falla a veces en el sandbox con `Failed to open display` en un test
cualquiera (`test-carousel`, `test-breakpoint-bin`): pasaba también sin este
parche; es el arranque de Xvfb de nixpkgs, no el código.

## Qué falta probar en sesión

- Malaysia de verdad (`./dev.sh demo`): F11, botón verde y `Ctrl+Super+F`,
  la barra negra bajando con la ventana desplazada 28 px y la headerbar
  apareciendo debajo, y la salida del foco con la barra bajada (ADR 0053,
  Consecuencias).
- Apps con split view (Nautilus, Ajustes) y visores que gestionan
  `reveal-top-bars` (Loupe, Showtime).

## Reproducir

Desde este directorio (`banco/`), con nix:

```sh
nix develop --impure -f shell.nix
cp ../../../../../barrierkit/crates/infinity-protocol/protocols/infinity-focus-v1.xml .
wayland-scanner server-header infinity-focus-v1.xml infinity-focus-v1-server-protocol.h
wayland-scanner private-code infinity-focus-v1.xml infinity-focus-v1-protocol.c
mkdir -p /tmp/malaysia-sim
gcc -shared -fPIC -o /tmp/malaysia-sim/focus-sim.so focus-sim.c infinity-focus-v1-protocol.c \
  $(pkg-config --cflags --libs libweston-16 wayland-server pixman-1 libdrm xkbcommon)
./run.sh probe 1 python3 probe.py
./shots.sh editor 1 gnome-text-editor --standalone
```

Weston usa su propio socket (`sim-<tag>`) en un `XDG_RUNTIME_DIR` propio
bajo `/tmp/malaysia-sim`; no toca la sesión del host.

## Addendum — titlebar superpuesta (ADR 0054)

Mismo banco. El módulo de weston ofrece ahora la versión 2
(`FOCUS_SIM_VERSION`, 2 por defecto), registra `set_overlay` y envía
`bar_height(28)` antes de cada `bar(1)`.

- gnome-calculator y gnome-text-editor con el paquete
  `libadwaita-global-menu` (`/nix/store/7wklqrbw…-libadwaita-1.9.3`): ligan
  la v2 y piden `set_overlay`. Con `bar(1)` la headerbar aparece opaca, con
  sombra, a 28 px del borde y encima del contenido. Comparando las capturas
  sin barra y con barra, solo cambian las filas 27–79 (headerbar y sombra):
  el contenido no se mueve. En
  [`capturas/v2-superpuesta.png`](capturas/v2-superpuesta.png) y
  [`capturas/v2-calculadora-detalle.png`](capturas/v2-calculadora-detalle.png)
  la franja negra de 28 px está pintada encima de la captura para simular la
  barra de Malaysia; weston no la dibuja.
- Con `FOCUS_SIM_VERSION=1`: sin `set_overlay` ni `bar_height`; la
  headerbar empuja el contenido como antes (en Malaysia, la ventana baja).
- El probe (`run.sh`) da los mismos estados que con la v1.
- Tests del fork: 13 casos en `test-focus-mode` (3 nuevos para la
  superposición); la suite del paquete, 71/71.
- barrierkit: los ejemplos `window_csd` y `sidebar` ligan la v2, envían
  `set_overlay` y reciben `bar_height`/`bar`, pero no pintan en weston sin
  GPU (pantalla negra), así que no hay captura. El layout está cubierto por
  tres tests nuevos del kit (`overlay_titlebar_tests`): en fullscreen el
  contenido empieza en y=0, la titlebar no está sin la barra y aparece en
  y≥28 con ella sin mover el contenido.
- El compositor no se ha podido compilar aquí (sin dev shell, ADR 0007).
  Sí se comprobó la API generada del protocolo v2 (`bar_height`,
  `Request::SetOverlay`) con un crate aparte, y que rustfmt no cambia nada.
