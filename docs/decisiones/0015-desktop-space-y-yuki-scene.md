# 0015 — El layout de ventanas permanece en `malaysia-scene`; no se adopta `desktop::Space`

Estado: aceptado.

## Contexto

Smithay trae `desktop::Space` detrás de la feature `desktop`, que está
desactivada (ADR 0009). Las ventanas de MalaysiaDreams llevan chrome del
compositor: el rectángulo exterior, la superficie del cliente y la región de
entrada no coinciden, y `Space` modela la geometría de la superficie del
cliente.

## Decisión

No se habilita `desktop` ni se adopta `Space`. `malaysia-scene` es la autoridad
del layout: conjunto ordenado de ventanas, geometría exterior por ventana,
hit-testing, stacking y raises; deriva de los insets el rectángulo de cliente,
la región de entrada y los límites visuales. Se usan los tipos de
`wayland_frontend` (`ToplevelSurface`, `PopupSurface`, `PositionerState`)
directamente. Para popups, la escena calcula el rectángulo objetivo y se lo pasa
a `PositionerState::get_unconstrained_geometry`; el compositor posee el árbol de
popups y los grabs de teclado/puntero.

## Consecuencias

- `window.rs` es la semilla del layout de la escena, no un placeholder.
- El Bloque 4 añade a la escena stacking, contención e insets; el Bloque 5,
  árbol de popups y grabs.
- `desktop` sigue desactivada; se revisará solo ante un requisito multi-salida o
  layer-shell concreto.

## Alternativas descartadas

- Adoptar `Space` y corregirlo con una tabla lateral de insets: recrea el
  problema de propiedad dentro de `Space` y obliga a reescribir todos los
  consumidores si se revierte.
- Habilitar `desktop` solo por `PopupManager`/grabs: arrastra una abstracción de
  escena que no encaja con el chrome.
