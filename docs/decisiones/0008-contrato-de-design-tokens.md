# 0008 — Los design tokens fluyen en una sola dirección, de Nix a Rust

Estado: superado por el ADR 0011. Se conserva como registro de lo que se rechazó: los tokens acabaron siendo constantes Rust escritas a mano en `yuki-design`, no generadas desde Nix.

## Contexto

Sin tokens, el mismo color o radio se copia en varios sitios.

## Decisión

`design/v1.nix` poseía los valores; un generador emitía un único archivo Rust versionado en el crate de diseño; solo ese crate re-exportaba los tokens; ningún otro leía Nix ni el archivo generado; no se permitían constantes de diseño a mano.

## Consecuencias

El generador y la comprobación de obsolescencia nunca se escribieron; el ADR 0011 invirtió la decisión.

## Alternativas descartadas

- Generar en `OUT_DIR` con `build.rs`: los diffs de diseño dejan de ser visibles.
