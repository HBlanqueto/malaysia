# 0006 — El flake es la única fuente de verdad de la toolchain

Estado: aceptado (cierra la pregunta abierta 4 del Bloque 0)

## Contexto

Dos archivos declaraban una versión de Rust. `rust-toolchain.toml` fijaba `1.98.1` y
listaba rustfmt y clippy; el devShell del flake suministraba `cargo` y `rustc`
desde nixpkgs. Dos declaraciones de una versión divergen tan pronto como una de
ellas se mueve, y el síntoma es un error de compilador que parece inexplicable porque la
toolchain que lo produjo no es la que el archivo afirma.

La situación en la máquina de desarrollo es más aguda que el caso general:
no hay `rustc` ni `rustup` en el `PATH` fuera de la shell de desarrollo. El
pin de `rust-toolchain.toml` no puede respetarse en absoluto, porque el programa que
lo leería — rustup — no está instalado y el `rustc` de la shell de desarrollo es una
compilación simple de nixpkgs, no un proxy de rustup. El archivo era documentación pretendiendo
ser configuración.

`nix develop -c rustc --version` y un `rustc --version` simple ejecutados dentro de
la shell ambos reportan `rustc 1.98.1 (48a229cea 2026-09-01)`, así que coinciden por
construcción: son el mismo binario.

## Opciones consideradas

1. **`rust-toolchain.toml` gana; el devShell lo sigue.** Esto requeriría
   rustup o un input `rust-overlay`/`fenix` para que la shell instale exactamente el
   release fijado. Añade un input de flake y un segundo mecanismo de toolchain a un
   proyecto con una sola máquina, y aún así no se respetaría fuera de la
   shell sin rustup, que no está instalado.
2. **El flake gana; mantener `rust-toolchain.toml` como espejo más una comprobación.** Dos
   fuentes permanecen y aún pueden discrepar; lo mejor que se ofrece es detectar la divergencia
   después de que ocurra, no prevenirla.
3. **El flake gana; eliminar `rust-toolchain.toml`.** Una sola fuente: el `cargo`/`rustc`
   de nixpkgs fijado por `flake.lock`. El `rust-version` propio de Cargo en el
   manifiesto del workspace permanece como el suelo de versión mínima soportada que cargo
   aplica durante la resolución, que es una preocupación diferente de la toolchain
   usada para compilar.

## Decisión

El flake es la única fuente de verdad para la toolchain (opción 3).
`rust-toolchain.toml` se elimina. La shell de desarrollo proporciona `cargo`, `rustc`,
`rustfmt` y `clippy` desde nixpkgs, y `flake.lock` fija la revisión exacta de nixpkgs.
`workspace.package.rust-version = "1.98"` permanece como el suelo MSRV y
no debe exceder la versión de rustc del flake.

### Subir la toolchain

1. Actualizar el input de nixpkgs: `nix flake update nixpkgs` (o
   `nix flake lock --update-input nixpkgs`).
2. Confirmar las nuevas versiones: `nix develop -c rustc --version` y
   `nix develop -c cargo --version`.
3. Si el nuevo release eleva el mínimo que el código necesita, actualizar
   `workspace.package.rust-version` en el mismo cambio.
4. Ejecutar `./dev.sh check` y hacer commit de `flake.lock` (y `Cargo.toml` si el MSRV
   se movió) juntos.

## Consecuencias

- Hay exactamente un lugar donde se elige una versión de toolchain; `flake.lock`
   lo registra y una subida de versión es una subida del input del flake.
- Los editores y herramientas deben ejecutarse dentro de la shell de desarrollo (ADR 0005). No hay
   ningún pin de rustup al que recurrir, que ya es la situación en esta máquina.
- `cargo` y `rustc` no siempre son el mismo point release — nixpkgs envió
   `cargo 1.98.0` junto a `rustc 1.98.1` aquí. Viajan como un conjunto; la
   línea de release, no el número de parche, es lo que el proyecto razona.
- El suelo MSRV en `Cargo.toml` describe lo que el código requiere, no lo que está
   instalado, y ambos deben mantenerse consistentes cuando uno de ellos se mueva.