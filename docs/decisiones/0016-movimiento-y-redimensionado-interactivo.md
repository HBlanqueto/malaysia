# 0016 — El move y el resize interactivos son estado propio del compositor

Estado: **aceptado** (Bloque 4)

## Contexto

El Bloque 4 añade move y resize interactivos. Un cliente los pide con `xdg_toplevel.move` y `xdg_toplevel.resize`, y el usuario también puede iniciarlos desde la propia decoración de Yuki (barra de título y zonas de borde). Smithay ofrece un framework de grabs — `smithay::input::pointer::PointerGrab`, instalado con `PointerHandle::set_grab` — que está disponible bajo la feature `wayland_frontend`, así que esta es una elección real, no una impuesta.

Hay un segundo consumidor, posterior: el Bloque 5 (popups) necesita keyboard y pointer grabs para el descarte, y esos *sí* están detrás de la feature `desktop` (`PopupKeyboardGrab`/`PopupPointerGrab`), que el ADR 0015 mantiene desactivada. Lo que se decida aquí no debe confundirse con eso.

## Opciones consideradas

1. **Un `PointerGrab` de Smithay por operación.** Tipos `MoveGrab` y `ResizeGrab` que implementan `PointerGrab`, instalados en el seat durante la operación. Este es el uso previsto del framework y detendría el movimiento automáticamente antes de que llegue al cliente.
2. **Un enum de grab propiedad del compositor, manejado desde la ruta de entrada existente.** Todo evento de puntero ya llega a `input.rs`; mientras un grab está activo, el movimiento actualiza la ventana y no se reenvía. El grab es un enum pequeño en `State`.

## Decisión

Opción 2. El Bloque 4 no instala grabs de Smithay. `State::grab` es `Option<Grab>` (`Move` o `Resize`), iniciado en una pulsación de decoración o una petición del cliente, avanzado por el movimiento del puntero, y limpiado en la liberación del puntero o cuando la ventana se destruye.

Las peticiones del cliente se comprueban con serial, no se confían: `xdg_toplevel.move` y `resize` solo se honran cuando su serial nombra el **implicit grab** del puntero — una pulsación de botón que aún se mantiene, hecha sobre la superficie del cliente que pide. Smithay instala ese grab por sí mismo, claveado por el serial de la pulsación (`DefaultGrab::button` establece un `ClickGrab`: `smithay-0.7.0/src/input/pointer/grab.rs:238-252`) y lo suelta cuando se libera el último botón (`:374-380`), por lo que la comprobación es `has_grab(serial)` más los datos de inicio del grab, exactamente como hace el compositor de referencia (`smithay-0.7.0/anvil/src/shell/xdg.rs:152-160`). Los cambios de focus, los raises, los configures y el keyboard enter no tocan el grab y no pueden invalidar una petición; un serial liberado, fabricado o ajeno no nombra ningún grab válido y es rechazado.

## Consecuencias

- El movimiento durante un grab no se reenvía al cliente; el compositor es dueño del puntero durante la operación. Hay un solo lugar que decide esto (`input.rs`), junto a la fuente del movimiento.
- El resize envía un configure que lleva el estado `resizing` de xdg durante la operación. El mínimo y el máximo del cliente, leídos de `SurfaceCachedState`, se respetan, y el `resized` de la escena mantiene fijo el borde opuesto cuando se arrastra un borde principal y un clamp de tamaño muerde.
- Las zonas de borde se miden contra los límites **decorados** de la ventana (`Scene::edge_at`), no contra la superficie del cliente, porque los dos no coinciden (ADR 0014). Las formas de cursor de resize se dibujan desde el tema Xcursor (`ew`/`ns`/`nwse`/`nesw`), con la flecha por defecto como fallback.
- La comprobación de serial es una sola función, `State::held_grab`, con unit tests para ninguna pulsación, un serial fabricado, una pulsación liberada y una pulsación mantenida que sobrevive a otro tráfico de seriales. `./dev.sh move` ejercita la ruta viva: `YUKI_TEST_INPUT` hace que el compositor pulse una vez, mantenga, y siga a la pulsación con movimientos y una liberación, de modo que `move_client` pueda repetir un gesto con forma de GTK — movimientos de umbral, una segunda ventana que toma el focus, luego move y resize con el serial original — y después enviar seriales liberados y fabricados que deben rechazarse. El mismo comando arrastra una headerbar real de Nautilus y afirma que su ventana se movió por el delta inyectado.
- El Bloque 5 no puede reutilizar esto para los popups: un grab de popup debe mover el foco de teclado y puntero y restaurarlo, que es exactamente lo que hacen los `PopupKeyboardGrab`/`PopupPointerGrab` condicionados por `desktop`. El grab del Bloque 4 deliberadamente no intenta ser eso; solo mueve y redimensiona una ventana.

## Referencias

- `crates/yuki/src/input.rs` — `pointer_press`, `drag`, `held_grab`, el cursor de borde.
- `crates/yuki/src/state.rs` — `Grab`.
- `crates/yuki/src/window.rs` — `resize`, `set_resizing`, `send_close`, `minimum_outer_size`/`maximum_outer_size`.
- `crates/yuki-scene/src/scene.rs` — `Scene::edge_at`, `resized`.
- Smithay `src/input/pointer/mod.rs` — `PointerGrab`, el framework no adoptado; `src/input/pointer/grab.rs` — `ClickGrab` y `GrabStartData`, el grab implícito que lee la comprobación de serial.