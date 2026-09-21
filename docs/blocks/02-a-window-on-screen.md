# Block 2 — A window on screen

> **Roadmap mapping.** This is the detailed record of roadmap **Block 2** (A
> window on screen). It is Parts 4–6 of the **nested session**, whose
> session-level record is `docs/blocks/01-session.md`; the divergences and Part
> 5 rulings cited here live there, and roadmap Block 3 is the other half of
> Part 6 (`docs/blocks/03-input.md`).

Status: **complete, with divergences.** The machine-checked acceptance is
`./dev.sh smoke`; whether a client's contents are legible stays hand-checked.

## Purpose

Block 2 turns a committed buffer into pixels: the winit backend and the GLES2
renderer draw each mapped window's surface into the nested window, and the
toplevel lifecycle decides when a client's surface is mapped at all. The seat
and its globals exist from Block 1, but they are only used once a window maps,
which is why keyboard focus and the cursor are Block 3.

Two roadmap bullets are deliberately implemented differently from the brief,
and both divergences are recorded in this repository:

- **Mapping happens on the first buffered commit, not on `ack_configure`.**
  The protocol maps a surface when it has a committed non-null buffer; mapping
  on the ack would show a surface with nothing in it. `01-session.md`
  (Divergences 1) is authoritative and overrides the brief's wording.
- **There is no `desktop::Space` wrapper.** `desktop` stays off (ADR 0009) and
  the tracker is hand-written in `window.rs`. ADR 0015 records a reason for
  keeping it that way; it was written when Block 4 made reversing it expensive.

## What was built

### `session.rs`

- The winit backend (`WinitEventLoop`, a `WinitGraphicsBackend` with a
  `GlesRenderer`) and the `OutputDamageTracker` live in `Session` beside
  `State`, because they are backend state and no protocol handler may reach
  them (ADR 0012).
- The loop is on demand, not continuous. A handler that changes something
  raises `State::needs_redraw`; the session turns that into a winit redraw
  request, and `draw` runs on `WinitEvent::Redraw`.
- `draw` turns every mapped window into render elements
  (`render_elements_from_surface_tree`, `Kind::Unspecified`) at its window
  offset in physical pixels, appends the cursor (Block 3), and draws them
  through `OutputDamageTracker::render_output` with the session background
  (`[0.06, 0.06, 0.09, 1.0]`) as the clear colour, submitting only the damage
  the tracker reports.
- Frame callbacks are sent after the frame is drawn and only from this path, so
  a client cannot drive the compositor faster than the output refreshes.
  `send_frame_callbacks` walks the surface tree because the shared helper is
  behind the `desktop` feature, which stays off.
- A host-window resize (`WinitEvent::Resized`) updates the output's mode and
  replaces the damage tracker, whose mode source is static; a fresh tracker
  resets the damage history, which is what the whole surface needs anyway.

### `window.rs`

- `Window`/`Windows` track the toplevels: the surface, the outer size, the
  configure bookkeeping, the insets and the mapped flag. `Window` was a
  read-only view from the start, so the draw and input paths never touched the
  tracker's internals; Block 4 rebuilt the storage around `neptune-scene` and
  kept the view.
- `new_toplevel` sends the initial configure immediately, carrying the usable
  size (the outer size minus the decoration insets). A toplevel that never
  receives one waits forever with no error.
- `ack_configure` accepts only the serial of the configure most recently sent,
  so a stale ack cannot mark a window ready (Part 5 ruling 4).
- A window maps on the first commit that carries a buffer *and* follows an ack.
  `handlers.rs::commit` calls `on_commit_buffer_handler` first — Smithay moves
  the pending buffer into the committed state there, and the old check could
  never see it — then the tracker reads that state.
- `usable` subtracts the insets from the outer size and never collapses below
  one pixel; insets are recomputed on every commit, and a change of effective
  insets reconfigures the client.

### `decoration.rs`

- The insets type was written before the first window was mapped (Part 4 of the
  session): the configure a toplevel receives carries its usable size, so the
  number is needed before any chrome exists. Insets are a function of the
  window's *current* role and state, not of the state at map time (ADR 0014);
  `Role::Popup` and `Role::Dialog` reserve nothing, and fullscreen and
  client-side chrome reserve nothing.

### `state.rs` and `handlers.rs`

- `State` holds the globals, the one seat, the output and its current size, the
  window tracker, the cursor status and the redraw flag. `logical_output_size`
  is the single place the output's mode size becomes logical coordinates (Part
  5 ruling 3), and `OUTPUT_SCALE` is the single scale literal (see Open
  questions).
- `wl_shm` advertises `Argb8888` and `Xrgb8888`; the render path imports the
  client's buffer through the surface tree.
- `Handlers` implements `CompositorHandler`, `ShmHandler`, `SelectionHandler`
  and the `xdg_shell` callbacks. `client_compositor_state` documents the
  invariant that every accepted client was inserted with `ClientState` by the
  socket handler.

## Decisions

- **ADR 0009** — the nested backend is winit with the GLES2 renderer;
  `desktop` stays off, which is also why `send_frame_callbacks` is hand-written.
- **ADR 0010** — Smithay is pinned to the crates.io 0.7.0 release.
- **ADR 0012** — the backend lives beside the compositor `State`.
- **ADR 0014** — decoration insets are a function of window state.
- **ADR 0015** — `desktop::Space` is not adopted; the tracker stays hand-written
  and `neptune-scene` grows into the window-layout authority (recorded when
  Block 4 made the choice hard to reverse).
- The mapping divergence is **Divergences 1** in `docs/blocks/01-session.md`.

## Verification

- **`./dev.sh smoke`** — launches a compositor and `foot` with
  `WAYLAND_DEBUG=1`, captures the client's trace and asserts, in order:
  `xdg_toplevel.configure` before the first non-null attach; a matching
  `ack_configure` serial before that attach; a non-null buffered commit the
  compositor reports as mapped; `wl_keyboard.enter` on that surface; and
  completion of the callback id the client asked for with `wl_surface.frame`
  (a bare `wl_callback.done`, including a `wl_display.sync`, is not accepted).
  Observed:

  ```
  smoke: compositor is listening on wayland-1
  smoke: configure(serial 2) -> ack(2) -> attach/commit -> keyboard.enter -> frame callback #25 done at line 104; cursor surfaces set: 0
  smoke: PASS
  ```

  `cursor surfaces set: 0` is expected: the harness cannot move the host
  pointer, so no client asks for a cursor (Block 3).
- **`./dev.sh check`** — carries the unit tests for this block's arithmetic and
  bookkeeping: `usable_size_subtracts_the_top_inset`, `..._both_axes`,
  `..._is_the_window_when_nothing_is_reserved` and `..._never_collapses`; the
  session error-chain tests; and the `neptune-scene` coordinate conversions the
  render offsets use.
- **`cargo run -p sctk-probe --bin parent_probe`** — maps two toplevels and
  toggles a parent on the child, asserting the child is reconfigured
  `1248×800 → 1280×800 → 1248×800` on a 1280×800 output (recorded in
  `01-session.md`); the probe's README has the command that finds a running
  compositor's socket.
- **`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`** — the
  documentation build is warning-free.

Not verified by a command, and left to hand-checking: that a real client's
contents are legible in the nested window. The acceptance client is `foot`,
because weston is not installed on this machine.

## Open questions

1. **`OUTPUT_SCALE` is a literal.** It is `1` behind one named constant, and
   its two call sites — `State::logical_output_size` and the element placement
   in `session::draw` — change together when scaling lands. A HiDPI output
   needs `2`; fractional scaling needs a per-surface value rather than one
   global integer. The `TODO(scale)` in `state.rs` names both call sites.
2. **A lost EGL context is logged, not recovered.** A draw failure is turned
   into a `tracing::error!` and the loop keeps running so a later event can try
   again; nothing re-creates the renderer or the framebuffer.
3. **The damage history is thrown away on a host resize.** The tracker's mode
   source is static, so a resize builds a fresh tracker rather than updating
   one. That redraws the whole surface, which is what a size change needs, but
   it is not a general damage-tracking story.

## Decision records

- `docs/decisions/0009-winit-backend-and-gles2.md` — the nested backend is
  winit with the GLES2 renderer, `desktop` off.
- `docs/decisions/0010-smithay-source-pin.md` — Smithay 0.7.0 from crates.io.
- `docs/decisions/0012-backend-beside-state.md` — backend state lives in
  `Session`, not `State`.
- `docs/decisions/0014-decoration-insets.md` — insets are a function of window
  state.
- `docs/decisions/0015-desktop-space-and-neptune-scene.md` — no
  `desktop::Space`; `neptune-scene` owns window layout.
- `docs/blocks/01-session.md` — the session-level record: Parts, Part 5
  rulings, Divergences 1.
