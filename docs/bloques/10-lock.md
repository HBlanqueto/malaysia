# Bloque 10 — Lock con autenticación

Estado: **implementado y aceptado en el panel real el 2026-10-21.**

## Alcance

- PAM en el proceso del lock (`yuki-shell lock`), no en el compositor. El
  servicio preferido es `/etc/pam.d/yuki-lock`; si no existe, cae a `login`.
- Campo de contraseña enmascarado, mensajes de error y estados de
  verificación.
- Auto-lock por `ext-idle-notify-v1` con `lock.idle_timeout_seconds`
  (300 s por defecto; `0` lo desactiva).
- `zwp_idle_inhibit_manager_v1`: una superficie inhibidora (mpv en pantalla
  completa) detiene el reloj y el auto-lock.
- Recuperación del lock si el cliente muere: el compositor sigue bloqueado y
  relanza `yuki-shell lock` con backoff 1/2/4/8/16 s, hasta 5 intentos.
- `Ctrl+Alt+L`: bloquea ahora, o relanza el lock si no hay superficie de lock
  viva. Reinicia el presupuesto de recuperación y nunca desbloquea.
- `Ctrl+Alt+Backspace` se ignora mientras la sesión está bloqueada.
- La sesión sigue bloqueada al cambiar de VT y volver.
- Garantía de no desbloqueo: si el cliente del lock muere, Smithay no llama a
  `unlock`; el compositor pinta un color sólido hasta que aparezca un cliente
  nuevo.

## Cómo probar

Sesión anidada:

```sh
./dev.sh run
# en otra terminal, con el WAYLAND_DISPLAY que imprima Yuki:
WAYLAND_DISPLAY=wayland-N yuki-shell lock
```

Sesión directa:

```sh
scripts/yuki-drm.sh lock
```

Para probar el auto-lock sin esperar 5 minutos, en
`~/.config/yuki/lock.domain`:

```toml
idle_timeout_seconds = 15
```

## Checklist de aceptación

- [x] Contraseña incorrecta: la sesión sigue bloqueada y se muestra el error.
- [x] Contraseña correcta: desbloquea y vuelve a la sesión.
- [x] Auto-lock a los 15 s sin input.
- [x] `mpv` en pantalla completa con `idle-inhibit` no dispara el lock.
- [x] Matar `yuki-shell lock` deja la sesión bloqueada.
- [x] El compositor relanza el lock con backoff 1/2/4/8/16 s.
- [x] Agotados los 5 intentos, la sesión sigue bloqueada.
- [x] `Ctrl+Alt+L` relanza el lock agotados los intentos.
- [x] `Ctrl+Alt+Backspace` no cierra Yuki con el lock activo.
- [x] `Ctrl+Alt+F2` y volver: la sesión sigue bloqueada.
- [x] El atajo del lock no llega al cliente con foco.
- [x] `/etc/pam.d/yuki-lock` existe en NixOS (el módulo lo crea); sin él, el
      fallback a `login` funciona.

## Evidencia en el log

```text
session locked
lock: starting PAM for <user>
lock: PAM refused: ...
session unlocked
lock client disappeared; scheduling recovery
lock recovery launched a new lock client
lock recovery gave up; the session stays locked
```

## Pendiente

- La aceptación manual del checklist anterior, en el panel real.
- Probar el ciclo completo de 5 relanzamientos y `Ctrl+Alt+L`.
- Confirmar que el stack PAM de NixOS (`security.pam.services.yuki-lock`) hace
  lo mismo que `login` en la máquina real.
