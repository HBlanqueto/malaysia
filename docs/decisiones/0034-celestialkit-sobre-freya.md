# 0034 — CelestialKit sobre Freya

Estado: **aceptado** (2026-10-22). Reemplaza al [ADR 0033](0033-skia-en-clientes-y-celestialkit-desde-otto-kit.md)
y modifica al [ADR 0024](0024-stack-de-render-del-toolkit.md) para los procesos
cliente.

## Contexto

El ADR 0033 puso Skia en los clientes y propuso basar CelestialKit en `otto-kit`
con `laye-rs` como motor de animación. La revisión del código mostró dos
problemas: `laye-rs` no tiene licencia y `otto-kit` no es un toolkit sobre el que
se pueda construir sin arrastrar decisiones ajenas. Freya (MIT) ya ofrece un
núcleo de UI reactiva, layout, accesibilidad y render Skia con crates separados
del backend de ventanas.

## Decisión

1. **CelestialKit se construye sobre los crates de núcleo de Freya**, fijados
   exactos a `=0.4.3`: `freya-core`, `freya-components` (con
   `default-features = false`), `freya-animation`, `freya-engine`, `torin` y
   `ragnarok`. **Nunca** se usan el crate paraguas `freya` ni `freya-winit`: el
   paraguas activa `winit` por defecto y `freya-winit` arrastra `winit`, `gtk`,
   `rfd`, `tray-icon`, `ash` y `glutin`.
2. **El kit tiene su propio runner Wayland** (SCTK + calloop + EGL), porque
   `winit` no soporta layer-shell ni `ext-session-lock-v1`. El runner cubre
   `zwlr_layer_shell_v1`, `ext-session-lock-v1` y, más adelante, `xdg_toplevel`.
3. **Skia solo en procesos cliente.** El compositor nunca enlaza crates `freya-*`
   ni Skia: sigue con GLES, `grade-graphics` y `cosmic-text`.
4. **Grade Animation es un módulo de `celestial-kit` sobre `freya-animation`**,
   con un resorte propio (duración + rebote) escrito desde cero. De `laye-rs`
   solo se toma la idea, no el código.
5. **Los tokens siguen saliendo de `yuki-design` y `yuki-appearance`.** El tema
   de `freya-components` se genera desde ellos. Nada de `material-design`.
6. **Infinity sigue igual**: opcional en el kit hasta que el compositor lo
   anuncie.
7. **`otto-kit` deja de ser fuente de código.** Queda solo como referencia de
   lógica (DBusMenu, estados de menús). Si algún día se copia código MIT de Otto,
   su aviso va a `NOTICE` en el mismo commit.
8. **Subir a Freya 0.5 cuando sea estable**, en su propio commit, regenerando
   Skia.

## Consecuencias

- `celestial-kit` gana un runner Wayland propio; el kit deja de ser un módulo de
  dibujo y pasa a ser una biblioteca de UI completa para la shell.
- El compositor no cambia su camino de render ni gana dependencias de Skia,
  Freya o `winit`.
- La derivación de Nix compila Skia para `freya-skia-bindings` 0.98.0, no para
  `skia-safe` 0.93. Su versión y features se comprueban contra el árbol real de
  Cargo.
- `grade-animation` deja de existir como crate: es `celestial-kit::animation`.
- Los avisos MIT de `otto-kit` y `laye-rs` desaparecen de `NOTICE`; no queda
  código suyo.

## Referencias

- [`0024`](0024-stack-de-render-del-toolkit.md) — stack de render, modificado
  por esta ADR para los clientes.
- [`0020`](0020-politica-de-diseno-y-unificacion.md) — chrome y efectos.
- [`0011`](0011-los-valores-de-diseno-son-constantes-rust.md) — tokens en
  `yuki-design`.
- [`0023`](0023-los-componentes-del-shell-corren-como-procesos-separados.md) —
  un proceso por componente.
- [`0025`](0025-la-shell-es-nativa.md) — shell nativa `yuki-shell`.
- [`0032`](0032-neptunia-como-fuente-de-diseno.md) — Neptunia como fuente.
- Freya — MIT, <https://github.com/marc2332/freya>.
