# Por hacer — plan de trabajo

Última actualización: 2026-10-22.

Este documento es la lista corta de lo que toca ahora. El plan de bloques de
largo plazo vive en [`roadmap.md`](roadmap.md); aquí solo se ordena la
ejecución y se apuntan las pruebas que faltan.

## Hecho el 2026-10-21

- **8a aceptado** en el panel real del ThinkPad con `wezterm`, touchpad y
  cambio de VT. Checklist en [`bloques/08-direct-session.md`](bloques/08-direct-session.md).
- **10 aceptado**: PAM, auto-lock, recuperación y `Ctrl+Alt+L`. Checklist en
  [`bloques/10-lock.md`](bloques/10-lock.md).
- **Probes corregidos**: adjuntaban el buffer antes del primer
  `xdg_surface.configure`; Smithay master lo rechaza con
  `unconfigured_buffer`. Ahora `smoke`, `block4`, `move`, `decoration`,
  `popup_probe` y `popup_live` pasan.
- **Nautilus**: una instancia abierta en GNOME absorbe el lanzamiento por
  D-Bus. Para probarla dentro de Yuki:
  `dbus-run-session -- nautilus --no-desktop`. Nautilus 50 pide decoración
  server-side y ADR 0019 la respeta.
- **Módulo NixOS** base: `programs.yuki.enable`, sesión, PAM, graphics y units.
- **Capturas al estilo macOS**: `Shift+Super+3` (pantalla), `Shift+Super+4`
  (región con cruz, `Espacio` para ventana, `Esc` cancela) y PrintScreen;
  guardan en el Escritorio XDG. Ver
  [`bloques/11-protocolos-de-uso-diario.md`](bloques/11-protocolos-de-uso-diario.md).
- **Escala fraccional verificada en anidado** a 1.25/1.5/2.0, con las capturas
  offscreen, el hotspot del cursor y el cursor ampliado del shake corregidos.
- **`./dev.sh demo`**: sesión anidada con wallpaper, menubar, dock y ventanas.

`foot` sigue siendo el cliente guionizable de `dev.sh smoke` porque negocia
server-side; `wezterm` es el cliente manual de aceptación.

## Bloque 9 — cerrado

Aceptado en panel real a 1.0, 1.25 y 1.5 (2026-10-21). Hecho: `State::scale`,
`YUKI_SCALE`, `wl_output` + fractional-scale, render, cursor y viewporter de la
shell; ver `bloques/09-escala-fraccional.md`.

- [x] Aceptación en panel real a 1.25 y 1.5 con `wezterm`, shell y puntero.
- [x] `display.scale` persistente en `display.domain` (`YUKI_SCALE` queda como
      override de desarrollo).
- [ ] Escala por salida/monitor cuando 8b introduzca varios outputs.
- [x] Redondeo de rectángulos lógicos (ADR 0004): floor posición, ceil tamaño.

## Ahora: escritorio usable (Bloque 13, adelantado)

Mapa completo de la UI: [`arquitectura/interfaz.md`](arquitectura/interfaz.md).

El diseño es **Neptunia** (ADR 0032): titlebar arriba de 32 px para ventanas SSD, radio 16,
semáforos de 12, tema claro por defecto.

- [x] Titlebar en el compositor: fondo, semáforos redondeados, título centrado,
      caché y botones cerrar/minimizar/maximizar. Ver
      `arquitectura/ventanas.md`.
- [x] Foco de puntero limitado al rectángulo del cliente (la titlebar no se
      trata como superficie del cliente).
- [x] Menubar y dock con la paleta clara de Neptunia.
- [x] `appearance.window_color` (`neutral`/`accent`/`wallpaper`) y
      `traffic_lights` en la titlebar; el color dominante del wallpaper lo
      calcula la shell y `window_tint_scope = "desktop"` tiñe menubar y dock.
- [ ] Launcher `yuki-shell launcher` + `Super+Space` (requiere foco de teclado
      en layer surfaces).
- [ ] Esquinas redondeadas, sombra, glifos de hover y doble clic para zoom.
- [x] Resize por bordes con banda exterior de 10 px, «L» de esquina de 20 px
      (2 px hacia dentro), botones de la titlebar con prioridad, solo ventanas
      SSD y exclusión de maximizado/fullscreen. Queda sumar sombra/CSS y tiled
      cuando exista tiling. Ver
      [`investigacion/resize-bordes-y-cursores.md`](investigacion/resize-bordes-y-cursores.md)
      (incluye el traspaso y los siguientes pasos).
- [ ] Documentar/portar el resto de tokens de Neptunia y, si se puede, generar
      `yuki-design` desde sus JSON.

## Shell sobre Freya — pendientes

- [ ] **Doble clic de la titlebar CSD**: leer `window.double_click_titlebar`
      de `~/.config/yuki/window.domain`; hoy el kit usa
      `TITLEBAR_DOUBLE_CLICK` como `TODO(ADR 0035)`.
- [ ] **Portapapeles propio por `wl_data_device`** (y selección primaria
      `zwp_primary_selection`). Hoy `freya-clipboard` aporta `copypasta`, que
      arrastra `x11rb` y una segunda copia de SCTK 0.20/calloop 0.14; el kit
      entrega un portapapeles vacío. Sustituirlo por un cliente Wayland propio
      y quitar `freya-clipboard` del grafo del kit cuando ningún control lo
      necesite.
- [ ] **Eliminar la segunda copia de SCTK y calloop**: la trae
      `freya-clipboard → smithay-clipboard → smithay-client-toolkit 0.20`
      frente a la 0.19 del runner. Se va con el portapapeles propio o subiendo
      el runner a 0.20 cuando sea compatible.
- [x] **Recuperación del lock con el runner nuevo**: matar el cliente con
      `kill -9` crea la superficie de recuperación, sin bucle (0 % de CPU) y
      con una sola `get_lock_surface` por salida y objeto `ext_session_lock_v1`
      (`WAYLAND_DEBUG=1`).
- [x] **Desbloqueo PAM**: el lock migrado se quedaba en `Verifying` tras el
      primer `Prompt` y no volvía a aceptar teclas tras un `Denied`; el campo
      quedaba bloqueado y el lock no se podía desbloquear. Corregido en
      `d4bd12c` usando el estado de `PasswordField` como única fuente de
      verdad, con el secreto nuevo en el segundo intento y `unlock` + `flush`.
      Probado a mano con un servicio PAM de prueba (contraseña mala → mensaje →
      contraseña buena → desbloqueo, sin recuperación). Falta repetirlo con
      credenciales reales y en panel físico.

### Contexto de las medidas de memoria

El RSS/PSS en reposo (wallpaper, menubar, dock) se midió en esta máquina con
**GPU real AMD radeonsi/renoir** (`GL_RENDERER: AMD Radeon Graphics (radeonsi,
renoir, ACO)`), **no con llvmpipe**. El PSS de ~47 MB ya reparte `libLLVM` entre
los procesos y en reposo solo corren tres componentes; Skia raster cargaría a
la CPU las animaciones y el redibujado de la shell. **Queda descartado.** Solo
se revisará si el ThinkPad mide algo muy distinto.

## Deuda técnica

- `crates/yuki-design/neptunia/default.nix` (generador de `neptunia.slint`)
  convierte duraciones, recuentos e intervalos en `px` porque Slint no tiene
  unidades para todo. Hay que separar esos tokens numéricos puros de las
  longitudes con unidad para no mentir en los outputs de otros toolkits.

## Después, en orden

1. **Bloque 11 — protocolos de uso diario.** `wp_presentation`,
   `xdg-activation`, `cursor-shape`, el shake-to-locate, `pointer-constraints` +
   `relative-pointer` y la captura (salida y ventana:
   `ext-output-image-capture-source-v1`,
   `ext-foreign-toplevel-image-capture-source-v1` y
   `ext-image-copy-capture-v1`) y las capturas macOS con `screenshot.domain`
   están hechos (incluido `target = "clipboard"`). Faltan la miniatura,
   `Control` al vuelo y la app de `Shift+Super+5`. Ver
   `bloques/11-protocolos-de-uso-diario.md`.
2. **Bloque 12a — portal nativo.** `Settings` /
   `org.freedesktop.appearance` **hecho** (backend D-Bus, `SettingChanged`,
   `xdg.portal.config.yuki` con GTK de fallback). Siguen `Screenshot` y
   `ScreenCast` sobre la captura del Bloque 11 y PipeWire. GTK sigue sirviendo
   FileChooser, Print y Notification.
3. **Bloque 12b — servicios de escritorio.** Notificaciones, agente polkit,
   keyring/Secret Service y AT-SPI.
4. **Bloques 8b/8c — multi-monitor y planos.** 8c después de 8b.

## Transversal NixOS

El módulo `nixosModules.default` ya instala el paquete, la sesión, PAM,
`hardware.graphics` y las units de la shell. Falta:

- [ ] `xdg.portal` con GTK como fallback y el portal de Yuki cuando exista.
- [ ] `services.graphical-desktop`, `security.polkit`, keyring y AT-SPI con
      `mkDefault`.
- [ ] Una VM NixOS que arranque la sesión y ejecute `yuki-session.target`.

## Futuro opcional (no ahora)

- **Supersampling tipo macOS:** renderizar a 2× lógico y reducir al panel para
  nitidez uniforme. Solo se reevalúa cuando 8b/9/11/12 estén cerrados y el
  escritorio aguante uso diario estable. No es un requisito de 9 ni sustituye
  a `wp_fractional_scale_v1`. Ver `investigacion/escalado-macos.md`.

## No hacer todavía

- Pasar el compositor a Vulkan.
- Fork propio de Smithay.
- Copiar código de Mutter/GNOME Shell/gnome-session (GPL) o de los portales
  LGPL. Se leen ideas; se reimplementa en Rust.
- 8b/8c antes de que 9 y 11 estén cerrados.
