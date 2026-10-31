# 0005 — Una máquina, una shell, un linker

Estado: aceptado (Bloque 0).

## Contexto

El proyecto apunta a una sola máquina. La compilación vive en `nix develop`,
mold acelera el enlace y `dev.sh` gestiona un proceso en segundo plano. Las
decisiones de entorno no deben depender del `PATH` del llamador.

## Decisión

- La shell de desarrollo es el entorno soportado. mold se instala allí y
  `RUSTFLAGS = "-C link-arg=-fuse-ld=mold"` se define en `flake.nix`; no se
  versiona configuración de linker en `.cargo/config.toml`.
- `dev.sh` señala solo el PID guardado, nunca el grupo de procesos. Antes de
  actuar comprueba que `/proc/<pid>/comm` sea el binario del compositor; un PID
  ajeno se trata como archivo obsoleto y se borra.
- `status` sigue los códigos LSB (0 running, 1 muerto con PID, 3 no corre);
  `restart` es `stop` + `run`.
- Los archivos de desarrollo viven bajo `$XDG_RUNTIME_DIR/malaysia-dev/` (o
  `/tmp/malaysia-$(id -u)`). El socket del compositor es un `wayland-N` propio.

## Consecuencias

- Compilar fuera de `nix develop` produce un binario distinto; está prohibido por
  el ADR 0007.
- `dev.sh` es específico de Linux por `/proc`, aceptado para este proyecto.
- Un PID reutilizado no puede hacer que se mate un proceso ajeno.

## Alternativas descartadas

- mold incondicional en `.cargo/config.toml`: rompe `nix build`, donde no está.
- Sondear mold en `PATH`: compilaciones distintas según el entorno e
  invalidación de la caché de cargo.
- Matar el grupo de procesos: puede matar la terminal que invoca `dev.sh`.
