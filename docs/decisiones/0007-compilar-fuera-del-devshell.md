# 0007 — Compilar fuera de la shell de desarrollo está prohibido

Estado: aceptado (cierra la pregunta abierta 5 del Bloque 0; el guard se movió a la compilación)

## Contexto

El linker mold se solicita mediante `RUSTFLAGS` establecido en la shell de desarrollo (ADR 0005).
Ejecuta `cargo build` fuera de `nix develop` y el flag está ausente: la compilación usa
el linker por defecto, produce un binario enlazado de forma diferente, y calienta una
caché de cargo separada. Ambos resultados son incorrectos por razones invisibles en la
salida — la compilación tiene éxito, simplemente no es la compilación que el proyecto pretende.

El ADR 0005 nombró a la shell de desarrollo como el entorno soportado pero no impidió que nadie
la abandonara. La primera versión de esta decisión puso el guard en `dev.sh`,
que atrapaba el punto de entrada sancionado pero no un `cargo build` directo. Esa
versión solo se sostenía porque no hay `rustc` ni `cargo` en el `PATH` fuera
de la shell en esta máquina: la garantía descansaba en una circunstancia de la configuración
actual, no en un mecanismo del proyecto, y caducaría el momento en que se instalara una
toolchain de sistema. Esta revisión mueve el guard a la compilación misma.

## Opciones consideradas

1. **No soportado, solo documentado.** Un `cargo build` crudo aún tiene éxito
   silenciosamente y produce un binario enlazado de forma diferente; el fallo solo se
   descubre leyendo la documentación, que es precisamente lo que la gente no hace.
2. **Marcador aplicado solo en `dev.sh`.** Cubre el punto de entrada sancionado y
   falla rápido, pero una invocación directa de `cargo` fuera de la shell pasa. Protegía
   la garantía solo mientras nada más pudiera compilar el crate.
3. **Cargo `rustc-wrapper`.** Establecer `build.rustc-wrapper` alcanza cada invocación de
   cargo, incluidas las crudas. Pero el wrapper también se ejecuta durante el `nix build`
   del paquete, donde el marcador está ausente, así que necesita su propia detección
   de shell — una pieza móvil insertada en cada compilación.
4. **Un build script en el crate, con `dev.sh` como ruta rápida.** El build
   script se ejecuta en cada `cargo build`/`check`/`test`/`clippy` del crate, así que
   una invocación directa queda atrapada, y no cuesta nada más allá de la compilación propia del
   crate. `dev.sh` aún rechaza antes y de forma más barata para la ruta común.

## Decisión

Compilar fuera de la shell de desarrollo está prohibido (opción 4). La shell de desarrollo exporta
`YUKI_DEV_SHELL=1`.

`crates/yuki/build.rs` se niega a compilar a menos que el marcador sea `1`, imprimiendo
el mismo mensaje que `dev.sh` y apuntando a este registro. Emite
`cargo:rerun-if-env-changed=YUKI_DEV_SHELL`, de modo que cambiar el marcador fuerza
al script a ejecutarse de nuevo y un paso previo exitoso no puede cachearse a través del
límite.

`dev.sh` mantiene `require_devshell`, llamado por `build`, `check` y `clean`. Falla
primero, antes de que cargo arranque, con el código de salida 7. Es la ruta rápida, ya no
la única. Los comandos que no compilan — `run`, `stop`, `restart`,
`status`, `log` — siguen funcionando fuera de la shell.

La derivación del paquete nativo establece `YUKI_DEV_SHELL = "1"` a propósito: es una
ruta de compilación sancionada y hermética, no interactiva, y no usa
mold.

## Consecuencias

- Un `cargo build`, `cargo check`, `cargo test` o `cargo clippy` directo con
  el marcador sin establecer ahora falla con el mensaje previsto en lugar de producir un
  binario enlazado de forma diferente.
- `run` funciona fuera de la shell porque la shell de desarrollo también hornea su ruta
  de búsqueda de librerías en tiempo de ejecución en el `RUNPATH` del binario. winit carga
  `libwayland-client` mediante `dlopen`, que solo consulta `RUNPATH` o
  `LD_LIBRARY_PATH`; sin la entrada el binario salía con
  `The wayland library could not be loaded` — el único caso donde "no
  compila, así que sigue funcionando fuera de la shell" no se sostenía. Las rutas horneadas
  son las mismas store paths que la shell exporta en `LD_LIBRARY_PATH`, que
  aún establece para las herramientas que se ejecutan como procesos hijo (las sondas).
- `cargo:rerun-if-env-changed` significa que el guard no puede ser derrotado por una compilación
  exitosa en caché: quitar el marcador re-ejecuta el script y falla.
- `dev.sh` sigue siendo el fallo más temprano y barato para la ruta común; las dos
  capas no entran en conflicto.
- La compilación del paquete no está bloqueada, porque establece el marcador deliberadamente.
- Esto es una barandilla (guardrail), no una sandbox: el marcador es una variable de entorno
  ordinaria y puede establecerse a mano. Existe para detener una omisión silenciosa
  accidental, que es lo que la versión anterior arriesgaba; no es un límite
  de seguridad.
- La garantía de la versión anterior — que cargo directo no podía compilar — se
  registra aquí como dependiente de la ausencia de una toolchain fuera de la shell,
  y ahora está reemplazada por una comprobación en la compilación.