# The lock (`malaysia-shell lock`)

> Overview map: [`interface.md`](interface.md). What the lock screen shows and why: ADR [0074](../decisions/0074-lock-screen-and-lock-transition.md).

Native component (ADR 0023) on `ext-session-lock-v1`. Guarantee: **if the client dies while the session is locked, the session does not unlock**.

## How it works

```
lock() ──► input cut, menu bar closes up, desktop fades under the wallpaper
          │ one ext_session_lock_surface_v1 per output, configured and drawn
          ▼
   fade + settle over, lock surfaces have buffers (or 3 s passed)
          ▼
   menu bar hidden, lock surfaces shown over the wallpaper in one frame
          ▼
   that frame presented ──► locked
```

1. `lock` asks for the session. The compositor cuts input and captures at once, sends `infinity_session_v1.locking` and starts the fade.
2. The client creates a surface per output (a role is not reused) and draws it; nothing of it shows yet.
3. The compositor shows the lock surfaces once the fade and the menu bar's settle are over and every surface has a buffer, or after 3 s. The menu bar stays on screen above them. It sends `locked` once that frame is presented.
4. On `locked` (`SessionUnlock::locked()`), the client fades in its power button, raises the date and time and fades in the user and the field.
5. Only a successful PAM call ends the lock: the client fades out its content in `DURATION_FAST_MS` and then calls `unlock_and_destroy`; it never unlocks on its own.

## What the client draws

The surface is transparent: the compositor draws the wallpaper under it.

- **Top right:** the active keyboard layout (`SessionUnlock::keyboard_layout`, with a short Fluent name when one exists) and the power button, in the slot the menu bar leaves free (`menubar::lock_row::LOCK_SLOT`). It opens Suspend, Restart and Shut Down, drawn inside the surface because a lock surface has no pop-ups. The status icons beside it are the menu bar's own.
- **Top center:** the short date («mié 11 de nov») and the time, `LOCK_TOP_PERCENT` of the screen height from the top, updated every minute; notifications from `on_lock_screen` below them.
- **Bottom center:** the AccountsService picture (or the initial), the GECOS name, the capsule field, the «?» button when AccountsService has a `PasswordHint`, and the line below the field for the PAM error or the hint. The field shows the PAM prompt while empty and asks the compositor to blur behind it (`set_blur_region`). It is light in every theme; «Reduce transparency» only drops the blur.

## The menu bar

The menu bar is the only bar on screen; it changes mode and is never replaced. It follows `use_screen_lock()`:

1. On `locking` it fades out its background, its left side, other apps' status items and its clock.
2. Its own extras slide right until they end at the lock screen's slot (`LOCK_SLOT`), and their ink turns `LOCK_INK`.
3. The compositor keeps drawing it above the lock surfaces and sends it frame callbacks while locked. It gets no input: the power button belongs to the lock surface.
4. On `unlocked` it plays this back in `DURATION_BASE`, while the compositor fades the desktop back in.

## Recovery

Smithay leaves the lock in `Defunct` (not in `Unlocked`) and a second `lock()` can take ownership; Malaysia keeps `locked == true`.

`lock.rs`:

- a destruction hook on each lock surface; if the last one disappears while the session is locked, it schedules recovery;
- a 3 s watchdog when a lock starts, in case the client dies before creating its first surface;
- clock and launch through the same `TimerSink`/`spawn::shell_component` as `idle.rs`;
- backoff of 1, 2, 4, 8 and 16 s, at most 5 attempts. Once exhausted, the session stays locked over the wallpaper and a manual `malaysia-shell lock` is accepted;
- neither recovery nor a second lock calls `unlock`. A second lock skips the transition.

## PAM and auto-lock

Authentication lives in the lock process (FFI to `libpam`, `malaysia-pam` thread); service `malaysia-lock` or `login`, password in `Zeroizing<String>`, and only a success calls `unlock_and_destroy`. `ext-idle-notify-v1` resets the clock on each input and `lock.idle_timeout_seconds` (default 0, disabled) launches the lock; `zwp_idle_inhibit_manager_v1` holds it back. With auto-lock active, the DRM backend takes a logind `delay` inhibitor and, on receiving `PrepareForSleep(true)`, launches the lock and releases the inhibitor once the session is locked (at most 4 s). A lock before sleep skips the transition. It lives in `sleep.rs`.

## Compositor side

- `SessionLockHandler::lock()` sets `State::locked`, clears the keyboard focus and starts `lock_transition`. While locking, the scene is the cursor, the menu bar, the wallpaper drawn over the desktop with a growing alpha, and the desktop.
- Once shown, the scene is the cursor, the lock surfaces with their blurred field and the wallpaper. Only `Background` surfaces of `malaysia-wallpaper` from an unsandboxed client are drawn; without one, `DESKTOP_CLEAR`. Frame callbacks go to the lock surfaces and the wallpaper.
- Keys go straight to the lock surface, bypassing an input method's grab, so it cannot read the password.
- `new_surface()` configures it to the output size, adds it to `lock_surfaces` and gives keyboard focus to the first one.
- `unlock()` only arrives through `unlock_and_destroy`; never through the client's death. It sends `infinity_session_v1.unlocked` and fades the desktop back in.
- With the session locked, `pointer_press` ignores windows/popups, `focus_window` does not yield focus and the dock's `activate_window` is ignored. Authentication belongs to the client; the compositor does not know PAM.
