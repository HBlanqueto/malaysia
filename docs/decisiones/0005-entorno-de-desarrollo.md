# 0005 — Una máquina, una shell, un linker

Estado: aceptado (Bloque 0).

## Contexto

El proyecto apunta a una sola máquina.

## Decisión

- La shell de desarrollo es el entorno soportado. mold se instala allí y `RUSTFLAGS = "-C link-arg=-fuse-ld=mold"` se define en `flake.nix`; no se versiona linker en `.cargo/config.toml`.
- `dev.sh` señala solo el PID guardado, nunca el grupo, y comprueba que `/proc/<pid>/comm` sea el compositor antes de actuar. `status` sigue los códigos LSB (0/1/3); `restart` es `stop` + `run`.
- Los archivos de desarrollo viven en `$XDG_RUNTIME_DIR/yuki-dev/` (o `/tmp/yuki-$(id -u)`). El socket es un `wayland-N` propio.

## Consecuencias

Compilar fuera del devShell produce un binario distinto (prohibido por el ADR 0007).

## Alternativas descartadas

- mold incondicional en `.cargo/config.toml`: rompe `nix build`.
