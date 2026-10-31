# Roadmap

El compositor se construye por bloques. **Este archivo es la única fuente de
verdad del estado**: qué está hecho, qué falta y cómo se prueba. El porqué de
cada decisión vive en los ADR; los hallazgos abiertos, en [`deuda.md`](deuda.md).

## Ahora (2026-10-31)

- **Menú global (Bloque 14).** Faltan 14b-2 (diálogos Suspender/Reiniciar/
  Apagar/Cerrar sesión), 14e (apps propias exportan dbusmenu) y 14f-2 (cliente
  `org.gtk.Menus`). 14c necesita navegación por teclado y scroll. Prueba:
  `./dev.sh demo` y los menús de una app real.
- **Modo foco.** `window.green_button = "focus"` solo registra la petición
  (`GreenButtonAction::Focus`); falta atenuar el resto y mantener la ventana al
  frente.
- **Captura de ventana con marco.** La captura de ventana devuelve solo el
  contenido del cliente; falta incluir la titlebar Tengen y los efectos.
- **Portal 12a.** `Settings` y el puente GTK/libadwaita (ADR 0050) están
  hechos. Faltan `Screenshot`, `ScreenCast` con
  PipeWire y los diálogos de permiso. Prueba: `./dev.sh screencopy` y
  `./dev.sh screencopy-window`.
- **NixOS.** El módulo ya instala paquete, sesión, PAM, `hardware.graphics`,
  units y `xdg.portal` con GTK de fallback. Faltan polkit, keyring/AT-SPI y una VM.

## Bloques

### 0 — Andamiaje

Workspace de Cargo, Smithay anclado por revisión (`023b78b1`), `tracing` y tipos
de coordenadas. **Hecho.** Divergencia: los tipos viven en `malaysia-scene`, no
en el compositor (ADR 0001), y `vow-design` no está vacío. Prueba: `./dev.sh run`.

### 1 — Un cliente se conecta

`CompositorState`, `ShmState`, `SeatState`, `XdgShellState`, socket Wayland y
globals (`wl_compositor`, `wl_shm`, `wl_output`, `wl_seat`, `xdg_wm_base`).
**Hecho.** Prueba: `./dev.sh run` y `wayland-info` lista todos los globals.

### 2 — Una ventana en pantalla

Backend winit, commit de doble buffer, configure inicial, importación `wl_shm` y
render. **Hecho.** Divergencia: el mapeo ocurre en el primer commit con buffer,
no en `ack_configure`; las ventanas se rastrean en `window.rs` sin
`desktop::Space` (ADR 0015). Prueba: `./dev.sh smoke`.

### 3 — Entrada

Foco de teclado al mapear, keymap/repeat, hit-testing de puntero, `set_cursor` y
serials centrales. **Hecho.** La escritura, el hover y los píxeles del cursor se
verifican a mano. Prueba: `./dev.sh smoke`.

### 4 — Ventanas manipulables

Mover, redimensionar por bordes, cerrar, apilar y click-to-focus. **Hecho.**
Pruebas: `./dev.sh block4` (50 ventanas), `./dev.sh move` y
`./dev.sh decoration`; la sensación de arrastre se prueba a mano.

### 5 — Popups

Árbol de geometría en `malaysia-scene`, superficies en el compositor,
restricciones contra el output, configure antes de mapear y cierre por clic
fuera/Esc/muerte del cliente. **Hecho.** Los popups hijos de layer-shell
funcionan desde `ab8e0b5`; el reposicionado reactivo sigue pendiente. Prueba:
`cargo run -p sctk-probe --bin popup_probe`.

### 6 — Portapapeles

`wl_data_device_manager` y selección primaria, con foco ligado al teclado.
**Hecho.** La selección es por seat; drag & drop no se tocó. Prueba: `wl-copy`
en un cliente y `wl-paste` en otro.

### 7 — linux-dmabuf y explicit sync

`zwp_linux_dmabuf_v1` v5 con feedback, sync explícito e implícito, release
diferido por fence y shell por EGL (`wl_egl_window`, sin `wl_shm`). **Hecho.**
Sin fallback a v3: sin render node el arranque falla. Prueba: `vkcube`, `mpv`
(gpu-next), wezterm y Firefox acelerados en anidado; `./dev.sh check`.

### 8 — Direct Session (DRM/KMS)

**8a hecha y aceptada** en el panel real el 2026-10-21 (DRM/KMS atómico,
libseat, libinput, GBM/EGL/GLES, un plano primario; cambio de VT y lock).
Prueba: `./dev.sh drm-vkms`. **8b** (hotplug, varios outputs, escala por
monitor) y **8c** (overlay/cursor planes, direct scanout) pendientes.

### 9 — Escala fraccional

`State::scale`, `display.scale` persistente, `wp_fractional_scale_v1`, render,
damage, cursor y viewporter de la shell. **Hecho y aceptado** en panel real a
1.0/1.25/1.5 el 2026-10-21. Pendiente: escala por salida con 8b. Prueba:
`./dev.sh demo` con `scale = 1.25` en `display.domain`.

### 10 — Lock con autenticación

PAM en el proceso del lock, campo enmascarado, auto-lock por
`ext-idle-notify-v1`, recuperación con backoff y `Ctrl+Alt+L` (bloquea o
relanza, nunca desbloquea). **Hecho y aceptado** en panel real el 2026-10-21.
Pendiente: repetir el desbloqueo con credenciales reales del ThinkPad.

### 11 — Protocolos de uso diario

`wp_presentation`, `xdg-activation`, `wp_cursor_shape`, shake-to-locate,
pointer constraints, relative pointer, captura de salida/ventana y capturas
macOS (`Shift+Super+3/4`, PrintScreen). **Aceptado en anidado el 2026-10-23.**
Pendiente: presentation en vkms/DRM, captura rotada y la app de
`Shift+Super+5` (temporizador, miniatura, `Control` al portapapeles, `Option`
sin sombra). Pruebas: `./dev.sh presentation`, `activation`, `pointers`,
`screencopy` y `screencopy-window`.

### 12 — Portales y servicios

**12a parcial:** `xdg-desktop-portal-malaysia` sirve `Settings`
(`org.freedesktop.appearance` desde `appearance.domain`, `SettingChanged`,
`UseIn=Malaysia` y GTK de fallback) y `org.gnome.desktop.interface`
`color-scheme`. **Puente GTK hecho (2026-10-31, ADR 0050):** acento exacto y
tinte en GTK 3/4 y libadwaita por CSS de usuario; GTK 3 oscuro por
`settings.ini`; el portal arranca con `malaysia-session.target`. Prueba:
`cargo test -p xdg-desktop-portal-malaysia`, y en sesión abrir Nautilus con
`window_color = "wallpaper"`. Faltan `Screenshot`/`ScreenCast` y los
diálogos de permiso. **12b pendiente:** notificaciones, polkit, keyring/Secret
Service y AT-SPI.

### 13 — Chrome y launcher

Launcher `malaysia-shell launcher` + `Super+Space` **hecho (2026-10-23)**.
Marco unificado **hecho (2026-10-24):** esquinas de 16 px, sombra
compositor-side, borde interior y titlebar SSD integrados; las CSD reciben
esquinas/sombra/borde sin titlebar. Glifos de hover y doble clic también hechos.
Pendiente: modo foco real y captura de ventana con marco. Prueba:
`./dev.sh titlebar` y `./dev.sh demo`.

### 14 — Menú global

`org_kde_kwin_appmenu` + `com.canonical.dbusmenu` (ADR 0036/0040). **Hechos:**
14a (popups en BarrierKit, aceptado 2026-10-24), 14b-1 (menú de escritorio,
2026-10-24), 14c (menú por app, 2026-10-25), 14d-1..3 (2026-10-26) y 14d-5/6
(2026-10-27). **Pendientes:** 14b-2 (diálogos del sistema), 14e (apps propias
exportan dbusmenu), 14f-2 (cliente `org.gtk.Menus`), navegación por teclado y
scroll. Prueba: `./dev.sh demo` con apps reales.

### 15 — Rebranding a MalaysiaDreams

Crates, binarios, D-Bus, variables `MALAYSIA_*`, rutas `~/.config/malaysia` y
`~/.local/share/malaysia`, Nix, PAM, portal y sesión. **Hecho (2026-10-26).**
Los nombres antiguos (Yuki, CelestialKit, Neptunia) solo se conservan en los ADR
como registro histórico (ADR 0043).

### 16 — Fondo en Vulkan

**16a en curso:** `malaysia-wallpaper` es un cliente propio con wgpu/Vulkan
sobre layer-shell, escala fraccional, mipmaps, dmabuf sin copias y recarga por
inotify de `wallpaper.domain` (ADR 0049). **16b** (HEIC por defecto) y **16c**
(fondos dinámicos y throttling de frame callbacks) pendientes. Prueba:
`./dev.sh wallpaper-check`.

### Transversal — NixOS

`nixosModules.default` (`programs.malaysia.enable`) instala paquete, sesión,
PAM, `hardware.graphics`, units de shell y `xdg.portal` con GTK de fallback.
Pendiente: `services.graphical-desktop`, `security.polkit`, keyring/AT-SPI y una
VM que arranque `malaysia-session.target`.
