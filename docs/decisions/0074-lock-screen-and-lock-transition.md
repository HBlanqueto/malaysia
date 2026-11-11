# 0074 — Lock screen over the wallpaper and the lock transition

Status: accepted and implemented (2026-11-11); revised the same day after the
first try on hardware: the menu bar stays on screen while locked, so one bar
changes mode instead of two being swapped, and the date and time sit lower.

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
   5. The menu bar stays too: the compositor draws the layer surface with
      namespace `malaysia-menubar` of an unrestricted client above the lock
      surfaces and keeps sending it frame callbacks. It receives no input.
   5. The lock surface paints no background. It draws only its content on a
      transparent buffer.
2. **Only the password field is blurred.**
   1. The lock surface requests a blur region equal to the field's
      rectangle, through `ext-background-effect-v1` (`bk::shell::set_blur_region`).
   2. ADR 0071 gives lock surfaces the `"wallpaper"` mode, so the field
      samples the cached blurred wallpaper. The compositor draws each
      rectangle of a lock surface as a capsule (`blur::capsules`).
   3. With `appearance.reduce_transparency` on (the default), the
      compositor draws no blur. The field then uses `LOCK_FIELD_FILL_OPAQUE`.
3. **What the lock screen shows.** All of it is drawn by
   `malaysia-shell lock`. Text and icons are `LOCK_INK` (white).
   1. **Top right: the status row.** From right to left: the power button,
      then the menu bar's own extras in the bar's order and under its
      `menubar.show_*` switches. Only the power button belongs to the lock
      surface; the extras are the menu bar itself (decision 4.3), and
      clicking them opens no menu because the bar gets no input.
   2. **The power button** opens Suspend, Restart and Shut Down, drawn
      inside the lock surface because a lock surface has no pop-ups. Each
      acts at once through `power.rs`; there is no Log Out while locked.
   3. **Top center:** the long date (for example «martes, 11 de noviembre»)
      and, below it, the time in large type, in the menubar clock's format
      (`%H:%M`). The block starts `LOCK_TOP_PERCENT` of the screen height
      from the top, so it keeps its place on any screen size.
   4. **Bottom center:** the user's picture, then their display name, the
      password field and the error line.
      - The picture is AccountsService's `IconFile`. Without one, it is a
        circle with the user's initial.
      - The display name comes from `system.rs` (`user_display_name`).
   5. **Notifications** move from the top right to below the clock. The
      rules are unchanged (`on_lock_screen`, `ShowPreviews`). They appear
      together with the user and the password field.
   6. **The field** shows the PAM prompt while it is empty («Contraseña»
      until PAM asks), grows as the user types and shows `•` per character.
      On an error it turns `LOCK_FIELD_ERROR` and the error appears below
      it. There is no separate prompt label, so «Contraseña» no longer
      appears twice.
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
      - Then, in `LOCK_SETTLE`, its own extras slide right until they end at
        the power button's slot, and their ink turns `LOCK_INK`. The menubar
        keeps that state while locked.
   4. **The swap.**
      - The swap happens when `LOCK_FADE + LOCK_SETTLE` has passed and every
        output has a lock surface with a committed buffer.
      - The compositor then shows the lock surfaces under the menubar, which
        stays where it is.
      - Once that frame is presented on every output, it sends `locked`.
        This fixes context item 6.
   5. **The lock screen's own entry.**
      - The lock surface's first frame draws nothing visible: the power
        button starts transparent. Nothing on screen is swapped.
      - The client starts its entry on `locked`
        (`SessionUnlock::locked()`), the first moment it is on screen.
      - Then the power button fades in, the date and time rise from
        `LOCK_RISE_OFFSET` below their place in `LOCK_RISE`, and the user,
        the field and any notifications fade in in `LOCK_LOGIN`.
   6. **One bar.**
      - The status icons are drawn only by the menubar, on the desktop and
        while locked; the lock surface draws no copy of them.
      - The power button's slot, `menubar::lock_row::POWER_SLOT`, is the one
        position both processes share. The menubar leaves it free and the
        lock surface draws the button there (`lock_row::status_row`).
   7. **The time limit.**
      - If any output has no lock surface buffer `LOCK_START_GRACE` (3 s)
        after `lock`, the compositor stops waiting. It presents the wallpaper
        and the menubar, or `DESKTOP_CLEAR` where there is no wallpaper.
      - It then sends `locked` and arms recovery as `lock.md` describes.
5. **Exceptions.**
   1. **Before sleep.** A lock started by `lock_before_sleep` skips the
      transition. The compositor swaps on the first frame with lock surface
      buffers, so `LOCK_WAIT` keeps its margin.
   2. **Reduced motion.** With `appearance.reduced_motion` on, nothing slides
      or rises.
      - The fade and the swap remain, in `DURATION_BASE`.
      - The menubar's kept items move to their new places in one step once
        the bar has faded, instead of sliding.
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
      `DURATION_FAST_MS`.
   2. It sends `unlock_and_destroy`. The status row stays on screen until
      then.
   3. The compositor then sends `unlocked`; the menubar is still in its
      locked layout.
   After that:
   - The menubar slides its items back and fades in the rest in
     `DURATION_BASE`.
   - The compositor fades the other surfaces in over the same time.
   - Input returns at once.
8. **Tokens.** These are in `components.shell.lock` in `ctdesign`. The
   values are the mock-up's and are tuned on hardware.

   | Token | Value |
   |---|---|
   | `LOCK_FADE_MS` | 1100 |
   | `LOCK_SETTLE_MS` | 600 |
   | `LOCK_RISE_MS` | 900 |
   | `LOCK_RISE_OFFSET` | 120 |
   | `LOCK_LOGIN_MS` | 600 |
   | `LOCK_CLOCK_FONT_SIZE` | 108 |
   | `LOCK_DATE_FONT_SIZE` | 24 |
   | `LOCK_TOP_PERCENT` (of the screen height) | 15 |
   | `LOCK_BOTTOM_MARGIN`, `LOCK_SPACING` | 92, 12 |
   | `LOCK_AVATAR_SIZE` | 52 |
   | `LOCK_FIELD_HEIGHT` | 30 (was 44) |
   | `LOCK_FIELD_MIN_WIDTH`, `LOCK_FIELD_MAX_WIDTH` | 108, 200 |
   | `LOCK_MASK_FONT_SIZE` | 18 |
   | `LOCK_INK`, `LOCK_FIELD_INK` | `#ffffff`, `#ffffffbf` |
   | `LOCK_FIELD_FILL`, `LOCK_FIELD_FILL_OPAQUE`, `LOCK_FIELD_ERROR` | `#ffffff2e`, `#5c5d61`, `#ff6b6b73` |

   `MENUBAR_LAYER_NAMESPACE` and `WALLPAPER_LAYER_NAMESPACE` name the two
   layer surfaces the compositor keeps. The old `LOCK_FONT_SIZE`,
   `LOCK_TITLE_SCALE`, `LOCK_FIELD_WIDTH`, `LOCK_LABEL_*`, `LOCK`,
   `LockPalette`, `theme::lock_palette` and the `lock-title` string are
   gone.

## Consequences

1. Locking takes about 1.7 s before `locked` instead of being immediate.
   Input and captures are cut from the first moment, so the delay leaves
   nothing exposed.
2. A tool that locks and then acts on `locked` (for example
   `loginctl lock-session` before closing a lid) waits for the transition.
   The sleep path skips it (decision 5.1).
3. The compositor draws two normal clients, the wallpaper and the menubar,
   while locked.
   - An unrestricted client could create its own `malaysia-wallpaper`
     background or `malaysia-menubar` layer and show it on the lock screen.
     It still cannot receive input or read the screen.
   - An unrestricted client already runs with the user's rights, so this
     adds no new reach.
   - A sandboxed client cannot do it.
4. An animated wallpaper keeps using the GPU while the screen is locked and
   on.
5. The printer extra asks CUPS for unfinished jobs over
   `/run/cups/cups.sock` (IPP Get-Jobs, RFC 8011) every 15 s, or sooner when
   `org.cups.cupsd.Notifier` signals. A job shorter than that may never show.

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
5. **The lock client draws its own copy of the status row.** This was the
   first implementation. On hardware the two copies showed together for a
   moment around the swap, since the bar and the lock surface do not commit
   in the same frame. The menubar now stays instead, without input and with
   other apps' status items faded out.
6. **The menubar keeps its menus while locked.** It would need input
   routing for the bar while locked. The power button stays in the lock
   surface instead.
7. **Send `locked` at once and fade afterwards.** This breaks [S] 3 and lets
   a suspend race the fade (context item 7).

## Plan

All four phases landed together on 2026-11-11:

1. The static lock screen over the wallpaper, with `locked` sent after the
   presented frame and the blurred field.
2. The transition: the `Locking` phase, `infinity_session_v1`, the menu
   bar closing up, the entry animation and the two exceptions.
3. The way back.
4. The printer extra on the bar and on the lock screen.

Still to do: try it on the ThinkPad and tune the tokens; the upstream
Smithay PR to resend `capabilities` (ADR 0071).
