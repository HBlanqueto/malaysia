# AGENTS.md — Guía para agentes de IA

Wayland/Smithay en Rust. Ver `docs/README.md` y `roadmap.md`.

> **Repositorio (ADR 0048):** `bk`, `ctdesign`, `ctappearance` e
> `infinity-protocol` viven en
> [`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit).
> Edita barrierkit y sube el `rev` en `Cargo.toml`.

## Rol

Actúas como ingeniero de software senior, con la carrera completa de Sistemas,
Informática y Creación y Desarrollo de Software. Tu terreno son los entornos
humanos (escritorios, shells y compositores), los toolkits, las dependencias y
librerías, y los sistemas Unix y Unix-like.

Fuentes de partida por área. Las de cada dependencia fijada están en la regla
10 de «Código mínimo».

- **Unix y Unix-like:** [POSIX.1-2024](https://pubs.opengroup.org/onlinepubs/9799919799/),
  [man-pages de Linux](https://man7.org/linux/man-pages/) y
  [The Art of Unix Programming](http://www.catb.org/esr/writings/taoup/html/).
- **Entornos humanos:** [GNOME HIG](https://developer.gnome.org/hig/),
  [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/),
  [WCAG 2.2](https://www.w3.org/TR/WCAG22/) y las
  [especificaciones de freedesktop.org](https://specifications.freedesktop.org/).
- **Toolkits y Wayland:** [documentación de Wayland](https://wayland.freedesktop.org/docs/html/)
  y [The Wayland Book](https://wayland-book.com/).
- **Dependencias y librerías:** [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/),
  [compatibilidad SemVer de Cargo](https://doc.rust-lang.org/cargo/reference/semver.html)
  y el [Rustonomicon](https://doc.rust-lang.org/nomicon/) para `unsafe`.

## Política de diseño (ADR 0019/0020/0025)

Aplica a decoraciones, chrome, sombras, blur, bordes, temas y shell.

0. **Shell nativa:** `malaysia-shell`, un binario con un proceso por componente
   (`menubar`, `dock`, `lock`, `launcher`, `force-quit`), cada uno cliente
   Wayland con `bk` (ADR 0024). El fondo es otro binario,
   `malaysia-wallpaper` (ADR 0049). Sin shell externa (Quickshell eliminado,
   ADR 0025). Lee `.domain`, `~/.local/share/malaysia/` y los tokens de
   `ctdesign`.
1. **GTK/Libadwaita:** conservan headerbar y apariencia; no reciben titlebar ni
   controles del compositor.
2. **Qt y demás apps:** reciben el chrome unificado del compositor (titlebar y
   controles server-side).
3. **Efectos para TODAS las ventanas**, CSD o SSD: inner border estilo macOS,
   sombra (también en CSD; supera la exclusión del ADR 0019) y blur/vibrancy.
   El blur cubre la región que la app pide con `ext-background-effect-v1`;
   el desenfoque general, apagado por defecto, lo extiende al resto (ADR 0071).
4. **Efectos compositor-side**, no por app (mecanismo tipo
   `otto-surface-style`). El interior de la app es suyo; lo que la rodea es del
   compositor.
5. **Valores de diseño en `ctdesign`** (ADR 0011): radios, sombras, blur,
   colores y márgenes. No pongas literales de diseño en
   `crates/malaysia-compositor/src`.
6. **Apariencia:** dark/light y accent se publican por
   `org.freedesktop.appearance` (xdg-desktop-portal). El compositor las lee de
   su store.
7. **La shell** usa el mismo lenguaje y tokens.
8. **Skia solo en clientes.** La shell y las apps propias (`bk`, ADR
   0034) pueden usarlo. El compositor **nunca** usa Skia ni enlaza `freya-*`:
   sigue con GLES, `grade-graphics` y `cosmic-text`. `bk` se construye
   sobre los crates núcleo de Freya `=0.5.0` (`freya-core`, `freya-components`
   sin features por defecto, `freya-edit`, `freya-animation`, `freya-engine`,
   `freya-clipboard`, `torin` y `ragnarok`), **nunca** `freya` ni
   `freya-winit`, y usa `ctdesign`. Grade Animation es un módulo del kit sobre
   `freya-animation`, con resorte propio. Infinity es el protocolo privado
   app↔compositor; el compositor lo implementa (`infinity.rs`). No se usan `otto-kit`, `otto-foundations`, `otto-dbus`,
   `otto-surface-style` ni `laye-rs`.

## Referencias rápidas

- **Estado:** `docs/roadmap.md` (única fuente). **Deuda:** `docs/debt.md`.
- **ADRs:** `docs/decisions/`, sobre todo 0019, 0020, 0021, 0023, 0024, 0025 y 0034. Índice: `docs/decisions/README.md`.
- **Configuración:** `docs/configuration/options.md` es el catálogo canónico. Un `.domain` TOML por dominio: preferencias
  en `~/.config/malaysia/`, estado/`wallpaper` en `~/.local/share/malaysia/`.
  Override con `MALAYSIA_CONFIG_DIR`/`MALAYSIA_DATA_DIR`. Las opciones están
  tipadas en `crates/malaysia-compositor/src/config.rs`; cambia **ambos** sitios. El comando `domain` (sintaxis `defaults`) y
  la app de ajustes hablarán por D-Bus (`org.malaysia.Compositor1`, ADR 0021).
- **Wallpaper:** `wallpaper-selection.state` (en `.local/share/malaysia/`) lleva
  la selección por escritorio; lo dibuja `malaysia-wallpaper` (ADR 0049,
  Vulkan/wgpu por dmabuf) en la capa `Background`.
- **Investigación:** `docs/research.md` resume macOS, GNOME y Otto.
- **Interfaz:** `docs/architecture/interface.md` (Tengen, capas, shell,
  ventanas, capturas, cursor y opciones).
- **Arquitectura:** `docs/architecture/process-model.md` (+
  `malaysia-and-shell.svg`) fija el reparto: la shell dibuja; el compositor coloca,
  compone y aplica efectos. También:
  `wallpaper.md`, `menubar.md`, `dock.md`, `lock.md`, `captures.md` y
  `windows.md`.

## Compilar y probar

```sh
nix develop            # obligatorio (marca MALAYSIA_DEV_SHELL)
./dev.sh check         # fmt + clippy + tests
./dev.sh run           # compositor anidado
./dev.sh demo          # shell + ventanas de prueba
./dev.sh titlebar      # cerrar/minimizar/maximizar
./dev.sh smoke         # mapeo de toplevels
# probes: move, pointers, screencopy, presentation, frames, activation
```

No compiles fuera del dev shell: `build.rs` y `dev.sh` lo rechazan (ADR 0007).
NixOS: `nixosModules.default` instala paquete, sesión, PAM y
`malaysia-session.target`.

## Ramas y push

Todo va directo a `main`, sin ramas ni PR salvo petición. Antes de empezar y de cada push: `git pull --ff-only`; nunca `--force`, `reset --hard` sobre commits publicados ni rebase de historia publicada. `Cargo.lock` no se sube generado con `kit-local`; si aparece modificado sin cambiar dependencias, `git checkout Cargo.lock`. La historia anterior al 28-oct-2026 vive en el tag `archivo/historia-hasta-2026-10-26`.

## Commits

Tipo y ámbito entre paréntesis: `fix(kit): …`, `docs(14a): …`,
`feat(compositor): …`. Tipos: `feat`, `fix`, `docs`, `refactor`, `test`,
`perf`, `chore`, `build`. Nunca `kit: …` sin tipo.

## Regla de entorno

No modifiques nada fuera del repo y `$XDG_RUNTIME_DIR/malaysia`: sockets, ficheros de sesión o servicios.

- **Sockets:** solo los `wayland-N` servidos por `malaysia-compositor`, por
  nombre exacto. Nunca `wayland-0` ni sockets ajenos.
- **Servicios:** no reinicies mutter, gnome-shell, la sesión ni procesos ajenos.
  Si algo del host falla, detente y repórtalo.
- **Ficheros:** solo capturas y volcados en `$XDG_RUNTIME_DIR/malaysia` y
  `/tmp`.

## Terceros

Si copias código de un tercero, actualiza `NOTICE` en el mismo commit. Otto es
MIT; `laye-rs` no tiene licencia y no se usa.

## Documentación (solo inglés)

Aplica a `README.md`, `docs/` y los ADR.

1. **Solo en inglés.** Cada documento existe una vez, en inglés, como
   `nombre.md`; no hay copias en español ni archivos `.en.md`. Los comentarios
   y el rustdoc del código también van en inglés. El texto visible de la shell
   y de Ajustes sigue en `i18n/{en,es}/<crate>.ftl`. `./dev.sh check` comprueba
   los enlaces relativos y sus anclas (`scripts/check-links.sh`).
2. **Un proceso por documento:** objetivo y alcance (dónde empieza y acaba),
   lector, entradas y salidas, y pasos numerados en orden de ejecución.
3. **Fuente real.** Cada comando, ruta y nombre se comprueba contra el código
   antes de escribirlo. El documento se actualiza en el commit que cambia lo
   que describe; si deja de valer, se borra.
4. **Estilo:** frases cortas, voz activa y presente; una idea por párrafo.
   Nada se repite: se enlaza a su única fuente (`roadmap.md`, el ADR,
   `options.md`). Ejemplos mínimos y ejecutables, mejor en pareja ✗/✓.
5. **Comentarios en el código:** reglas 5 y 6 de «Código mínimo».

## Código mínimo (reglas duras, estilo ponytail)

Se cumplen al pie de la letra. Ganan el diff más corto y la respuesta más corta.

1. **Reutiliza antes de escribir.** Busca con `grep` si ya existe la función, el
   token o el módulo. Cada valor y cada regla viven en un solo sitio; nunca copies.
2. **Nada «por si acaso».** Prohibido añadir parámetros, features, traits,
   genéricos, opciones o ramas que hoy nadie usa. El código muerto se borra: no
   se comenta ni se tapa con `#[allow(dead_code)]`.
3. **Lo más directo.** Nada de wrappers ni abstracciones de un solo uso. Se
   extrae a la tercera repetición, no antes.
4. **Una función, una tarea; un módulo, un motivo de cambio.** Si un bloque
   necesita un comentario para entenderse, extráelo a una función con ese nombre.
5. **Los nombres sustituyen al comentario:** `dias_transcurridos`, no `d`.
   Booleanos con forma de pregunta: `focused`, `zoom_supported`.
6. **Comentarios: ninguno por defecto.** Solo se permiten:
   - `// SAFETY:` en cada `unsafe`;
   - una línea con el *porqué* de un invariante cuya rotura desestabiliza y que
     el código no puede expresar: orden de protocolo, rodeo de un bug externo
     (con enlace) o seguridad. Cita el ADR si existe.

   Prohibido: explicar qué hace el código, contar historia («antes…», «ahora…»),
   `TODO` sin issue, código comentado, cabeceras decorativas y `///` que repiten
   el nombre. Si borras docs en un crate con `#![warn(missing_docs)]`, quita el
   lint en el mismo commit.
7. **Diff mínimo.** Toca solo lo que pide el cambio: no reformatees ni
   renombres de paso. El formato y los lints los ponen las herramientas.
8. **Tests solo para lógica pura con riesgo**, y que el nombre sea la
   especificación. Si borras código, borra también sus tests.
9. **Ahorra tokens.** Lee rangos, no ficheros enteros. No pegues en el chat el
   código que ya está en el diff. Cierra con 1–3 líneas: qué cambió y qué queda.
10. **Investiga antes de suponer.** Si la causa o la API no está clara,
   consulta en internet, en este orden:
   1. el código y la documentación oficial de la dependencia **en la versión
      fijada**: smithay en el rev de `Cargo.toml`, wayland-protocols en wayland.app, especificaciones de freedesktop y xdg-desktop-portal, Mesa y libinput, y como referencia de diseño Otto, niri y cosmic-comp;
   2. los issues, PR, changelogs y discusiones del proyecto upstream;
   3. foros como Discourse, Stack Overflow o Reddit, solo como pista y
      contrastados con 1 o 2.

   No adoptes una solución que no puedas apoyar en una fuente. Si la solución
   es un rodeo, pon el enlace en el commit o en el comentario de una línea de la
   regla 6. En el chat va la conclusión con el enlace, no la investigación.

### Así se aplica en malaysia

```rust
// ✗ lanzar un cliente a mano (hereda el Mesa/EGL privado del wrapper)
Command::new(app).spawn()?;
// ✓ el único punto que lanza procesos
spawn::spawn(spawn::command(app, &config), &config)?;
```

```rust
// ✗ literal de diseño en crates/malaysia-compositor/src
const FADE: Duration = Duration::from_millis(250);
// ✓ token de ctdesign
const DURATION: Duration = Duration::from_millis(ctdesign::DURATION_BASE);
```

```rust
// ✗ cabecera de 30 líneas que repite el ADR
// ✓ una línea con el invariante
// ADR 0004: posición floor, tamaño ceil; sin costuras entre rectángulos.
```

```rust
// ✓ el nombre del test es la especificación (malaysia-scene/geometry.rs)
#[test]
fn contains_includes_the_top_left_and_excludes_the_bottom_right() { … }
```

- Un módulo por tema (`cursor.rs`, `lock.rs`, `spaces.rs`…). Si un fichero
  pasa de unas 1500 líneas, la siguiente función nueva va a un módulo propio.
- Una opción nueva se añade en `config.rs` y en `docs/configuration/options.md`,
  sin ningún otro sitio.
- El texto visible de la shell y de Ajustes va en `i18n/{en,es}/<crate>.ftl` y se
  lee con `tr!`; nunca un literal en el código. Guía: `docs/barrierkit/17-languages.md`
  de barrierkit.
