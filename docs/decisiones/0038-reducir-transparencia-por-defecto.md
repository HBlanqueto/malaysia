# 0038 — Reducir transparencia por defecto

Estado: **aceptado** (2026-10-24).

## Contexto

Los mockups 16 y 17 del lienzo de Neptunia muestran la barra de menús, los
menús, el dock y las barras laterales del kit con fondos sólidos. El token
`shell-bar` seguía siendo translúcido y la barra y el dock lo pintaban tal cual:
sobre un wallpaper de color fuerte el resultado no era el color del tema.
Todavía no existe desenfoque compositor-side.

## Decisión

1. **Opaco por defecto.** `appearance.reduce_transparency` es `true` si falta
   en `appearance.domain`. Con ese valor, `Resolved` elige las variantes
   opacas de la barra, los menús, el dock y el sidebar.
2. **La opacidad se genera en `yuki-design`.** `build.rs` compone cada token
   translúcido sobre `bg-window` y emite las paletas `*_OPAQUE` y los fondos
   `SIDEBAR_*_OPAQUE`. Un token ya opaco se conserva igual.
3. **La elección vive en `Resolved`.** `celestial-kit` y la shell no vuelven a
   decidir por scheme ni aplican alfa fijo: consumen `resolved.shell`,
   `resolved.menu` y `resolved.sidebar`.
4. **El desenfoque queda para el compositor.** Cuando exista, se implementará
   como shader en `grade-graphics` y se activará con
   `reduce_transparency = false`; hasta entonces ese valor conserva los tokens
   translúcidos actuales.

## Consecuencias

- La barra, los menús y el dock muestran el color sólido del tema sobre
  cualquier wallpaper.
- El ajuste se guarda desde ya: cambiar a `false` no activa blur todavía, pero
  deja el camino preparado.
- El resaltado abierto del título de la barra sigue siendo una capa
  translúcida sobre el fondo opaco, así que mantiene el contraste.
- No hay cambios en el compositor en este ADR.

## Referencias

- [`0020`](0020-politica-de-diseno-y-unificacion.md) — efectos para todas las
  ventanas.
- [`0024`](0024-stack-de-render-del-toolkit.md) — stack de render y punto de
  entrada del blur.
- [`0032`](0032-neptunia-como-fuente-de-diseno.md) — Neptunia como fuente.
- `crates/yuki-design/neptunia/tokens.json` — `shell-bar`, `bg-raised`,
  `bg-sunken` y `bg-hover`.
- `crates/yuki-appearance/src/lib.rs` — `reduce_transparency` y `Resolved`.
- `crates/celestial-kit/src/theme.rs` — consumo de las paletas resueltas.
