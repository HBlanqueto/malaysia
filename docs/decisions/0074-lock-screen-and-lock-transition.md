# 0074 — Lock screen over the wallpaper and the lock transition

Status: accepted (2026-11-11). Nothing is implemented yet; see «Plan».

## Goal and scope

This ADR fixes what the lock screen shows and how the desktop turns into it.
It starts when a client sends `ext_session_lock_manager_v1.lock` and ends when
the session is locked and the lock screen is fully drawn; it also covers the
way back after `unlock_and_destroy`. Authentication (PAM) and lock recovery
stay as [`lock.md`](../architecture/lock.md) describes them. Readers: whoever
works on `malaysia-shell lock`, the menubar, the compositor's lock path
(`lock.rs`, `scene_elements.rs`, `frame_clock.rs`) or the Infinity protocol.

Labels: [S] specification at the cited revision, [I] read from Malaysia's
code on 2026-11-11.

## Context

### What Malaysia shows today [I]

1. `malaysia-shell lock` (`lock.rs`) fills every output with
   `palette.bg_surface` of the `dark-tint` theme (`#252628`), in light and
   dark mode alike.
2. In the middle: «Sesión bloqueada» at `LOCK_FONT_SIZE × LOCK_TITLE_SCALE`,
   the PAM prompt as a label, a 360 × 44 field filled with the accent color
   that shows «Contraseña» or `•` per character, and an error line.
3. Up to four notification cards sit at the top right when
   `notifications.when_locked` is on.
4. There is no clock, date, user, status icon or power control.
5. While `State::locked` is true, `build_output_elements` draws only the
   cursor and the lock surfaces; `frame_clock.rs` sends frame callbacks only
   to them.
6. `SessionLockHandler::lock` sets `locked`, cuts input and calls
   `confirmation.lock()` in the same dispatch, before any frame shows the
   locked state.
7. `idle.rs` launches the lock before sleep and `sleep.rs` waits for
   `locked` for at most `LOCK_WAIT` (4 s).
8. `malaysia-wallpaper` draws a `Background` layer surface with namespace
   `malaysia-wallpaper` on each output (ADR 0049, 0073). The menubar is a
   layer surface with namespace `malaysia-menubar` (`menubar/bar.rs`).
9. The menubar's right side shows, from right to left: the clock, the
   status items of other apps (StatusNotifierItem) and the built-in extras
   (`menubar/extras.rs`): battery, Wi-Fi and Bluetooth, each behind a
   `menubar.show_*` switch. There is no printer extra.

### What `ext-session-lock-v1` allows [S]

From `ext-session-lock-v1.xml` in wayland-protocols 0.32.13:

1. On `locked` the compositor «must blank all outputs with an opaque color
   such that their normal content is fully hidden».
2. «The only surfaces that should be rendered while the session is locked are
   the lock surfaces […] and optionally, at the compositor's discretion,
   special privileged surfaces such as input methods or portions of desktop
   shell UIs.»
3. «The locked event must not be sent until a new "locked" frame […] has
   been presented on all outputs and no security sensitive normal/unlocked
   content is possibly visible.» Context item 6 breaks this rule.
4. Lock surfaces «won't be displayed by the compositor unless the lock is
   successful and the locked event is sent».
5. The compositor «may wait for the client to create and render session lock
   surfaces before sending the locked event», with «a reasonable time limit».

### The target

The approved mock-up, «Del escritorio al bloqueo»:

- The lock screen shows the wallpaper, sharp. Only the password field blurs
  what is behind it.
- The status icons stay at the top right.
- The date and time sit at the top center.
- The user and the password field sit at the bottom center.
- When the desktop locks, windows, the dock, notifications and the left side
  of the menubar fade out slowly. The status icons close up to the right and
  stay. The date and time rise from below.

## Decision

1. **The wallpaper stays behind the lock screen.**
   1. While locked, the compositor draws the `Background` layer surfaces
      under the lock surfaces. The protocol allows this as a «portion of
      desktop shell UI» (context [S] 2).
   2. It draws only surfaces with namespace `malaysia-wallpaper` that belong
      to an unrestricted client (`security::unrestricted`). Any other
      `Background` surface stays hidden.
   3. Without such a surface on an output, that output shows
      `ctdesign::DESKTOP_CLEAR`, which is opaque, as today.
   4. The wallpaper keeps receiving frame callbacks while locked, so dynamic
      and animated wallpapers (ADR 0073) keep running.
   5. The lock surface paints no background. It draws only its content on a
      transparent buffer.
2. **Only the password field is blurred.**
   1. The lock surface requests a blur region equal to the field's rounded
      rectangle, through `ext-background-effect-v1`.
   2. ADR 0071 gives lock surfaces the `"wallpaper"` mode, so the field
      samples the cached blurred wallpaper. This needs ADR 0071's pending
      lock-surface item.
   3. With `appearance.reduce_transparency` on (the default), the
      compositor draws no blur. The field then uses an opaque fill token.
3. **What the lock screen shows.** All of it is drawn by
   `malaysia-shell lock`. Text and icons are white with a soft shadow, so
   they read over any wallpaper.
   1. **Top right: the status row.** From right to left: the power button,
      the battery (if the machine has one), Wi-Fi, Bluetooth (if it is on)
      and the printer (if a job is active). They use the menubar's extras
      and `menubar.show_*` switches, read-only: clicking them opens no menu.
   2. **The power button** opens a menu with Suspend, Restart… and Shut
      Down…. It uses `power.rs`; there is no Log Out while locked.
   3. **Top center:** the long date (for example «martes, 11 de noviembre»)
      and, below it, the time in large type, in the menubar clock's format
      (`%H:%M`).
   4. **Bottom center:** the user's picture, then their display name, the
      password field and the error line.
      - The picture is AccountsService's `IconFile`. Without one, it is a
        circle with the user's initial.
      - The display name comes from `system.rs` (`user_display_name`).
   5. **Notifications** move from the top right to below the clock. The
      rules are unchanged (`on_lock_screen`, `ShowPreviews`). They appear
      together with the user and the password field.
   6. **The field** reads «Contraseña» while it is empty, grows as the user
      types and shows `•` per character. On an error it turns
      `palette.danger` and the error appears below it. The PAM prompt shows
      only when it is not the plain password prompt (for example «Código de
      verificación»), so «Contraseña» no longer appears twice.
4. **The lock transition.**
   1. **The phases.** The compositor's lock state becomes
      `Unlocked → Locking → Locked`.
      - Every check that reads `state.locked` today (input, captures,
        foreign toplevel, Infinity, theme fade) treats `Locking` as locked.
      - Input and captures stop as soon as `lock` arrives, as today.
   2. **During `Locking`, the compositor fades everything else out.**
      - Every surface except the wallpaper (decision 1) and the menubar
        fades from opacity 1 to 0 in `LOCK_FADE`: windows, pop-ups, the dock,
        notifications, other layers and decorations.
      - It does so through the alpha that surface elements already take
        (`render_elements_from_surface_tree`).
      - Frame callbacks still reach the fading surfaces until they are
        hidden.
   3. **The menubar closes up.**
      - On `lock`, the compositor sends the new Infinity event `locking`
        (decision 6). It carries the duration of the fade.
      - Over `LOCK_FADE`, the menubar fades out its background, its left
        side, other apps' status items and its clock.
      - Then, in `LOCK_SETTLE`, the kept items slide right to the positions
        of the lock screen's status row. The menubar keeps drawing that state
        until it is hidden.
   4. **The swap.**
      - The swap happens when `LOCK_FADE + LOCK_SETTLE` has passed and every
        output has a lock surface with a committed buffer.
      - The compositor then hides the menubar and shows the lock surfaces in
        the same frame.
      - Once that frame is presented on every output, it sends `locked`.
        This fixes context item 6.
   5. **The lock screen's own entry.**
      - The lock surface's first frame draws the status row at the
        menubar's final positions, without the power button. The swap is
        therefore invisible.
      - Then the power button fades in, the date and time rise from
        `LOCK_RISE_OFFSET` below their place in `LOCK_RISE`, and the user,
        the field and any notifications fade in in `LOCK_LOGIN`.
   6. **One layout for both.**
      - The menubar's final state and the lock screen's status row come from
        one function in `malaysia-shell`, `menubar::lock_status_layout`, fed
        by the same tokens.
      - The two processes never exchange positions.
   7. **The time limit.**
      - If any output has no lock surface buffer `LOCK_START_GRACE` (3 s)
        after `lock`, the compositor stops waiting. It hides the menubar and
        presents the wallpaper alone, or `DESKTOP_CLEAR` where there is none.
      - It then sends `locked` and arms recovery as `lock.md` describes.
5. **Exceptions.**
   1. **Before sleep.** A lock started by `lock_before_sleep` skips the
      transition. The compositor swaps on the first frame with lock surface
      buffers, so `LOCK_WAIT` keeps its margin.
   2. **Reduced motion.** With `appearance.reduced_motion` on, nothing slides
      or rises.
      - The fade and the swap remain, in `DURATION_BASE`.
      - The menubar's kept items cross-fade to their new places instead of
        sliding.
   3. **Defunct lock.** When the lock client dies, the compositor keeps the
      wallpaper and the last lock surfaces, and keeps the policy of
      `lock.md`.
6. **The Infinity event.**
   1. A new global, `infinity_session_v1`, joins `infinity-protocol` in
      barrierkit. It has two events:
      - `locking(duration_ms)`: the session is locking, and the fade lasts
        `duration_ms`. A value of 0 means no transition.
      - `unlocked`: the session is unlocked.
   2. It grants nothing and carries no secret, so any client may bind it.
      Only the menubar uses it.
7. **The way back.** On a successful PAM answer, the lock client does three
   things, in order:
   1. It fades its date, time, user, field and notifications out in
      `DURATION_FAST`.
   2. It sends `unlock_and_destroy`. The status row stays on screen until
      then.
   3. The compositor then sends `unlocked`, and the menubar redraws in its
      locked layout.
   After that:
   - The menubar slides its items back and fades in the rest in
     `DURATION_BASE`.
   - The compositor fades the other surfaces in over the same time.
   - Input returns at once.
8. **Tokens.** These go in `components.shell.lock` in `ctdesign`. The values
   are the mock-up's and are tuned on hardware.

   | Token | Value |
   |---|---|
   | `LOCK_FADE` | 1100 ms |
   | `LOCK_SETTLE` | 600 ms |
   | `LOCK_RISE` | 900 ms |
   | `LOCK_RISE_OFFSET` | 120 |
   | `LOCK_LOGIN` | 600 ms |
   | `LOCK_CLOCK_FONT_SIZE` | 108 |
   | `LOCK_DATE_FONT_SIZE` | 24 |
   | `LOCK_AVATAR_SIZE` | 52 |
   | `LOCK_FIELD_HEIGHT` | 30 (was 44) |
   | `LOCK_FIELD_MIN_WIDTH` | 108 |
   | `LOCK_FIELD_MAX_WIDTH` | 200 |
   | `LOCK_BOTTOM_MARGIN` | 92 |

   The following tokens are removed with their code: `LOCK_TITLE_SCALE`,
   `LOCK_FIELD_WIDTH` and the `lock-title` string. `lock_palette` stays only
   for the defunct-lock fallback color.

## Consequences

1. Locking takes about 1.7 s before `locked` instead of being immediate.
   Input and captures are cut from the first moment, so the delay leaves
   nothing exposed.
2. A tool that locks and then acts on `locked` (for example
   `loginctl lock-session` before closing a lid) waits for the transition.
   The sleep path skips it (decision 5.1).
3. The compositor draws one normal client, the wallpaper, while locked.
   - An unrestricted client could create its own `malaysia-wallpaper`
     background and show an image on the lock screen. It still cannot
     receive input or read the screen.
   - An unrestricted client already runs with the user's rights, so this
     adds no new reach.
   - A sandboxed client cannot do it.
4. An animated wallpaper keeps using the GPU while the screen is locked and
   on.
5. The printer item needs a new menubar extra that reads CUPS jobs. Until
   then the status row has no printer.

## Rejected alternatives

1. **Keep a solid background.** It contradicts the approved design and makes
   the lock screen look unrelated to the desktop.
2. **Blur the whole wallpaper.** It was rejected in review: the wallpaper
   stays sharp and only the field blurs.
3. **The lock client draws the menubar and fades it.** The protocol forbids
   showing lock surfaces before `locked`, and `locked` must wait until normal
   content is hidden. So the lock client cannot be on screen while the
   desktop fades. The menubar has to animate itself.
4. **The compositor fades the menubar like any other surface.** The status
   icons would vanish and reappear. The design keeps them on screen.
5. **The menubar stays visible while locked**, as a privileged shell
   surface. It would need input routing for its menus while locked, and it
   would show apps' status items. The lock client draws its own status row
   instead.
6. **Send `locked` at once and fade afterwards.** This breaks [S] 3 and lets
   a suspend race the fade (context item 7).

## Plan

1. **Phase 1: static lock screen.**
   - Draw the wallpaper behind lock surfaces (decision 1).
   - Send `locked` after the presented frame (decision 4.4).
   - Ship the new layout in `malaysia-shell lock` (decision 3), without the
     printer, plus the new tokens.
   - Blur the field once ADR 0071's lock-surface item lands.
2. **Phase 2: the transition.**
   - The `Locking` phase and the fade.
   - `infinity_session_v1` and the menubar's closing up.
   - The entry animation of the lock screen.
   - The sleep and reduced-motion exceptions.
3. **Phase 3:** the way back (decision 7).
4. **Phase 4:** the printer extra in the menubar and on the lock screen.
