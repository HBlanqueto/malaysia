# Bloque 8a — Direct Session mínima

Estado: **implementado y aceptado en el panel real del ThinkPad el
2026-10-21.** Las pruebas manuales de cliente se hacen con `wezterm`; `foot`
sigue siendo el cliente guionizable del smoke test porque negocia
server-side y `wezterm` no.

## Alcance

- DRM/KMS atómico, libseat, libinput.
- GBM + EGL + `GlesRenderer`, el mismo renderer que winit.
- `DrmCompositor` sobre el conector interno (eDP), en su modo preferido.
- **Un solo plano primario**: sin overlay ni cursor planes. El puntero se
  compone como un elemento.
- Un frame por vblank, con damage tracking de `DrmCompositor`.
- Cambio de VT (`Ctrl+Alt+F1..F12`) con pausa/restauración.
- La sesión bloqueada sigue bloqueada al cambiar de VT y volver.
- Winit sigue siendo el backend de desarrollo; comparte `State`, protocolos,
  escena, entrada y bloqueadores de dmabuf.

Fuera de 8a: hotplug, monitores externos, escala fraccional real, direct
scanout, XWayland y spawns. Ver la división 8a/8b/8c en
[`../roadmap.md`](../roadmap.md).

## Cómo arrancar

Desde un TTY, sin `nix develop`, con el binario empaquetado:

```sh
nix build .#default
./result/bin/yuki --drm --log-level=info
```

El archivo de sesión ya apunta a `@out@/bin/yuki --drm`, así que un display
manager arranca la sesión directa sin flags extra.

## Shell: units y arranque

Tras crear el socket, `--drm` importa `WAYLAND_DISPLAY` y
`XDG_CURRENT_DESKTOP` al gestor de usuario y arranca `yuki-session.target`
con `systemctl --user`. Al salir, detiene el target. Si el target no existe,
el compositor sigue funcionando sin shell y registra cómo instalar las units.

Forma normal: el módulo `nixosModules.default` del flake
(`programs.yuki.enable = true`) instala el paquete, registra la sesión en el
display manager, crea `/etc/pam.d/yuki-lock` y define `yuki-session.target` con
un proceso por componente. No hacen falta enlaces manuales.

Para probar a mano sin el módulo, enlazando las units de
`result/lib/systemd/user/` desde la raíz del repo (`systemctl --user link` exige
rutas absolutas):

```sh
systemctl --user link "$PWD/result/lib/systemd/user/yuki-session.target"
systemctl --user link "$PWD/result/lib/systemd/user/yuki-shell@.service"
systemctl --user daemon-reload
systemctl --user enable yuki-shell@wallpaper.service \
  yuki-shell@menubar.service yuki-shell@dock.service
```

El target arranca las instancias habilitadas; `yuki-session.target` por sí
solo no las conoce. `enable` crea los enlaces en `yuki-session.target.wants`;
no uses `--now` desde GNOME, porque arrancaría la shell en la sesión
equivocada.

## Script de lanzamiento

`scripts/yuki-drm.sh` automatiza el setup, el arranque desde un TTY, los
clientes, el lock y la salida de emergencia:

```sh
scripts/yuki-drm.sh setup          # desde GNOME: build + units
scripts/yuki-drm.sh run            # desde un TTY: yuki --drm
scripts/yuki-drm.sh client wezterm # desde otro TTY: cliente en Yuki
scripts/yuki-drm.sh lock           # desde otro TTY: bloquea
scripts/yuki-drm.sh status         # pid, socket y log
scripts/yuki-drm.sh stop           # mata Yuki, también con lock
```

## Salida de emergencia

- **`Ctrl+Alt+Backspace`** cierra el compositor **solo si la sesión no está
  bloqueada**. Con el lock activo se ignora: la salida es cambiar de VT y
  matar Yuki desde otra TTY. El `comm` del binario envuelto es
  `.yuki-wrapped`: `pkill -TERM -x .yuki-wrapped`; con el binario de
  desarrollo es `pkill -TERM -x yuki`.
- `Ctrl+Alt+F1..F12` y `Ctrl+Alt+Backspace` los consume Yuki: no llegan al
  cliente con foco.
- También valen `SIGTERM`/`SIGINT` desde otra TTY o el cierre normal de la
  sesión.
- Para volver a GDM: desde otra TTY, `pkill -f .yuki-wrapped`; después
  `Ctrl+Alt+F1` o `sudo systemctl restart display-manager`. No hace falta
  tocar `~/nixdots`.

## Prueba aislada con vkms

`./dev.sh drm-vkms` prueba el backend directo sin tocar el panel ni la entrada
real. Necesita `sudo` no interactivo (`sudo -v` antes) y `ffmpeg`. Carga
`vkms` si hace falta, busca su `cardN` por el driver/sysfs de vkms (nunca por
número),
arranca Yuki con `LIBSEAT_BACKEND=noop`, `--drm-device <vkms>` y `--no-input`,
conecta `pattern_live`, pide la captura por `SIGUSR1` y verifica los cuatro
cuadrantes. El `trap` mata Yuki y descarga `vkms` aunque falle. También lee
`/proc/<pid>/fd` para comprobar que no se abrió otro `cardN` ni `/dev/input`.

Cubre: modeset atómico, GBM/EGL en el render node de amdgpu, import dma-buf
para scanout en vkms, vblank, frames con daño y la captura de depuración.

No cubre: el panel eDP real, el touchpad/libinput, el cambio de VT ni la
pausa/reanudación de libseat, el lock + VT, ni el target del shell (se omite a
propósito en esta prueba). Eso sigue en el checklist del panel real.

## Checklist de aceptación

- [x] Desde un TTY, `./result/bin/yuki --drm` arranca sobre el panel interno.
- [x] Se ven wallpaper, menubar y dock.
- [x] `wezterm` abre y acepta teclado. Sin launcher: desde otro TTY, con el
      mismo usuario, `WAYLAND_DISPLAY=wayland-1 wezterm &` (confirma el nombre
      que imprime Yuki) y vuelve al VT de Yuki.
- [x] Firefox abre acelerado.
- [x] El touchpad mueve el puntero; tap-to-click y scroll natural usan los
      defaults de libinput.
- [x] `Ctrl+Alt+F2` y volver: la imagen y la entrada siguen intactas; no se
      filtran fds.
- [x] Bloquear, cambiar de VT y volver: la sesión sigue bloqueada.
- [x] Con el lock activo, `Ctrl+Alt+Backspace` no cierra Yuki; el atajo no
      llega al cliente.
- [x] Matar el cliente del lock no desbloquea.
- [x] Agotados los 5 relanzamientos, `Ctrl+Alt+L` relanza el lock.
- [x] Winit sigue igual: `dev.sh smoke`, `dev.sh block4`, `dev.sh move`,
      `dev.sh decoration`.
- [x] `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] `nix build .#default`.
- [ ] `cargo deny check licenses` (no está instalado en el dev shell actual).

La secuencia completa, la evidencia que hay que guardar y el estado de cada
prueba están en [`../por-hacer.md`](../por-hacer.md).

## Notas para la prueba manual

- El conector debe ser el interno. Si no hay eDP conectado, 8a usa el primer
  conector conectado; si no hay ninguno, falla al arrancar.
- No hay hotplug: conectar o desconectar un monitor externo no se atiende.
- El renderer es GLES; no hay camino Vulkan ni fallback legacy.
- El lock no necesita un cliente nuevo para probar la garantía: se puede
  bloquear con `yuki-shell lock`, cambiar de VT y volver.
- El movimiento relativo (touchpad y TrackPoint) no lo cubría vkms: la prueba
  usa `--no-input`. Ahora `InputEvent::PointerMotion` suma el delta a la
  posición actual y limita el puntero al output.
- El keymap sale de `org.freedesktop.locale1` (`X11Layout`, `X11Model`,
  `X11Variant`, `X11Options`, lo mismo que `localectl status`). Las variables
  `XKB_DEFAULT_*` tienen prioridad como override explícito.
- `Ctrl+Alt+L` bloquea o relanza el lock; `Ctrl+Alt+Backspace` no cierra Yuki
  con el lock activo.
- Las apps de instancia única (Nautilus) se activan en GNOME si ya están
  abiertas. Para probarlas dentro de Yuki, usa
  `dbus-run-session -- nautilus --no-desktop` (o cierra la instancia de GNOME).
