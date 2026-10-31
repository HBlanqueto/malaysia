# 0003 — `deny(unsafe_code)` en el compositor, `forbid` en el resto

Estado: aceptado (Bloque 0).

## Contexto

El compositor necesitará `unsafe` acotado para dmabuf, GL y GBM. `forbid` no
puede relajarse sin editar la raíz del crate; `deny` permite un
`#[allow(unsafe_code)]` explícito y revisable. Los crates hoja no tienen una
superficie FFI previsible.

## Decisión

`malaysia-scene`, `vow-design` y los futuros crates hoja usan
`#![forbid(unsafe_code)]`. `malaysia-compositor` usa `#![deny(unsafe_code)]`
tanto en `lib.rs` como en `main.rs`. La única excepción actual es la
inicialización de EGL/GBM en `udev.rs`, permitida en ese módulo (ADR 0030).

## Consecuencias

- El compositor no gana `unsafe` en silencio: cada relajación es una excepción
  nombrada y visible en revisión.
- Los crates hoja nunca pueden tener `unsafe`.

## Alternativas descartadas

- `forbid` en todas partes: obligaría a editar la raíz del crate y luego a
  relajaciones locales no revisables.
- `deny` en todas partes: pierde la garantía permanente de los crates hoja.
