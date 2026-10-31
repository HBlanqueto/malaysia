# 0013 — El dispatch de clientes hace poll de un descriptor duplicado del display

Estado: aceptado (sesión anidada, parte 2).

## Contexto

El patrón canónico de Smithay envuelve el `Display` en un `Generic` y usa `unsafe { get_mut() }`; este crate prohíbe `unsafe` (ADR 0003).

## Decisión

Los datos del loop son `Session { display, state }`. Se inserta un `Generic` sobre un segundo fd del display (`BorrowedFd::try_clone_to_owned`); al ser legible, el closure desestructura `Session` y llama a `display.dispatch_clients(state)`. El display conserva el original y el poller vigila la copia. Dispatch y flush son no bloqueantes; un error se registra y el loop sigue.

`WAYLAND_DISPLAY` no se muta en el entorno del compositor: el socket se registra al arrancar y a los hijos se les pasa con `Command::env`.

## Consecuencias

`State` sigue siendo solo protocolo; `Session` es privado de `session.rs`.

## Alternativas descartadas

- `calloop-wayland-source`: es para clientes, no para `wayland_server::Display`.
