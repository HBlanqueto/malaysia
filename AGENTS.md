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
  `celestial-kit` presentado por `EGL`/dmabuf (Bloque 7c, ADR 0027) en la capa `Background`.
- **Investigación**: `docs/investigacion/` — cómo renderiza macOS y comparación
  con Wayland (contexto de diseño).
- **Arquitectura**: `docs/arquitectura/yuki-y-shell.md` (+ `yuki-y-shell.svg`)
  — **quién decide cada cosa entre Yuki (compositor) y la shell (`yuki-shell`)**:
  la shell dibuja el contenido, Yuki coloca/compone/regula. Léelo antes de tocar
  la integración de la shell.

## Estado actual

- Bloques 0–6 implementados; **Bloque 7 hecho**: **7a** (linux-dmabuf v5 con
  feedback), **7b** (explicit sync + sync implícito, release diferido por
  fence) y **7c** (la shell presenta por EGL, un solo camino, sin `wl_shm`)
  (`docs/bloques/07-linux-dmabuf-y-explicit-sync.md`). Ya corren wezterm,
  Firefox, vkcube (Vulkan) y mpv (gpu-next) acelerados dentro de Yuki anidado.
  Los ADR 0026 (dmabuf + explicit sync) y 0027 (shell por EGL) fijan el diseño.
  **7.1 hecho**: el texto de la shell arreglado (`shape_until_scroll` +
  formato Alpha de swash), daño real medido, y **captura de depuración por
  `SIGUSR1`** (volcado PNG, `scripts/capture-validate.sh`; smithay no trae
  protocolo de captura — `ext-image-copy-capture` llegó en el Bloque 11). La captura
  descubrió y corrigió el orden de capas (front-to-back) y el primer render del
  dock.
  **7.2 hecho**: la captura expuso el **bug de la matriz UV** del present EGL
  (`uniform_matrix_3_f32_slice` espera column-major y se pasaba row-major →
  toda la superficie muestreaba la fila 0): el dock no mostraba sus tiles, el
  lock no mostraba su texto y el menubar repetía una fila. Corregido a
  column-major sin flip; verificado por captura (lock con texto en la posición
  del frame, dock con 2 tiles). Los toplevels existentes SÍ se anuncian al
  bindear (`ext-foreign-toplevel-list` y `zwlr`). Probes nuevos `popup_live` y
  `decor_live` (sctk-probe). Nota: la captura re-renderiza offscreen, así que
  no detecta bugs de daño parcial en pantalla.
- Shell: componentes nativos `yuki-shell wallpaper`, `menubar`, `dock` y `lock`
  (`docs/arquitectura/`). El compositor publica la app activa en
  `~/.local/share/yuki/window.domain`, sirve `ext-foreign-toplevel-list-v1` +
  `zwlr_foreign_toplevel_management_v1` (dock), `ext-session-lock-v1` (lock,
  con la garantía de no desbloquear si el cliente muere) y `zwp_linux_dmabuf_v1`
  v5 (7a).
- Backend winit anidado (GLES2), backend DRM/KMS **8a** implementado y
  aceptado en el panel real (2026-10-21, `wezterm`, touchpad y VT); **8b/8c**
  planificados. `dev.sh drm-vkms` sigue pasando.
- **Bloque 10 implementado y aceptado (2026-10-21):** PAM en el proceso
  del lock, UI de contraseña, auto-lock por `ext-idle-notify-v1` con
  `zwp_idle_inhibit_manager_v1`, recuperación del lock con backoff y límite
  de 5 intentos, y `Ctrl+Alt+L` para bloquear/relanzar sin desbloquear.
  Checklist en `docs/bloques/10-lock.md`.
- **Bloque 11 parcial:** `wp_presentation`, `xdg-activation`, `cursor-shape`,
  el shake-to-locate, `pointer-constraints` + `relative-pointer`, la captura
  (salida y ventana, `ext-image-copy-capture-v1`) y el atajo PrintScreen
  (guarda en el Escritorio XDG) hechos. Ver
  `docs/bloques/11-protocolos-de-uso-diario.md`.
- **Bloque 12 planificado:** portales (`xdg-desktop-portal-yuki`) y servicios
  de escritorio (notificaciones, polkit, keyring, AT-SPI); depende de la
  captura del Bloque 11. El plan corto vive en `docs/por-hacer.md`.
- **Bloque 9 parcial:** `State::scale`, `display.scale` persistente y el
  redondeo floor/ceil (ADR 0004) hechos; faltan la aceptación en panel real y
  la escala por salida de 8b. Ver `docs/bloques/09-escala-fraccional.md`.

## Cómo compilar y probar

```sh
nix develop            # el dev shell es obligatorio (marca YUKI_DEV_SHELL)
./dev.sh check         # fmt + clippy + tests
./dev.sh run           # lanza el compositor anidado
./dev.sh smoke         # prueba de mapeo de toplevels
# probes de protocolo: move, pointers, screencopy, presentation, activation
```

No compiles fuera del dev shell: `build.rs` y `dev.sh` lo rechazan (ADR 0007).

En NixOS, el flake expone `nixosModules.default` (`programs.yuki.enable = true`):
instala el paquete, registra la sesión, crea `/etc/pam.d/yuki-lock` y arranca
`yuki-session.target` con los componentes de la shell.

## Regla de entorno (permanente)

Nunca borres ni modifiques nada fuera del repo y de `$XDG_RUNTIME_DIR/yuki`:
ni sockets de Wayland del host, ni ficheros de la sesión, ni servicios. En
concreto:

- **Sockets**: solo los de Yuki (`wayland-N` servidos por el binario `yuki`),
  y solo por nombre exacto. Nunca borres `wayland-0` del host ni cualquier
  otro socket que no haya creado Yuki.
- **Servicios del host**: nunca reinicies mutter, gnome-shell, la sesión ni
  ningún proceso ajeno al repo. Si algo del host falla o falta (p. ej. un
  socket de Wayland desaparece), **detente y repórtalo**; no intentes
  recuperarlo tú.
- **Ficheros**: solo se escriben capturas y volcados dentro de
  `$XDG_RUNTIME_DIR/yuki` y de `/tmp` para diagnósticos del propio repo.