# Bloque 4 — Ventanas manipulables

> **Mapeo del roadmap.** Este es el registro detallado del **Bloque 4** del
> roadmap (Ventanas manipulables). Su trabajo de layout vive en `yuki-scene`
> (ADR 0015) y su modelo de interacción en el ADR 0016.

Estado: **completo, con huecos de comprobación manual.** La aceptación verificada
por máquina es `./dev.sh block4`; la sensación del arrastre, los visuales del
resize y el cambio de forma del cursor bajo el puntero se comprueban a mano.

## Propósito

El Bloque 4 hace manipulable una ventana mapeada: se puede mover, redimensionar y
cerrar, se apila y toma el foco al hacer click, y sus bordes muestran el cursor de
resize correspondiente. El trabajo se divide en el límite que dibujó el ADR 0015:
`yuki-scene` es dueño de dónde están las ventanas y cómo se apilan; el
compositor es dueño de las superficies de protocolo y de la interacción.

## Qué se construyó

### `yuki-scene`

- `Rect::contains`, `Rect::right` y `Rect::bottom` — los predicados de rectángulo
  que el layout necesita (right y bottom son exclusivos).
- `scene::{WindowId, Edge, Scene}` — una pila ordenada de atrás hacia adelante con
  clave de id opaco, con `insert`/`remove`/`raise`, geometría exterior,
  hit-testing de puntos (`topmost_at`) y zonas de borde de resize (`edge_at`). No
  tiene dependencia de Smithay; el compositor convierte en el límite.
- `scene::resized` — el rectángulo exterior decorado producido al arrastrar un
  borde o una esquina: los bordes principales mueven el origen y mantienen el
  borde opuesto fijo, y el tamaño se limita a un mínimo y un máximo opcional.
- `yuki` ya no solo se sienta junto a `yuki-scene`; depende de él.

### `crates/yuki`

- **`window.rs`** está reconstruido alrededor de la escena. `WindowData` contiene
  la `ToplevelSurface`, el tamaño exterior y la contabilidad de configure; una
  vista `Window` la une a la geometría de la escena para los lectores. `Windows`
  expone apilamiento, `raise`, `topmost`, hit-testing (`window_under`,
  `edge_under`, `surface_under`), `move_to`, `resize`, `set_resizing`,
  `send_close`, `id_of`/`surface_of`, `minimum_outer_size`/`maximum_outer_size`
  (un máximo de cero es ilimitado), y `outer`. El mapeo inserta la ventana en la
  escena y registra `window mapped`.
- **`input.rs`** es dueño de la interacción: una banda de borde inicia un resize,
  el botón primario en la banda decorada inicia un move, y un click sobre el
  cliente eleva y enfoca. El motion maneja la grab o actualiza el foco y el cursor
  de borde; la liberación la termina. `forget_window` elimina una ventana destruida
  de la escena y de cualquier grab, y mueve el foco de teclado a la nueva ventana
  más alta o lo limpia.
- **`state.rs`** contiene `Grab` (move o resize) y el handshake del foco del seat.
- **`handlers.rs`** responde a `move_request` y `resize_request` después de la
  comprobación de serial, y enruta `toplevel_destroyed` a `forget_window`.
- **`cursor.rs`** carga las formas de resize `ew`/`ns`/`nwse`/`nesw` del tema
  Xcursor, junto con la por defecto, y la ruta de dibujo elige la forma que Smithay
  reporta. Una forma o tema faltante vuelve a la flecha por defecto.
- **`handlers.rs`** sirve ambos protocolos de decoración:
  `zxdg_decoration_manager_v1` y `org_kde_kwin_server_decoration_manager`. El modo
  que cada cliente negocia se guarda por ventana en `yuki-scene` y se lee para
  insets, hit-testing y dibujo (ADR 0019).

### Cierre

`Windows::send_close` envía `xdg_toplevel.close`, y un cliente que sale arranca su
ventana a través de `toplevel_destroyed`. Yuki no dibuja ningún botón de cierre
ni vincula ningún gesto para cerrar; un cierre iniciado por el compositor se deja
para un bloque posterior, así que el cierre lo impulsa el cliente (o matándolo).

## Decisiones

- **ADR 0015** — el layout de ventanas vive en `yuki-scene`; `desktop::Space`
  de Smithay no se adopta. El Bloque 4 es el primer consumidor de esa decisión.
- **ADR 0016** — el move y el resize interactivos son estado propio del compositor
  (`State::grab`), no `PointerGrab`s de Smithay, y las solicitudes de los clientes
  se comprueban por serial contra la última pulsación de puntero.

## Verificación

La aceptación verificada por máquina es un comando cuyo código de salida es el
resultado:

- **`./dev.sh block4`** — lanza un compositor y tres tandas del cliente
  `window_stress` (cincuenta toplevels decorados con buffers shm, luego
  destruidos). Verifica que los contadores de objetos vivos vuelven a la línea
  base de arranque después de cada tanda (sin clientes, `wl_surface`s o toplevels
  huérfanos), y que la memoria residente no crece de la tanda 2 a la 3 por encima
  del umbral indicado. Observado:

  ```
  block4: batch 1 settled at 89680 kB
  block4: batch 2 settled at 89720 kB
  block4: batch 3 settled at 89720 kB
  block4: 150 windows removed over 3 batches, live objects restored to baseline; resident memory warm-up 89680 kB, batch 2 89720 kB, batch 3 89720 kB, batch 2->3 growth 0 kB (asserted below 1024 kB)
  block4: PASS
  ```

  La primera tanda es el warm-up. El umbral es 1 MiB: después del warm-up una
  tanda debería reutilizar las mismas asignaciones, así que el crecimiento es ruido
  del allocator (decenas de kB), mientras que una fuga por ventana sería mucho
  mayor — incluso ~20 kB por ventana en 50 ventanas es ~1 MB. Tanto la
  restauración de objetos vivos como el umbral de memoria se verifican. La prueba
  de fugas se demuestra falible: con `YUKI_TEST_LEAK_WINDOW` seteado, el
  compositor deja deliberadamente una ventana atrás y `block4` falla con
  `last seen Some(LiveObjects { clients: 0, surfaces: 0, toplevels: 1 })`.

  El mismo comando luego redimensiona ventanas a través del backend de test: cada
  escenario obtiene un compositor fresco, que presiona una esquina o el borde
  izquierdo (`YUKI_TEST_PRESS_AT`) de una ventana `resize_client` fresca y
  maneja la grab con sus motions scripteados. El harness verifica el tamaño final
  desde el reporte propio del cliente — crecimiento de esquina sin límite, el mismo
  crecimiento limitado a un máximo del cliente, y un ancho reducido sostenido por
  un mínimo del cliente. Observado:

  ```
  block4: resize grow PASS (initial 1280x800, final 1328x832, expected 1328x832)
  block4: resize maximum PASS (initial 1280x800, final 1290x810, expected 1290x810)
  block4: resize minimum PASS (initial 1280x800, final 1270x800, expected 1270x800)
  block4: PASS
  ```
- **`./dev.sh move`** — lanza un compositor con `YUKI_TEST_INPUT=1` y ejecuta
  tres casos. `move_client` repite un gesto con forma de GTK: pulsación, tres
  motions que pasan un umbral de arrastre, una segunda ventana que toma el foco, y
  luego `xdg_toplevel.move` y `xdg_toplevel.resize` con el serial de la pulsación
  original mientras el botón está mantenido. El harness verifica que el compositor
  aceptó ambos, movió la ventana por el delta de puntero inyectado, y rechazó los
  seriales liberado y fabricado; el cliente verifica que recibió un configure que
  lleva el estado `resizing`. `hover_client` luego verifica las coordenadas locales
  a la superficie de un hover sin botones: el arrastre mueve la ventana a un origen
  no cero y la liberación termina la grab, el compositor hace hover del puntero, y
  el harness verifica el enter y el motion que el cliente leyó. Finalmente arrastra
  una headerbar real de Nautilus y verifica que su ventana se movió por el mismo
  delta (UNVERIFIED si Nautilus no está instalado). Observado:

  ```
  move_client: got the press serial 7
  move_client: mapped a second window to take focus
  move_client: sent move/resize with serial 7 after a threshold and a focus change, while held
  move_client: sent move/resize with serial 3735879681 (fabricated)
  move_client: received a configure with the resizing state
  move_client: sent move/resize with serial 7 after the release
  move_client: PASS
  hover_client: got the press serial 7
  hover_client: sent move/resize with serial 7
  hover_client: enter x=32 y=32
  hover_client: motion x=40 y=40
  hover_client: PASS
  move_probe: the hovered client read the pointer at the surface-local positions
  move_probe: nautilus PASS (its headerbar drag moved the window by the pointer delta)
  move_probe: PASS
  ```
- **`./dev.sh check`** — lleva los tests unitarios: el apilamiento, el
  hit-testing, las zonas de borde y el clamping de `resized` de la escena; la
  comprobación de serial de grab mantenida (`held_grab`); los límites de tamaño
  exterior (un máximo de cliente no seteado sigue ilimitado, un límite seteado
  lleva los insets, un mínimo de cero deja un píxel); el mapeo de borde a cursor;
  el mapeo de esquinas del puntero del anfitrión; los mapeos del modo de decoración
  y el almacenamiento en la escena; y los tests de píxeles fuera de pantalla que
  verifican que el frame está vertical y que el cursor cae en su posición ajustada
  por hotspot (ADR 0018).
- **`./dev.sh smoke`** — verifica la secuencia de mapeo y que un cliente (`foot`)
  recibe el configure de decoración del lado del servidor antes de su primer
  attach de buffer, así la reconstrucción de `window.rs` no regresionó el mapeo y
  el modo de decoración queda fijado a tiempo.
- **`./dev.sh decoration`** — ejecuta aplicaciones reales dentro de Yuki y
  verifica el protocolo de decoración y el modo que cada una negoció, más el
  cliente de cambio de modo (ADR 0019). Ver el seguimiento más abajo.

No verificado por un comando, y dejado a la comprobación manual: la sensación del
arrastre y si la ventana mantiene el offset de grab bajo el puntero; los visuales
del resize y cómo se sienten el mínimo y el máximo limitados; que la forma del
cursor realmente cambie cuando el puntero cruza un borde; y el click-to-focus y el
apilamiento tal como se ven.

## Seguimiento: defectos encontrados después de aceptar el Bloque 4

- **Un máximo de cliente no seteado colapsaba cada resize a 1×1.**
  `outer_size_bounds` leía el `max_size` de cero del cliente — sin máximo seteado —
  a través de un piso `max(1.0)` destinado al mínimo, así que
  `maximum_outer_size` reportaba un píxel y `resized` limitaba cada resize
  impulsado por puntero, de un borde o una esquina, a 1×1. El Bloque 4 se aceptó
  con esto porque el test de estrés de 50 ventanas nunca redimensiona. El fix
  mantiene un límite de cero en cero (ilimitado, como lo lee `resized`) y aplica
  el piso de un píxel solo al mínimo. Los tres escenarios de resize en
  `./dev.sh block4` fallan con los límites viejos (`grow` y `minimum` terminan en
  1×1) y pasan con el fix; `./dev.sh check` fija el mapeo.
- **El frame se renderizaba al revés.** El transform de la salida de winit se
  mantiene `Normal` y la fuente de modo estática del tracker de damage lleva
  `Flipped180`, así que el volteo ocurre solo en la matriz de proyección y ningún
  cliente ve `flipped_180` en `wl_output.geometry` (ADR 0018). Los tests de píxeles
  fuera de pantalla en `./dev.sh check` fallan con el transform viejo y pasan con
  el fix; verifican la orientación del cuadrante y la posición ajustada por hotspot
  del cursor.
- **Forzar el modo del lado del servidor puso la titlebar de Yuki en clientes
  con autodecoración.** El ADR 0017 queda superado por el ADR 0019: un cliente que
  negocia y acepta el lado del servidor obtiene decoraciones del compositor, uno
  que pide explícitamente el lado del cliente o nunca negocia mantiene su propio
  chrome y no recibe sombra, el modo queda fijado antes del primer attach, y un
  cambio después del mapeo reconfigura modo y geometría. Ambos protocolos de
  decoración se sirven, y el modo vive por ventana en `yuki-scene`.
- **Las coordenadas de hover del puntero estaban desplazadas por el origen de la
  ventana.** `pointer_motion` pasaba el punto local a la superficie de
  `surface_under` donde Smithay espera el origen de la superficie en coordenadas
  globales (`smithay-0.7.0/src/input/pointer/mod.rs:220-243`), así que un cliente
  leía el origen de la ventana para cada motion de hover — y una pulsación podía
  caer en un widget que no estaba cerca del puntero. El fix pasa `location - local`.
  `hover_client` a través de `./dev.sh move` verifica las posiciones locales a la
  superficie de punta a punta; con el mapeo viejo falla leyendo `enter x=60 y=40`
  (el origen) donde se espera `x=32 y=32`.

`./dev.sh decoration` es el registro de la verificación por aplicación, y
`decoration_client` fija la regla de cambio de modo. Lo que mostró:

| Aplicación | Protocolo | Modo | Chrome | Resultado |
| ----------- | -------- | ---- | ------ | ------ |
| nautilus (GTK4 + libadwaita) | KDE | ninguno negociado | ninguno | verificado |
| gtk3-demo, gtk3-widget-factory | KDE | cliente | ninguno | verificado |
| gtk4-demo | KDE | ninguno negociado | ninguno | verificado |
| gtk3-plain, gtk4-plain (compilados) | KDE | servidor | inset de 32 px | verificado |
| qt6 (`qml`) | xdg-decoration | servidor | inset de 32 px | verificado |
| gtk4-widget-factory | KDE | — | — | no verificado: el binario de la demo aborta antes del mapeo |
| qt5 | — | — | — | no verificado: no está instalado |
| onlyoffice-desktopeditors | ninguno | ninguno | ninguno | no verificado: el bundle solo trae el plugin de plataforma XCB |

GTK4 pregunta a través del protocolo KDE **antes de que su xdg_toplevel exista**,
así que un modo para una superficie no rastreada se recuerda y se aplica cuando se
crea el toplevel; `gtk4-plain` fija ese ordenamiento. Si se ven dos titlebars no es
verificable por máquina aquí y se mantiene manual — y es discutible hasta que
exista un pintor de chrome: Yuki reserva la banda de la titlebar pero aún no
dibuja titlebar, hairline ni sombra.

## Preguntas abiertas

1. **Sin restricciones de movimiento.** Una ventana se puede arrastrar más allá de
   la salida y solo se recupera arrastrándola de vuelta; el tiling y el snapping no
   están modelados.
2. **Maximizado y fullscreen no son especiales.** Todavía reciben una titlebar y
   zonas de borde, así que una ventana maximizada se puede arrastrar. El resumen se
   detiene en ventanas ordinarias.
3. **El foco no sigue al puntero.** Cambia solo al hacer click y al destruirse la
   ventana enfocada.
4. **La grab y el override del cursor interactúan.** Un cursor de resize solo se
   restaura al por defecto cuando el estado activo es `Named`; un cliente que puso
   su propio cursor de superficie lo mantiene, que es correcto sobre su contenido
   pero no sobre la decoración de Yuki.
5. **`YUKI_TEST_INPUT` es solo de test pero vive en código de producción.** El
   compositor presiona una vez, en la primera ventana que se mapea, mantiene el
   botón, sigue la pulsación con motions y aplica un arrastre una vez que se acepta
   una solicitud, así una probe puede ejercitar la ruta iniciada por el cliente.
   Está apagado a menos que la variable esté seteada, y es la única costura de test
   dentro del propio compositor en lugar de en las probes solo de dev.
   `YUKI_TEST_PRESS_AT` y `YUKI_TEST_DRAG` apuntan la pulsación y dimensionan
   el arrastre, y `YUKI_TEST_HOVER` hace hover de puntos sin botones después de
   la liberación para que un cliente pueda verificar las coordenadas que recibe.

## Registros de decisión

- `../decisiones/0015-desktop-space-y-yuki-scene.md` — la escena es dueña del
  layout de ventanas.
- `../decisiones/0016-movimiento-y-redimensionado-interactivo.md` — los moves y
  resizes son estado propio del compositor, con validación de serial.
- `../decisiones/0017-xdg-decoration-del-lado-del-servidor.md` — xdg-decoration se
  sirve y siempre del lado del servidor. Superado por el 0019.
- `../decisiones/0018-transformacion-de-renderizado-de-winit.md` — la salida de
  winit voltea en tiempo de render a través de la fuente de modo del tracker de
  damage; `wl_output` se mantiene `normal`.
- `../decisiones/0019-politica-de-decoracion.md` — el lado del servidor es el
  predeterminado y la respuesta a un modo no seteado, pero una solicitud explícita
  del lado del cliente se honra.