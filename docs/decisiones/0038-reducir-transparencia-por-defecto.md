# 0038 — Reducir transparencia por defecto

Estado: **aceptado** (2026-10-24).

## Contexto

Los mockups muestran barra, menús, dock y sidebar con fondos sólidos, pero el
token `shell-bar` seguía siendo translúcido: sobre un wallpaper fuerte no se veía
el color del tema. Aún no hay blur compositor-side.

## Decisión

1. `appearance.reduce_transparency` es `true` si falta en
   `appearance.domain`. Con ese valor, `Resolved` elige las variantes opacas de
   barra, menús, dock y sidebar.
2. La opacidad se genera en `vow-design`: `build.rs` compone cada token
   translúcido sobre `bg-window` y emite las paletas `*_OPAQUE`.
3. La elección vive en `Resolved`; el kit y la shell consumen
   `resolved.shell`, `resolved.menu` y `resolved.sidebar` y no deciden por scheme.
4. El blur queda para el compositor; con `reduce_transparency = false` se
   conservan los tokens translúcidos actuales.

## Consecuencias

- La shell muestra el color sólido del tema sobre cualquier wallpaper.
- Cambiar a `false` no activa blur todavía, pero deja el camino preparado.
- El resaltado del título sigue siendo una capa translúcida sobre fondo opaco.

## Alternativas descartadas

- Mantener translúcido por defecto: el color del tema no se lee sobre fondos
  fuertes.
- Aplicar alfa fijo en cada componente: duplica la decisión y rompe los tokens.
