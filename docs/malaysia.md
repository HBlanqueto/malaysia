# MalaysiaDreams

Un compositor Wayland construido sobre Smithay, desarrollado bloque a bloque. Cada
bloque es un incremento que funciona: la casilla es **puede correr**, lo que significa
que el comando que la sigue pone en marcha el comportamiento de ese bloque y la
comprobación lo demuestra.

## Bloques

La lista canónica de bloques y la prueba de aceptación de cada uno viven en
[`roadmap.md`](roadmap.md). La casilla es **puede correr**: el comando que la sigue
pone en marcha el comportamiento de ese bloque.

- [x] **Bloque 0 — Andamiaje.** `./dev.sh run`
- [x] **Bloque 1 — Conseguir que un cliente se conecte.** `./dev.sh run`;
      `wayland-info` lista todos los globals.
- [x] **Bloque 2 — Una ventana en pantalla.** `./dev.sh run`, comprobado por
      `./dev.sh smoke`. Ver
      [`bloques/02-ventana-en-pantalla.md`](bloques/02-ventana-en-pantalla.md).
- [x] **Bloque 3 — Input.** `./dev.sh run`, comprobado por `./dev.sh smoke`; la
      escritura y el cursor renderizado se comprueban a mano. Ver
      [`bloques/03-entrada.md`](bloques/03-entrada.md).
- [x] **Bloque 4 — Ventanas manipulables.** `./dev.sh block4` abre y cierra 50
      ventanas y comprueba que la escena se vacíe; la sensación del arrastre y los
      visuales del redimensionado se comprueban a mano.
- [x] **Bloque 5 — Popups.** Posicionados con ajuste de restricciones, configurados
      antes de mapear y cerrados por clic fuera / Esc / muerte del cliente;
      verificado con `popup_probe`. Ver
      [`bloques/05-popups.md`](bloques/05-popups.md).
- [x] **Bloque 6 — Portapapeles.** Copiar y pegar entre clientes, y selección
      primaria; verificado con `wl-copy`/`wl-paste`. Ver
      [`bloques/06-portapapeles.md`](bloques/06-portapapeles.md).

Los documentos por bloque viven en `docs/bloques/`, las decisiones en
`docs/decisiones/`.

## Estructura

El workspace de Cargo tiene los crates del compositor y, desde el modelo de
procesos de la shell (ADR 0023), los de la shell nativa:

| Crate | Rol | Ver |
|---|---|---|
| `malaysia-compositor` | el compositor (display server, decoración, input, render) | `docs/bloques/` |
| `vow-design` | los design tokens (radios, colores, sombras — ADR 0011) | — |
| `malaysia-scene` | layout de ventanas y popups: geometría, stacking, hit-testing (ADR 0015) | `crates/malaysia-scene/src/scene.rs` |
| `grade-graphics` | los shaders GLSL compartidos (≈ Core Graphics/Core Text) | `docs/glossary.md` |
| `barrier-kit` | el toolkit de la shell y las apps (≈ AppKit): render GLES, texto | `docs/glossary.md`, `docs/arquitectura/wallpaper.md` |
| `malaysia-shell` | un binario, un proceso por componente: `menubar`, `dock`, `lock`, ... | `docs/arquitectura/modelo-de-procesos.md` |
| `malaysia-wallpaper` | fondo de escritorio en Vulkan/wgpu (ADR 0049) | `docs/arquitectura/wallpaper.md` |
| `tools/sctk-probe` | clientes de prueba (dev only) | `tools/sctk-probe/README.md` |

Los nombres siguen el reparto de macOS (glosario): BarrierKit ≈ AppKit, Grade
Graphics ≈ Core Graphics/Core Text, Grade Animation ≈ Core Animation, Infinity ≈
SkyLight.

## La shell

La shell corre como **procesos nativos separados** (ADR 0023/0025): un binario
`malaysia-shell`, un proceso por componente, cada uno un cliente Wayland propio.
No hay dependencias de shell externas; el toolkit es `barrier-kit`, construido
sobre los crates de núcleo de Freya con Skia en los procesos cliente (ADR 0034).

Estado de los componentes:

- **Wallpaper** (`malaysia-wallpaper`) — hecho. Dibuja la capa `Background` con
  un renderizador **Vulkan**/wgpu propio, presentado por dmabuf (Bloque 16a,
  ADR 0049; sin copias por CPU), y vigila `wallpaper.domain` (cambio en vivo).
  Ver [`arquitectura/wallpaper.md`](arquitectura/wallpaper.md).
- **Menubar** (`malaysia-shell menubar`) — hecho. Capa `Top` con zona exclusiva:
  app activa (el compositor la publica en `window.domain`) + reloj. Ver
  [`arquitectura/menubar.md`](arquitectura/menubar.md).
- **Dock** (`malaysia-shell dock`) — hecho. Capa `Top` en el borde inferior con zona
  exclusiva: lista las apps (`ext-foreign-toplevel-list-v1`) y las activa al
  clic (`zwlr_foreign_toplevel_management_v1`). Ver
  [`arquitectura/dock.md`](arquitectura/dock.md).
- **Lock** (`malaysia-shell lock`) — hecho. `ext-session-lock-v1`: bloquea la
  sesión, autentica con PAM (`malaysia-lock`, fallback `login`) y garantiza que
  **no se desbloquea si el cliente muere**: lo relanza con backoff y límite de
  intentos mientras `locked` sigue siendo `true`. El compositor lo lanza solo tras
  `lock.idle_timeout_seconds` de inactividad, respetando
  `zwp_idle_inhibit_manager_v1`. Ver
  [`arquitectura/lock.md`](arquitectura/lock.md).
- Supervisión: unidades systemd de usuario en el flake (`malaysia-shell@.service`,
  `malaysia-session.target`); `scripts/test.sh` las emula en desarrollo.

## Desarrollo

Compilar y probar se hacen dentro del dev shell, en la sesión gráfica de la máquina
(`run`, `stop`, `restart`, `status` y `log` también funcionan desde un shell normal —
ADR 0007):

```sh
nix develop
./dev.sh run       # arranca el compositor anidado en una ventana de winit
./dev.sh demo      # sesión anidada con wallpaper, menubar, dock y ventanas
./dev.sh demo --dark   # ídem, con appearance.color_scheme forzado a oscuro
./dev.sh demo --light  # ídem, en claro
./dev.sh lab       # sesión anidada con apps reales para pruebas manuales
./dev.sh lab --light   # ídem, con apariencia clara
./dev.sh smoke     # compila y ejecuta la prueba de mapeo de toplevels
./dev.sh check     # formato, clippy y pruebas
./dev.sh --help    # todos los comandos
```

`demo --dark|--light` copia `~/.config/malaysia` a un directorio temporal, escribe
allí `color_scheme` y lo borra al salir: nunca toca la configuración real.
Los ficheros de desarrollo (pid, log y logs de los clientes del lab) viven en
`$XDG_RUNTIME_DIR/malaysia-dev-<hash>`, con un hash corto de la ruta del checkout
(`git rev-parse --show-toplevel`): dos worktrees pueden correr `run`, `demo` o
`lab` a la vez sin compartir ni el pid ni el log.
`scripts/demo.sh` hace `git pull --ff-only`, compila y lanza el demo (pasa los
mismos argumentos) desde la raíz del repo; fuera del dev shell falla con un
mensaje.

`lab` es igual que `demo` pero también lanza, si están instaladas, `nautilus`,
`zenity` (o `gnome-calculator`), `kate`/`kwrite`, `telegram-desktop` y `firefox`,
cada una con su log en `$XDG_RUNTIME_DIR/demo-<nombre>.log`. Al arrancar imprime
una checklist de pruebas manuales; la sesión se cierra con Ctrl-C o cerrando la
ventana anfitriona. `scripts/lab.sh` actualiza, compila y lanza `lab` de la
misma forma que `scripts/demo.sh`. Las apps de prueba son opcionales en el
devShell (`labApps` en `flake.nix`). Kate no está en `labApps` porque su
closure KDE dispara la entrada en frío; para probarlo:

```sh
nix shell nixpkgs#kdePackages.kate --command ./dev.sh lab --light
```
