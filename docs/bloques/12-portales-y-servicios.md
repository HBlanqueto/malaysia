# Bloque 12 — Portales y servicios de escritorio

Estado: **12a en curso.** El portal `Settings` está implementado; faltan
`Screenshot`/`ScreenCast` y el Bloque 12b.

## 12a — `xdg-desktop-portal-yuki`

Backend propio (ADR 0031), en un proceso aparte que arranca
`xdg-desktop-portal` por activación D-Bus. No es un cliente Wayland ni forma
parte de la shell; ver
[`../arquitectura/modelo-de-procesos.md`](../arquitectura/modelo-de-procesos.md):

```
app (GTK/Qt/Electron)
  │ org.freedesktop.portal.Settings      (frontend)
  ▼
xdg-desktop-portal
  │ org.freedesktop.impl.portal.Settings (backend, config yuki)
  ▼
xdg-desktop-portal-yuki
  │ lee (nunca escribe) ~/.config/yuki/appearance.domain
  ▼
org.freedesktop.appearance: color-scheme, accent-color, contrast, reduced-motion
```

- **Nombre y objeto**: `org.freedesktop.impl.portal.desktop.yuki`,
  `/org/freedesktop/portal/desktop`, interfaz
  `org.freedesktop.impl.portal.Settings` versión 2.
- **Valores**: `color-scheme` (`1` dark, `2` light; `auto` se resuelve a light
  con el mismo `ColorScheme::resolved()` que usa el compositor), `accent-color`
  (`(ddd)` en sRGB, del `appearance.accent_color` o el accent del palette de
  `yuki-design`), `contrast` y `reduced-motion` (`0`/`1`).
- **En vivo**: sondea el mtime de `appearance.domain` y `wallpaper.state` cada
  250 ms y emite `SettingChanged` por cada clave que cambió. Un fichero ausente
  vuelve a los defaults; uno mal formado conserva los últimos valores y avisa.
- **Errores**: namespace o clave desconocidos devuelven
  `org.freedesktop.portal.Error.NotFound`.
- **Configuración**: `share/xdg-desktop-portal/portals/yuki.portal` con
  `UseIn=Yuki`; el módulo NixOS escribe `xdg.portal.config.yuki` con `Settings`
  → `yuki` y `default` → `gtk`, e instala `xdg-desktop-portal-gtk` como
  fallback.
- **Sin diálogo de permiso**: es lectura de preferencias; `Screenshot` y
  `ScreenCast` sí lo llevarán.

### Archivos

| Pieza | Archivo |
|---|---|
| Backend y watcher | `crates/xdg-desktop-portal-yuki/src/settings.rs` |
| Arranque D-Bus | `crates/xdg-desktop-portal-yuki/src/main.rs` |
| Registro del portal | `share/xdg-desktop-portal/portals/yuki.portal` |
| Activación D-Bus | `share/dbus-1/services/org.freedesktop.impl.portal.desktop.yuki.service` |
| Empaquetado | `flake.nix` (instala portal, `.portal` y `.service`) |
| Módulo NixOS | `nix/yuki.nix` (`xdg.portal`) |

### Verificación

En el dev shell, con un bus de sesión aislado:

```sh
nix develop
cargo build -p xdg-desktop-portal-yuki
YUKI_CONFIG_DIR=/tmp/yuki-cfg dbus-run-session -- sh -c '
  ./target/debug/xdg-desktop-portal-yuki &
  sleep 0.5
  gdbus call --session --dest org.freedesktop.impl.portal.desktop.yuki \
    --object-path /org/freedesktop/portal/desktop \
    --method org.freedesktop.impl.portal.Settings.ReadAll "[]"
'
```

Comprobado:

- `ReadAll` devuelve `org.freedesktop.appearance` con los cuatro valores.
- `Read` de `color-scheme` y `accent-color` devuelve los tipos correctos
  (`u` y `(ddd)`); una clave desconocida da `NotFound`.
- Al cambiar `color_scheme` y `contrast` en `appearance.domain`, llegan
  `SettingChanged` de las dos claves (y del accent si cambia el palette).
- El binario empaquetado se activa por D-Bus con `$out/share` en
  `XDG_DATA_DIRS` (servicio `org.freedesktop.impl.portal.desktop.yuki`).

Pendiente de aceptación en la sesión real: que Firefox/GTK lean el tema desde
el frontend `xdg-desktop-portal` y reaccionen a `SettingChanged`.

### Pendiente de 12a

- `org.freedesktop.impl.portal.Screenshot` sobre `ext-image-copy-capture-v1`:
  captura por defecto al Escritorio XDG (`yuki::xdg::desktop_dir()`), URI de
  vuelta, opción de portapapeles.
- `org.freedesktop.impl.portal.ScreenCast` con PipeWire (dmabuf, modificadores
  y fences), reutilizando la captura del Bloque 11.
- Diálogos de permiso de la shell y concesión por app.
- Aceptación: Firefox/Chromium comparten ventana y monitor; OBS recibe el
  stream; dark/light cambia sin reiniciar; el portal GTK sigue sirviendo
  FileChooser.

## 12b — servicios de escritorio

Notificaciones (`org.freedesktop.Notifications`), agente polkit,
keyring/Secret Service y AT-SPI. Autostart por `xdg-desktop-autostart` si no lo
cubre el módulo de NixOS. Sin empezar.

## Transversal NixOS

El módulo `programs.yuki.enable` ya instala el paquete, la sesión, PAM,
`hardware.graphics`, las units de la shell y, desde este bloque, el portal de
Yuki con GTK de fallback. Faltan las opciones de `mkDefault` para
`services.graphical-desktop`, `security.polkit`, keyring/AT-SPI y una VM que
arranque `yuki-session.target`.
