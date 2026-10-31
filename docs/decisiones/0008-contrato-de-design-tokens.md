# 0008 — Los design tokens fluyen en una sola dirección, de Nix a Rust

Estado: **superado por el ADR 0011.** Se conserva como registro de lo que se
consideró y se rechazó: los tokens acabaron siendo constantes Rust escritas a
mano en `vow-design`, no generadas desde Nix.

## Contexto

Sin tokens, el mismo color o radio se copia en varios sitios. Este contrato se
escribió antes de que existiera ningún valor de diseño.

## Decisión

El contrato era: `design/v1.nix` posee los valores; un generador emite un único
archivo Rust versionado en el crate de diseño; solo ese crate re-exporta los
tokens; ningún otro crate lee Nix ni el archivo generado; no se permiten
constantes de diseño escritas a mano.

## Consecuencias

- El generador y la comprobación de obsolescencia nunca se escribieron; el
  ADR 0011 invirtió la decisión.
- El versionado de diseño era explícito: un cambio semántico exigía un `vN.nix`
  nuevo.

## Alternativas descartadas

- Generar en `OUT_DIR` con `build.rs`: los diffs de diseño dejan de ser visibles.
- Escribir tokens a mano y dejar Nix como documentación: dos fuentes sin
  sincronía.
