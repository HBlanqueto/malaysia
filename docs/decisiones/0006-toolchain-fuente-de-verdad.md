# 0006 — El flake es la única fuente de verdad de la toolchain

Estado: aceptado (cierra la pregunta abierta 4 del Bloque 0).

## Contexto

`rust-toolchain.toml` y el devShell declaraban versiones de Rust que podían
divergir. En esta máquina no hay rustup, así que el archivo era documentación
pretendiendo ser configuración.

## Decisión

El flake es la única fuente. Se elimina `rust-toolchain.toml`; el devShell
proporciona `cargo`, `rustc`, `rustfmt` y `clippy` desde el nixpkgs fijado por
`flake.lock`. `workspace.package.rust-version` queda como suelo MSRV y no debe
superar la versión del flake.

Para subir la toolchain: actualizar nixpkgs, comprobar `rustc`/`cargo`, ajustar
el MSRV si hace falta, ejecutar `./dev.sh check` y commitear `flake.lock` (y
`Cargo.toml` si cambió el mínimo) juntos.

## Consecuencias

- Una subida de toolchain es una subida del input del flake.
- Editores y herramientas deben ejecutarse dentro del devShell (ADR 0005).
- `cargo` y `rustc` viajan como un conjunto de nixpkgs; la línea de release, no
  el parche, es lo que se razona.

## Alternativas descartadas

- `rust-toolchain.toml` con rust-overlay/fenix: añade un segundo mecanismo y no
  funciona fuera de la shell sin rustup.
- Mantener el archivo como espejo con comprobación: dos fuentes que aún pueden
  discrepar.
