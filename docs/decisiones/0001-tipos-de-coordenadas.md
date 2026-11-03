# 0001 — Tipos de coordenadas: los de Smithay en el compositor, los nuestros en `yuki-scene`

Estado: aceptado, revisado.

## Contexto

Mezclar espacio lógico, físico y de buffer es una fuente clásica de bugs; Smithay ya ofrece tipos que convierten la mezcla en error de compilación.

## Decisión

El compositor usa `smithay::utils` sin wrapper. Los tipos propios viven en `malaysia-scene/src/geometry.rs` y son solo lógicos. El paso a físico y a buffer usa los tipos de Smithay, donde el compositor alimenta el render.

## Consecuencias

Handlers y render usan un único conjunto de tipos; las dos geometrías quedan separadas por un límite de crate y la superficie de conversión es pequeña.

## Alternativas descartadas

- Tipos propios en el compositor y conversión en cada handler: replicaba la geometría en toda la API de Smithay (se revirtió).
