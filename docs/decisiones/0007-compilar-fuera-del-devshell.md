# 0007 — Compilar fuera de la shell de desarrollo está prohibido

Estado: aceptado (cierra la pregunta abierta 5 del Bloque 0; el guard se movió a la compilación).

## Contexto

Sin la shell falta `RUSTFLAGS` con mold: el binario se enlaza distinto y se calienta otra caché. Un guard solo en `dev.sh` no atrapa un `cargo build` directo.

## Decisión

La shell exporta `YUKI_DEV_SHELL=1`. `build.rs` se niega a compilar si no vale `1`, imprime el mensaje de `dev.sh` y emite `cargo:rerun-if-env-changed=YUKI_DEV_SHELL`. `dev.sh` mantiene `require_devshell` para `build`, `check` y `clean` como fallo temprano; `run`, `stop`, `restart`, `status` y `log` funcionan fuera.

La derivación Nix establece el marcador a propósito y envuelve los binarios con `makeWrapper`: fija `__EGL_VENDOR_LIBRARY_DIRS`, `LIBGL_DRIVERS_PATH`, `GBM_BACKENDS_PATH` y `LD_LIBRARY_PATH`, y guarda los valores previos en `YUKI_ORIG_*`.

## Consecuencias

`cargo build/check/test/clippy` directo falla con el mensaje previsto. El paquete Nix arranca fuera de la shell; el binario de `target/` sigue dependiendo del devShell. No es una sandbox: es una barandilla contra omisiones.

## Alternativas descartadas

- Solo documentación: el error se descubre tarde.
- Marcador solo en `dev.sh`: no cubre `cargo` directo.
- `rustc-wrapper`: se ejecuta también en `nix build` y necesita su propia detección.
