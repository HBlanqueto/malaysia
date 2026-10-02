# El lock (`yuki-shell lock`)

El cuarto componente nativo del shell (ADR 0023): la pantalla de bloqueo.
Usa **`ext-session-lock-v1`** (en el plan de ADR 0023 desde el principio) y
cumple su garantía central: **si el cliente muere mientras la sesión está
bloqueada, la sesión no se desbloquea** — así lo exige el protocolo
("If the client dies while the session is locked, the compositor must not
unlock the session in response", `ext-session-lock-v1`, wayland-protocols).

## Cómo funciona

```
yuki-shell lock ──► ext_session_lock_manager_v1.lock(surface)
        │               │
        │    locked     ▼  (el compositor confirma)
        ▼   ────────────►  crea una ext_session_lock_surface_v1 por output
        │                    │
        │   configure        ▼  (width, height, serial)
        │  ──────────────►  ack_configure + attach + commit (EGL → dmabuf)
        ▼                    ▼
     el compositor solo renderiza las lock surfaces (y el cursor)
```

1. **Pedir el lock**: `yuki-shell lock` enlaza `ext_session_lock_manager_v1` y
   manda `lock`. El compositor responde `locked` o `finished`.
2. **Crear la superficie**: en `locked`, crea una `ext_session_lock_surface_v1`
   por output (una superficie `wl_surface` nueva por output — un rol no se
   reutiliza). El compositor manda `configure` con el tamaño exacto.
3. **Dibujar**: sobre el fondo sólido, un mensaje centrado (la UI de
   autenticación — contraseña, passphrase, botón de desbloqueo — es trabajo
   futuro). Se presenta por **EGL** (Bloque 7c, ADR 0027) con un
   `wl_egl_window` por superficie (una por output). `ack_configure` antes del
   primer commit (sctk lo hace solo).
4. **Desbloquear**: solo la (futura) autenticación, con `unlock_and_destroy`.
   Este cliente jamás desbloquea por su cuenta.

## El lado del compositor (`crates/yuki/src/lock.rs`)

- **`SessionLockManagerState`** (smithay) + `SessionLockHandler`:
  - `lock()`: marca `State::locked`, limpia el foco de teclado (ningún cliente
    normal recibe teclas), pide redibujar y confirma. Mientras `locked`, la ruta
    de dibujo **solo renderiza las lock surfaces** (`session::draw`): ni
    ventanas, ni paneles, ni popups — y los frame callbacks solo van a ellas.
  - `new_surface()`: la configura con el tamaño del output, la añade a
    `State::lock_surfaces` y le da el foco de teclado a la primera (así el
    cliente de lock recibe las teclas para autenticar).
  - `unlock()`: solo se llega por el request explícito `unlock_and_destroy` del
    cliente. **Nunca** por la muerte del cliente.
- **La garantía de no desbloquear**: smithay no llama a `unlock` cuando muere
  el cliente. Si las lock surfaces desaparecen (cliente muerto), la ruta de
  dibujo cae al color sólido de fondo y la sesión **sigue bloqueada**. Un nuevo
  cliente `yuki-shell lock` puede volver a pedir el lock y hacerse cargo
  (recuperación que el protocolo permite).
- **Input mientras bloqueado** (`input.rs`): `pointer_press` ignora ventanas,
  popups, mover/redimensionar (solo se reenvía el evento al foco, la lock
  surface); `pointer_motion` enfoca siempre la lock surface en el origen;
  `focus_window` no cede foco a ventanas; `activate_window` (dock) se ignora.

## Divergencias

- **Sin autenticación todavía**: el lock bloquea, dibuja y recibe input, pero
  desbloquear (la autenticación) es trabajo futuro. El compositor no expone un
  desbloqueo por fuera del protocolo.
- **Foco de teclado sin clipboard**: al enfocar la lock surface no se tocan el
  clipboard ni la selección primaria: ningún cliente normal debe ver la
  selección bloqueado.
- **`locked` se confirma al momento**: smithay manda `locked` al confirmar; la
  primera frame bloqueada es el color sólido (el "blanking" del protocolo), así
  que ningún contenido normal es visible después del evento.
- El manager se anuncia a **todos** los clientes (`|_| true`); restringirlo al
  cliente lanzado por el compositor es política que se puede afinar.

## Aceptación (verificada en vivo)

- `wayland-info` anuncia `ext_session_lock_manager_v1`.
- `yuki-shell lock`: el compositor registra `session locked` y `new session
  lock surface`; el cliente sigue vivo sin errores de protocolo (el commit
  pasa la validación de tamaño de smithay: ack + dimensiones exactas).
- **Garantía**: `kill -9` al lock → **no aparece `session unlocked`**, el
  compositor y los demás componentes siguen intactos, y la sesión queda
  bloqueada con el color de respaldo.
- **Recuperación**: un nuevo `yuki-shell lock` estando bloqueado vuelve a
  crear la superficie (toma el control).
- **Input**: con la sesión bloqueada, una ventana que mapea no toma foco de
  teclado y no se renderiza.
- Todo verde: clippy `-D warnings`, fmt, 115 tests. Aceptación verificada además con EGL: el lock bloquea (`session locked`) y matar al cliente con `SIGKILL` **no desbloquea** la sesión.

## Relación

- ADR 0023 (procesos y protocolos, caso de falla "lock se queda bloqueado").
- `menubar.md` / `dock.md` / `wallpaper.md` — componentes hermanos.
- `session.rs` (dibujo bloqueado), `input.rs` (input bloqueado),
  `foreign.rs` (dock no activa ventanas bloqueado).