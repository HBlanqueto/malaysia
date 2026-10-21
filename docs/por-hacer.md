# Por hacer — plan de trabajo

Última actualización: 2026-10-21.

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

`foot` sigue siendo el cliente guionizable de `dev.sh smoke` porque negocia
server-side; `wezterm` es el cliente manual de aceptación.

## Ahora: cerrar Bloque 9

Hecho: `State::scale`, `YUKI_SCALE`, `wl_output` + fractional-scale, render,
cursor y viewporter de la shell; ver `bloques/09-escala-fraccional.md`.

Falta:

- [ ] Aceptación en panel real a 1.25 y 1.5 con `wezterm`, shell y puntero.
- [ ] Sustituir `YUKI_SCALE` por una opción de configuración persistente.
- [ ] Escala por salida/monitor cuando 8b introduzca varios outputs.
- [ ] Redondeo de rectángulos lógicos (ADR 0004) a escala no entera.

## Después, en orden

1. **Bloque 11 — protocolos de uso diario.** `wp_presentation` ya está hecho;
   faltan `xdg-activation`, `cursor-shape`, `pointer-constraints` y captura
   (`ext-image-copy-capture` + `ext-image-capture-source`). Ver
   `bloques/11-protocolos-de-uso-diario.md`.
2. **Bloque 12a — portal nativo.** Primero `Settings` /
   `org.freedesktop.appearance`; después `Screenshot` y `ScreenCast` sobre la
   captura del Bloque 11 y PipeWire. GTK sigue sirviendo FileChooser, Print y
   Notification.
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
