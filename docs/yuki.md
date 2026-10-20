# yuki

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
| `yuki` | el compositor (display server, decoración, input, render) | `docs/bloques/` |
| `yuki-design` | los design tokens (radios, colores, sombras — ADR 0011) | — |
| `yuki-scene` | layout de ventanas y popups: geometría, stacking, hit-testing (ADR 0015) | `crates/yuki-scene/src/scene.rs` |
| `grade-graphics` | los shaders GLSL compartidos (≈ Core Graphics/Core Text) | `docs/glossary.md` |
| `celestial-kit` | el toolkit de la shell y las apps (≈ AppKit): render GLES, texto | `docs/glossary.md`, `docs/arquitectura/wallpaper.md` |
| `yuki-shell` | un binario, un proceso por componente: `yuki-shell wallpaper`, `menubar`, ... | `docs/arquitectura/modelo-de-procesos.md` |
| `tools/sctk-probe` | clientes de prueba (dev only) | `tools/sctk-probe/README.md` |

Los nombres siguen el reparto de macOS (glosario): CelestialKit ≈ AppKit, Grade
Graphics ≈ Core Graphics/Core Text, Grade Animation ≈ Core Animation, Infinity ≈
SkyLight.

## La shell

La shell corre como **procesos nativos separados** (ADR 0023/0025): un binario
`yuki-shell`, un proceso por componente, cada uno un cliente Wayland propio.
No hay dependencias de shell externas; el toolkit es `celestial-kit` (ADR 0024).

Estado de los componentes:

- **Wallpaper** (`yuki-shell wallpaper`) — hecho. Dibuja la capa `Background` con
  el render GLES de `celestial-kit`, presentado por **EGL**/dmabuf sobre un
  `wl_egl_window` (Bloque 7c, ADR 0027; sin copias por CPU), y vigila
  `wallpaper.domain` (cambio en vivo). Ver
  [`arquitectura/wallpaper.md`](arquitectura/wallpaper.md).
- **Menubar** (`yuki-shell menubar`) — hecho. Capa `Top` con zona exclusiva:
  app activa (el compositor la publica en `window.domain`) + reloj. Ver
  [`arquitectura/menubar.md`](arquitectura/menubar.md).
- **Dock** (`yuki-shell dock`) — hecho. Capa `Top` en el borde inferior con zona
  exclusiva: lista las apps (`ext-foreign-toplevel-list-v1`) y las activa al
  clic (`zwlr_foreign_toplevel_management_v1`). Ver
  [`arquitectura/dock.md`](arquitectura/dock.md).
- **Lock** (`yuki-shell lock`) — hecho. `ext-session-lock-v1`: bloquea la
  sesión, autentica con PAM (`yuki-lock`, fallback `login`) y garantiza que
  **no se desbloquea si el cliente muere**: lo relanza con backoff y límite de
  intentos mientras `locked` sigue siendo `true`. El compositor lo lanza solo tras
  `lock.idle_timeout_seconds` de inactividad, respetando
  `zwp_idle_inhibit_manager_v1`. Ver
  [`arquitectura/lock.md`](arquitectura/lock.md).
- Supervisión: unidades systemd de usuario en el flake (`yuki-shell@.service`,
  `yuki-session.target`); `scripts/test.sh` las emula en desarrollo.

## Desarrollo

Compilar y probar se hacen dentro del dev shell, en la sesión gráfica de la máquina
(`run`, `stop`, `restart`, `status` y `log` también funcionan desde un shell normal —
ADR 0007):

```sh
nix develop
./dev.sh run       # arranca el compositor anidado en una ventana de winit
./dev.sh smoke     # compila y ejecuta la prueba de mapeo de toplevels
./dev.sh check     # formato, clippy y pruebas
./dev.sh --help    # todos los comandos
```