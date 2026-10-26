# Roadmap de bloques

El compositor se construye por bloques. Cada bloque es un incremento funcional con
una prueba de aceptación. Este archivo es la lista canónica de qué son los bloques;
los documentos por bloque en el directorio `bloques/` registran qué construyó de
hecho cada bloque, incluyendo cualquier divergencia deliberada con respecto al resumen.

## Bloque 0 — Andamiaje

- Workspace de Cargo con `malaysia-compositor` y `vow-design` (vacío)
- Smithay anclado a un `rev` fijo, con las features: `backend_winit`, `renderer_gl`,
  `wayland_frontend`, `desktop`
- `tracing` + `tracing-subscriber` con filtrado basado en variables de entorno
- Tipos de coordenadas: `Logical`, `Buffer`, `Physical` como tipos distintos desde el commit 1
- Compila y se ejecuta, sin hacer nada

## Bloque 1 — Conseguir que un cliente se conecte

- `State` que contiene `CompositorState`, `ShmState`, `SeatState`, `XdgShellState`,
  `OutputManagerState`, `DataDeviceState`
- Bucle de eventos calloop más un socket de Wayland, con `WAYLAND_DISPLAY` exportado al hijo
- `wl_compositor`, `wl_subcompositor`, `wl_shm` con `Argb8888`/`Xrgb8888`
- `wl_output` con `geometry`, `mode`, `scale`, `done`
- `wl_seat` con capabilities y un keymap xkb válido
- `xdg_wm_base` respondiendo `ping`/`pong`
- Aceptación: `weston-info` enumera todos los globals

## Bloque 2 — Una ventana en pantalla

- Backend winit y un bucle de renderizado básico
- `CompositorHandler::commit` aplicando correctamente el estado de doble buffer
- `XdgShellHandler::new_toplevel` → `configure` inicial antes de que el cliente adjunte un buffer
- `ack_configure` → mapeo de la ventana
- Un módulo dedicado que envuelve `desktop::Space` — ningún otro archivo lo llama directamente
- Importación y renderizado de buffers `wl_shm`
- Aceptación: `weston-simple-shm` muestra su patrón

## Bloque 3 — Entrada

- Foco de teclado al mapear, `wl_keyboard.enter`/`leave`
- Keymap, modificadores y repetición de teclas
- Foco de puntero mediante hit-testing, `motion`, `button`, `axis`
- Cursor: el `wl_pointer.set_cursor` del cliente y un cursor por defecto
- Contabilidad de serials — un único contador central, nunca serials fabricados
- Aceptación: `foot` se abre, muestra un prompt, acepta escritura y el layout es correcto

## Bloque 4 — Ventanas manipulables

- Arrastrar para mover y `xdg_toplevel.move`
- Redimensionado mediante `xdg_toplevel.resize` y sus bordes
- Cerrar: `close` y limpieza cuando el cliente muere
- Apilado y click-to-focus
- Aceptación: abrir y cerrar 50 ventanas sin superficies huérfanas y sin crecimiento de memoria

## Bloque 5 — Popups

- `XdgShellHandler::new_popup` y `PopupSurface` (`wayland_frontend`)
- Un árbol de popups propiedad de `malaysia-scene`; el `desktop::PopupManager` de
  Smithay está detrás de la feature `desktop`, que el ADR 0015 mantiene desactivada
- Posicionamiento contra el rectángulo decorado del padre desde ese árbol
- Ajustes de restricciones (flip, slide, resize) contra el área utilizable del
  monitor, pasando el rectángulo objetivo relativo al padre a
  `PositionerState::get_unconstrained_geometry` (`wayland_frontend`); el
  `get_geometry` sin ajustar también está disponible
- `configure` de popup antes de mapear, portando la geometría ajustada
- Grabs de teclado y puntero propiedad del compositor, con validación de serials;
  el `PopupKeyboardGrab`/`PopupPointerGrab` de Smithay están detrás de `desktop`
- Un clic fuera cierra desde la hoja hasta la raíz; `Esc` cierra un nivel
- Submenús anidados
- Aceptación: un menú de `foot` contra el borde inferior se voltea hacia arriba; un submenú más un
  clic fuera cierra ambos; matar al cliente con un popup abierto no deja huérfanos

## Bloque 6 — Portapapeles

- `wl_data_device_manager` y selección primaria (`zwp_primary_selection_device_manager_v1`)
- El foco del portapapeles y de la selección primaria sigue al foco de teclado
  (`set_data_device_focus` / `set_primary_focus`)
- Aceptación: copiar en `foot` y pegar en otra instancia

## Bloque 7 — linux-dmabuf y explicit sync

- `zwp_linux_dmabuf_v1` (v5, con feedback) en el compositor; el `main_device`
  del feedback es el render node del dispositivo del renderer GLES de winit, y
  los formatos son los que reporta el propio renderer. Sin `wl_drm`. Sin
  fallback a v3: sin render node el arranque falla con un error claro.
- Explicit sync (`wp_linux_drm_syncobj_manager_v1`, solo si el dispositivo
  soporta `syncobj_eventfd`) más el sync implícito (un *blocker* pre-commit que
  espera a que el dmabuf sea legible); un solo camino de commit. El **release
  point se aplaza** hasta que la GPU termina de leer (fence EGL exportada como
  fd nativo + source one-shot del loop; `EGL_ANDROID_native_fence_sync`
  obligatorio al arrancar).
- La shell presenta por EGL/dmabuf (`wl_egl_window`) en lugar de copiar a
  `wl_shm`; se elimina `wl_shm` de la shell (camino único). Subida de textura
  **parcial** (`glTexSubImage2D` del rectángulo dañado) en menubar/dock.
- **Cierre**: memoria en reposo medida con PSS/USS por proceso y GTT/VRAM
  (antes/después), el wallpaper mejora con EGL y el coste exclusivo de los
  componentes de texto (≈ 19 MB USS + 22 MiB GPU) es borderline pero no
  "claramente mayor" que el umbral pactado; el lock bloquea por EGL y matar su
  cliente no desbloquea la sesión.
- Aceptación: wezterm y Firefox acelerados dentro de MalaysiaDreams anidado (más vkcube y
  mpv); la shell sin copias por CPU. Ver `bloques/07-linux-dmabuf-y-explicit-sync.md`
  y los ADR 0026/0027.

## Bloque 8 — Direct Session (DRM/KMS)

Se divide en tres entregas. 8a es la única con código en este árbol.

- **8a — sesión mínima.** DRM/KMS atómico, libseat, libinput, GBM + EGL +
  `GlesRenderer`, `DrmCompositor` sobre el conector interno (eDP) en su modo
  preferido, **un solo plano primario** (sin overlay ni cursor planes), un
  frame por vblank con damage tracking, cambio de VT con pausa/restauración y
  la garantía del lock. Winit sigue siendo el backend de desarrollo. Sin
  hotplug, sin monitores externos y sin direct scanout.
- **8b — hotplug y monitores externos.** Varios conectores, reubicación de
  outputs, `wl_output` y `xdg-output` por conector, elección de modo y escala
  por monitor, orientación, primario, espejo y configuración persistente.
  Manejo de desconexión y de estados de energía del panel. Sin planos todavía.
- **8c — planos y direct scanout.** Planos overlay y cursor, asignación de
  superficies a KMS, comprobación de formato/modificador y opacidad, criterios
  de promoción y expulsión del direct scanout, y pruebas en vkms. Depende de
  8b para el caso multi-monitor.
- Aceptación 8a: **hecha** en el panel del ThinkPad el 2026-10-21, con
  `wezterm`, touchpad y cambio de VT. `./dev.sh drm-vkms` sigue pasando. Ver
  `bloques/08-direct-session.md`.

## Bloque 9 — Escala fraccional

- **Hecho:** `State::scale` (`output::Scale`), `display.scale` persistente en
  `display.domain` (`MALAYSIA_SCALE` queda como override de desarrollo), `wl_output`
  entero + `wp_fractional_scale_v1` exacto, escala en `logical_output_size`,
  render, damage tracker, cursor y viewporter de la shell, y el redondeo
  floor/ceil de rectángulos lógicos (ADR 0004). Ver
  `bloques/09-escala-fraccional.md`.
- **Pendiente:** aceptación en panel real a 1.25/1.5; escala por salida/monitor
  con 8b.
- Aceptación: una ventana y la shell se ven nítidas a escala 1.25/1.5; el
  puntero cae donde se ve; el panel real lo confirma.

## Bloque 10 — Lock con autenticación

- Implementado: PAM en el proceso del lock (`malaysia-lock`, fallback `login`),
  campo enmascarado + estados, auto-lock por `ext-idle-notify-v1`,
  `zwp_idle_inhibit_manager_v1` y recuperación del lock (destruction hook,
  watchdog de arranque y backoff 1/2/4/8/16 s, máximo 5 intentos).
- Atajo `Ctrl+Alt+L`: bloquea ahora o relanza el lock cuando no hay superficie
  de lock viva. Nunca desbloquea.
- Aceptación: **hecha** en el panel real el 2026-10-21 (contraseña
  incorrecta/correcta, auto-lock, mpv fullscreen, 5 relanzamientos y
  `Ctrl+Alt+L`). Ver `bloques/10-lock.md`.

## Bloque 11 — Protocolos de uso diario

Estado: **aceptado en anidado el 2026-10-23**; falta la presentación en vkms o
panel real (el probe existe y `drm-vkms` lo ejecuta con `Vsync` obligatorio).

- **`wp_presentation` — hecho y probado en anidado.** Global v2 con
  `CLOCK_MONOTONIC`, feedback `presented`/`discarded` tras presentar el frame y
  en `vblank` para DRM. `./dev.sh presentation` cubre timestamps crecientes y
  commit sin daño. Ver `bloques/11-protocolos-de-uso-diario.md`.
- **`xdg-activation-v1` — hecho.** Política estricta: superficie enfocada,
  serial del último teclado/botón y token de menos de 30 s; activa/restaura la
  ventana.
- **`wp_cursor_shape_v1` — hecho.** `TabletSeatHandler` completo, 35 formas
  mapeadas al tema Xcursor y restauración del cursor del cliente tras un borde.
- **Shake to locate — hecho.** El puntero crece con animación suave hasta 3×
  y encoge en cuanto el ratón se detiene; valores en `vow-design`.
- `xdg-output` ya se anuncia con `OutputManagerState::new_with_xdg_output`;
  no está pendiente.
- **`zwp_pointer_constraints_v1` + `zwp_relative_pointer_manager_v1` — hechos.**
  El global se anuncia; el handler activa la constraint sobre la superficie con
  foco, `locked` congela el puntero y `confined` lo limita a la región, y cada
  movimiento reenvía su delta a los `zwp_relative_pointer_v1`. Probes:
  `./dev.sh pointers`.
- **Captura de pantalla — hecha.** `ext-output-image-capture-source-v1` +
  `ext-foreign-toplevel-image-capture-source-v1` + `ext-image-copy-capture-v1`:
  constraints de sesión (tamaño, SHM y dmabuf con el render node), `frame()`
  encolado y completado por el backend (render offscreen a SHM por filas o
  directo al dmabuf del cliente), tanto de la salida como de una ventana.
  Probes: `./dev.sh screencopy` y `./dev.sh screencopy-window`. Falta el portal
  del Bloque 12. El volcado PNG de depuración por `SIGUSR1` (`capture.rs`,
  Bloque 7.1) sigue como herramienta de desarrollo.
- Opcional para uso diario: `keyboard-shortcuts-inhibit` (escritorio remoto).
  No bloquea la aceptación del Bloque 11.
- Aceptación en anidado (2026-10-23): SHM y dmabuf, cursor incluido/omitido,
  ventana con y sin titlebar SSD, `wp_presentation` con timestamps crecientes y
  `xdg-activation` con token válido y tres rechazos. Ver
  `bloques/11-protocolos-de-uso-diario.md`. Pendiente: repetir presentation en
  vkms/DRM real.

## Bloque 12 — Portales y servicios de escritorio

Se divide en dos entregas. Depende de la captura del Bloque 11.

- **12a — `xdg-desktop-portal-malaysia`.** Backend propio para lo que depende del
  compositor (ADR 0031). **Settings hecho**: `org.freedesktop.appearance`
  (dark/light, accent-color, contraste y reduced-motion) desde
  `appearance.domain`, con `SettingChanged`; portal registrado con
  `UseIn=Malaysia` y `xdg.portal.config.malaysia` con GTK de fallback.
  - `Screenshot` y `ScreenCast` sobre `ext-image-copy-capture` +
    `ext-image-capture-source` y PipeWire (dmabuf, modificadores, fences).
    Las capturas de ventana se guardan por defecto en el Escritorio del
    usuario, resuelto con `malaysia_compositor::xdg::desktop_dir()` desde XDG (nombre
    localizado, nunca fijo).
  - Diálogos de permiso de la shell y concesiones por app.
  - `xdg.portal.config.malaysia` enruta solo estas interfaces a MalaysiaDreams; FileChooser,
    AppChooser, Print, Notification y Access siguen en `-gtk`.
- **12b — servicios de escritorio.** Notificaciones
  (`org.freedesktop.Notifications`), agente polkit, keyring/Secret Service y
  AT-SPI para accesibilidad. Autostart por `xdg-desktop-autostart` si no lo
  cubre el módulo de NixOS.
- Aceptación 12a: Firefox/Chromium comparten una ventana y un monitor por el
  portal; OBS recibe el stream; dark/light cambia desde `.domain` sin
  reiniciar; el portal GTK sigue sirviendo FileChooser. (Settings verificado
  por D-Bus en anidado; falta la comprobación con una app real y el resto de
  interfaces.)
- Aceptación 12b: una notificación de una app aparece en la shell; una
  operación privilegiada pide contraseña por el agente polkit; Orca puede
  leer la shell.

## Bloque 13 — Chrome y launcher (adelantado)

Antes de seguir con los portales, el escritorio necesita chrome y una forma de
abrir apps. El diseño es **Neptunia** (ADR 0032).

- **Titlebar Neptunia — hecha.** Barra superior de 32 px para ventanas SSD,
  semáforos de 12 px,
  título centrado con `cosmic-text`, caché por ventana y botones de cerrar,
  minimizar y maximizar; modos de color `neutral`/`accent`/`wallpaper` con
  tinte de la titlebar y semáforos derivados; también se elevan las ventanas al enfocarlas. Ver
  [`arquitectura/ventanas.md`](arquitectura/ventanas.md).
- **Launcher — hecho (2026-10-23).** `malaysia-shell launcher` en la capa Overlay,
  `Super+Space`, foco exclusivo de capa, búsqueda y ranking `.desktop`,
  lanzamiento con token de activación e iconos. Latencia release 121 ms hasta
  la superficie configurada. Ver `bloques/13-chrome-y-launcher.md`.
- **Marco unificado de ventana — hecho (2026-10-24).** Esquinas redondeadas
  (16 px) en el shader `rounded_clip`, sombra compositor-side con `border.frag`
  + `shadow.frag`, inner border/hairline y titlebar SSD integrados en un solo
  elemento; las ventanas CSD reciben esquinas, sombra y borde pero no titlebar.
  Los diálogos usan el mismo marco y se centran sobre la ventana padre. Ver
  [`arquitectura/ventanas.md`](arquitectura/ventanas.md) y el ADR 0037.
- **Fundido de apariencia — hecho.** Al cambiar claro/oscuro el compositor
  captura la salida entera en una textura GLES y la funde con una curva
  ease-in-out cúbica de 420 ms, manteniendo el cursor por encima. Ver el
  [ADR 0044](decisiones/0044-fundido-de-apariencia.md).
- **Pendiente:** captura de ventana con marco (Bloque 11).
- **Después:** el orden es **Bloque 14 (menú global) → 12a** (`Settings` con una
  app real, `Screenshot`/`ScreenCast`) **→ 12b → 8b → 8c**. La aceptación manual
  del launcher con teclado real queda pendiente del ThinkPad.

## Bloque 14 — Menú global

Documentado en los [ADR 0036](decisiones/0036-menu-global.md) y
[0040](decisiones/0040-menu-global-real.md) y en
[`investigacion/global-menu.md`](investigacion/global-menu.md). Usa
`org_kde_kwin_appmenu` + `com.canonical.dbusmenu`.

- **14a — Popups en BarrierKit.** Aceptado (2026-10-24, ThinkPad, sesión
  anidada): `xdg_popup` con padre layer-shell en el runner propio, navegación
  con teclado en el ejemplo, cierre al clic fuera y restauración del foco.
- **14b-1 — Menú «MalaysiaDreams» de escritorio.** Aceptado (2026-10-24, sesión anidada):
  títulos **Sistema** y **MalaysiaDreams/nombre de app**, submenús Ir, Archivo, Edición,
  Ventana y Sistema; el menú Ventana usa `zwlr_foreign_toplevel_management_v1`.
- **14b-2 — Diálogos del sistema.** Suspender, Reiniciar, Apagar, Cerrar sesión.
- **14c — Menú por aplicación (respaldo).** Implementado (2026-10-25, sesión
  anidada): reutiliza `malaysia-shell/src/desktop.rs` para el nombre y las ventanas
  de la app; la barra muestra ❄ · {App} · Ventana · Ayuda con Ocultar/Mostrar
  todo/Salir, Minimizar/Maximizar/Cerrar, los submenús de disposición con
  iconos y el Ayuda con buscador. Maximizar queda pendiente del `State` de wlr
  y de `set_maximized` en el compositor. Ver `bloques/14-menu-global.md`.
- **14d — Menú real.** Arquitectura en el
  [ADR 0040](decisiones/0040-menu-global-real.md); 14d no usa AT-SPI.
  - **14d-1:** compositor, `org_kde_kwin_appmenu` + Infinity v2 (`app_menu`);
    la barra solo loguea el evento.
  - **14d-2:** **implementado (anidado, 2026-10-26)**: cliente dbusmenu y
    barra: hilo D-Bus con zbus, caché, carga perezosa, fusión de títulos,
    buscador de Ayuda sobre el árbol real, iconos de mapa de bits y
    `menubar.app_menus`.
  - **14d-3:** **implementado (anidado, 2026-10-26)**: árbol completo en
    segundo plano (timeout de 2 s, 1 s tras el foco, una petición por
    dirección) y aceptación con Kate y Firefox; el Registrar sigue servido por
    la barra desde 14d-2. GTK3 queda para `gtk_shell1`/`org.gtk.Menus`; el
    casado por pid del Registrar, pendiente de XWayland. Las dos preferencias
    de Firefox viven en `~/nixdots`, nunca en el repo.
- **14e — Apps de BarrierKit exportan dbusmenu.** Sin atajo privado por
  Infinity.
- **Después:** `gtk_shell1`/`org.gtk.Menus`, HUD en el launcher y menú de atajos
  por app.

Detalle del menú «MalaysiaDreams» y de las medidas de Neptunia en
[`arquitectura/menubar.md`](arquitectura/menubar.md#menú-global-planeado-bloque-14).

## Transversal — NixOS

- **Hecho:** `nixosModules.default` (`programs.malaysia.enable`) instala el
  paquete, registra la sesión, crea `/etc/pam.d/malaysia-lock`, activa
  `hardware.graphics` y define `malaysia-session.target` con un proceso por
  componente.
- **Pendiente:** `xdg.portal` con `-gtk` como fallback y el portal de MalaysiaDreams
  cuando exista; `services.graphical-desktop`, `security.polkit`, keyring y
  AT-SPI con `mkDefault`; y una VM NixOS que arranque la sesión y ejecute
  `malaysia-session.target`.

## Después del Bloque 12

- D-Bus de configuración (ADR 0021), el comando `domain` y la app de ajustes.
  XWayland sigue diferido.

## Futuro opcional — supersampling tipo macOS

**No se implementa ahora.** Es un modo de calidad que solo se reevaluará cuando
el escritorio base sea competente:

- 8b y 9 aceptados en panel real (multi-monitor y escala);
- Bloque 11 cerrado (protocolos de uso diario y captura);
- Bloque 12a/12b cerrado (portal, notificaciones, polkit, keyring, AT-SPI);
- uso diario estable durante un periodo razonable, sin regresiones graves.

La idea es renderizar la escena completa a 2× de la resolución lógica y reducir
el framebuffer al panel, como macOS, para que los clientes que no soportan
fractional-scale también se vean nítidos. Coste: más memoria y GPU, y un blit
final. No sustituye al modelo Wayland de escala por salida/superficie; sería una
opción adicional, nunca el camino por defecto sin medir.

Ver [`investigacion/escalado-macos.md`](investigacion/escalado-macos.md).

## Dónde está este repositorio

- **Smithay — actualizado.** La subida a `023b78b1c5a05ca512767cacc07efca55fae8be4`
  (ADR 0028) está hecha y la aceptación con el backend anidado pasó.
- **Bloque 0 — hecho, con divergencias.** Los tipos de coordenadas viven en el
  crate `malaysia-scene`, no en el compositor (ADR 0001). Smithay está anclado por
  `rev` a `023b78b1c5a05ca512767cacc07efca55fae8be4` con `desktop` desactivado
  (ADR 0009, ADR 0010). El nivel de log se establece con `--log-level`,
  no con filtrado basado en variables de entorno. `vow-design` no está vacío.
- **Bloque 1 — hecho.** `wayland-info` (el dev shell no tiene `weston-info`) enumera
  todos los globals. Véase `bloques/00-cimientos.md` y el registro a nivel de sesión
  `bloques/01-sesion.md`, que cubre los Bloques 1–3 juntos.
- **Bloque 2 — hecho, con divergencias.** El mapeo ocurre en el primer commit con
  buffer, no en `ack_configure` (`bloques/01-sesion.md`, Divergencias 1).
  Los toplevels se rastrean en `crates/malaysia-compositor/src/window.rs` sin envoltorio de
  `desktop::Space` (ADR 0009; la elección es ahora una decisión escrita, ADR
  0015). El cliente de aceptación es `foot`, ya que weston no está instalado. Véase
  `bloques/02-ventana-en-pantalla.md`.
- **Bloque 3 — hecho, con lagunas verificadas a mano.** El foco de teclado, el
  keymap/repeat, el motion/button/axis del puntero, el soporte de cursor y la
  contabilidad central de serials están implementados. `./dev.sh smoke` comprueba que el compositor dibuja su
  puntero por defecto y verifica cualquier `set_cursor` del cliente contra el enter de puntero que
  responde. La escritura, la respuesta del puntero, la salida legible y los píxeles del
  cursor renderizado permanecen verificados a mano. Véase `bloques/03-entrada.md`.
- **Bloque 4 — hecho, con lagunas verificadas a mano.** El movimiento (arrastre de
  la titlebar y `xdg_toplevel.move`), el redimensionado (zonas de borde y `xdg_toplevel.resize`, todos
  los bordes, min/max respetados), cerrar, el apilado, el click-to-focus y los cursores de
  redimensionado de borde están implementados en `malaysia-scene` y en el compositor. Los
  serials de `xdg_toplevel.move`/`resize` se validan contra la última pulsación del puntero. `./dev.sh block4` ejecuta
  tres tandas de 50 ventanas y comprueba que la escena vuelve a estar vacía (recuento de
  objetos vivos) sin crecimiento de memoria por tanda; `./dev.sh move` ejercita el
  move/resize iniciado por el cliente con serials válidos y fabricados. La sensación de arrastre, los
  visuales de redimensionado y la forma del cursor cambiando bajo el puntero se
  verifican a mano. Véase `bloques/04-ventanas-manipulables.md`. Dos seguimientos
  posteriores a la aceptación arreglaron el frame invertido verticalmente y reemplazaron la
  política de decoración siempre del lado del servidor (ADRs 0018 y 0019); `./dev.sh
  decoration` verifica la política contra aplicaciones reales.
- **Bloque 5 — hecho, con divergencias.** Los popups se rastrean en `popup.rs`
  (árbol de geometría en `malaysia-scene`, superficies en el compositor), con
  ajuste de restricciones contra el output (`get_unconstrained_geometry`),
  configure antes de mapear, render encima de las ventanas, y cierre por clic
  fuera, Esc y muerte del cliente. Verificado en vivo con `popup_probe`
  (volteo del menú y sin huérfanos). Ver `bloques/05-popups.md`.
- **Bloque 6 — hecho.** Se anuncian `wl_data_device_manager` y
  `zwp_primary_selection_device_manager_v1`. El portapapeles y la selección primaria
  siguen el foco de teclado mediante `set_data_device_focus` y `set_primary_focus`
  (verificado: `wl-copy` → `wl-paste` entre clientes). La aceptación de copiar y
  pegar entre dos instancias de `foot` se comprueba a mano. Véase
  `bloques/06-portapapeles.md`.
- **Bloque 7 — hecho.** 7a (linux-dmabuf v5 con feedback) y 7b (explicit
  sync + sync implícito) hechos: `zwp_linux_dmabuf_v1` con el render node como
  `main_device`, `wp_linux_drm_syncobj_manager_v1` solo si el dispositivo lo
  soporta, un solo camino de blocker en el commit y release diferido por fence.
  7c hecho: la shell presenta por EGL (`wl_egl_window`), sin `wl_shm`. vkcube
  (RADV), mpv (gpu-next), wezterm y Firefox corren acelerados. Véase
  `bloques/07-linux-dmabuf-y-explicit-sync.md` y los ADR 0026/0027.
- **Bloque 8a — implementado y aceptado en el panel real.** DRM/KMS atómico,
  libseat, libinput, GBM/EGL/GLES y un plano primario. `./dev.sh drm-vkms`
  pasó (modeset, render en el render node, scanout en vkms, vblank, daño y
  captura). Winit sigue igual: `smoke`, `block4`, `move`, `decoration`,
  `popup_probe` y `popup_live` pasan en el backend anidado. La aceptación en el
  panel eDP (touchpad, VT y lock + VT) se cerró el 2026-10-21 con `wezterm`.
  Ver `bloques/08-direct-session.md`, ADR 0029 y ADR 0030.
- **Bloque 9 — aceptado.** `State::scale` sustituye el literal global; la escala
  entera y fraccionaria llega a `wl_output`, render, damage tracker, cursor y
  shell, y `display.scale` es persistente (`MALAYSIA_SCALE` es el override de
  desarrollo). Verificado en anidado a 1.25/1.5/2.0 y aceptado en panel real a
  1.0/1.25/1.5, incluidas capturas y cursor. Falta la escala por salida de 8b.
  Ver `bloques/09-escala-fraccional.md`.
- **Bloque 10 — implementado y aceptado.** PAM en el proceso del lock, UI de
  contraseña, auto-lock por `ext-idle-notify-v1` con
  `zwp_idle_inhibit_manager_v1`, recuperación del lock con backoff y límite de
  5 intentos, y `Ctrl+Alt+L` para bloquear o relanzar el lock sin desbloquear.
  Aceptado en el panel real el 2026-10-21. Ver `bloques/10-lock.md`.
- **Integración NixOS — hecha la base.** El flake expone
  `nixosModules.default` (`programs.malaysia.enable`): paquete, sesión, PAM,
  `hardware.graphics` y units de la shell. Faltan portal, keyring, AT-SPI,
  polkit y una VM de prueba (sección Transversal).
- **Bloque 12 — parcial.** 12a/Settings hecho; faltan Screenshot, ScreenCast,
  los diálogos de permiso y 12b (notificaciones, polkit, keyring, AT-SPI).
- **Bloque 14 — 14a, 14b-1, 14c, 14d-2 y 14d-3 en anidado; 14b-2 y 14e
  pendientes.** El menú global está decidido (`org_kde_kwin_appmenu` +
  `com.canonical.dbusmenu`, ADR 0036) y repartido en 14a–14e. 14a (popups en
  BarrierKit) aceptado en el ThinkPad el 2026-10-24 en sesión anidada. 14b-1
  (menú «MalaysiaDreams» de escritorio en `malaysia-shell menubar`) aceptado en anidado el
  2026-10-24. 14c (menús de la app enfocada) implementado el 2026-10-25; el
  compositor aún no envía `State` ni implementa `set_maximized`.
- **Ajustes del sistema (`malaysia-settings`) — integrado.** Se abre desde el menú
  ❄ de `malaysia-shell menubar`, se empaqueta en Nix con su `.desktop` y se
  documenta en los ADR 0041 y 0042. Hoy solo tiene la sección Apariencia; más
  dominios llegarán con el backend D-Bus de configuración (ADR 0021).

## Numeración de bloques

Este archivo es la única autoridad sobre los números de bloque. Dos cuerpos de
trabajo están deliberadamente nombrados y no numerados, para que no puedan confundirse
con un bloque del roadmap:

- **La sesión anidada** — el trabajo registrado en `bloques/01-sesion.md`
  (anidamiento de winit, mapeo de toplevels y entrada). Abarca los Bloques 1–3 de este
  roadmap, y las Partes 1–6 que usa el documento son Partes de esa sesión,
  nunca números de bloque.
- **Direct Session** — el hito de producción sobre DRM/KMS, que ahora es el
  **Bloque 8** del roadmap.

Ningún documento ni comentario de este repositorio usa "Bloque N" para algo distinto
del bloque N de este roadmap.
## Bloque 15 — Rebranding a MalaysiaDreams

- Renombrar crates, paquetes, librerías y binarios a los nombres de Malaysia
  (`malaysia-compositor`, `malaysia-shell`, `barrier-kit`, etc.).
- Renombrar identificadores de runtime: D-Bus (`org.malaysia.Compositor1`),
  variables de entorno (`MALAYSIA_*`), rutas (`~/.config/malaysia`), systemd,
  PAM, portal y sesión (`MalaysiaDreams` / `Malaysia`).
- Actualizar Nix: flake, módulo NixOS (`programs.malaysia`), derivación Skia,
  `dev.sh` y scripts auxiliares.
- Actualizar documentación viva: README.md, AGENTS.md, docs/README.md,
  docs/roadmap.md, glosario, docs/malaysia.md y ADR 0043.
- Aceptación: compila, pasa tests/clippy/fmt, `nix flake check` limpio, y no
  quedan los nombres anteriores fuera de `docs/decisiones/`, `docs/bloques/`,
  `docs/investigacion/` y el ADR 0043.

## Trabajo fuera de bloque — migración de BarrierKit a Freya

Derivado del ADR 0034; no es un bloque nuevo ni renumera nada.

- Construir BarrierKit sobre los crates de núcleo de Freya `=0.4.3` y añadir su
  runner Wayland (layer-shell, ext-session-lock, xdg_toplevel futuro).
- Compilar Skia para `freya-skia-bindings` 0.98.0 en Nix y guardar versión y
  features contra el árbol de Cargo.
- Migrar `malaysia-shell` componente a componente (wallpaper, menubar, dock, lock) y
  eliminar el camino GLES de la shell.
- Mantener Grade Animation como módulo del kit sobre `freya-animation`.

Estado (2026-10-22): runner Wayland, tema, controles y los cuatro componentes
migrados; Skia de Nix para `freya-skia-bindings` 0.98.0. La recuperación del
lock con el runner nuevo se verificó (matar el cliente con `kill -9` crea una
superficie nueva, una sola `get_lock_surface` por salida y sin bucle). El
`duplicate_output` anterior era de dos clientes de lock a la vez (uno de
recuperación y otro manual), no de un lock nuevo heredando estado. El
desbloqueo PAM del lock migrado estaba roto (el campo se quedaba en
`Verifying`/`Error` y no aceptaba más teclas); corregido con el estado de
`PasswordField` como fuente de verdad. Siguen escalas fraccionales,
accesibilidad y la prueba con credenciales reales. **Pendiente la aceptación en
el ThinkPad.** Las medidas de RSS/PSS se tomaron con radeonsi (GPU real), no con
llvmpipe.
