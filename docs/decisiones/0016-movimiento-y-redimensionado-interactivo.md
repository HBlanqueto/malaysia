# 0016 — El move y el resize interactivos son estado propio del compositor

Estado: aceptado (Bloque 4).

## Contexto

El move y el resize llegan por `xdg_toplevel.move`/`resize` y por la decoración
del compositor. Smithay ofrece `PointerGrab`, pero el grab de popups del Bloque 5
está detrás de `desktop` y no se reutiliza aquí.

## Decisión

`State::grab` es `Option<Grab>` (`Move` o `Resize`), iniciado por una pulsación
en la decoración o por una petición del cliente, avanzado con el movimiento y
limpiado al soltar o al destruir la ventana. Las peticiones del cliente se
validan contra el implicit grab del puntero: el serial debe nombrar una pulsación
aún mantenida sobre la superficie que pide (`has_grab(serial)` + datos de
inicio). No se instalan grabs de Smithay para move/resize.

Las zonas de borde se miden contra el rectángulo decorado (`Scene::edge_at`), no
contra la superficie del cliente. Se respetan mínimos/máximos del cliente y el
borde opuesto queda fijo al redimensionar.

## Consecuencias

- El puntero no se reenvía al cliente durante el grab; hay un solo lugar que lo
  decide (`input.rs`).
- `./dev.sh move` ejercita la ruta viva, incluidos seriales liberados y
  fabricados; `./dev.sh decoration` cubre la política con apps reales.
- Los popups necesitan sus propios grabs (Bloque 5); este no intenta ser eso.

## Alternativas descartadas

- Un `PointerGrab` de Smithay por operación: era viable, pero el estado propio
  es más simple para una sola operación y no arrastra el framework de grabs.
