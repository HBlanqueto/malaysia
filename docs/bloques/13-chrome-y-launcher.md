# Bloque 13 — Chrome y launcher

Estado: **launcher implementado y probado en anidado** (2026-10-23). La
aceptación manual completa depende del teclado del panel real; el entorno de
desarrollo captura `Super+Space` antes de que llegue a Yuki.

## Launcher (`yuki-shell launcher`)

- Superficie `Overlay` con `KeyboardInteractivity::Exclusive`, sin ancla (el
  compositor la centra) y tamaño `LAUNCHER_*` de Neptunia. Se cierra con `Esc`,
  al perder el foco, al lanzar una app y con `SIGTERM`.
- Aparece y desaparece con Grade Animation (`Spring`): opacidad 0→1 y escala
  `LAUNCHER_SCALE_FROM`→1 al abrir, la inversa al cerrar.
- Búsqueda con el componente `Input` de Freya, que usa `freya-edit` para
  cursor, selección, `Ctrl+A`, `Backspace` y flechas. El campo tiene el foco
  desde el primer frame.
- Lista navegable con `↑`/`↓`; `Enter` o un clic lanzan la entrada
  seleccionada. Sin resultados muestra «Sin resultados».
- El compositor da foco de teclado a las capas `Top`/`Overlay` exclusivas al
  mapearse y a las `OnDemand` al pulsarlas. Un clic en una ventana no quita el
  foco a una capa exclusiva. Al desmapearse, el foco vuelve a la ventana que lo
  tenía (si sigue viva) o a ninguna. El lock sigue teniendo prioridad absoluta.

## Atajo

`Super+Space` es fijo, no una opción de dominio. Si el launcher no corre, el
compositor lanza `yuki-shell launcher` con el `WAYLAND_DISPLAY` de Yuki; si
corre, le manda `SIGTERM`. Con la sesión bloqueada no hace nada. Está
documentado en [`../configuracion/opciones.md`](../configuracion/opciones.md).

## Apps y búsqueda

- Lee `$XDG_DATA_HOME/applications` y cada `$XDG_DATA_DIRS/applications`, con la
  precedencia XDG (gana el primer id). En NixOS entran `~/.nix-profile/share` y
  `/run/current-system/sw/share` como directorios XDG normales.
- Respeta `Hidden`, `NoDisplay`, `OnlyShowIn`/`NotShowIn` (escritorio `Yuki`),
  `TryExec` y `Type=Application`.
- Usa `Name`, `GenericName`, `Keywords` e `Icon` con locale (`es_ES` y `es`
  antes que el valor sin locale).
- Ranking: prefijo de `Name` > inicio de palabra > subcadena >
  `Keywords`/`GenericName`; sin mayúsculas ni tildes; empate alfabético. Con la
  búsqueda vacía, las apps con ventana abierta (el «dock») primero y después el
  resto por nombre.
- `Exec` se limpia de `%f %F %u %U %i %c %k`, respetando comillas y escapes;
  `Path` es el directorio de trabajo. `Terminal=true` usa `YUKI_TERMINAL`,
  luego `TERMINAL`, y si no hay ninguno, `wezterm -e`.
- El proceso se lanza con `setsid`, sin la environment de Mesa del compositor
  (mismo criterio que ADR 0007). Antes de lanzar pide un token de
  `xdg-activation` con el serial de la última tecla y lo pasa como
  `XDG_ACTIVATION_TOKEN` y `DESKTOP_STARTUP_ID`. Si el token no llega en 250 ms,
lanza igualmente sin él para que `Enter` no se quede bloqueado; el proceso
arranca, solo pierde el traspaso de foco.
- Iconos: `YUKI_ICON_THEME` (por defecto `Adwaita`), después `hicolor` y
  `pixmaps`; SVG con Skia y PNG por tamaño. Si no hay icono, la inicial en un
  cuadrado de Neptunia.

## Latencia

Medición release en la GPU real (AMD radeonsi, winit anidado): **121,3 ms**
desde el `spawn` del proceso hasta el `layer surface configured`; el primer
frame se presenta en el siguiente frame del compositor (≤16,7 ms a 60 Hz), por
debajo de los 150 ms. **No se cambia a un proceso persistente**: el arranque
directo cumple el presupuesto y evita una unidad systemd y el protocolo de
señales adicional.

## Pruebas

- Unitarias en `crates/yuki` para la política de foco de capas y el atajo
  (`Super+Space`, incluido el caso bloqueado).
- Unitarias en `crates/yuki-shell` para el parser `.desktop` (locale, `Hidden`,
  `OnlyShowIn`, `TryExec`), la limpieza de `Exec` y el ranking (prefijo,
  tildes, empates, apps abiertas).
- Goldens de `freya-testing` del launcher vacío, con resultados y sin
  resultados, en claro y oscuro (`crates/yuki-shell/tests/golden/`).
- Manual en anidado: el launcher se abrió, «fire» seleccionó Firefox y `Enter`
  lo lanzó; Firefox mapeó y recibió el foco. `Esc` cerró la superficie. La
  inyección de `Super+Space` desde el host queda capturada por el atajo global
  del host, así que el toggle real se cubre con el test unitario; falta
  repetirlo con el panel/teclado del ThinkPad.

## Pendiente de chrome (Bloque 13)

- Esquinas redondeadas (16 px), sombra, glifos de hover, doble clic para zoom y
  titlebar en la captura de ventana.
