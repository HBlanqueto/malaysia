# 0002 — Exponer `crates/yuki` como biblioteca y como binario a la vez

Estado: aceptado (Bloque 0).

## Contexto

Un crate solo binario no tiene interfaz externa: `dead_code` castiga los `pub` sin uso y `tests/` ni los doctests pueden importar sus módulos.

## Decisión

Un paquete con `src/lib.rs` (módulos reutilizables) y `src/main.rs` (arranque que usa la biblioteca). `lib.rs` lleva `#![deny(unsafe_code)]` y `#![warn(missing_docs)]`. `cli` y `logging` son del binario.

## Consecuencias

Los módulos futuros son testeables sin `#[allow(dead_code)]`; el árbol gana `src/lib.rs`, una desviación deliberada.

## Alternativas descartadas

- Solo binario: exigía usos falsos o un compile-fail que no se ejecuta.
