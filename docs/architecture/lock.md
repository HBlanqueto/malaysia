# The lock (`malaysia-shell lock`)

> Overview map: [`interface.md`](interface.md).

Native component (ADR 0023) on `ext-session-lock-v1`. Guarantee: **if the client dies while the session is locked, the session does not unlock**.

## How it works

```
lock() ──► locked ──► one ext_session_lock_surface_v1 per output
          │ configure (width, height, serial)
          ▼
   ack_configure + attach + commit through the kit (Skia on Vulkan)
          ▼
   the compositor renders only lock surfaces and cursor
```

1. `lock` requests the session and the compositor confirms `locked`.
2. On `locked`, the client creates a new surface per output (a role is not reused) and receives the exact size.
3. It draws a solid background, a masked field, a «verifying» state and the PAM error; the kit presents it with Vulkan (barrierkit ADR 0056).
4. Only a successful PAM call triggers `unlock_and_destroy`; the client never unlocks on its own.

## Recovery

Smithay leaves the lock in `Defunct` (not in `Unlocked`) and a second `lock()` can take ownership; Malaysia keeps `locked == true`.

`lock.rs`:

- a destruction hook on each lock surface; if the last one disappears while the session is locked, it schedules recovery;
- a 3 s watchdog when a lock is confirmed, in case the client dies before creating its first surface;
- clock and launch through the same `TimerSink`/`spawn::shell_component` as `idle.rs`;
- backoff of 1, 2, 4, 8 and 16 s, at most 5 attempts. Once exhausted, the session stays locked in a solid color and a manual `malaysia-shell lock` is accepted;
- neither recovery nor a second lock calls `unlock`.

## PAM and auto-lock

Authentication lives in the lock process (FFI to `libpam`, `malaysia-pam` thread); service `malaysia-lock` or `login`, password in `Zeroizing<String>`, and only a success calls `unlock_and_destroy`. `ext-idle-notify-v1` resets the clock on each input and `lock.idle_timeout_seconds` (default 0, disabled) launches the lock; `zwp_idle_inhibit_manager_v1` holds it back. With auto-lock active, the DRM backend takes a logind `delay` inhibitor and, on receiving `PrepareForSleep(true)`, launches the lock and releases the inhibitor once the session is locked (at most 4 s). It lives in `sleep.rs`.

## Compositor side

- `SessionLockHandler::lock()` sets `State::locked`, clears the keyboard focus, confirms and redraws. While `locked`, the draw renders only lock surfaces and frame callbacks go only to them and to the cursor surface. Keys go straight to the lock surface, bypassing an input method's grab, so it cannot read the password.
- `new_surface()` configures it to the output size, adds it to `lock_surfaces` and gives keyboard focus to the first one.
- `unlock()` only arrives through `unlock_and_destroy`; never through the client's death.
- With the session locked, `pointer_press` ignores windows/popups, `focus_window` does not yield focus and the dock's `activate_window` is ignored. Authentication belongs to the client; the compositor does not know PAM.
