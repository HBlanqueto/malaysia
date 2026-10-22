# GNOME como referencia para Yuki

Estado: **investigación, no dependencia.** Este documento recoge qué leer de
GNOME, qué funciones concretas sirven de referencia y qué no se debe copiar.
No hay código de GNOME en el árbol de Yuki.

Revisado el 2026-10-21 contra las puntas de `mutter`, `gnome-shell`,
`gnome-session` y `xdg-desktop-portal-gnome`. Los clones son de referencia y
viven fuera del repo (por ejemplo en `/tmp/gnome-ref/`); no se vendorizan.

## Regla de licencias

| Repo | Licencia | Se puede copiar a Yuki (MIT) |
|---|---|---|
| `mutter` | GPL-2.0-or-later | No |
| `gnome-shell` | GPL-2.0-or-later | No |
| `gnome-session` | GPL-2.0-or-later | No |
| `xdg-desktop-portal-gnome` | LGPL-2.1-or-later | No |
| `xdg-desktop-portal-gtk` | LGPL-2.1-or-later | No |
| `gsettings-desktop-schemas` | LGPL-2.1-or-later | No |

Se leen **ideas, nombres de funciones y contratos**; se reimplementa en Rust.
Copiar código GPL/LGPL cambiaría la licencia de Yuki y obligaría a revisar todo
el árbol. Esto es distinto de Otto (MIT) y Smithay (MIT), que sí están en
`NOTICE`.

## Repositorios que importan

| Repo | Qué aporta | Utilidad para Yuki |
|---|---|---|
| `mutter` | compositor, KMS, outputs, protocolos Wayland | Bloques 8b, 8c, 9 y 11 |
| `gnome-shell` | shell, panel, dash, lock, notificaciones | UX y modelo de estados de `yuki-shell` |
| `gnome-session` | sesión, inhibidores, systemd/logind | integración de sesión y apagado |
| `xdg-desktop-portal-gnome` | Settings, ScreenCast, Screenshot | portal nativo futuro |
| `xdg-desktop-portal-gtk` | FileChooser, AppChooser, Print, Notification | fallback genérico |
| `gsettings-desktop-schemas` | nombres y defaults de opciones | catálogo de preferencias |

## 1. Salida, escala y KMS → Bloques 9, 8b y 8c

| Archivo/función de Mutter | Qué aprender | En Yuki |
|---|---|---|
| `src/backends/meta-monitor-manager.c:meta_monitor_manager_apply_monitors_config` | aplicar una configuración completa de monitores | futuro 8b |
| `...:meta_monitor_manager_calculate_monitor_mode_scale` | elegir escala según modo/tamaño | Bloque 9 |
| `...:meta_monitor_manager_update_logical_monitors` | reconstruir monitores lógicos tras hotplug | 8b |
| `...:meta_monitor_manager_ensure_initial_config` | configuración inicial al arrancar | 8b |
| `...:meta_monitor_manager_handle_get_resources` y `handle_apply_monitors_config` | API D-Bus de configuración de pantallas | futura app de ajustes |
| `src/backends/meta-logical-monitor.c:meta_logical_monitor_update` y `meta_logical_monitor_get_scale` | escala/transformación por monitor lógico | Bloque 9 (base hecha) |
| `src/backends/native/meta-kms-update.c:meta_kms_update_mode_set` | modeset atómico | `udev.rs` |
| `...:meta_kms_update_assign_plane` | asignar plano KMS | 8c |
| `...:meta_kms_update_merge_from` y `meta_kms_update_realize` | agrupar y enviar un update atómico | `DrmCompositor` |
| `...:meta_kms_device_process_update_sync` | cierre/flip y sincronización | vblank/commit |
| `src/backends/native/meta-onscreen-native.c:meta_onscreen_native_swap_buffers_with_damage` | presentar daño | `session.rs` |
| `...:meta_onscreen_native_supports_direct_scanout` | criterios de promoción | 8c |
| `...:meta_onscreen_native_direct_scanout` | poner el buffer del cliente directo al plano | 8c |
| `...:meta_onscreen_native_is_buffer_scanout_compatible` | formatos/modificadores válidos | 8c |

## 2. Composición, daño y frames → rendimiento

| Archivo/función | Qué aprender |
|---|---|
| `mutter/src/compositor/compositor.c:meta_compositor_sync_stack` | sincronizar apilado real con el orden de escena |
| `...:meta_compositor_sync_window_geometry` | geometría y repintado |
| `...:meta_compositor_before_paint` / `after_paint` | hooks de frame |
| `mutter/src/compositor/meta-compositor-view-native.c:meta_compositor_view_native_maybe_assign_scanout` | decidir si la vista va a scanout |
| `mutter/src/compositor/meta-window-actor.c:meta_window_actor_update_regions` | regiones opacas/daño |
| `...:meta_window_actor_is_opaque` | cuándo no hace falta mezclar |
| `...:meta_window_actor_get_scanout_candidate` | candidato a scanout |
| `...:meta_window_actor_has_damage` | daño pendiente |
| `mutter/clutter/clutter/clutter-frame-clock.c:clutter_frame_clock_schedule_update` | cuándo despertar la GPU |
| `...:clutter_frame_clock_notify_presented` | realimentación de presentación |
| `...:clutter_frame_clock_notify_discarded` | frame descartado |
| `...:clutter_frame_clock_set_mode` | modos de reloj según carga |

Esto es más útil que cambiar a Vulkan: el coste está en daño, orden de escena y
despertar la GPU.

## 3. Entrada, foco, idle y sesión

| Archivo/función | Qué aprender | En Yuki |
|---|---|---|
| `mutter/src/wayland/meta-wayland-seat.c:meta_wayland_seat_set_input_focus` | foco de entrada del seat | `input.rs` |
| `...:meta_wayland_seat_get_current_surface` | superficie bajo el foco | `input.rs` |
| `mutter/src/wayland/meta-wayland-keyboard.c:meta_wayland_keyboard_set_focus` | enter/leave y teclado | `input.rs` |
| `...:meta_wayland_keyboard_handle_event` | eventos y repetición | `input.rs` |
| `mutter/src/wayland/meta-wayland-pointer.c:meta_wayland_pointer_update` | motion/button/axis | `input.rs` |
| `...:meta_wayland_pointer_set_focus` / `send_enter` / `send_leave` | foco de puntero | `input.rs` |
| `...:meta_wayland_pointer_set_cursor_shape` | cursor-shape | Bloque 11 |
| `mutter/src/wayland/meta-wayland-data-device.c:meta_wayland_data_device_start_drag` | DnD | futuro |
| `...:meta_wayland_data_device_end_drag` | fin de drag | futuro |
| `...:meta_wayland_data_device_set_selection` | portapapeles | ya hecho |
| `mutter/src/backends/meta-input-settings.c:meta_input_settings_add_device` | aplicar ajustes al device | pendiente |
| `...:meta_input_settings_changed_cb` | cambio de ajustes en vivo | pendiente |
| `...:meta_input_settings_set_device_matrix` | touchpad/transform | pendiente |
| `mutter/src/backends/meta-idle-manager.c:meta_idle_manager_reset_idle_time` | reset por actividad | `idle.rs` |
| `mutter/src/backends/meta-idle-monitor.c:meta_idle_monitor_add_idle_watch` | watch de idle | `idle.rs` |
| `...:meta_idle_monitor_add_user_active_watch` | aviso al volver el usuario | `idle.rs` |
| `mutter/src/wayland/meta-wayland-idle-inhibit.c:meta_wayland_idle_inhibit_init` | inhibidores | ya hecho |
| `gnome-session/gnome-session/gsm-manager.c:gsm_manager_inhibit` | semántica de inhibit | unidades/user target |
| `...:gsm_manager_logout` | logout ordenado | futuro |
| `gnome-session/gnome-session/gsm-systemd.c:gsm_systemd_set_inhibitors` | pasar inhibidores a systemd/logind | futuro |
| `...:gsm_systemd_prepare_shutdown` | preparar apagado | futuro |
| `...:gsm_shell_open_end_session_dialog` | diálogo de fin de sesión | futuro |

## 4. Protocolos del Bloque 11

| Archivo/función | Qué aprender |
|---|---|
| `mutter/src/wayland/meta-wayland-fractional-scale.c:meta_wayland_fractional_scale_maybe_send_preferred_scale` | cuándo mandar `preferred_scale` |
| `mutter/src/wayland/meta-wayland-viewporter.c:meta_wayland_init_viewporter` | viewport y destino lógico |
| `mutter/src/wayland/meta-wayland-activation.c:meta_wayland_activation_token_create_new_resource` | tokens y seriales de activación |
| `mutter/src/wayland/meta-wayland-cursor-shape.c:meta_wayland_init_cursor_shape` | cursor-shape y fallback |
| `mutter/src/wayland/meta-wayland-pointer-constraints.c:meta_wayland_pointer_constraint_new` | lock/confine |
| `...:meta_wayland_pointer_constraint_maybe_enable` | activación por foco |
| `...:meta_wayland_pointer_constraint_calculate_effective_region` | región efectiva |
| `mutter/src/wayland/meta-wayland-presentation-time.c:meta_wayland_presentation_time_notify_frame_presented` | feedback de presentación |
| `...:meta_wayland_presentation_time_notify_frame_discarded` | frame descartado |
| `mutter/src/wayland/meta-wayland-transaction.c:meta_wayland_transaction_commit` | commit atómico con blockers |
| `...:meta_wayland_transaction_add_dma_buf_source` | esperar dmabuf |
| `...:meta_wayland_transaction_add_drm_syncobj_source` | explicit sync |
| `mutter/src/wayland/meta-wayland-dma-buf.c:meta_wayland_dma_buf_feedback_send` | feedback de dmabuf |
| `...:meta_wayland_dma_buf_try_acquire_scanout` | import para scanout |
| `...:meta_wayland_dma_buf_fds_for_wayland_buffer` | fds del buffer |
| `mutter/src/wayland/meta-wayland-linux-drm-syncobj.c:meta_wayland_surface_explicit_sync_validate` | validación de syncobj |
| `...:meta_wayland_sync_point_set` | punto de sincronización |
| `mutter/src/wayland/meta-wayland-outputs.c:meta_wayland_output_set_monitor` | `wl_output` por monitor |
| `...:meta_wayland_compositor_update_outputs` | anunciar/retirar outputs |

Mutter **no** implementa `ext-image-copy-capture`; GNOME captura con su portal y
`org.gnome.Mutter.ScreenCast`. Para el protocolo Wayland puro, niri/Smithay es
mejor referencia; para el portal, `xdg-desktop-portal-gnome/src/screencast.c` y
`gnomescreencast.c`.

## 5. Shell: UX, no código

| Archivo/clase de GNOME Shell | Qué copiar como idea | En Yuki |
|---|---|---|
| `js/ui/layout.js:LayoutManager._updateVisibility` y `_startupAnimationSession` | layout por monitor y arranque | `menubar.rs`, `dock.rs` |
| `js/ui/windowManager.js:_mapWindow`, `_destroyWindow`, `_minimizeWindow`, `_maximizeWindow`, `_switchWorkspace` | ciclo de ventana y workspace | `window.rs`, `yuki-scene` |
| `js/ui/screenShield.js:ScreenShield.lock` | bloqueo e idle | `lock.rs` |
| `js/ui/unlockDialog.js:UnlockDialog._updateVisibility` | foco del campo y estados | `lock.rs` |
| `js/ui/panel.js:Panel` | barra superior | `menubar.rs` |
| `js/ui/dash.js:Dash` | dock | `dock.rs` |
| `js/ui/messageTray.js:MessageTray` | notificaciones | pendiente |
| `js/ui/notificationDaemon.js:NotificationDaemon` | D-Bus de notificaciones | pendiente |
| `js/ui/osdWindow.js`, `quickSettings.js`, `calendar.js` | OSD, ajustes rápidos, calendario | futuros componentes |

No se copia el código JS/Clutter; solo el modelo de estados y la UX.

## 6. Portales: `xdg-desktop-portal-gtk` frente a uno propio

### Cómo funciona

`xdg-desktop-portal` es el **frontend** al que llaman las apps
(`org.freedesktop.portal.*`). El frontend elige un **backend**
(`org.freedesktop.impl.portal.*`) según `portals.conf` y
`XDG_CURRENT_DESKTOP`. No hay que implementar todos los portales: se puede
enrutar cada interfaz a un backend distinto.

En NixOS, `xdg.portal.config.yuki` escribe
`/etc/xdg/xdg-desktop-portal/yuki-portals.conf`, y `xdg.portal.extraPortals`
instala los backends.

### Qué cubre cada uno

| Interfaz | `-gtk` | `-hyprland` | `-gnome` | Yuki |
|---|---|---|---|---|
| FileChooser, AppChooser, Print, Notification, Access | Sí | No (usa GTK) | Parcial | GTK |
| Settings / apariencia | Sí, desde GSettings | No | Sí, desde GNOME | **propio** |
| Portapapeles de capturas | Sí (`St.Clipboard` + `MetaSelection`) | No | Sí | **propio** (ver [`portapapeles-capturas.md`](portapapeles-capturas.md)) |
| Screenshot | No | Sí | Sí | **propio** (Bloque 11) |
| ScreenCast | No | Sí | Sí | **propio** (Bloque 11) |
| Inhibit | Sí (limitado) | No | Sí | propio o GTK |
| GlobalShortcuts | No | Sí | Sí | futuro |
| InputCapture | No | Sí | Sí | futuro |
| RemoteDesktop | No | No | Sí | futuro |

Hyprland es el ejemplo claro: usa `xdg-desktop-portal-hyprland` para
**ScreenCast, Screenshot, GlobalShortcuts e InputCapture**, y deja el resto
(FileChooser, etc.) al portal GTK. No reimplementa lo genérico. Su portal es
BSD-3-Clause, pero su lógica está atada a los protocolos/IPC de Hyprland; sirve
como referencia de arquitectura, no como código portable a Yuki.

### Beneficios de un `xdg-desktop-portal-yuki`

1. **ScreenCast sin `wlr-screencopy`.** Yuki ya eligió
   `ext-image-copy-capture-v1` + `ext-image-capture-source-v1` (Bloque 11). Un
   portal propio usa ese camino; el de GTK no implementa ScreenCast y el de wlr
   exige protocolos wlroots que Yuki no tiene.
2. **Cero copias a CPU.** Captura por dmabuf/EGL y entrega a PipeWire con
   modificadores y fences; evita readbacks y respeta explicit sync.
3. **Apariencia desde `.domain`.** `org.freedesktop.appearance` (dark/light,
   accent, contraste) sale de nuestro store. El portal GTK lo lee de GSettings,
   que Yuki no usa; sin portal propio habría dos fuentes de verdad.
4. **Permisos y diálogos nativos.** El portal puede preguntar con la shell de
   Yuki, recordar concesiones por app y limitar la captura a una ventana o
   monitor concreto, en vez de un diálogo GTK ajeno.
5. **Inhibit/GlobalShortcuts/InputCapture** integrados con el compositor
   (idle, teclas, EIS/libei), no con logind o un protocolo de otro compositor.
6. **Rendimiento y latencia.** Un portal propio puede capturar la superficie
   correcta del scene graph y reutilizar texturas; los portales genéricos suelen
   pasar por screencopy/shm.
7. **Seguridad.** La sesión bloqueada, las ventanas marcadas como sensibles y
   las restricciones por app se aplican en el mismo sitio que la captura.
8. **NixOS limpio.** `xdg.portal.config.yuki` enruta solo las interfaces
   propias a Yuki y deja FileChooser/Print/Notification a GTK; no se arrastra
   GSettings ni GTK al escritorio.

### Coste

- Implementar los objetos D-Bus de sesión/request (`CreateSession`,
  `SelectSources`, `Start`, `Close`, señales `Response`).
- PipeWire: negociación de formato, modificadores, dmabuf, fences y ciclo de
  vida de los buffers.
- Diálogo de permisos en la shell y almacén de concesiones/restore tokens.
- Captura de monitor/ventana/región, cursores y multi-monitor.
- Pruebas con Firefox, Chromium, OBS y Flatpak.
- Mantener versiones de protocolo.

No hace falta implementar todo. El orden recomendado:

1. **Fase 0 (ahora):** `xdg-desktop-portal-gtk` para FileChooser, AppChooser,
   Print, Notification, Access y Lockdown. Sin ScreenCast.
2. **Fase 1:** portal propio solo para **Settings** (`org.freedesktop.appearance`:
   color-scheme, accent-color, contraste, `SettingChanged`). Es pequeño y
   desbloquea dark/light en Firefox/GTK sin GSettings.
3. **Fase 2:** portal propio para **Screenshot** y **ScreenCast** sobre
   `ext-image-copy-capture` + PipeWire; enrutar esas interfaces a Yuki y el
   resto a GTK.
4. **Fase 3:** Inhibit, GlobalShortcuts y, si se quiere, RemoteDesktop/
   InputCapture y notificaciones nativas.

### Recomendación

Sí merece la pena un `xdg-desktop-portal-yuki`, pero **no como sustituto de
GTK**, sino como backend especializado para lo que depende del compositor:
Settings, Screenshot y ScreenCast. Mantener GTK para lo genérico es menos
código, más estándar y evita reimplementar diálogos de ficheros o impresión.

## Tareas derivadas (por hacer)

- [ ] Bloque 9: leer `meta_monitor_manager_calculate_monitor_mode_scale` y
      `meta_logical_monitor_update`; la base ya está en `State::scale`; falta panel y configuración.
- [ ] Bloque 8c: comparar `meta_kms_update_assign_plane` y
      `meta_onscreen_native_direct_scanout` con el `DrmCompositor` de Yuki.
- [ ] Bloque 11: usar Mutter como referencia para fractional scale, activation,
      cursor-shape, pointer constraints y presentation.
- [x] Bloque 11: implementar `ext-image-copy-capture` +
      `ext-image-capture-source` antes del portal de captura (captura de salida
      hecha; falta la de ventana).
- [ ] Portal Fase 1: `xdg-desktop-portal-yuki` con `Settings` y
      `org.freedesktop.appearance`.
- [ ] Portal Fase 2: `ScreenCast`/`Screenshot` con PipeWire; configurar
      `xdg.portal.config.yuki` para enrutar solo esas interfaces.
- [ ] NixOS: añadir `services.graphical-desktop`, `security.polkit`,
      `xdg.portal`, keyring y AT-SPI con `mkDefault`.
- [ ] Shell: notificaciones, agente polkit y accesibilidad AT-SPI (post-Bloque 11).

## Qué no merece la pena leer

Clutter/Cogl, el motor de render de Mutter, GNOME Shell en JS, dconf/GSettings
como implementación, XWayland, Evolution/Tracker y los servicios
`core-os-services`. Son grandes y no encajan con Smithay + Rust + shell nativa.

## Clones de referencia

No se clonan dentro del repo. Para leer:

```sh
git clone --depth 1 --filter=blob:none \
  https://gitlab.gnome.org/GNOME/mutter.git /tmp/gnome-ref/mutter
git clone --depth 1 --filter=blob:none \
  https://gitlab.gnome.org/GNOME/gnome-shell.git /tmp/gnome-ref/gnome-shell
git clone --depth 1 --filter=blob:none \
  https://gitlab.gnome.org/GNOME/gnome-session.git /tmp/gnome-ref/gnome-session
git clone --depth 1 --filter=blob:none \
  https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome.git \
  /tmp/gnome-ref/xdg-desktop-portal-gnome
git clone --depth 1 --filter=blob:none \
  https://github.com/flatpak/xdg-desktop-portal-gtk.git \
  /tmp/gnome-ref/xdg-desktop-portal-gtk
git clone --depth 1 --filter=blob:none \
  https://github.com/hyprwm/xdg-desktop-portal-hyprland.git \
  /tmp/gnome-ref/xdg-desktop-portal-hyprland
```

Los commits concretos que se revisaron están en el historial de este documento;
si se actualiza la referencia, se anota la nueva revisión y qué cambió.
