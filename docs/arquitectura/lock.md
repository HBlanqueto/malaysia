# El lock (`malaysia-shell lock`)

> Mapa general: [`interfaz.md`](interfaz.md).

Componente nativo (ADR 0023) sobre `ext-session-lock-v1`. Garantía: **si el cliente muere con la sesión bloqueada, la sesión no se desbloquea**.

## Cómo funciona

```
lock() ──► locked ──► una ext_session_lock_surface_v1 por output
          │ configure (ancho, alto, serial)
          ▼
   ack_configure + attach + commit por EGL/dmabuf
          ▼
   el compositor solo renderiza lock surfaces y cursor
```

1. `lock` pide la sesión y el compositor confirma `locked`.
2. En `locked`, el cliente crea una superficie nueva por output (un rol no se reutiliza) y recibe el tamaño exacto.
3. Dibuja fondo sólido, campo enmascarado, estado «verificando» y error de PAM; presenta por EGL (ADR 0027).
4. Solo un PAM correcto llama a `unlock_and_destroy`; el cliente nunca desbloquea por su cuenta.

## Recuperación

Smithay deja el lock en `Defunct` (no en `Unlocked`) y un segundo `lock()` puede tomar la propiedad; MalaysiaDreams mantiene `locked == true`.

`lock.rs`:

- destruction hook en cada lock surface; si desaparece la última con la sesión bloqueada, programa la recuperación;
- watchdog de 3 s al confirmar un lock, por si el cliente muere antes de crear su primera superficie;
- reloj y arranque por el mismo `TimerSink`/`shell_command` que `idle.rs`;
- backoff 1, 2, 4, 8 y 16 s, máximo 5 intentos. Agotados, la sesión sigue bloqueada en color sólido y un `malaysia-shell lock` manual se acepta;
- ni la recuperación ni un segundo lock llaman a `unlock`.

## PAM

La autenticación vive en el proceso del lock, no en el compositor. Adaptación del `pam.rs` de Otto (MIT; ver `NOTICE`): FFI a `libpam`, sin crate intermedio. `pam_authenticate` corre en un hilo `malaysia-pam`; el render sigue.

- Contraseña en `Zeroizing<String>`; copias temporales también se borran.
- Servicio `malaysia-lock`; si no existe, `login` (se registra).
- En NixOS: `security.pam.services.malaysia-lock = { };`. En otras distribuciones, `pam_unix.so` sin ruta absoluta (libpam busca en su `LIBPAM_MODDIR`).
- Un fallo muestra el texto de PAM y deja reintentar; un acierto llama a `unlock_and_destroy`, hace `flush` y termina el proceso.

## Bloqueo automático

`ext-idle-notify-v1` se anuncia en el compositor. Cada input reinicia el reloj; `lock.idle_timeout_seconds` (default 300; 0 desactiva) decide cuándo lanzar `malaysia-shell lock`. `zwp_idle_inhibit_manager_v1` se respeta: con un inhibidor vivo no se arman idle ni auto-lock, y la lista se limpia al morir la superficie.

## Lado del compositor

- `SessionLockHandler::lock()` marca `State::locked`, limpia el foco de teclado, confirma y redibuja. Mientras `locked`, el draw solo renderiza lock surfaces y los frame callbacks solo van a ellas.
- `new_surface()` configura al tamaño del output, añade a `lock_surfaces` y da el foco de teclado a la primera.
- `unlock()` solo llega por `unlock_and_destroy`; nunca por la muerte del cliente.
- Con la sesión bloqueada, `pointer_press` ignora ventanas/popups, `focus_window` no cede foco y `activate_window` del dock se ignora.

## Divergencias y verificación

- La autenticación es del cliente; el compositor no conoce PAM. Al enfocar la lock surface no se toca el portapapeles ni la selección primaria. `locked` se confirma al momento; la primera frame es el color sólido. El manager se anuncia a todos los clientes; restringirlo al lanzado por el compositor es una política futura.
- `wayland-info` anuncia `ext_session_lock_manager_v1`; `malaysia-shell lock` registra `session locked` y `new session lock surface`.
- `kill -9` al lock no produce `session unlocked`, el compositor sigue y la sesión queda bloqueada con el color de respaldo. Un `malaysia-shell lock` nuevo recupera la superficie, y una ventana nueva no toma foco ni se renderiza.
- Prueba manual: contraseña mala/buena, auto-lock y mpv fullscreen con inhibidor.
