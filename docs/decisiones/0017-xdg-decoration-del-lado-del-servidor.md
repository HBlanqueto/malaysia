# 0017 — xdg-decoration se sirve, y siempre del lado del servidor

Estado: **superado por el ADR 0019** (fue aceptado como seguimiento del Bloque 4)

## Contexto

Yuki dibuja su propio chrome de ventana — barra de título, borde, sombra (ADR 0014). Un cliente solo deja que el compositor lo decore si sabe que el compositor quiere hacerlo: `zxdg_decoration_manager_v1` es el protocolo para eso. Sin el global, un cliente decorador asume que nadie dibujará chrome por él y cae en **decoraciones del lado del cliente**.

Eso es exactamente lo que pasó con `foot`: su trace bajo un Yuki anterior no mostró ningún global `zxdg_decoration_manager_v1` y el mensaje `no decoration manager available - using CSDs unconditionally`. Una ventana de `foot` podía por tanto mostrar su propia barra de título dentro de las decoraciones de Yuki: doble chrome.

El protocolo permite al compositor mantener el lado del servidor incluso cuando el cliente pide el lado del cliente, siempre que diga qué modo eligió en el configure de decoración.

## Decisión

Anunciar `zxdg_decoration_manager_v1` y configurar el modo **server-side** siempre. `new_decoration` y `request_mode`/`unset_mode` se responden todos con un configure que lleva `ServerSide`; a un cliente que pide el lado del cliente se le dice igualmente server-side. Cada configure de xdg ya lleva el modo, así que el cliente lo aprende antes de adjuntar su primer buffer.

Un cliente que nunca vincula el global no se ve afectado: conserva las decoraciones que tuviera, que para un cliente no decorador significa ninguna.

## Consecuencias

- Un cliente decorador deja de dibujar su propio chrome, por lo que la barra de título y el borde de Yuki no se duplican. `foot` ahora vincula el global, pide server-side, y recibe `configure(server_side)` antes de su primer `wl_surface.attach`; el harness de smoke afirma ese orden.
- Yuki debe suministrar por sí mismo una affordance de cierre; aún no dibuja ningún botón de cierre, y el cierre iniciado por el compositor no tiene gesto (el Bloque 4 eliminó el click con botón central). Eso es una brecha de decoración, no de protocolo.
- El modo se decide en el configure inicial, por lo que no hay ninguna ventana que mapee primero y cambie de decoración después.

## Referencias

- Smithay `src/wayland/shell/xdg/decoration.rs` — `XdgDecorationState`, `XdgDecorationHandler`.
- `crates/yuki/src/state.rs` — `decoration_state`.
- `crates/yuki/src/handlers.rs` — `XdgDecorationHandler`.
- `crates/yuki/src/window.rs` — `send_configure` establece `ServerSide`.
- `tools/sctk-probe/src/bin/smoke.rs` — la afirmación de server-side-antes-de-attach.