# 0003 — `deny(unsafe_code)` en el compositor, `forbid` en el resto

Estado: aceptado (Bloque 0)

## Contexto

El Bloque 0 prohíbe `unsafe`. Los atributos a nivel de crate que expresan esto son
`#![forbid(unsafe_code)]` y `#![deny(unsafe_code)]`:

- `forbid` no puede ser anulado por ningún atributo interno ni flag de línea de comandos.
  Una vez establecido, `unsafe` nunca puede habilitarse en ese crate sin editar el
  atributo en sí.
- `deny` produce el mismo error por defecto pero puede relajarse localmente con
  `#[allow(unsafe_code)]` en un elemento específico.

`crates/yuki` eventualmente necesitará `unsafe` en lugares estrechos y auditados:
importar un file descriptor de dma-buf o hablar con GL efectivamente requiere FFI,
y no existe un wrapper seguro para cada llamada. Establecer `forbid` en el compositor
significaría editar la raíz del crate la primera vez que eso ocurra, y cualquier
relajación posterior sería invisible en el historial en lugar de ser el deliberado y
revisable `#[allow]` que `deny` fuerza.

Los crates hoja pequeños (design tokens, más adelante un kit de widgets compartidos) no tienen
una superficie FFI previsible, así que `forbid` es la declaración correcta y permanente allí.

## Opciones consideradas

1. `forbid(unsafe_code)` en todas partes. La más fuerte, pero garantiza una edición
   de la raíz del crate — y luego relajaciones locales no revisables — en el momento en que el
   soporte de GL o dmabuf aterrice.
2. `deny(unsafe_code)` en todas partes. Uniforme, pero pierde la oportunidad de hacer que los
   crates hoja sean permanentemente seguros.
3. `forbid` en los crates hoja, `deny` en el compositor.

## Decisión

Opción 3. `crates/yuki-design` (y más adelante `yuki-scene`, `yuki-kit`) usan
`#![forbid(unsafe_code)]`. `crates/yuki` usa `#![deny(unsafe_code)]`, tanto
en `lib.rs` como en `main.rs`. Introducir `unsafe` en cualquier lugar aún requiere un
`#[allow(unsafe_code)]` explícito en el elemento, que es visible en la revisión.

## Consecuencias

- El compositor no puede ganar `unsafe` silenciosamente; cada relajación es una excepción
  nombrada y revisable.
- Los crates hoja nunca pueden ganar `unsafe` en absoluto.
- El Bloque 0 en sí no contiene `unsafe`, así que aún no existe ningún `#[allow(unsafe_code)]`.