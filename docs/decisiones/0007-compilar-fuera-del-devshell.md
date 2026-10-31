# 0007 — Compilar fuera de la shell de desarrollo está prohibido

Estado: aceptado (cierra la pregunta abierta 5 del Bloque 0; el guard se movió a
la compilación).

## Contexto

Sin la shell falta `RUSTFLAGS` con mold: el binario se enlaza distinto y se
calienta otra caché. Un guard solo en `dev.sh` no atrapa un `cargo build`
directo, y la garantía dependía de que no hubiera toolchain fuera de la shell.

## Decisión

La shell de desarrollo exporta `MALAYSIA_DEV_SHELL=1`. `build.rs` del
compositor se niega a compilar si el marcador no es `1`, imprime el mismo
mensaje que `dev.sh` y emite `cargo:rerun-if-env-changed=MALAYSIA_DEV_SHELL`.
`dev.sh` mantiene `require_devshell` para `build`, `check` y `clean` como fallo
temprano; `run`, `stop`, `restart`, `status` y `log` funcionan fuera.

La derivación Nix establece el marcador a propósito y envuelve los binarios con
`makeWrapper`: fija `__EGL_VENDOR_LIBRARY_DIRS`, `LIBGL_DRIVERS_PATH`,
`GBM_BACKENDS_PATH` y `LD_LIBRARY_PATH`, y guarda los valores previos en
`MALAYSIA_ORIG_*` para que los clientes lanzados los restauren.

## Consecuencias

- `cargo build/check/test/clippy` directo falla con el mensaje previsto.
- El paquete Nix arranca fuera de la shell; el binario de `target/` sigue
  dependiendo del devShell.
- El guard no es una sandbox: es una barandilla contra omisiones accidentales.

## Alternativas descartadas

- Solo documentación: el error se descubre tarde.
- Marcador solo en `dev.sh`: no cubre `cargo` directo.
- `rustc-wrapper`: se ejecuta también en `nix build` y necesita su propia
  detección de shell.
