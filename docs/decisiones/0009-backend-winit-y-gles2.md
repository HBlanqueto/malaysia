# 0009 — El backend anidado es winit con el renderizador GLES2

Estado: aceptado (sesión anidada, parte 1)

## Contexto

La sesión anidada tiene que mostrar un cliente real en una ventana mientras se
construye Yuki. Eso necesita una fuente de eventos para la ventana, una
fuente de entrada y un renderizador. Smithay ofrece varios backends y tres
renderizadores, y la elección condiciona lo que se puede probar en la máquina de
desarrollo (un único escritorio ejecutando una sesión Wayland).

Los requisitos apuntan en una dirección: el compositor debe ejecutarse *dentro*
de la sesión existente, no tomar el control de la máquina. Tomar el control del
dispositivo DRM haría que cada prueba fuera un cierre de sesión, y necesitaría
un seat y privilegios elevados.

## Opciones consideradas

1. **Backend udev/DRM.** El camino de producción: hardware real, scanout real,
   entrada libinput. Pero toma el control de un seat, no puede ejecutarse como
   ventana, y no puede ejercitarse desde una sesión SSH o anidada. Herramienta
   equivocada para la sesión anidada.
2. **Backend X11.** `backend_x11` de Smithay se ejecuta como un cliente X11. La
   sesión anfitriona es Wayland pero sí tiene Xwayland; aun así, es una
   indirección extra y el camino de winit es el que Smithay documenta para el
   desarrollo.
3. **Backend winit.** Se ejecuta como un cliente Wayland o X11 normal de la
   sesión anfitriona, abre una ventana, y proporciona tanto la fuente de entrada
   como un contexto EGL. Pierde el scanout por hardware y el realismo de varios
   outputs, que la sesión anidada explícitamente no necesita. Smithay advierte
   que winit "does not integrate into calloop too well", pero sí expone
   `WinitEventLoop` como un `EventSource`.
4. **Renderizador: pixman.** Software puro, sin GPU, sin shaders. Sencillo, pero
   más lento y fuera del camino que el proyecto quiere para el scanout real más
   adelante.
5. **Renderizador: GLES2 mediante `GlesRenderer`.** El renderizador que el
   backend winit devuelve por defecto y el que usan tanto smallvil como anvil.
   No requiere código de shaders por nuestra parte; los shaders de Smithay son
   internos.

## Decisión

Usar el backend winit con el renderizador GLES2 (`GlesRenderer`) de Smithay. La
dependencia habilita solo `backend_winit`, `renderer_gl` y `wayland_frontend`,
con `default-features = false`, de modo que las rutas de DRM, libinput, udev,
x11 y vulkan no se compilan. Una ventana, un output.

## Consecuencias

- El desarrollo y las pruebas ocurren de forma anidada, junto a la sesión
  anfitriona; el compositor puede iniciarse y detenerse como cualquier otro
  cliente (el envoltorio `dev.sh` de la ADR 0007 sigue aplicándose).
- Las rutas de udev/DRM y libinput no se prueban en este hito y se registran
  como preguntas abiertas en `../bloques/01-sesion.md`.
- El renderizador es GLES2 tras la abstracción de Smithay; Yuki no escribe
  ningún shader en la sesión anidada, en línea con el alcance. Un hito posterior
  que necesite dibujo personalizado puede revisar esto.
- Como `desktop` no está habilitado, el helper de gestión de ventanas `Space`
  de Smithay no está disponible. Es una elección deliberada para la sesión
  anidada, no un accidente de la lista de features: `Space` se sustituye por
  `yuki-scene` en el hito de Direct Session, así que habilitarlo ahora
  añadiría una dependencia que se eliminaría casi de inmediato. `Space` se
  justifica con muchas ventanas repartidas en varios outputs; la sesión anidada
  tiene una ventana en un output, y la división de state de la ADR 0012 ya
  asume que el toplevel se rastrea directamente. Por tanto, el toplevel se
  rastrea a mano, tras un límite de módulo, de modo que el hito de Direct
  Session reescribe un módulo en lugar de cada punto de llamada. La feature
  `desktop` permanece desactivada durante toda la sesión anidada; la ADR 0015
  registra por qué permanece desactivada también para el layout de ventanas, no
  solo para este bloque.
- El recorte de features por defecto es una decisión de superficie de compilación
  que puede ampliarse una feature cada vez cuando un hito necesite otro backend.