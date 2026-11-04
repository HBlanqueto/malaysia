# sctk-probe

Clientes Wayland solo para desarrollo, que manejan Malaysia mientras se
desarrolla. Este paquete **no** forma parte del compositor: es un miembro del
workspace excluido de la compilación por defecto (`default-members` en el
`Cargo.toml` raíz). Un `cargo build`/`cargo test` sin más en la raíz del
repositorio no lo compila; nombra el paquete para compilarlo a propósito.

Estos clientes se conectan a lo que indiquen `WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR`,
así que corren contra Malaysia o contra la sesión del host sin ningún cambio.
`smoke` y `block4` son distintos: lanzan el propio Malaysia y manejan clientes
contra él.

Todo esto corre dentro del dev shell (`nix develop`). La forma más sencilla de
correr el smoke test es el script del proyecto, que compila el compositor y este
arnés y propaga el resultado:

```sh
./dev.sh smoke
```

Las secciones siguientes muestran los comandos `cargo` subyacentes, para cuando
quieras correr un binario directamente.

## `parent_probe`

Demuestra que los márgenes de decoración dependen del estado *actual* de una
ventana, no del estado que tenía al mapearse. Mapea dos toplevels y alterna un
padre en la hija mientras ambas están registradas:

- inicial (sin padre): decorada, configurada un titlebar izquierdo más estrecha;
- `set_parent`: la hija pasa a ser un diálogo, así que se reconfigura más ancha;
- sin padre: la hija vuelve exactamente a su tamaño inicial.

La sonda comprueba los tres tamaños de configure y sale con código distinto de
cero si no reproduce el ciclo. Es una comprobación de pasa/falla, no una salida
para leer a ojo.

Córrela contra un compositor en marcha. Mientras Malaysia corre con
`./dev.sh run`, el compositor hereda el `XDG_RUNTIME_DIR` de quien lo llama y
nombra allí su socket automáticamente (Smithay elige el primer `wayland-N`
libre, `wayland-1` cuando el host ya ocupa `wayland-0`); `malaysia-dev` guarda
el pid y el log, no el socket. Lee el nombre que registró el compositor en vez
de suponer uno:

```sh
runtime="${XDG_RUNTIME_DIR:?the compositor inherits this for its socket}"
# The compositor logs the name it bound, inside ANSI colour codes.
sock=$(sed 's/\x1b\[[0-9;]*m//g' "$runtime/malaysia-dev/malaysia.log" |
    sed -n 's/.*socket=\(wayland-[0-9]*\).*/\1/p' | tail -n 1)
XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="$sock" \
    cargo run -p sctk-probe --bin parent_probe
```

## `smoke`

Un smoke test basado en aserciones para la secuencia de mapeo de un toplevel.
Lanza el compositor, arranca un cliente real (`foot` por defecto) con
`WAYLAND_DEBUG=1`, captura la traza del cliente y comprueba, en orden:
`xdg_toplevel.configure` antes del primer attach no nulo; un serial de
`ack_configure` coincidente antes de ese attach; un commit no nulo con buffer
que el compositor da por mapeado; `wl_keyboard.enter` en la superficie mapeada;
y la finalización del id de callback *concreto* que pidió `wl_surface.frame`. No
acepta un `wl_callback.done` suelto, así que un ida y vuelta de
`wl_display.sync` no se confunde con un frame.

Sale con código distinto de cero si falta algún paso o llega fuera de orden.

```sh
./dev.sh smoke          # builds malaysia-compositor and the harness, then runs it
cargo run -p sctk-probe --bin smoke   # run it directly, if already built
```

Variables de entorno: `MALAYSIA_BIN` (ruta al binario del compositor) y
`MALAYSIA_SMOKE_CLIENT` (el cliente que se maneja).

## `block4`

El arnés de aceptación del Bloque 4. Lanza el compositor y tres tandas de
`window_stress`; cada una abre y destruye cincuenta toplevels decorados.
Comprueba que los recuentos de objetos vivos del compositor (clientes,
`wl_surface`s, toplevels registrados) vuelven a la base del arranque tras cada
tanda, y que la memoria residente no crece más de 1 MiB de la tanda 2 a la 3 (la
primera tanda es de calentamiento). Comprueba ambas cosas e imprime las tres
mediciones.

Después corre tres escenarios de redimensionado, cada uno con un compositor
nuevo, un `resize_client` nuevo y `MALAYSIA_TEST_INPUT=1`: el compositor pulsa
una esquina o un borde de la ventana mapeada y sus movimientos guionizados
manejan el grab. El arnés comprueba el tamaño configurado final: crecimiento por
la esquina sin máximo del cliente, el mismo crecimiento limitado por un máximo
del cliente, y un ancho reducido que frena un mínimo del cliente:

```
block4: resize grow PASS (initial 1280x800, final 1328x832, expected 1328x832)
block4: resize maximum PASS (initial 1280x800, final 1290x810, expected 1290x810)
block4: resize minimum PASS (initial 1280x800, final 1270x800, expected 1270x800)
```

```sh
./dev.sh block4                        # builds malaysia-compositor and the probes, then runs it
cargo run -p sctk-probe --bin block4   # run it directly, if already built
```

Variables de entorno: `MALAYSIA_BIN`, `MALAYSIA_STRESS_BIN` y
`MALAYSIA_RESIZE_BIN`.

## `window_stress`

El cliente que maneja `block4`. Abre cincuenta toplevels decorados con buffers
de memoria compartida, espera un configure en cada uno, luego los cierra todos y
sale. Por sí solo solo sirve para observar el desmontaje de un compositor:

```sh
cargo run -p sctk-probe --bin window_stress
```

## `resize_client`

El cliente que maneja `block4` para las aserciones de redimensionado. Mapea un
toplevel decorado por el cliente, aplica un límite de `MALAYSIA_RESIZE_MIN` o
`MALAYSIA_RESIZE_MAX` (un desplazamiento `x,y` desde el configure inicial, en
coordenadas de superficie, antes de mapearse) e imprime cada configure que
recibe, y luego el tamaño cuando el flujo se estabiliza:

```
resize_client: configure width=1280 height=800 resizing=false
resize_client: configure width=1328 height=832 resizing=true
resize_client: final width=1328 height=832
```

Está decorado por el cliente, así que su tamaño configurado es su tamaño de
ventana, sin un reconfigure de decoración que compita con el primer configure.
Sale con código distinto de cero si nunca recibe configure o si ningún
redimensionado se estabiliza.

## `decoration_probe`

El arnés de aceptación de la Fase 6 para la política de decoración (ADR 0019).
Lanza un Malaysia y, para cada aplicación real, la corre dentro de ese
compositor con `WAYLAND_DEBUG=1`, captura la traza del cliente y comprueba lo
que negoció: el protocolo de decoración que enlazó (xdg-decoration, el de KDE o
ninguno), el modo que configuró Malaysia y si la superficie quedó reducida por
el titlebar. La reducción se lee del ancho del configure frente al ancho de la
salida, así que vale aunque el primer configure ya la lleve (GTK4 pregunta antes
de que exista su toplevel). Ignora los configures posteriores a un
redimensionado o cambio de estado que inicia el cliente, porque un puntero del
host puede hacer clic en una ventana y redimensionarla.

Las aplicaciones se localizan con variables `MALAYSIA_APP_<NAME>`; una sin
binario se informa como `UNVERIFIED` en vez de saltarse en silencio, y una
aplicación que sale antes de mapear una ventana se informa igual. Saber si se
*ven* dos titlebars exige comparar píxeles, así que sigue siendo manual.

```sh
./dev.sh decoration                        # builds malaysia-compositor and the probes, then runs it
cargo run -p sctk-probe --bin decoration_probe   # run it directly, if already built
```

Variables de entorno: `MALAYSIA_BIN`, `MALAYSIA_DECORATION_CLIENT` (el cliente
de cambio de modo, que por defecto se busca junto a la sonda),
`MALAYSIA_APP_NAUTILUS`, `MALAYSIA_APP_GTK3_DEMO`,
`MALAYSIA_APP_GTK3_WIDGET_FACTORY`, `MALAYSIA_APP_GTK3_PLAIN`,
`MALAYSIA_APP_GTK4_DEMO`, `MALAYSIA_APP_GTK4_WIDGET_FACTORY`,
`MALAYSIA_APP_GTK4_PLAIN`, `MALAYSIA_APP_QT6` (con
`MALAYSIA_APP_QT6_QML_IMPORT_PATH`), `MALAYSIA_APP_QT5` y
`MALAYSIA_APP_ONLYOFFICE`.

`gtk3_plain.c` y `gtk4_plain.c`, en `gtk/`, son clientes mínimos de ventana
simple para la rama server-side, porque todas las demos de GTK usan headerbar.
Compílalos contra el mismo GTK con el que corre la sonda:

```sh
cc gtk/gtk3_plain.c -o /tmp/gtk3_plain $(pkg-config --cflags --libs gtk+-3.0)
cc gtk/gtk4_plain.c -o /tmp/gtk4_plain $(pkg-config --cflags --libs gtk4-wayland)
MALAYSIA_APP_GTK3_PLAIN=/tmp/gtk3_plain MALAYSIA_APP_GTK4_PLAIN=/tmp/gtk4_plain \
    ./dev.sh decoration
```

## `decoration_client`

El cliente que corre `decoration_probe` para la única regla de la política que
las aplicaciones reales no permiten ejercitar: un cambio de modo después del
mapeo (ADR 0019, regla 4). Mapea una ventana que pide decoraciones server-side,
espera el configure server-side del compositor (un titlebar más estrecho), pide
client-side en el primer frame y comprueba que el compositor la configura
entonces un titlebar más ancha. Sale con código distinto de cero si el segundo
configure nunca llega o si los tamaños no cuadran.

## `move_probe`

Ejercita la ruta de `xdg_toplevel.move`/`resize` que inicia el cliente y las
coordenadas locales de superficie que ve un cliente en el movimiento del
puntero. Corre tres casos, cada uno con un compositor nuevo y
`MALAYSIA_TEST_INPUT=1` (el compositor pulsa una vez, mantiene el botón, sigue la
pulsación con movimientos y lo suelta, algo que una sonda no puede hacer por sí
misma):

- `move_client` repite un gesto con forma de GTK: pulsación, tres movimientos
  más allá de un umbral de arrastre, una segunda ventana que toma el foco, y
  luego `move` y `resize` con el serial de la pulsación original mientras el
  botón sigue pulsado. El arnés comprueba que ambos se aceptaron, que la ventana
  se movió el delta del puntero inyectado y que los seriales soltados e
  inventados se rechazaron en ambas peticiones.
- `hover_client` comprueba las coordenadas locales de superficie de un hover sin
  botón: el arrastre guionizado mueve la ventana a un origen distinto de cero y
  la liberación termina el grab; después el compositor pasa el puntero por los
  puntos globales de `MALAYSIA_TEST_HOVER` y el cliente debe leer cada uno en la
  posición que ocupa sobre la superficie. Un compositor que pasa a Smithay el
  punto local de superficie donde espera el origen de la superficie hace que
  todos los clientes lean el origen de la ventana, y eso falla aquí.
- Nautilus, un cliente real GTK4 + libadwaita, recibe un arrastre guionizado de
  la headerbar; el arnés comprueba que su ventana se movió el mismo delta. Si
  Nautilus no está instalado, o nunca mapea una ventana, el caso se informa como
  UNVERIFIED en vez de fallido.

```sh
./dev.sh move                          # builds malaysia-compositor and the probes, then runs it
cargo run -p sctk-probe --bin move_probe   # run it directly, if already built
```

Variables de entorno: `MALAYSIA_BIN`, `MALAYSIA_MOVE_BIN`, `MALAYSIA_HOVER_BIN`
y `MALAYSIA_APP_NAUTILUS`.

## `pointer_lock`

Mapea un toplevel decorado por el cliente y, en el primer `wl_pointer.enter` (la
entrada guionizada del compositor pasa el puntero sobre la ventana), pide a
`zwp_pointer_constraints_v1` un bloqueo persistente sin región. Pasa cuando el
compositor envía `locked`, algo que solo hace tras activar la restricción para
la superficie que hay bajo el puntero. Agota el tiempo si el global se anuncia
pero nunca se activa.

## `relative_pointer`

Mapea un toplevel, enlaza `zwp_relative_pointer_manager_v1`, crea un puntero
relativo para el puntero del seat y espera un `relative_motion` con delta
distinto de cero de los movimientos guionizados. Agota el tiempo si el
compositor nunca reenvía el movimiento a los clientes de puntero relativo.

Un solo comando maneja ambos, cada uno contra su propio compositor, porque la
pulsación guionizada se inyecta una vez y queda mantenida:

```sh
./dev.sh pointers
```

## `screencopy`

Captura la salida mediante `ext-image-copy-capture-v1`. Crea una fuente para el
primer `wl_output`, abre una sesión y, con las restricciones del compositor,
reserva un buffer SHM que encaje, lo adjunta a un frame y captura. Pasa con el
evento `Ready` del frame y además comprueba que el píxel central es el color de
borrado del compositor (no está en blanco), lo que detecta un orden de canales
erróneo o una copia vacía. También informa de si la sesión ofreció
restricciones DMA-BUF.

```sh
./dev.sh screencopy
```

## `screencopy_window`

Captura una ventana mediante `ext-image-copy-capture-v1`. Mapea una ventana,
espera su handle de `ext-foreign-toplevel-list`, crea con él una fuente de
captura y la captura en un buffer SHM; comprueba que el píxel central es el
contenido propio (gris) de la ventana y no el fondo.

```sh
./dev.sh screencopy-window
```

## `presentation`

Mapea una ventana, pide a `wp_presentation` feedback del siguiente commit y
comprueba que llega el evento `presented` con una marca de tiempo monótona y un
refresco. Un evento `discarded`, o agotar el tiempo, falla.

```sh
./dev.sh presentation
```

## `activation`

Mapea dos toplevels de un mismo cliente (A y luego B, así que B tiene el foco) y
usa el serial de la pulsación de la entrada guionizada para pedir un token de
activación para A; después activa A con él. Pasa cuando el teclado vuelve a
entrar en A, lo que significa que el compositor aceptó el token y movió el foco.
Córrelo con `MALAYSIA_TEST_INPUT=1` (el objetivo `./dev.sh activation` lo hace).

```sh
./dev.sh activation
```

## `move_client`

El cliente que maneja `move_probe`: mapea un toplevel, espera la pulsación
guionizada y tres movimientos, mapea entre medias una segunda ventana para que
el compositor la eleve y le dé el foco, y luego envía `move` y `resize` con el
serial de la pulsación original. Tras la liberación del compositor, envía ambos
otra vez con ese serial y ambos con uno inventado. Sale con código distinto de
cero si no llega ningún configure de redimensionado.

## `hover_client`

El cliente que maneja `move_probe` para la aserción de coordenadas. Mapea una
ventana decorada por el cliente, así que el origen de su superficie es
exactamente el origen de su ventana, y envía el `move` y el `resize` que
responde el gesto guionizado: el move hace que el arrastre guionizado del
compositor mueva la ventana a un origen distinto de cero; el resize hace que
suelte el botón. Después comprueba que el `wl_pointer.enter` y el
`wl_pointer.motion` que recibe en el hover sin botón llevan las posiciones
locales de superficie de `MALAYSIA_EXPECT_HOVER` (una lista de `x,y` separada
por `;`), y sale con código distinto de cero si no coinciden o si el hover nunca
llega.
