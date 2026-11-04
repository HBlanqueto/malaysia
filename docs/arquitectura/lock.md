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

Smithay deja el lock en `Defunct` (no en `Unlocked`) y un segundo `lock()` puede tomar la propiedad; Malaysia mantiene `locked == true`.

`lock.rs`:

- destruction hook en cada lock surface; si desaparece la última con la sesión bloqueada, programa la recuperación;
- watchdog de 3 s al confirmar un lock, por si el cliente muere antes de crear su primera superficie;
- reloj y arranque por el mismo `TimerSink`/`shell_command` que `idle.rs`;
- backoff 1, 2, 4, 8 y 16 s, máximo 5 intentos. Agotados, la sesión sigue bloqueada en color sólido y un `malaysia-shell lock` manual se acepta;
- ni la recuperación ni un segundo lock llaman a `unlock`.

## PAM y auto-lock

La autenticación vive en el proceso del lock (FFI a `libpam`, hilo `malaysia-pam`); servicio `malaysia-lock` o `login`, contraseña en `Zeroizing<String>`, y solo un acierto llama a `unlock_and_destroy`. `ext-idle-notify-v1` reinicia el reloj en cada input y `lock.idle_timeout_seconds` (default 0, desactivado) lanza el lock; `zwp_idle_inhibit_manager_v1` lo frena.
## Lado del compositor

- `SessionLockHandler::lock()` marca `State::locked`, limpia el foco de teclado, confirma y redibuja. Mientras `locked`, el draw solo renderiza lock surfaces y los frame callbacks solo van a ellas y a la superficie del cursor.
- `new_surface()` configura al tamaño del output, añade a `lock_surfaces` y da el foco de teclado a la primera.
- `unlock()` solo llega por `unlock_and_destroy`; nunca por la muerte del cliente.
- Con la sesión bloqueada, `pointer_press` ignora ventanas/popups, `focus_window` no cede foco y `activate_window` del dock se ignora. La autenticación es del cliente; el compositor no conoce PAM.
