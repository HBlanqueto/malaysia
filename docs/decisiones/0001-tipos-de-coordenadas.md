# 0001 — Tipos de coordenadas: los de Smithay en el compositor, los nuestros en `yuki-scene`

Estado: **aceptado, revisado** (la decisión original del Bloque 0 se revirtió en la revisión)

## Contexto

Yuki manipula tres nociones diferentes de posición:

- **Lógica** — coordenadas del escritorio, independientes de la escala de la salida.
- **Física** — píxeles reales en una salida, tras aplicar su factor de escala.
- **Buffer** — píxeles dentro del buffer de un cliente, antes de que el compositor
  los mapee sobre una salida.

Mezclarlas es una fuente común de bugs en los compositores, así que el Bloque 0 exigía tipos
que conviertan la mezcla en un error de compilación. Smithay ya incluye esos tipos, y ambos
proyectos de referencia los usan directamente:

- **niri** importa `smithay::utils::{Logical, Physical, Raw, Size, ...}` y
  solo añade la selección de escala de monitor (`src/utils/scale.rs`); no tiene un módulo
  de geometría propio.
- **Otto** también usa `smithay::utils::Point<f64, Physical>` en su
  código de renderizado.

Smithay modela esto con `Point<N, Kind>`, `Size<N, Kind>` y
`Rectangle<N, Kind>`, donde `Kind` es uno de `Logical`, `Physical`, `Buffer`,
`Raw` (además de un `Client` interno). La aritmética está bloqueada a un único `Kind`, y las
conversiones son métodos explícitos que toman un `Scale<N>`:
`to_physical(scale)`, `to_logical(scale)`, `to_buffer(scale, transform)`.
`Logical` es `f64`; las coordenadas físicas y de buffer son enteras.

## Decisión original (superada)

Definir nuestros propios `Point<Space>`, `Size<Space>` y `Rect<Space>` en
`crates/yuki/src/geometry.rs` y convertir en el límite con Smithay una vez
que se introduzca Smithay. Esto se justificó como una forma de mantener el núcleo libre de
los feature flags de Smithay.

## Por qué se revirtió

La decisión original calculó mal el costo. No se trata de "una capa de conversión
más adelante". Cada API de Smithay que un compositor toca — los handlers de protocolo, el
positioner, `Space`, el seat, los eventos de entrada, la geometría de salida — habla
en `smithay::utils::Point<i32, Logical>` y compañía. Tener un tipo paralelo significa
convertir en cada una de esas callbacks, que es exactamente la redundancia que la
regla del proyecto de "sin abstracción especulativa" existe para prevenir. Sin implementaciones
reales no había evidencia para justificar un wrapper, y los proyectos
de referencia que ya pagaron ese costo (niri, Otto) ambos eligieron los tipos de
Smithay.

La división es por crate, no por preferencia:

- **`crates/yuki`** (el compositor) usa `smithay::utils` directamente. Sin
  wrapper, sin capa de conversión.
- **`yuki-scene`** (creado en el Bloque 0 para esta geometría; el scene graph
  en sí es un hito posterior) tiene su propia geometría, porque el scene graph
  no debe depender de Smithay.
- **La conversión ocurre exactamente en un único límite**: donde el compositor alimenta
  el scene graph.

## Decisión

1. El compositor usa `smithay::utils` para todos los tipos de coordenadas. Se añade una
   dependencia de Smithay cuando el primer módulo realmente necesite geometría, no
   antes, así que el Bloque 0 no introduce ninguna.
2. Los tipos propios se mueven a `crates/yuki-scene/src/geometry.rs` ahora,
   con `yuki-scene` creado como miembro del workspace que contiene solo ese
   módulo. El crate se declara desde el inicio para que los tipos se compilen y
   sus tests se ejecuten en cada `./dev.sh check`; dejar el archivo fuera del
   árbol de módulos equivaldría a `#[allow(dead_code)]`.
3. En ese módulo, las coordenadas lógicas son `f64` y las físicas/de buffer
   son `i32`, para que la animación de resortes y las escalas fraccionarias no queden
   cuantizadas. La conversión a enteros ocurre solo al entrar en un espacio
   entero.

## Consecuencias

- Los handlers de protocolo y el código de renderizado usan un único conjunto de tipos sin
  conversiones, que es el objetivo de todo esto.
- Existen dos nociones de geometría en el proyecto (las de Smithay y las de
  `yuki-scene`), pero están separadas por un límite de crate en lugar de
  dispersas por el compositor, así que la superficie de conversión es pequeña y
  localizable.
- `yuki-scene` existe antes que el scene graph, sin dependencias,
  de modo que su geometría se valida mientras el scene graph aún se está
  diseñando.
- El compositor no tiene geometría propia, así que no puede depender accidentalmente
  de los tipos del crate de la escena en lugar de los de Smithay.
- Cuando aterrice el primer módulo que consuma Smithay, la dependencia y su
  conjunto de features deben elegirse y justificarse en ese momento.
- Esto se registra como una pregunta abierta en `../bloques/00-cimientos.md`.

## Referencias

- Smithay `src/utils/geometry.rs` — `Logical`, `Physical`, `Buffer`, `Raw`,
  `Point<N, Kind>`, `Size<N, Kind>`, `Rectangle<N, Kind>`, `to_physical`,
  `to_logical`, `to_buffer`.
- niri `src/utils/scale.rs` — usa los tipos de Smithay directamente.
- Otto `Cargo.toml` y código de renderizado — usa los tipos de `smithay::utils` directamente.