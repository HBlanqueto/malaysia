# Bloque 5 — Popups

## Qué se construyó

Los popups (`xdg_popup`) estaban en un estado mínimo: el compositor enviaba un
configure con la geometría cruda del positioner y no los rastreaba. Este bloque
los implementa de verdad: posicionamiento con ajuste de restricciones, un
configure antes de mapear, render, y cierre por clic fuera, Esc y muerte del
cliente.

- **Árbol de popups en `yuki-scene`** (`scene.rs`, ADR 0015). Un popup tiene un
  `PopupId`, un padre (`PopupParent`: `Window`, `Popup` para submenús, o `Layer`
  para un menú de la menubar) y un rectángulo posicionado. El `Scene` los apila
  **encima de todas las ventanas** y ofrece hit-testing (`popup_under`),
  ancestros (`popup_ancestors`) y subárboles (`popup_subtree`, para cerrar un
  menú con sus submenús).
- **Rastreador en el compositor** (`popup.rs`). Un `Popups` guarda la
  `PopupSurface`, su id, su padre y su geometría absoluta; el estado aplica la
  lógica: crear (`add_popup`), mapear en el primer commit con buffer
  (`popup_commit`), reposicionar (`reposition_popup`) y cerrar
  (`dismiss_popups` / `remove_popup_subtree`).
- **Ajuste de restricciones.** Contra el área utilizable del output (relativa al
  padre), vía `PositionerState::get_unconstrained_geometry` (flip/slide/resize).
  El popup se configura con la geometría ajustada **antes** de mapear.
- **Geometría del padre.** El popup se ancla contra el rect usable del toplevel
  (`Windows::usable_rect` — el `window_geometry` xdg), contra el rect de otro
  popup, o contra la geometría de una layer surface — `get_popup` de
  `zwlr_layer_surface_v1` ya funciona porque `WlrLayerShellState` engancha el
  padre y `Layers::geometry_of` lo devuelve.
- **Render.** Los popups mapeados se dibujan encima de las ventanas (entre las
  capas `Bottom` y `Top`) en `session.rs`, y reciben frame callbacks.
- **Cierre.**
  - **Clic fuera**: una pulsación fuera de todo popup abierto los cierra todos
    (de hoja a raíz), y el clic actúa sobre lo que haya detrás.
  - **Esc**: cierra un nivel — el popup más alto.
  - **Grab explícito** (`xdg_popup.grab`): se valida el serial contra el grab
    implícito del puntero; un serial ajeno se rechaza.
  - **Muerte del cliente**: `popup_destroyed` elimina el subárbol — sin
    huérfanos.

## Divergencias con respecto al resumen

- El resumen del roadmap hablaba de "un árbol de popups propiedad de
  `yuki-scene`". Aquí el árbol de *geometría* está en `yuki-scene` y el de
  *superficies* en el compositor (`popup.rs`), igual que se reparten ventanas
  (ADR 0015): la escena no toca protocolos.
- El área utilizable es por ahora **el output completo** (no hay paneles que
  resten); cuando la menubar/dock existan, el target de restricción se reduce a
  su zona exclusiva.
- `PopupParent::Layer` no estaba previsto; lo añade `yuki-scene` para los menús
  con padre layer-surface (la menubar futura).

## Verificación

- `cargo check`, `cargo clippy --workspace --all-targets -- -D warnings` y
  `cargo test --workspace` pasan (incluye tests de `yuki-scene` para el árbol de
  popups); `cargo fmt --check` limpio.
- **`popup_probe`** (`tools/sctk-probe`): mapea un toplevel y un popup anclado a
  su borde inferior con `gravity = bottom` — que quedaría fuera del output — y
  exige que el compositor lo **voltee hacia arriba**. En vivo, contra la sesión
  anidada:
  - `popup created … absolute … y: 640` (el popup, que sin ajuste estaría en
    `y≈760..880`, se volteó para caber en `640..760`).
  - `popup mapped`.
  - Al salir el cliente: `popup subtree removed` — sin huérfanos.
- La aceptación del resumen ("un menú de `foot` contra el borde inferior se
  voltea hacia arriba; un submenú más un clic fuera cierra ambos") queda para la
  comprobación manual con una app real.

## Notas

- `get_popup` de layer-shell se sirve por el propio `WlrLayerShellState` de
  Smithay (engancha el padre a la layer surface); el compositor solo tiene que
  conocer la geometría del padre, que `Layers::geometry_of` ya da.
- Los popups no toman foco de teclado: el foco sigue en el toplevel padre
  (o donde estuviera), como manda el protocolo.
- El reposicionado reactivo (`reactive` en el positioner) no se implementa aún;
  `reposition_request` recalcula y confirma con `repositioned`, pero no se
  re-posiciona al mover el padre.