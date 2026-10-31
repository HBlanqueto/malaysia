# 0015 — El layout de ventanas permanece en `yuki-scene`; no se adopta `desktop::Space`

Estado: aceptado.

## Contexto

`desktop::Space` está detrás de la feature `desktop`, desactivada (ADR 0009). Las ventanas llevan chrome del compositor: rectángulo exterior, superficie del cliente y región de entrada no coinciden, y `Space` modela la geometría de la superficie del cliente.

## Decisión

No se habilita `desktop` ni se adopta `Space`. `yuki-scene` es la autoridad del layout: conjunto ordenado de ventanas, geometría exterior, hit-testing, stacking y raises; deriva de los insets el rectángulo de cliente, la región de entrada y los límites visuales. Se usan los tipos de `wayland_frontend` (`ToplevelSurface`, `PopupSurface`, `PositionerState`) directamente. Para popups, la escena calcula el rectángulo objetivo y lo pasa a `PositionerState::get_unconstrained_geometry`; el compositor posee el árbol de popups y los grabs de teclado/puntero.

## Consecuencias

`window.rs` es la semilla del layout, no un placeholder. El Bloque 4 añade stacking, contención e insets; el Bloque 5, árbol de popups y grabs. `desktop` se revisará solo ante un requisito multi-salida o layer-shell concreto.

## Alternativas descartadas

- Adoptar `Space` con una tabla lateral de insets: recrea el problema dentro de `Space` y obliga a reescribir los consumidores si se revierte.
- Habilitar `desktop` solo por `PopupManager`/grabs: arrastra una escena que no encaja.
