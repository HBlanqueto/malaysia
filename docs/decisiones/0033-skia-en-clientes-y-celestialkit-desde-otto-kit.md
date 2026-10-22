# 0033 — Skia en clientes y CelestialKit desde otto-kit

Estado: **aceptado** (2026-10-22). Modifica el [ADR 0024](0024-stack-de-render-del-toolkit.md).

## Contexto

El ADR 0024 eligió GLES + `cosmic-text` + shaders propios para todo el stack y
dejó `celestial-kit` como un toolkit por escribir. Otto ya tiene resuelto ese
mismo problema con `otto-kit` (MIT, Riccardo Canalicchio), Skia y `laye-rs`.
Reescribir a mano lo que ya existe es peor que reutilizarlo con una frontera
clara: el compositor no necesita Skia, pero la shell y las apps propias sí se
benefician de un canvas 2D completo.

La frontera importante sigue siendo la del ADR 0024: **el compositor dibuja
ventanas y efectos; la shell y las apps propias dibujan su propia UI**. Son
procesos distintos y pueden usar stacks de render distintos sin duplicar el
lenguaje visual, porque los tokens viven en `yuki-design`.

## Decisión

1. **Skia se permite en los procesos cliente** (`yuki-shell` y las apps propias)
   y **nunca en el compositor**. El compositor sigue con GLES, `grade-graphics`
   y `cosmic-text`.
2. **CelestialKit (`celestial-kit`, ≈ AppKit) parte de `otto-kit`** (MIT,
   Riccardo Canalicchio). Se trae de forma **selectiva**, se renombra a nombres
   propios (nada de `otto_*`) y usa los tokens de `yuki-design`.
3. **Grade Animation (≈ Core Animation) pasa a ser el crate `grade-animation`**
   sobre **`laye-rs`** (fork `HBlanqueto/layers`, rama `neptunia`, fijado por
   `rev` en `Cargo.toml`).
4. **Infinity (≈ SkyLight) es el protocolo privado app↔compositor** equivalente
   a `otto-surface-style`: esquinas, sombras, desenfoque y animaciones. Hasta
   que el compositor lo implemente, el kit lo trata como **opcional**.
5. **Grade Graphics se queda como los shaders del compositor.** El kit iguala el
   aspecto con los mismos tokens de `yuki-design`; no necesita los shaders del
   compositor para dibujar su propio contenido.
6. **No se usan `otto-foundations`, `otto-dbus` ni `otto-surface-style`.**
   Se sustituyen por `yuki-design`, `yuki-appearance` e Infinity,
   respectivamente.

## Consecuencias

- El compositor no gana una dependencia de Skia ni cambia su camino de render
  (ADR 0009, 0024, 0027).
- `celestial-kit` puede reutilizar widgets, layout y dibujo de `otto-kit`, pero
  con nombres propios, sin `otto_*` en la API pública y con los tokens de
  `yuki-design` como única fuente de colores y medidas.
- Las animaciones dejan de ser un módulo interno y pasan a `grade-animation`
  sobre `laye-rs`. El fork se fija por commit para que el build sea
  reproducible.
- Infinity puede aparecer en el kit desde el principio como capacidad opcional:
  si el compositor no lo anuncia, el kit sigue funcionando con Wayland estándar.
- `grade-graphics` sigue siendo el punto de encuentro visual del compositor; el
  kit no lo usa para renderizar, solo comparte los tokens.
- Los avisos MIT de `otto-kit` y `laye-rs` viven en `NOTICE`.

## Referencias

- [`0024`](0024-stack-de-render-del-toolkit.md) — stack de render del toolkit,
  modificado por esta ADR.
- [`0020`](0020-politica-de-diseno-y-unificacion.md) — chrome y efectos.
- [`0011`](0011-los-valores-de-diseno-son-constantes-rust.md) — tokens en
  `yuki-design`.
- [`0023`](0023-los-componentes-del-shell-corren-como-procesos-separados.md) —
  un proceso por componente.
- [`0025`](0025-la-shell-es-nativa.md) — shell nativa `yuki-shell`.
- [`0032`](0032-neptunia-como-fuente-de-diseno.md) — Neptunia como fuente.
- `otto-kit` — MIT, <https://github.com/nongio/otto>.
- `laye-rs` — fork `HBlanqueto/layers`, rama `neptunia`, commit `6706e9b`.
