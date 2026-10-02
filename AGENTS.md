# AGENTS.md — Guía para agentes de IA en Yuki

Yuki es un compositor Wayland en Rust sobre Smithay. Si eres un agente de IA (o un
humano) trabajando en este repo, **lee esto primero**. La documentación completa
vive en `docs/` (índice: `docs/README.md`).

---

## ⚠️ POLÍTICA DE DISEÑO — OBLIGATORIA (ADR 0019 + ADR 0020 + ADR 0025)

**No la ignores.** Cualquier cambio que toque decoraciones, chrome, sombras, blur,
bordes, temas o la shell debe respetar esta política. Está escrita en
`docs/decisiones/0020-politica-de-diseno-y-unificacion.md` (y en la 0019 y 0025).

### Regla de oro: chrome separado, efectos para todos

0. **La shell es nativa: `yuki-shell` (ADR 0025).** Un binario, un proceso por
   componente (`wallpaper`, `menubar`, `dock`, `lock`, ...), cada uno un cliente
   Wayland propio dibujando con `celestial-kit` (ADR 0024). No hay dependencias
   de shell externas (la anterior, Quickshell, se eliminó con el ADR 0025). La
   shell se integra en profundidad, leyendo el almacén de Yuki
   (`appearance.*`, `dock.*`, `menubar.*`, los estados en `~/.local/share/yuki/`)
   y usando los tokens de `yuki-design`.

1. **GTK/Libadwaita (apps GNOME) → NO reciben la barra de título de Yuki.**
   Conservan su headerbar y su apariencia nativa (client-side). No les pongas un
   titlebar del compositor ni sus controles.

2. **Qt y las demás apps (Electron, nuevas, terceros) → SÍ reciben el chrome
   unificado de Yuki** (barra de título + controles del compositor, server-side).

3. **Los efectos de superficie del compositor se aplican a TODAS las ventanas**,
   sin importar el modo de decoración:
   - **Inner border** estilo macOS (borde interior fino).
   - **Sombra** (también en ventanas CSD — supera la exclusión del ADR 0019).
   - **Blur / vibrancy** (el compositor difumina lo que hay detrás de superficies
     translúcidas; habilita un tema transparente).

   Es decir: si el usuario hace un tema transparente, **las apps GTK deben poder
   usar el blur del compositor**. No asumas que una ventana CSD está exenta de
   efectos; al revés: los efectos del compositor la enmarcan igual.

4. **Los efectos se implementan compositor-side**, no por cada app (mecanismo tipo
   `otto-surface-style`). El interior de las apps GTK es de la app; lo que rodea a
   la ventana es del compositor.

5. **Todos los valores de diseño viven en `yuki-design`** (ADR 0011): radios,
   sombras, blur, colores, márgenes del inner border. No pongas literales de
   diseño en `crates/yuki/src`.

6. **Preferencias de apariencia**: dark/light y accent se publicarán vía
   `org.freedesktop.appearance` (xdg-desktop-portal), como GNOME. El compositor
   debe leerlas de su propio store de preferencias, no de variables sueltas.

7. **El shell propio** (top bar, dock, menús del sistema) usa el mismo lenguaje de
   diseño y los mismos tokens que decoraciones y efectos.

---

## Referencias rápidas

- **Índice de documentación**: `docs/README.md`
- **Roadmap de bloques (fuente de verdad)**: `docs/roadmap.md`
- **Bloques implementados**: `docs/bloques/` (00-cimientos … 07-linux-dmabuf-y-explicit-sync)
- **Decisiones (ADRs)**: `docs/decisiones/` — sobre todo **0019** (decoración),
  **0020** (política de diseño), **0021** (superficie de configuración) y
  **0023** (los componentes del shell corren como procesos separados, uno por
  cliente Wayland; ver `docs/arquitectura/modelo-de-procesos.md`), **0024**
  (stack de render del toolkit) y **0025** (la shell es nativa, `yuki-shell`).
  La 0019 documenta cómo negocian GTK3/GTK4/Qt. Índice y decisiones abiertas:
  `docs/decisiones/README.md`.
- **Configuración**: `docs/configuracion/opciones.md` — **catálogo canónico de
  opciones**, con **nombres y valores idénticos a macOS** (`appearance`, `window`,
  `dock.autohide`, `mineffect`...). **Como macOS, un archivo por dominio** con
  extensión **`.domain`** (el formato propio de Yuki, como `.plist` en macOS):
  preferencias en `~/.config/yuki/` (`~/Library/Preferences`), estado/`wallpaper`
  en `~/.local/share/yuki/` (`~/Library/Application Support`). Override con
  `YUKI_CONFIG_DIR`/`YUKI_DATA_DIR`. Las opciones viven tipadas en
  `crates/yuki/src/config.rs`; si añades/cambias una, actualiza AMBOS sitios. La
  futura app de ajustes y el comando **`domain`** (sintaxis de `defaults`:
  `read`/`write`/`delete`/`find`; "domain" solo es una referencia a *Jujutsu
  Kaisen*) hablarán por D-Bus (`org.yuki.Compositor1`) — ADR 0021. **Wallpaper**:
  el dominio `wallpaper.domain` (en `.local/share/yuki/`) lleva el catálogo y la
  selección actual en `[desktops]`, como `com.apple.wallpaper`; lo dibuja el
  componente nativo **`yuki-shell wallpaper`** (ADR 0023/0024, render GLES de
  `celestial-kit` presentado por `wl_shm`) en la capa `Background`.
- **Investigación**: `docs/investigacion/` — cómo renderiza macOS y comparación
  con Wayland (contexto de diseño).
- **Arquitectura**: `docs/arquitectura/yuki-y-shell.md` (+ `yuki-y-shell.svg`)
  — **quién decide cada cosa entre Yuki (compositor) y la shell (`yuki-shell`)**:
  la shell dibuja el contenido, Yuki coloca/compone/regula. Léelo antes de tocar
  la integración de la shell.

## Estado actual

- Bloques 0–6 implementados; **Bloque 7** en curso: **7a** (linux-dmabuf v5 con
  feedback) y **7b** (explicit sync + sync implícito, con el release diferido
  por fence) hechos; **7c** (la shell presenta por EGL) pendiente
  (`docs/bloques/07-linux-dmabuf-y-explicit-sync.md`). Ya corren wezterm,
  Firefox, vkcube (Vulkan) y mpv (gpu-next) acelerados dentro de Yuki anidado.
  El ADR 0026 fija dmabuf + explicit sync.
- Shell: componentes nativos `yuki-shell wallpaper`, `menubar`, `dock` y `lock`
  (`docs/arquitectura/`). El compositor publica la app activa en
  `~/.local/share/yuki/window.domain`, sirve `ext-foreign-toplevel-list-v1` +
  `zwlr_foreign_toplevel_management_v1` (dock), `ext-session-lock-v1` (lock,
  con la garantía de no desbloquear si el cliente muere) y `zwp_linux_dmabuf_v1`
  v5 (7a).
- Backend winit anidado (GLES2); **Bloque 8 = Direct Session** (DRM/KMS)
  planificado.
- `TODO(scale)` en `crates/yuki/src/state.rs:133` (HiDPI).

## Cómo compilar y probar

```sh
nix develop            # el dev shell es obligatorio (marca YUKI_DEV_SHELL)
./dev.sh check         # fmt + clippy + tests
./dev.sh run           # lanza el compositor anidado
./dev.sh smoke         # prueba de mapeo de toplevels
```

No compiles fuera del dev shell: `build.rs` y `dev.sh` lo rechazan (ADR 0007).