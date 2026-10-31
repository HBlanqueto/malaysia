# 0002 — Exponer el compositor como biblioteca y como binario

Estado: aceptado (Bloque 0).

## Contexto

Un crate solo binario no tiene interfaz externa: `dead_code` castiga los `pub`
sin uso y `tests/` ni los doctests pueden importar sus módulos. El compositor
crecerá con piezas reutilizables que necesitan tests.

## Decisión

Un solo paquete con `src/lib.rs` (módulos reutilizables) y `src/main.rs`
(carcasa de arranque que usa la biblioteca). `lib.rs` lleva
`#![deny(unsafe_code)]` y `#![warn(missing_docs)]` (ADR 0003). `cli` y
`logging` son módulos del binario.

## Consecuencias

- Los módulos futuros son testeables y no necesitan `#[allow(dead_code)]`.
- El árbol gana `src/lib.rs`; es una desviación deliberada del encargo.
- La garantía compile-fail de geometría se movió con `geometry.rs` a
  `malaysia-scene` (ADR 0001).

## Alternativas descartadas

- Solo binario: exigía usos falsos o un compile-fail comentado que no se ejecuta.
- Un crate `-core` separado: prematuro antes de tener nada que separar.
