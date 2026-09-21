# Block roadmap

The compositor is built in blocks. Each block is a working increment with an
acceptance test. This file is the canonical list of what the blocks are; the
per-block documents in this directory record what each block actually built,
including any deliberate divergence from the brief.

## Block 0 — Scaffolding

- Cargo workspace with `neptune` and `neptune-design` (empty)
- Smithay pinned to a fixed `rev`, with features: `backend_winit`, `renderer_gl`,
  `wayland_frontend`, `desktop`
- `tracing` + `tracing-subscriber` with env-based filtering
- Coordinate types: `Logical`, `Buffer`, `Physical` as distinct types from commit 1
- Builds and runs, doing nothing

## Block 1 — Getting a client to connect

- `State` holding `CompositorState`, `ShmState`, `SeatState`, `XdgShellState`,
  `OutputManagerState`, `DataDeviceState`
- calloop event loop plus a Wayland socket, with `WAYLAND_DISPLAY` exported to the child
- `wl_compositor`, `wl_subcompositor`, `wl_shm` with `Argb8888`/`Xrgb8888`
- `wl_output` with `geometry`, `mode`, `scale`, `done`
- `wl_seat` with capabilities and a valid xkb keymap
- `xdg_wm_base` answering `ping`/`pong`
- Acceptance: `weston-info` lists every global

## Block 2 — A window on screen

- winit backend and a basic render loop
- `CompositorHandler::commit` applying double-buffered state correctly
- `XdgShellHandler::new_toplevel` → initial `configure` before the client attaches a buffer
- `ack_configure` → window mapping
- A dedicated module wrapping `desktop::Space` — no other file calls it directly
- `wl_shm` buffer import and rendering
- Acceptance: `weston-simple-shm` displays its pattern

## Block 3 — Input

- Keyboard focus on map, `wl_keyboard.enter`/`leave`
- Keymap, modifiers, and key repeat
- Pointer focus via hit-testing, `motion`, `button`, `axis`
- Cursor: the client's `wl_pointer.set_cursor` and a default cursor
- Serial accounting — one central counter, never fabricated serials
- Acceptance: `foot` opens, shows a prompt, accepts typing, and the layout is correct

## Block 4 — Manipulable windows

- Drag-to-move and `xdg_toplevel.move`
- Resize via `xdg_toplevel.resize` and its edges
- Close: `close` and cleanup when the client dies
- Stacking and click-to-focus
- Acceptance: open and close 50 windows with no orphaned surfaces and no memory growth

## Block 5 — Popups

- `PopupManager` and `XdgShellHandler::new_popup`
- Position against the parent in your own space
- Constraint adjustments (flip, slide, resize) against the monitor's usable area
- Popup `configure` before mapping
- `PopupKeyboardGrab` / `PopupPointerGrab` with serial validation
- A click outside closes from leaf to root; `Esc` closes one level
- Nested submenus
- Acceptance: a `foot` menu against the bottom edge flips upward; a submenu plus a
  click outside closes both; killing the client with a popup open leaves no orphans

## Block 6 — Clipboard

- `wl_data_device_manager`, selection and `primary_selection`
- Acceptance: copy in `foot` and paste into another instance

## Where this repository stands

- **Block 0 — done, with divergences.** The coordinate types live in the
  `neptune-scene` crate, not in the compositor (ADR 0001). Smithay is pinned to
  the crates.io 0.7.0 release with `desktop` deliberately off, not a git `rev`
  with `desktop` on (ADR 0009, ADR 0010). The log level is set by `--log-level`,
  not env-based filtering. `neptune-design` is not empty.
- **Block 1 — done.** `wayland-info` (the dev shell has no `weston-info`) lists
  every global. See `docs/blocks/00-foundations.md` and, for the later parts,
  `docs/blocks/01-session.md`.
- **Block 2 — done, with divergences.** Mapping happens on the first buffered
  commit, not on `ack_configure` (`docs/blocks/01-session.md`, Divergences 1).
  Toplevels are tracked in `crates/neptune/src/window.rs` with no
  `desktop::Space` wrapper (ADR 0009). The acceptance client is `foot`, since
  weston is not installed.
- **Block 3 — partial.** Keyboard focus, keymap/repeat, pointer motion/button/
  axis and central serial accounting are implemented. **Cursor support is
  missing**: `wl_pointer.set_cursor` and a default cursor are not handled.
  Acceptance is partly asserted by `./dev.sh smoke`; typing, pointer response
  and legible output remain hand-checked.
- **Block 4 — not started.**
- **Block 5 — not started.** Popups are configured with raw geometry only.
- **Block 6 — not started.** `wl_data_device_manager` is advertised, but no
  clipboard behaviour is implemented.

## Numbering conflict, unresolved

The repository's own documents and code comments already use "Block 2" to mean
the DRM/KMS backend that replaces winit (ADR 0012, `state.rs`, `session.rs`,
`window.rs`, `docs/hardware.md`). That is **not** this roadmap's Block 2, which
is the already-built window-on-screen increment, and DRM/KMS does not appear in
this roadmap at all. `docs/blocks/01-session.md` likewise calls the whole
nested-session work "Block 1", spanning this roadmap's Blocks 1–3. The numbering
must be reconciled before it is used for planning; it is flagged here rather
than silently rewritten.
