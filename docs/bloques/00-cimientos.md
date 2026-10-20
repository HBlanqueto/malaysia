# Bloque 0 del roadmap — Andamiaje (Cimientos)

> **Correspondencia con el roadmap.** Este es el registro detallado del **Bloque 0** del
> roadmap (Andamiaje), al que aquí se llama "Cimientos". El trabajo posterior está nombrado, no
> numerado: la **sesión anidada** (`01-sesion.md`, Bloques 1–3 del
> roadmap) y el **hito Direct Session** (el backend DRM/KMS, sin numerar).

Estado: **completo.** El Bloque 0 consta de seis partes:

1. el workspace, el toolchain y los perfiles de compilación;
2. los tipos de coordenadas de `yuki-scene`, con sus pruebas y ADRs;
3. el arranque del binario, el análisis de argumentos y el logging;
4. `dev.sh`, su subcomando `watch`, y `../hardware.md`;
5. los valores por defecto del sistema de `yuki-design`;
6. este documento.

Este documento describe qué contiene el bloque, qué no contiene deliberadamente,
las decisiones detrás de él y las preguntas que quedan abiertas para hitos posteriores.

## Propósito

El Bloque 0 construye el esqueleto sobre el que se apoyan todos los bloques posteriores: un
workspace, una forma consistente de compilarlo y ejecutarlo, un modelo de coordenadas que
convierte los tres tipos de posición con los que trata un compositor en tipos distintos, y un binario
mínimo que arranca, registra y se apaga. **No** implementa un compositor. El
bucle de eventos, los manejadores de protocolo y el renderizado son trabajo de la sesión anidada
(Bloques 1–3 del roadmap).

La regla rectora del bloque es que nada se añade sin un consumidor. Un
tipo, una dependencia, un trait o una API se introducen solo cuando el código real lo necesita, y
cada dependencia lleva una justificación de una línea en `Cargo.toml`.

## Qué se construyó

### Workspace y toolchain

- **Workspace virtual** (`Cargo.toml`). La raíz no contiene ningún crate propio,
  solo ajustes compartidos y perfiles de compilación. Resolver 3, edition 2024, Rust
  mínimo 1.98.
- **Tres crates** bajo `crates/`:
  - `yuki` — el compositor, tanto como biblioteca como binario (ADR 0002).
    `#![deny(unsafe_code)]`.
  - `yuki-scene` — el hogar del scene graph. El Bloque 0 contiene solo su
    módulo de geometría; el graph en sí es un hito posterior.
    `#![forbid(unsafe_code)]`, sin dependencias.
  - `yuki-design` — los valores por defecto del diseño del sistema, como constantes
    de tiempo de compilación documentadas. `#![forbid(unsafe_code)]`, sin dependencias.

  La sesión anidada añadió un cuarto miembro, solo de desarrollo, en `tools/sctk-probe`: los clientes
  probe usados para verificar el compositor. Vive fuera de `crates/` y está
  excluido de la compilación por defecto, por lo que no forma parte del compositor.
- **Toolchain desde el flake.** No hay `rust-toolchain.toml`: el `cargo`/`rustc`
  de nixpkgs del dev shell, anclados por `flake.lock`, son los que compilan, y el
  `rust-version` de `Cargo.toml` es el mínimo (ADR 0006).
- **Dev shell Nix** (`flake.nix`) — el único entorno de compilación soportado. Provee
  las herramientas de Rust, `cargo-watch`, mold y shellcheck, y las bibliotecas
  nativas que necesitan los hitos posteriores (libinput, libglvnd, seatd, libxkbcommon, mesa,
  wayland, udev). Establece `RUSTFLAGS` para solicitar el linker mold y exporta
  `YUKI_DEV_SHELL=1` (ADR 0005, ADR 0007).
- **Configuración de Cargo** (`.cargo/config.toml`) — `git-fetch-with-cli` para
  entornos sin un libgit2 utilizable, y un comentario que registra por qué el flag del
  linker *no* está aquí.

### Perfiles de compilación

- `dev` — el código propio de Yuki sin optimizar, las dependencias ligeramente optimizadas, para
  que las reconstrucciones incrementales de nuestro código sigan siendo baratas sin que el código de
  terceros sea dolorosamente lento.
- `dev-fast` — hereda `dev`, desactiva la información de debug para enlazar más rápido. Para
  iterar sobre errores de compilación.
- `release` — thin LTO, una unidad de codegen, panic = abort, símbolos eliminados.

### Tipos de coordenadas

`crates/yuki-scene/src/geometry.rs` define `Point<N, Space>`,
`Size<N, Space>` y `Rect<N, Space>` para los tres espacios `Logical`,
`Physical` y `Buffer`. La aritmética está bloqueada a un único espacio, de modo que mezclar
espacios es un error de compilación — verificado por un doctest `compile_fail` — y cada
conversión es un método `to_*` con nombre que toma una escala explícita. Las coordenadas
lógicas son `f64`; las físicas y de buffer son `i32`. Véase el ADR 0001
para saber por qué estos tipos viven aquí en lugar de en el compositor, y el ADR 0004 para
la pregunta sin resolver de redondear rectángulos al espacio entero.

### El binario

`crates/yuki` arranca, instala el logging, emite una línea `yuki starting`,
y entiende `--log-level`, `--help` y `--version`. El análisis de argumentos está
escrito a mano contra `std::env::args`: una única opción no justifica una
dependencia de parsing. Las pruebas cubren el parser (8) y el mapeo de niveles de log (4).
El ciclo de vida del proceso en el Bloque 0 era "esperar `SIGTERM`/`SIGINT`"; la sesión anidada
reemplazó esa espera con su bucle de eventos, y el manejo de señales se movió con ella.

### `yuki-design`

Los valores por defecto del sistema: colocación y métricas de la titlebar, colores de los
botones, forma de la ventana, tipografía, la sombra de dos capas y las paletas clara y
oscura. Son constantes simples y structs `const` pequeños — `Rgba`, `FocusPair`,
`Titlebar`, `TitlebarButtons`, `WindowStyle`, `Font`, `ShadowStyle`, `Palette`
— con `LIGHT`, `DARK`, `TITLEBAR`, `WINDOW`, `TEXT` y `SHADOW` como los
valores exportados. Los colores son `f32` en `0.0..=1.0`, construidos con `rgb8`/`rgba8` para
que los literales sigan siendo legibles y ningún color se convierta por frame.

Dos cosas están modeladas deliberadamente. Los colores que difieren entre una ventana enfocada y
una no enfocada son un `FocusPair`, de modo que un valor no enfocado es su propia
constante en lugar de un multiplicador aplicado después. La línea interior del modo oscuro es
`Option<Rgba>`, ausente en modo claro, de modo que un renderer puede saltarse el pase en lugar
de dibujar algo invisible. El acento proviene de la configuración del host;
el doc del módulo registra el archivo, la línea y el formato `#rrggbb` original
(ADR 0011).

### `dev.sh`

Un envoltorio shell POSIX alrededor de cargo y una instancia en segundo plano del binario.
Ejecuta `./dev.sh --help` para la lista; los comandos son `build [--release]`,
`run [--release]`, `watch [--release]`, `stop`, `restart`, `status`, `log [-f]`,
`clean`, `check`, `smoke [--release]`, `block4 [--release]`,
`move [--release]` y `decoration`. Usa solo coreutils, cargo, `setsid`
y `cargo-watch`,
es limpio bajo `shellcheck`, y mantiene los archivos de ejecución bajo
`$XDG_RUNTIME_DIR/yuki-dev/` (con respaldo en `/tmp/yuki-$(id -u)`). El
socket de Wayland del compositor no está ahí — Smithay lo nombra automáticamente `wayland-N` en
el `XDG_RUNTIME_DIR` propio del compositor, que `run` pasa desde el
llamador (`tools/sctk-probe/README.md` tiene la receta para conectar un cliente).
Los códigos de salida están agrupados para que los llamadores puedan ramificar:

| Código | Significado |
| ---- | ------- |
| 0 | éxito |
| 1 | `status`: proceso muerto con un archivo PID sobrante |
| 2 | error de uso |
| 3 | `run`: ya en ejecución; `status`: no en ejecución |
| 4 | falta una herramienta requerida |
| 5 | el binario no pudo arrancar |
| 6 | `log`: no hay archivo de log |
| 7 | `build`/`check`/`clean`/`watch`/`smoke`: no dentro del dev shell |
| 8 | `watch`: el watcher salió inesperadamente |
| 9 | `smoke`/`block4`/`move`/`decoration`: falló la compilación o la prueba |

`status` sigue la convención de scripts init de LSB (en ejecución = 0, muerto con archivo
PID = 1, no en ejecución = 3), de modo que puede usarse como booleano en scripts.
`restart` es `stop` seguido de `run` y nunca consulta `status`. `stop`
señala solo el PID guardado — nunca al grupo de procesos — y tanto la comprobación de
ejecución como `stop` confirman que el PID pertenece a un proceso cuyo `/proc/<pid>/comm`
es `yuki`, de modo que un PID reutilizado se trata como obsoleto en lugar de señalizarse.
`start` lanza el compositor con `setsid`, en su propia sesión, de modo que un watcher
que mata a sus propios hijos no puede derribar la instancia con él; setsid hace exec
sin forking aquí, de modo que el PID guardado sigue siendo el PID real del compositor.

`watch` reconstruye en cada cambio bajo `crates/` (y `src/`, si existe) y
reinicia la instancia en segundo plano; la compilación y el reinicio están unidos por `&&`, de modo
que una compilación fallida deja intacta la instancia en ejecución y su salida del compilador en la
terminal. Ctrl-C detiene ambos.

### `../hardware.md`

El primer contacto con la GPU de la máquina, registrado desde la primera ejecución de la sesión
anidada: CPU, GPU, kernel, Mesa 26.2.3, OpenGL ES 3.2, EGL 1.5, `PLATFORM_WAYLAND_KHR`
y la cadena del renderer literal. Existe como el punto conocido-bueno con el que comparar
cuando el hito Direct Session pase a DRM/KMS directo y la plataforma EGL
cambie.

## Qué no se construyó deliberadamente

Lo siguiente está fuera del alcance del Bloque 0: el bucle de eventos; un struct `State`;
los manejadores de protocolo; el registro de globals; el renderizado y los shaders; el manejo de
entrada; la gestión de ventanas (`xdg-shell` y compañía); el scene graph en sí;
la carga de configuración; la integración con session/logind; y cualquier lógica de layout. La
sesión anidada (Bloques 1–3 del roadmap) comenzó el primero de estos con el backend
winit; el resto permanece para los hitos que lo requieran. No hay
`design/v1.nix` ni generador de tokens (ADR 0011).

## Espacios de coordenadas

Yuki tiene tres nociones de posición, y confundirlas es una fuente clásica
de bugs en compositores:

- **Logical** — coordenadas de escritorio, independientes de la escala del output.
- **Physical** — píxeles reales en un output, tras su factor de escala.
- **Buffer** — píxeles dentro del buffer de un cliente, antes de mapearse sobre un output.

El Bloque 0 los convierte en tipos distintos. `crates/yuki` **no** define
geometría: usa `smithay::utils` directamente, porque cada API de Smithay habla
esos tipos y un envoltorio forzaría conversiones en cada callback.
`yuki-scene` no puede depender de Smithay, por lo que posee sus propios tipos. La conversión
ocurre exactamente en un límite, donde el compositor alimenta la escena. El razonamiento
completo, incluyendo por qué se revirtió la decisión original de "poseer los tipos en el compositor",
está en el ADR 0001.

## Dependencias

Cada dependencia está listada en `[workspace.dependencies]` con una justificación
de una línea. Las dependencias de runtime añadidas en el Bloque 0, en su totalidad:

| Crate | Justificación |
| ----- | ------------- |
| `anyhow` | Contexto de error ergonómico solo en `main`; las bibliotecas usan errores tipados. |
| `thiserror` | Deriva enums de error tipados en módulos con forma de biblioteca. |
| `tracing` | Logging estructurado y spans; los spans son necesarios para mediciones posteriores. |
| `tracing-subscriber` | Instala el subscriber global que emite los eventos de `tracing`. |
| `signal-hook` | Manejo seguro de SIGTERM/SIGINT; std no tiene API de señales y el bloque prohíbe `unsafe`. |

La sesión anidada añadió `smithay` (solo las features de backend winit, renderer GLES2 y
frontend Wayland) como framework del compositor, anclado al release 0.7.0 de
crates.io (ADR 0009, ADR 0010). También añadió una dependencia solo de desarrollo,
`smithay-client-toolkit`, usada únicamente por `tools/sctk-probe`; ya estaba en el
graph a través de winit. `yuki-scene` y `yuki-design` siguen sin
dependencias. Las dependencias transitivas están bloqueadas en `Cargo.lock`. La
biblioteca estándar se prefiere siempre que sea suficiente; un crate de parsing de argumentos
y uno de globbing se consideraron y se rechazaron.

## Flujo de trabajo de desarrollo

Todo ocurre dentro del dev shell:

```sh
nix develop
./dev.sh check          # rustfmt, clippy -D warnings, pruebas
./dev.sh build          # compilación de debug
./dev.sh run            # arranca en segundo plano, escribe un archivo PID
./dev.sh watch          # reconstruye y relanza al cambiar (Ctrl-C detiene)
./dev.sh status         # 0 en ejecución, 1 archivo PID obsoleto, 3 detenido
./dev.sh log -f         # sigue el log
./dev.sh stop           # SIGTERM, luego SIGKILL tras un periodo de gracia de 5 s
./dev.sh restart        # stop, luego run
./dev.sh clean          # stop, elimina target/ y los archivos de runtime
```

El shell solicita el linker mold a través de `RUSTFLAGS`, de modo que las compilaciones dentro del
shell son idénticas de una ejecución a otra y la caché de cargo se mantiene caliente (ADR 0005). Compilar
fuera del shell está prohibido y se detecta tanto por `dev.sh` como por el build script del
crate (ADR 0007).

### Verificación

El Bloque 0 se aceptó con `./dev.sh check`: 12 pruebas en `yuki` (parsing de
argumentos y mapeo de niveles de log), 16 en `yuki-scene` (conversiones de coordenadas
y aritmética en el mismo espacio), un doctest `compile_fail` que demuestra que la aritmética de
puntos entre espacios no compila, luego rustfmt y `clippy -D warnings`. La sesión
anidada hizo crecer el mismo comando con sus propias pruebas y el harness de probes; su
documento registra lo que ahora cubre el comando. `cargo doc` compila sin warnings porque cada
elemento público está documentado; `yuki-design` es donde más se ejercita eso,
ya que es casi enteramente datos públicos.

## Preguntas abiertas

1. **Redondear rectángulos al espacio entero.** Los `to_physical` y
   `to_buffer` actuales redondean posición y tamaño al más cercano. Dos superficies adyacentes
   pueden entonces abrir una costura de un píxel. Piso-posición/techo-tamaño es el arreglo probable, pero
   no está construido: el ADR 0004 mantiene la decisión abierta hasta que algo realmente
   coloque o dañe una superficie. *(ADR 0004)*
2. **Conversión directa Physical ↔ Buffer.** Los tipos convierten solo entre logical
   y los espacios enteros. Un camino directo de physical a buffer, y las transforms
   de buffer, serán necesarios una vez que los buffers de los clientes se mapeen sobre los outputs;
   la API actual no expresa en absoluto una transform de superficie.
3. **`Physical` y `Buffer` son ambos `i32`.** Son distintos solo a nivel de
   tipo. Si el scene graph llegará a necesitar que carguen tipos numéricos
   diferentes — `u32` para las extensiones de buffer, por ejemplo — aún no se sabe.
4. **`watch --release` nunca se ejercitó.** El flag toma el mismo camino que
   `run`, y el rechazo de argumentos incorrectos está probado, pero ninguna reconstrucción real
   con `--release` se ejecutó a través del watcher, porque una reconstrucción release de todo el árbol es
   larga. Se cree correcto por construcción, no verificado por una ejecución.
5. **GTK4/libadwaita dibuja sus propias esquinas y sombra.** Resuelto por el ADR 0019:
   la negociación del cliente decide — una petición explícita del lado del cliente (que hace una
   ventana con headerbar) se honra, y tal ventana no recibe titlebar, hairline ni
   sombra del compositor. Yuki todavía no dibuja ningún chrome en absoluto, por lo que el
   renderer que consume el modo es trabajo futuro.
6. **`setsid` y el PID guardado.** `start` depende de que `setsid` haga exec sin
   forkear, de modo que `$!` es el PID real del compositor. Eso se cumple cuando el llamador
   no es ya un líder de grupo de procesos, lo cual es cierto bajo el watcher; no se
   probó desde una invocación directa de shell interactivo, donde el control de trabajos
   podría cambiar la disposición.
7. **El límite de Smithay del ADR 0001 no está probado.** Ningún código del Bloque 0 consume
   geometría, y Smithay solo entró con la sesión anidada como backend winit,
   no como consumidor de geometría. El primer módulo que use Smithay
   y convierta a los tipos de `yuki-scene` mostrará si la división de un solo límite
   es tan pequeña como afirma el ADR 0001.
8. **Empaquetado y la entrada de sesión.** `share/wayland-sessions/yuki.desktop`
   está presente y el `packages.default` del flake todavía apunta a `./`, de modo que el
   paquete instalaría una entrada de sesión cuyo `Exec` lanza el binario.
   Ni el paquete ni la entrada se han ejercitado contra una sesión real.
   Registrado, no arreglado, hasta que haya un compositor que valga la pena lanzar.

Resuelto durante el Bloque 0, conservado aquí para que la numeración y los ADRs coincidan:
la fuente de verdad del toolchain (ADR 0006), compilar fuera del dev shell
(ADR 0007) y el contrato de valores de diseño (ADR 0011, que sustituye al ADR 0008).
El subcomando `watch` en sí se resolvió una vez que existió una ventana; su única
esquina sin verificar es la pregunta 4 anterior.

## Registros de decisiones

- `../decisiones/0001-tipos-de-coordenadas.md` — los tipos de Smithay en el
  compositor, los nuestros en `yuki-scene`.
- `../decisiones/0002-objetivos-de-biblioteca-y-binario.md` — por qué `yuki` es a la vez
  biblioteca y binario.
- `../decisiones/0003-politica-de-codigo-unsafe.md` — `deny` en el compositor,
  `forbid` en los crates hoja.
- `../decisiones/0004-redondeo-de-rectangulos-de-superficie.md` — el redondeo, dejado abierto.
- `../decisiones/0005-entorno-de-desarrollo.md` — una máquina, un shell,
  un linker.
- `../decisiones/0006-toolchain-fuente-de-verdad.md` — el flake es dueño de la
  versión del toolchain.
- `../decisiones/0007-compilar-fuera-del-devshell.md` — las compilaciones externas están
  prohibidas y fallan rápido.
- `../decisiones/0008-contrato-de-design-tokens.md` — el contrato de tokens de
  Nix a Rust, sustituido por el 0011.
- `../decisiones/0009-backend-winit-y-gles2.md` — el backend y el renderer
  anidados (sesión anidada).
- `../decisiones/0010-anclaje-de-smithay.md` — Smithay anclado a crates.io
  0.7.0 (sesión anidada).
- `../decisiones/0011-valores-de-diseno-como-constantes-rust.md` — los valores de diseño son
  constantes escritas a mano en `yuki-design`.
- `../decisiones/0012-backend-junto-al-state.md` — el backend vive junto a
  `State` (sesión anidada).
- `../decisiones/0013-dispatch-consume-el-fd-duplicado.md` — el dispatch del cliente consulta
  un fd de display duplicado (sesión anidada).
- `../decisiones/0014-insets-de-decoracion.md` — los insets de decoración son una función
  del estado actual de una ventana (sesión anidada).
- `../decisiones/0015-desktop-space-y-yuki-scene.md` — el layout de ventanas vive
  en `yuki-scene`; `desktop::Space` no se adopta (Bloque 4).
- `../decisiones/0016-movimiento-y-redimensionado-interactivo.md` — los movimientos y redimensionados son
  estado propio del compositor, con verificación de serials (Bloque 4).
- `../decisiones/0017-xdg-decoration-del-lado-del-servidor.md` — el global
  `zxdg_decoration_manager_v1` se sirve y siempre selecciona el lado del servidor
  (Bloque 4). Sustituido por el 0019.
- `../decisiones/0018-transformacion-de-renderizado-de-winit.md` — el output de winit se voltea en
  tiempo de renderizado a través de la fuente de modo del damage tracker; `wl_output` permanece
  `normal` (seguimiento del Bloque 4).
- `../decisiones/0019-politica-de-decoracion.md` — el lado del servidor es el valor por defecto y
  la respuesta a un modo sin establecer, pero una petición explícita del lado del cliente se honra
  (seguimiento del Bloque 4).