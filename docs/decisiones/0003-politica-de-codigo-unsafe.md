# 0003 — `deny(unsafe_code)` en el compositor, `forbid` en el resto

Estado: aceptado (Bloque 0).

## Contexto

El compositor necesitará `unsafe` acotado para dmabuf, GL y GBM. `forbid` no se relaja sin editar la raíz del crate; `deny` permite un `#[allow(unsafe_code)]` explícito y revisable.

## Decisión

`yuki-scene`, `yuki-design` y los futuros crates hoja usan `#![forbid(unsafe_code)]`. `yuki` usa `#![deny(unsafe_code)]` en `lib.rs` y `main.rs`. La única excepción actual es la inicialización EGL/GBM de `udev.rs` (ADR 0030).

## Consecuencias

Cada `unsafe` del compositor es una excepción nombrada y visible; los crates hoja nunca pueden tenerlo.

## Alternativas descartadas

- `forbid` en todas partes: obliga a editar la raíz y luego a relajaciones no revisables.
- `deny` en todas partes: pierde la garantía permanente de los crates hoja.
