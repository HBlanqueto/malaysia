# 0012 — El backend vive junto al State del compositor

Estado: aceptado (sesión anidada, parte 1)

## Contexto

La sesión anidada convierte el stub privado `State` de `session.rs` en el state
real del compositor. Actualmente guarda una sola cosa, el `LoopSignal` de
calloop, y la ventana de winit, el `GlesRenderer` y el rastreador de daños viven
en locales dentro de `session::run`.

El hito de Direct Session sustituye winit por DRM/KMS. La pregunta que zanja
este registro es dónde van los handles del backend: dentro de `State`, donde los
manejadores de protocolo pueden alcanzarlos, o junto a él, donde no pueden.
Hacerlo mal y cada manejador tendrá que editarse cuando cambie el backend.

Dos referencias en la misma versión de Smithay muestran los dos polos:

- **niri** (`src/backend/mod.rs`) pone un enum `Backend` de tres variantes
  dentro de su `State` (`State { backend, niri }`), con un método por
  capacidad (`render`, `seat_name`, `import_dmabuf`, ...) y un accessor `&mut`
  por variante. Los manejadores llaman a través del enum y nunca nombran un
  backend concreto.
- **smallvil** (`src/state.rs`, `src/winit.rs`) mantiene el state del protocolo
  en `Smallvil` y el backend winit en un local capturado por el closure del
  loop. El state del compositor nunca ve el backend.

## Opciones consideradas

1. **Campos concretos de backend en `State`.** `state.backend: WinitGraphicsBackend<GlesRenderer>`,
   `state.renderer`, `state.damage_tracker`. Lo más sencillo hoy, pero cada
   manejador que solicita un frame o importa un buffer nombra los tipos de
   winit, de modo que la parte 2 del hito de Direct Session edita manejadores en
   todo el árbol. Rechazada.
2. **El enum `Backend` de niri en `State`.** El punto de intercambio es un tipo,
   y el compilador mantiene a los manejadores honestos entre variantes. Pero
   niri lo necesita porque distribuye TTY, winit y headless *simultáneamente*;
   Yuki tiene exactamente un backend por ejecución. Un enum con una sola
   variante hoy es la abstracción especulativa que prohíben las reglas del
   proyecto, y además tendría que crecer con un método por cada operación de
   backend que descubramos. El encapado es correcto; el enum no toca aún.
3. **Backend junto a `State` (la forma de smallvil).** `State` es solo
   contabilidad de protocolo y de escena; el dueño del loop (`session.rs`)
   guarda la ventana, el renderizador y el rastreador de daños en su propio
   ámbito. Los manejadores reciben `&mut State` y físicamente no pueden nombrar
   un tipo de backend. Adoptada.

## Decisión

`State` no guarda ningún tipo de backend. Vive en su propio módulo público
(`crate::state`) y contiene los structs del state del protocolo y la señal del
loop — lo suficiente para mutar el state del compositor y despertar el loop. El
backend permanece en `session.rs`, capturado por las fuentes de eventos que lo
poseen.

La interfaz de `State` al backend es estrecha e independiente del backend: los
manejadores cambian el state y llaman a `State::stop` (o, más tarde, ponen una
marca de redibujo y despiertan el loop), y el loop lee el state cuando el backend
le pide dibujar. Ningún tipo de winit, GLES, DRM o scanout aparece en `State` ni
en ningún manejador.

`State` ganará `DisplayHandle`, `CompositorState`, `ShmState`, `SeatState`,
`XdgShellState`, `OutputManagerState`, el `Seat`, el envoltorio de la escena y
el tiempo de inicio a medida que aterricen las partes que los crean. Los structs
del protocolo no pueden construirse antes: los constructores de Smithay
(`CompositorState::new`, `ShmState::new`, `XdgShellState::new`,
`OutputManagerState::new_with_xdg_output`) requieren que el tipo de state
implemente antes sus traits `Dispatch`/`GlobalDispatch`, y esos impls son los
propios manejadores.

## Consecuencias

- Sustituir winit por DRM/KMS es cirugía en `session.rs` (y, cuando crezca, un
  módulo `backend` junto a `state.rs`) más la escena. Ningún manejador cambia.
- `session.rs` sigue siendo el único dueño del loop y de la ventana. Cuando
  llegue el hito de Direct Session y haya dos backends reales, la opción 2 puede
  reconsiderarse desde una posición basada en evidencia en lugar de suposiciones.
- Los manejadores son comprobables y legibles sin una pila gráfica, porque solo
  dependen de `State`.
- El coste es una indirección en el camino de dibujo: el loop debe leer qué ha
  cambiado de `State` en lugar de que los manejadores rendericen directamente.
  Esa indirección es el ciclo de redibujado que la parte 6 necesita de todos
  modos, así que no es extra.
- `State` es público, así que sus campos forman parte de la API del crate. Eso
  es deliberado — los manejadores en `crates/yuki` son los consumidores.