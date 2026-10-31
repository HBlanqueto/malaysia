# 0016 — El move y el resize interactivos son estado propio del compositor

Estado: aceptado (Bloque 4).

## Contexto

Move y resize llegan por `xdg_toplevel.move`/`resize` y por la decoración.

## Decisión

`State::grab` es `Option<Grab>` (`Move` o `Resize`), iniciado por una pulsación en la decoración o una petición del cliente, avanzado con el movimiento y limpiado al soltar o destruir la ventana. Las peticiones se validan contra el implicit grab del puntero: el serial debe nombrar una pulsación aún mantenida sobre la superficie que pide (`has_grab(serial)` + datos de inicio). No se instalan grabs de Smithay para move/resize.

Las zonas de borde se miden contra el rectángulo decorado (`Scene::edge_at`), no contra la superficie del cliente. Se respetan mínimos/máximos y el borde opuesto queda fijo.

## Consecuencias

El puntero no se reenvía al cliente durante el grab; `./dev.sh move` ejercita la ruta viva, incluidos seriales liberados y fabricados, y `./dev.sh decoration` cubre apps reales.

## Alternativas descartadas

- Un `PointerGrab` de Smithay por operación: era viable, pero el estado propio es más simple y no arrastra el framework.
