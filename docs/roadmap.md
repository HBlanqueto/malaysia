# Roadmap

El compositor se construye por bloques. **Este archivo es la única fuente de
verdad del estado**: qué está hecho, qué falta y cómo se prueba. El porqué de
cada decisión vive en los ADR; los hallazgos abiertos, en [`deuda.md`](deuda.md).

## Ahora (2026-10-31)

- **Menú global (Bloque 14).** 14f-2 aceptado (2026-11-01); falta aceptar
  14b-2 (diálogos del sistema), los mnemónicos con Alt y el scroll de menús
  largos; la pantalla completa desde el menú Ventana sigue pendiente. 14d-1,
  14e y 14f-1 están aceptados (ver §14). Prueba: `./dev.sh demo`
  y los menús de una app real.
- **Espacios y modo foco (Bloque 17, ADR 0051).** 17a, 17b y 17c
  hechos: fila de espacios, deslizamiento con franja negra y modo foco con el
  botón verde, el borde superior en foco (17d), y la barra negra y los
  semáforos de barrierkit que aparecen con ella (17e). Falta el experimento
  con libadwaita (17f).
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

`org_kde_kwin_appmenu` + `com.canonical.dbusmenu` (ADR 0036/0040) y
`gtk_shell1` → `org.gtk.Menus` (ADR 0045). Las fuentes de estado son el
documento del bloque fusionado en `fb02921` (`git show
fb02921^:docs/bloques/14-menu-global.md`) y el código citado.

**Hechos:**

- 14a (popups en BarrierKit): aceptado 2026-10-24; submenús hermanos
  re-aceptados 2026-10-28.
- 14b-1 (menú de escritorio): 2026-10-24.
- 14c (menú por app con respaldo de `.desktop`): 2026-10-25.
- 14d-1 (compositor, `org_kde_kwin_appmenu` + evento `app_menu` de Infinity
  v2): implementado; `7c3bfd1` y `fb4005f` en el tag
  `archivo/historia-hasta-2026-10-26`, código en
  `crates/malaysia-compositor/src/appmenu.rs`.
- 14d-2/3 (cliente dbusmenu, árbol completo): 2026-10-26.
- 14d-5/6 (títulos recordados, `MENU_GRACE`, separadores): 2026-10-27.
- 14e (apps propias exportan dbusmenu): aceptado 2026-10-26 con el ejemplo
  `contador` (barrierkit `crates/barrier-kit/examples/contador.rs`,
  `crates/barrier-kit/src/platform/app_menu_export.rs`).
- 14f-1 (`gtk_shell1` v5 + evento `gtk_menus` de Infinity v3): aceptado
  2026-10-28 en sesión anidada (`crates/malaysia-compositor/src/gtk.rs`; la
  barra solo registra la dirección, `crates/malaysia-shell/src/menubar/bar.rs`).
- Teclado en los popups (barrierkit `crates/barrier-kit/src/menu.rs`,
  `on_global_key_down`): ↑↓, Inicio/Fin, → abre submenú, ← cierra el nivel,
  Enter/Espacio activan, ←→ en el menú raíz pasan al título vecino
  (`with_navigation`, usado en `crates/malaysia-shell/src/menubar/menu.rs`).
  Esc lo resuelve el compositor (`State::dismiss_topmost_popup`).

**Pendientes:**

- 14f-2: aceptado 2026-11-01 en sesión anidada con
  `gtk3-demo-application` (menú Preferences en la barra; `7a8199d`,
  `menubar/gtkmenus.rs`, ADR 0045 §5–6).
- 14b-2: implementado 2026-11-01, falta aceptarlo en anidado.
  - Suspender va directo; Reiniciar…, Apagar… y Cerrar sesión… abren
    `malaysia-shell power <acción>` (confirmación con Esc/Cancelar y Enter).
    logind: `Suspend`/`Reboot`/`PowerOff(true)` y `TerminateSession` con
    `XDG_SESSION_ID`, que el compositor importa al user manager
    (`crates/malaysia-shell/src/power.rs`).
  - `MALAYSIA_POWER_DRY_RUN=1` (por defecto en `./dev.sh`, forzado en el
    compositor anidado) solo escribe la llamada en el log.
  - Forzar salida… (⌥❖Esc o el menú ❄): `malaysia-shell force-quit` lista
    una fila por app de `zwlr_foreign_toplevel` con su icono; el botón pide
    `force_quit` (Infinity v4, barrierkit `8b96ae7`) y el compositor envía
    SIGKILL al pid de las credenciales del socket.
  - Acerca de este equipo: `malaysia-shell about`, ventana fija con semáforos
    como «Acerca de este Mac». Modelo por DMI (Lenovo guarda «ThinkPad …» en
    `product_version`); dibujo `about/thinkpad.svg` para ThinkPad y
    `about/generic.svg` para el resto. Subtítulo con las pulgadas del panel
    (EDID). Filas cortas: procesador, memoria, disco de arranque («SSD de 512
    GB», siguiendo LUKS/LVM), número de serie si es legible y versión; «Más información…»
    abre Ajustes (`crates/malaysia-shell/src/dialogs.rs`). Mide 280×430 como en macOS y muestra solo procesador, memoria, disco, serie y versión.
- Mnemónicos, Espacio y scroll: implementados 2026-11-01, falta aceptarlos en
  anidado (barrierkit `bdbd583` y `a2394a6`, `menu.rs`).
  - dbusmenu y GTK convierten `_` en `MNEMONIC_MARK` (U+2063, invisible y sin
    ancho); con Alt pulsado la fila subraya esa letra y al soltarlo vuelve al
    texto normal. Las comparaciones de títulos y la búsqueda la ignoran.
  - Espacio activa solo sin Ctrl ni Alt (`inserts_text`): Ctrl+Espacio es del
    método de entrada y Alt+Espacio del menú de ventana. En la búsqueda de
    Ayuda un espacio se escribe en la consulta.
  - Menús más altos que la salida: el menú raíz pide `resize_y` sin `slide_y`
    (no se sube encima de la barra) y los submenús `slide_y` + `resize_y`.
    `MenuView` muestra franjas ▲/▼ que desplazan al pasar el cursor, la rueda
    desplaza y el foco del teclado queda a la vista; los submenús se anclan a
    la fila visible (`PopupHandle::scroll_shift`).
- «Ajustes del sistema…» del menú ❄ no abre el cliente con
  `MALAYSIA_TEST_INPUT` en la sesión anidada.
- Aceptación en DRM de 14a, 14b-1 y 14c.
- 14f-3 (libadwaita, ADR 0052): el fork `HBlanqueto/libadwaita` publica el
  menú ☰ como menubar con `x-adw-app-menu` y la barra lo pliega en el título
  de la app. Falta el overlay de Nix con el fork y aceptarlo con una app
  libadwaita real.

**Fuera del bloque (anotado):** HUD en el launcher, menú de atajos por app y
casado por pid del Registrar (XWayland).

Prueba: `./dev.sh demo` con apps reales.

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

### 17 — Espacios y modo foco

Concepto y plan en el ADR 0051.

- **17a — Fila de espacios. Hecho (2026-11-01), sin aceptar en sesión.**
  `malaysia_scene::spaces` guarda la fila (hasta 99 escritorios; los de foco
  no cuentan) y dónde vive cada ventana. El compositor saca de la escena las
  ventanas de otros espacios (`Windows::set_off_space`, aparte de minimizar).
  `Ctrl+Alt+←/→` cambia de espacio y `Ctrl+Alt+N` añade un escritorio. Una
  ventana nueva va al escritorio mostrado y un diálogo va con su padre.
  Enfocar una ventana de otro espacio (dock, activación) muestra su espacio.
  Prueba: `./dev.sh demo`, abrir ventanas, `Ctrl+Alt+N`, abrir otra y alternar
  con `Ctrl+Alt+←/→`; el log muestra `space shown`.
- **17b — Deslizamiento. Hecho (2026-11-01), sin aceptar en sesión.** Al
  cambiar de espacio la fila se desliza (`SPACE_SLIDE_MS` 480 ms, curva
  `SPACE_SLIDE_EASE`) con una franja negra de `SPACE_GAP` (48 px) entre
  espacios; los tokens están en `vow-design` (barrierkit `55614a3`). La barra
  superior (panel Top anclado arriba con zona exclusiva) y el fondo se dibujan
  dentro de cada escritorio y se van con él; el dock y los demás paneles
  quedan fijos. Las ventanas del destino reciben la entrada desde el primer
  frame y una segunda pulsación reorienta el deslizamiento desde donde está.
  `appearance.reduced_motion` lo quita. Prueba: `./dev.sh demo`, dos
  escritorios con ventanas y `Ctrl+Alt+←/→`.
- **17c — Modo foco. Hecho (2026-11-01), sin aceptar en sesión.** El
  semáforo verde (`green_button = "focus"`), el verde propio de una app CSD,
  F11 (`set_fullscreen`) y `Ctrl+Super+F` crean un espacio de foco a la
  derecha del escritorio. La ventana recibe `fullscreen` y crece desde su
  marco hasta la salida, con la esquina pasando de 16 a 0, mientras la fila se
  desliza. La ventana se dibuja por encima del dock, que no se mueve hasta
  quedar tapado. En el espacio de foco la barra y el dock no se dibujan ni
  reciben clics, y el fondo es negro. Salir hace lo inverso y borra el espacio
  al terminar; cerrar la ventana lo borra al instante. Durante la animación se
  ignoran `Ctrl+Alt+←/→`. Al terminar de crecer, la animación sigue
  dibujando la ventana a pantalla completa hasta que el cliente entrega su
  buffer a ese tamaño (máximo 300 ms), para no mostrar un frame negro; el log
  `focus transition handed over` dice cuánto esperó. Prueba: `./dev.sh demo`,
  botón verde de una ventana SSD, `Ctrl+Alt+←/→` entre el escritorio y el
  foco, y verde otra vez. Una ventana de ancho fijo (Ajustes) nunca entra en
  foco: su verde la estira en vertical. Las apps CSD de barrierkit piden
  fullscreen con su verde (barrierkit `ec9ff5a`), y un `set_maximized` sobre
  una ventana en foco la saca.
- **17d — Borde superior. Hecho (2026-11-01), sin aceptar en sesión.** En un
  espacio de foco quieto, el puntero en el borde superior baja la barra sobre
  una franja negra (`DURATION_BASE`, 220 ms). Una ventana SSD recupera su
  titlebar Tengen bajo la barra, encima del contenido y a lo ancho; sus
  semáforos funcionan (el verde sale del foco) y esa franja no llega al
  cliente. Una ventana CSD (barrierkit, libadwaita) baja entera la altura de
  la barra, también para la entrada, y su propia barra queda visible. La barra
  sube cuando el puntero baja más de dos titlebars por debajo de ella, salvo
  con un menú abierto. La barra negra con texto blanco llega con el 17e. Prueba: `./dev.sh demo`, modo
  foco con una ventana SSD y con una CSD, y el puntero arriba.
- **17e — Infinity. Hecho (2026-11-01), sin aceptar en sesión.** Infinity v5 (barrierkit `ee5b141`) añade el evento `space`:
  posición, número de espacios, escritorio y estado de la barra sobre un
  foco (`focus_bar`: none, hidden, revealed). El compositor lo envía al
  bindear y en cada cambio; la shell lo publica en `Toplevels::space`. Sobre
  un espacio de foco la menubar se pinta con `FOCUS_BAR_BACKGROUND` (negro) y
  `FOCUS_BAR_TEXT` (blanco). Para las apps, el protocolo aparte
  `infinity_focus_v1` (barrierkit `c8965c3`) está abierto a cualquier
  cliente: el objeto de una superficie recibe `bar(1)` cuando la barra baja
  sobre la ventana CSD en foco (y la ventana baja con ella) y `bar(0)` cuando
  sube. El kit oculta los semáforos de su titlebar y de su barra lateral en
  fullscreen y los muestra solo con `bar(1)`, como Finder. Prueba: modo foco
  con una app de barrierkit (sin semáforos), puntero arriba (aparecen),
  puntero abajo (se van).
- **17f — libadwaita.** Experimento con el fork. Pendiente.
- **14f-3 — Menú global para apps libadwaita.** Implementado: parche
  `nix/patches/libadwaita-global-menu.patch`, paquete
  `packages.<system>.libadwaita-global-menu` y opción de NixOS
  `programs.malaysia.globalMenu.libadwaita.enable` con
  `system.replaceDependencies` (ADR 0052). Falta aceptarlo en sesión anidada
  con una app real (`gnome-text-editor` o `gnome-calculator`).

### Transversal — NixOS

`nixosModules.default` (`programs.malaysia.enable`) instala paquete, sesión,
PAM, `hardware.graphics`, units de shell y `xdg.portal` con GTK de fallback.
Pendiente: `services.graphical-desktop`, `security.polkit`, keyring/AT-SPI y una
VM que arranque `malaysia-session.target`.
