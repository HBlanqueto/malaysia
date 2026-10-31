# 0006 — El flake es la única fuente de verdad de la toolchain

Estado: aceptado (cierra la pregunta abierta 4 del Bloque 0).

## Contexto

`rust-toolchain.toml` y el devShell declaraban versiones de Rust que podían divergir; sin rustup en la máquina, el archivo era documentación pretendiendo ser configuración.

## Decisión

El flake es la única fuente. Se elimina `rust-toolchain.toml`; el devShell da `cargo`, `rustc`, `rustfmt` y `clippy` desde el nixpkgs fijado por `flake.lock`. `workspace.package.rust-version` queda como suelo MSRV y no debe superar la versión del flake. Para subir la toolchain: actualizar nixpkgs, comprobar versiones, ajustar MSRV si hace falta, `./dev.sh check` y commitear `flake.lock` (y `Cargo.toml`) juntos.

## Consecuencias

Una subida de toolchain es una subida del input del flake; editores y herramientas corren en el devShell (ADR 0005).

## Alternativas descartadas

- `rust-toolchain.toml` con rust-overlay/fenix: segundo mecanismo y no funciona sin rustup.
- Mantener el archivo como espejo con comprobación: dos fuentes que pueden discrepar.
