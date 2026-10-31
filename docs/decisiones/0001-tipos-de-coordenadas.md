# 0001 — Tipos de coordenadas: Smithay en el compositor, los nuestros en `malaysia-scene`

Estado: **aceptado, revisado**.

## Contexto

Hay tres espacios: lógico (escritorio), físico (píxeles de salida) y buffer
(píxeles del cliente). Mezclarlos es una fuente clásica de bugs; Smithay ya
ofrece tipos que convierten la mezcla en un error de compilación.

## Decisión

1. El compositor usa `smithay::utils` directamente, sin wrapper.
2. Los tipos propios viven en `malaysia-scene/src/geometry.rs`: lógicos `f64`,
   físicos y de buffer `i32`; la conversión a entero ocurre al entrar en un
   espacio entero.
3. La conversión entre ambos mundos ocurre en un único límite: donde el
   compositor alimenta la escena.

## Consecuencias

- Los handlers y el render usan un solo conjunto de tipos.
- Hay dos geometrías, pero separadas por un límite de crate; la superficie de
  conversión es pequeña y localizable.
- El compositor no puede depender accidentalmente de la geometría de escena.

## Alternativas descartadas

- Tipos propios en el compositor y conversión en cada handler: se revirtió
  porque replicaba la geometría en toda la API de Smithay.
- Diferir la creación de `malaysia-scene`: se creó desde el inicio para que sus
  tipos y tests compilen en cada `./dev.sh check`.
