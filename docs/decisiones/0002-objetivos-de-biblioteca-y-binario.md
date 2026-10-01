# 0002 — Exponer `crates/yuki` como biblioteca y como binario a la vez

Estado: aceptado (Bloque 0)

## Contexto

El diseño solicitado para `crates/yuki` solo lista un punto de entrada binario
(`main.rs`, `cli.rs`, `logging.rs`, `geometry.rs`): un crate solo binario. Rust
moderno, sin embargo, trata un crate binario como carente de interfaz externa, y por lo
tanto aplica el lint `dead_code` a los elementos `pub` que nada dentro del crate
alcanza. Un crate solo binario tampoco puede ser ejercitado por tests de integración ni
doctests: `tests/` no puede importar los módulos del binario, y `cargo test` no
ejecuta doctests para objetivos binarios.

Este razonamiento se aplicó originalmente a los tipos de coordenadas, que eran
API pública aún no consumida. Desde el ADR 0001, esos tipos se han movido al
crate `yuki-scene`, así que el objetivo de biblioteca del compositor está actualmente vacío.
Se mantiene porque el compositor crecerá con bloques de construcción con forma de biblioteca
(state, conversión del límite de la escena, tokens) que necesitan ser testeables, y
porque eliminar y volver a añadir el objetivo a medida que aterricen sería churn.

## Opciones consideradas

1. **Solo binario.** Coincidir exactamente con el conjunto de archivos listado, pero añadir un uso
   falso de la geometría desde `main` para silenciar `dead_code`, y conformarse con un ejemplo
   de compile-fail comentado. El comentario no se ejecutaría realmente.
2. **Biblioteca y binario en un solo paquete.** Añadir `src/lib.rs` (el compositor
   reutilizable) junto a `src/main.rs` (una delgada carcasa de arranque/apagado), como hace niri.
   Los elementos `pub` pasan a formar parte de la interfaz de la biblioteca, y los doctests se ejecutan.
3. **Un crate `yuki-core` separado para la geometría.** Correcto pero prematuro: divide
   el proyecto en tres crates antes de que haya nada que separar.

## Decisión

Opción 2. `crates/yuki/src/lib.rs` declara los módulos que forman la
biblioteca, y `crates/yuki/src/main.rs` usa el crate de biblioteca
(`yuki::...`) para las piezas reutilizables. La biblioteca lleva
`#![deny(unsafe_code)]` y `#![warn(missing_docs)]` (ver ADR 0003).

Al momento de escribir esto la biblioteca no expone módulos, porque su único
candidato (`geometry`) se movió a `yuki-scene` en el ADR 0001. `cli` y
`logging` son módulos del binario: son preocupaciones de arranque, no bloques de construcción
reutilizables.

## Consecuencias

- Los módulos futuros con forma de biblioteca son testeables y están exentos de la
  estrictez `dead_code` del crate binario; `clippy -D warnings` se mantiene verde sin un
  uso artificial ni un `#[allow(dead_code)]`.
- La estructura de directorios listada gana un archivo, `src/lib.rs`. Esta es una
  desviación deliberada del encargo del Bloque 0 y se señala para revisión.
- Los módulos solo de arranque (`cli`, `logging`) viven en el binario, mientras que los
  bloques de construcción reutilizables viven en la biblioteca. La división es una convención, no un
  límite impuesto.
- La garantía compile-fail para la mezcla de coordenadas se movió con `geometry.rs` a
  `yuki-scene` (ADR 0001); ya no es un doctest en este crate.