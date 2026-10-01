# 0013 — El dispatch de clientes hace poll de un descriptor de archivo duplicado del display

Estado: aceptado (sesión anidada, parte 2)

## Contexto

La parte 2 de la sesión anidada da a Yuki un `Display<State>`, un socket a la
escucha y una forma de introducir las peticiones de los clientes en el loop de
calloop sin bloquearlo nunca. El puente de calloop es la única elección real en
esta parte, y las respuestas obvias están bloqueadas en este árbol.

El patrón canónico de Smithay, usado por ambas referencias, envuelve el propio
display en un `Generic` y lo saca de nuevo para despachar:

```rust
insert_source(Generic::new(display, Interest::READ, Mode::Level), |_, display, data| {
    unsafe { display.get_mut().dispatch_clients(data) };
    Ok(PostAction::Continue)
})
```

`Generic::get_mut` es `pub unsafe fn` — la referencia mutable que entrega podría
soltar el objeto con poll mientras sigue registrado. `crates/yuki` prohíbe el
código unsafe (ADR 0003), así que ese patrón no está disponible.

## Opciones consideradas

1. **`calloop-wayland-source`.** El crate existe exactamente para esta forma de
   problema, pero adapta un `wayland_client::EventQueue` y `Connection` — es del
   lado del cliente. Un compositor tiene un `wayland_server::Display`.
   Rechazada.
2. **`Generic<Display<State>>` con `unsafe { get_mut() }`.** El patrón de la
   comunidad (anvil, smallvil). Correcto, pero viola la política de unsafe del
   crate por una sola llamada. Rechazada.
3. **Un `EventSource` escrito a mano que envuelva el display.** Su
   `process_events` recibe `&mut Self::Metadata`, y el `State` del loop llega por
   separado como datos del loop; la fuente no tiene forma de alcanzar `State`,
   así que no puede llamar a `dispatch_clients(&mut State)`. Rechazada.
4. **Los datos del loop llevan ambos, y un `Generic` hace poll de un fd
   duplicado.** Convertir los datos del loop en `Session { display, state }` e
   insertar un `Generic` sobre un segundo descriptor de archivo para el display,
   obtenido de forma segura con `BorrowedFd::try_clone_to_owned`. Cuando es
   legible, el closure desestructura `Session` en sus dos campos disjuntos y
   llama a `display.dispatch_clients(state)`. Adoptada.

## Decisión

Despachar mediante la opción 4. El display lo posee `Session`, que son los datos
del loop de calloop; `State` sigue siendo solo de protocolo como exige la ADR
0012, y los manejadores siguen recibiendo `&mut State`. No se añade ninguna
dependencia nueva y no se usa unsafe.

El descriptor duplicado se refiere al mismo socket, así que la disponibilidad
del poll se comparte con el descriptor propio del display; el display conserva
el original y el poller vigila la copia. `dispatch_clients` y `flush_clients`
son no bloqueantes, así que la fuente solo se ejecuta cuando hay algo que leer y
un cliente que deja de leer o escribir no puede atascar el loop. Un error de
dispatch o flush se registra y el loop continúa: un cliente problemático no
detiene el compositor.

## Consecuencias

- `State` ya no son los datos del loop de calloop; lo es `Session`, y es privado
  a `session.rs`. `State` sigue siendo el tipo que mutan los manejadores de
  protocolo, así que la garantía de la ADR 0012 no cambia.
- Registrar el display cuesta una duplicación `fcntl` en el arranque. Es el
  precio de no usar unsafe; se paga una vez, no por evento.
- Cuando el hito de Direct Session sustituya el backend, esta fuente no se toca:
  el backend accede a `Session.state` de la misma manera, y el display no es
  específico del backend.
- `WAYLAND_DISPLAY` **nunca se establece en el entorno del propio compositor**.
  En la edición 2024 `std::env::set_var` es `unsafe` porque un lector
  concurrente es comportamiento indefinido, y este crate prohíbe unsafe — pero
  mutar el entorno completo del proceso también está mal por sí mismo: un
  compositor anidado que exporta su propio nombre de socket puede entonces
  confundirse a sí mismo o a un hermano con el anfitrión. El nombre del socket
  se registra en el arranque, y a un hijo que el compositor lanza se le da la
  variable con `Command::env`, que es seguro y está limitado a ese hijo. El
  primer lanzamiento (sesión anidada, parte 6) lo implementa; hasta entonces el
  arnés de pruebas pasa `WAYLAND_DISPLAY` explícitamente.
- `wayland-info` (de `wayland-utils`) y `foot` se unen al dev shell para que las
  comprobaciones de aceptación de la sesión anidada se ejecuten contra
  herramientas ancladas.
- `dev.sh` pasa el entorno del shell que llama al compositor sin cambios. El
  compositor es a su vez un cliente de la sesión anfitriona, así que winit debe
  resolver el display anfitrión exactamente como lo hace el que llama; un
  intento anterior de darle un `XDG_RUNTIME_DIR` privado (con `WAYLAND_DISPLAY`
  absolutizado) rompía el arranque cada vez que los dos discrepaban. El socket
  de servidor propio del compositor lo elige `ListeningSocketSource::new_auto`,
  que enlaza el primer `wayland-1` libre (nunca el `wayland-0` del anfitrión).