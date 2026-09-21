# Block 3 — Input

> **Roadmap mapping.** This is the detailed record of roadmap **Block 3**
> (Input), the other half of Part 6 of the **nested session**
> (`docs/blocks/01-session.md`, Part 6 input; roadmap Block 2 is that Part's
> rendering half, `docs/blocks/02-a-window-on-screen.md`). Cursor support was
> the last piece, added after the session's other parts.

Status: **complete, with hand-checked gaps.** `./dev.sh smoke` checks the
machine-visible parts; typing with the right layout, pointer response and the
cursor's pixels are hand-checked.

## Purpose

Block 3 makes the seat real: a mapped window takes keyboard focus and receives
`wl_keyboard.enter`; keys reach it through the keymap and repeat configured at
seat creation; the pointer hits the window under it and delivers motion,
buttons and scroll; and the pointer is drawn — the client's own image when it
sets one, the compositor's arrow otherwise. Every serial comes from Smithay's
single counter; none is invented.

## What was built

### `input.rs`

- `process_input_event` translates Smithay's unified `InputEvent` form into
  seat events, so the module is backend-agnostic and keeps working when the
  Direct Session milestone replaces winit:
  - keyboard input goes through `Seat::get_keyboard` and
    `keyboard.input` with `FilterResult::Forward`, carrying a serial from the
    central counter;
  - absolute pointer motion is converted with `State::logical_output_size`
    (the winit backend reports it as a fraction of the window);
  - pointer buttons are delivered to the pointer; the compositor's own press
    logic runs before the protocol event;
  - `PointerAxis` becomes an `AxisFrame` with `value`, `v120` where the backend
    reports it, and a finger-source `stop` for a zero axis.
- Pointer focus is by hit-testing the window under the point, and motion
  updates both the focus and the edge cursor. The two places that build a
  pointer focus tuple — `pointer_motion` and the scripted press the test backend
  injects — hand Smithay the surface *origin* it expects, not the surface-local
  point; the scripted press goes through `pointer_motion`, so the two cannot
  drift apart.
- Keyboard focus exists from the first map: `handlers.rs::commit` calls
  `focus_window` when a window maps, which sends `wl_keyboard.enter`. When a
  window is destroyed, `forget_window` passes focus to the new topmost window
  or clears it (`clear_keyboard_focus`).
- Block 4 extended the button path with the compositor's own grabs (edge
  resize, decorated-band move, click-to-focus) and validated
  `xdg_toplevel.move`/`resize` serials against the pointer's held press
  (ADR 0016).

### `cursor.rs` and the draw path

- A client chooses its pointer with `wl_pointer.set_cursor`. Smithay handles
  the request and, before calling `SeatHandler::cursor_image`, validates its
  serial against the pointer's **last enter serial**, which this compositor
  generated from the single counter in `input.rs`; a request with any other
  serial never reaches `SeatHandler::cursor_image`. `handlers.rs::cursor_image`
  stores the status and requests a redraw.
- `CursorImageStatus` is one of three things in the draw path:
  - a client surface, drawn at the pointer position minus its hotspot;
  - `Hidden` (a null surface), which draws nothing;
  - a named cursor — the default arrow — loaded once from the Xcursor theme
    (`XCURSOR_THEME`, `XCURSOR_SIZE`), with a built-in outlined triangle when
    no theme can be read. Loading is the one new dependency, `xcursor`, which
    Smithay does not provide.
- `session.rs::append_cursor` draws the client's surface tree or the
  compositor's shape over the windows; `cursor_origin` places both at the
  pointer minus the hotspot, so the two paths stay in step. A client cursor
  surface can die while it is still the pointer image; `reset_dead_cursor`
  falls back to the compositor's arrow rather than drawing a dead surface.
- Block 4 added the edge resize shapes to the same table.

### Seat and keymap

- The seat is created in `State::new` and the keyboard is added before the seat
  is usable, because a seat with no keymap is worse than no seat: clients that
  accepted the capability get no keymap and misbehave. `add_keyboard` is called
  with `XkbConfig::default()` and a 200 ms repeat delay at 25/s; a failure is
  fatal to startup. The session logs the loaded keymap (`English (US)`).

## Decisions

- **One serial counter.** Serials come from Smithay's `SERIAL_COUNTER`;
  ADR 0016 extends the rule with the held-grab validation of client requests.
- **ADR 0012** — the seat and the input state live in `State`; the backend and
  the renderer do not.
- **ADR 0013** — client requests are dispatched by polling a duplicated display
  descriptor; input events arrive on the winit source and are forwarded to the
  same `State`.
- The cursor's `set_cursor` serial check is not a policy of this repository: it
  is Smithay's, against the pointer enter it generated. No second counter or
  parallel check exists.

## Verification

- **`./dev.sh smoke`** — runs the compositor at debug level and asserts the
  input-visible half of the mapping sequence: `wl_keyboard.enter` reaches the
  mapped surface after its commit, and any `wl_pointer.set_cursor` in the trace
  used a serial of a `wl_pointer.enter` the client had already received. It
  also asserts the compositor drew its own default pointer. Observed:

  ```
  smoke: compositor is listening on wayland-1
  smoke: configure(serial 2) -> ack(2) -> attach/commit -> keyboard.enter -> frame callback #25 done at line 104; cursor surfaces set: 0
  smoke: PASS
  ```

  `cursor surfaces set: 0` is not a gap in the check: the harness cannot move
  the host pointer, so no client cursor is requested; the compositor's default
  pointer is drawn and the trace has nothing to pair.
- **`./dev.sh check`** — carries the unit tests this block's path relies on:
  `input::tests::host_pointer_corners_map_to_logical_corners` and
  `host_pointer_moves_down_in_logical_space` drive the real
  `process_input_event` through a test backend built to winit's contract; the
  fallback cursor tests (`the_fallback_image_is_a_full_rgba_buffer`,
  `the_fallback_hotspot_is_the_tip`, `the_fallback_arrow_is_visible`); the
  offscreen pixel test `the_cursor_lands_at_the_hotspot_adjusted_position`; and
  the smoke parser tests that accept a `set_cursor` matching a preceding enter
  and reject one whose serial no enter carried, one before any enter, and a
  null (hide) cursor.
- **`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`** — the
  documentation build is warning-free.

Not verified by a command, and left to hand-checking: that typing reaches the
client with the correct layout, that the pointer responds where it points, that
the client's output is legible, and that the drawn cursor's pixels look right.
The smoke harness cannot move the host pointer, so it can never make a client
set a cursor live; it checks the compositor's default pointer and the
`set_cursor`/enter serial pairing, not the finished pixels.

## Open questions

1. **The smoke harness cannot move the host pointer.** Live client
   `set_cursor`, hover feedback and the cursor's pixels stay hand-checked.
2. **Keyboard focus does not follow the pointer.** It changes on click and on
   the focused window's destruction only; hover does not raise or focus.
3. **Touch and gesture events are ignored.** `process_input_event` forwards
   keyboard, pointer motion, buttons and axis, and drops the rest; the winit
   backend reports them, so adding them is a handler, not a new source.
4. **The `deny(unsafe_code)` policy must be redefined before the Direct Session
   milestone.** The nested session already bent it twice (the dispatch source
   and exporting `WAYLAND_DISPLAY`); the question itself is session-level and
   is recorded in `01-session.md`.

## Decision records

- `docs/decisions/0009-winit-backend-and-gles2.md` — the nested backend and
  feature set this input path runs in (winit, GLES2, `wayland_frontend`;
  `desktop` off).
- `docs/decisions/0012-backend-beside-state.md` — `State` owns the seat; the
  backend lives in `Session`.
- `docs/decisions/0013-dispatch-polls-duplicated-fd.md` — how client requests
  and input events reach the same `State`.
- `docs/decisions/0016-interactive-move-and-resize.md` — the held-grab serial
  validation Block 4 added on top of the single counter.
- `docs/blocks/01-session.md` — the session-level record: Parts, Part 5
  rulings, the unsafe-policy open question.
