# Bloque 6 — Portapapeles

## Qué se construyó

El compositor ya anunciaba `wl_data_device_manager`, pero sin comportamiento: no
había nada detrás de la selección. Este bloque conecta el portapapeles y la selección
primaria al foco de teclado, que es exactamente lo que los clientes necesitan para
copiar y pegar entre sí.

- **`wl_data_device` (portapapeles).** El estado ya existía (`DataDeviceState`) y el
  dispatch ya estaba delegado (`delegate_data_device!`). Lo que faltaba era decirle a
  Smithay a qué cliente apunta la selección: `set_data_device_focus` se llama ahora
  siempre que cambia el foco de teclado, con el `Client` de la superficie enfocada
  (`input.rs::focus_window`) o con `None` cuando no hay foco
  (`input.rs::clear_keyboard_focus`).
- **Selección primaria (`zwp_primary_selection_device_manager_v1`).** Se crea un
  `PrimarySelectionState`, se implementa `PrimarySelectionHandler` y se delega con
  `delegate_primary_selection!`. El foco se mantiene con `set_primary_focus`, en los
  mismos dos puntos que el portapapeles.
- **El `DisplayHandle` en `State`.** `set_data_device_focus` y `set_primary_focus`
  necesitan el display, que `State` no guardaba (ADR 0013 mantiene el display fuera
  del estado para que su borrow esté limpio al despachar). La solución guarda un
  `DisplayHandle` — un manejador clonable, no un borrow del `Display` — así que la
  invariante de ADR 0013 se conserva.

## Divergencias con respecto al resumen

Ninguna funcional. El resumen decía "selection y `primary_selection`"; ambas se
implementan. La única diferencia es de forma: el resumen del roadmap no preveía la
necesidad del `DisplayHandle` en `State`, que se resolvió guardando un manejador en
vez de el `Display` (ver arriba).

## Verificación

- `cargo check`, `cargo clippy --workspace --all-targets -- -D warnings` y
  `cargo test --workspace` pasan; `cargo fmt --check` limpio.
- `wayland-info` dentro de la sesión anidada lista tanto
  `wl_data_device_manager` como `zwp_primary_selection_device_manager_v1`.
- Prueba funcional con `wl-clipboard` sobre la sesión anidada: `wl-copy "prueba"` y
  a continuación `wl-paste` devuelve `prueba`, probando el portapapeles de cliente a
  cliente.
- La aceptación del bloque (copiar en una instancia de `foot` y pegar en otra)
  queda para la comprobación manual.

## Notas

- El portapapeles es per-seat: la selección es un dato del `Seat`, no global.
- La selección primaria permite pegar con el botón central tras seleccionar texto,
  como en X11; su foco se mantiene al mismo tiempo que el del portapapeles.
- No se ha tocado el drag & drop; `DataDeviceState` ya lo servía y los grabs de
  cliente/servidor siguen como estaban.