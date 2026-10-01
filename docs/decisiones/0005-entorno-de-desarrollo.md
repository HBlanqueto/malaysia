# 0005 — Una máquina, una shell, un linker

Estado: aceptado (Bloque 0)

## Contexto

Yuki apunta a una única máquina de desarrollo y aún no se distribuye a nadie. Eso
elimina un conjunto de restricciones de portabilidad que un proyecto de propósito general tendría
que respetar, y el Bloque 0 elige gastar esa libertad en lugar de protegerse contra
entornos que no existen.

Varias decisiones de desarrollo interactúan:

- **El entorno de compilación.** El proyecto se compila dentro de `nix develop`, que
  fija la toolchain de Rust, las librerías nativas y el linker. No existe una
  ruta de compilación soportada fuera de la shell.
- **El linker.** mold enlaza el workspace notablemente más rápido que el linker
  por defecto. La herramienta debe estar tanto instalada como realmente solicitada, y una solicitud
  que dependa del `PATH` ambiente hace que el comportamiento de compilación se ramifique y voltee el
  fingerprint de cargo (invalidando toda la caché de compilación) cuando cambia.
- **Gestión de procesos.** `dev.sh` inicia el compositor en segundo plano y
  debe poder detener exactamente ese proceso sin afectar la shell que lo
  invocó.
- **Archivos en tiempo de ejecución.** Los archivos de PID y log necesitan una ubicación
  escribible, por usuario.

## Opciones consideradas

### Solicitar mold

1. **Incondicionalmente en `.cargo/config.toml`.** Cada invocación de cargo recibe
   el flag, incluido `nix build` del paquete, donde mold no está presente —
   lo que rompe esa compilación. Tampoco puede confinarse a la shell de desarrollo.
2. **Sondear el `PATH` en `dev.sh`.** Habilita mold solo cuando se encuentre, así que funciona en
   máquinas sin él. Pero el comportamiento de compilación entonces depende del
   entorno ambiente, dos desarrolladores en el mismo commit pueden compilar de forma diferente, y
   alternar el flag invalida toda la caché de cargo — una ruta ramificada que el
   proyecto evita conscientemente.
3. **Instalar mold en la shell de desarrollo y establecer `RUSTFLAGS` allí.** Cualquiera dentro
   de `nix develop` tiene mold por construcción y cada compilación dentro de la shell usa el
   flag idéntico. El flag nunca llega a la compilación del paquete.

### Detener el proceso en segundo plano

1. **Señalar al grupo de procesos** (`kill -- -PID`). Conveniente para matar un
   árbol completo, pero también señala a la shell invocante cuando `dev.sh` se ejecuta
   de forma interactiva, lo que puede matar la sesión de terminal.
2. **Señalar el PID guardado.** Seguro, pero confía en el PID; si el compositor ha
   salido y el PID fue reutilizado, señalaría a un proceso no relacionado.
3. **Señalar el PID guardado después de confirmar que es Yuki.** Leer
   `/proc/<pid>/comm` y exigir que sea igual a `yuki` antes de señalar o
   reportar "running". Solo Linux, lo cual está bien aquí.

## Decisión

- La shell de desarrollo es el entorno soportado. mold se instala allí y
  `RUSTFLAGS = "-C link-arg=-fuse-ld=mold"` se establece en `flake.nix`. No se
  hace commit de ninguna configuración de linker a `.cargo/config.toml` y `dev.sh` no
  sondea mold. Las compilaciones dentro de la shell son idénticas y estables en caché.
- `dev.sh` señala solo el PID guardado, nunca el grupo de procesos, y tanto la
  comprobación de running como `stop` verifican que `/proc/<pid>/comm` sea `yuki` antes de
  actuar. Un PID que no pertenezca a Yuki se trata como un archivo de PID obsoleto
  y se elimina en lugar de señalar.
- `status` sigue los códigos de salida de los init scripts LSB (0 running, 1 muerto con un
  archivo de PID, 3 no corriendo) para que pueda usarse como booleano, mientras que `restart` es
  `stop` seguido de `run` y nunca consulta `status`.
- Los archivos en tiempo de ejecución viven bajo `$XDG_RUNTIME_DIR/yuki-dev/`, con respaldo
  en `/tmp/yuki-$(id -u)` cuando `XDG_RUNTIME_DIR` está sin establecer o no existe. El
  directorio específico de desarrollo mantiene el pid y el log fuera del namespace de sockets
  de la sesión; el socket propio del compositor se nombra automáticamente `wayland-N` en el
  `XDG_RUNTIME_DIR` del llamador, que `run` pasa sin cambios (ADR 0013).

## Consecuencias

- Entrar vía `nix develop` es la única ruta soportada; un `cargo build`
  simple compila una compilación diferente (linker de sistema, caché separada). Aceptado para un
  proyecto de una sola máquina, y registrado como una pregunta abierta en
  `../bloques/00-cimientos.md` por si se añade CI.
- `dev.sh` es específico de Linux por causa de `/proc`. No hay un requisito de
  portabilidad que justifique una alternativa.
- La compilación del paquete nativo no se ve afectada por la elección de linker del desarrollador.
- Un PID reutilizado no puede hacer que `dev.sh` detenga un proceso no relacionado.