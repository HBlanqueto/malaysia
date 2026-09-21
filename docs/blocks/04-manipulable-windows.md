# Block 4 — Manipulable windows

> **Roadmap mapping.** This is the detailed record of roadmap **Block 4**
> (Manipulable windows). Its layout work lives in `neptune-scene` (ADR 0015) and
> its interaction model in ADR 0016.

Status: **complete, with hand-checked gaps.** The machine-checked acceptance is
`./dev.sh block4`; drag feel, resize visuals and the cursor shape changing under
the pointer are hand-checked.

## Purpose

Block 4 makes a mapped window manipulable: it can be moved, resized and closed,
it stacks and takes focus on click, and its edges show the matching resize
cursor. The work is split at the boundary ADR 0015 drew: `neptune-scene` owns
where windows are and how they stack; the compositor owns the protocol surfaces
and the interaction.

## What was built

### `neptune-scene`

- `Rect::contains`, `Rect::right` and `Rect::bottom` — the rectangle predicates
  the layout needs (right and bottom are exclusive).
- `scene::{WindowId, Edge, Scene}` — an ordered back-to-front stack keyed by an
  opaque id, with `insert`/`remove`/`raise`, outer geometry, point hit-testing
  (`topmost_at`) and resize edge zones (`edge_at`). It has no Smithay
  dependency; the compositor converts at the boundary.
- `scene::resized` — the decorated outer rectangle produced by dragging an edge
  or corner: leading edges move the origin and keep the opposite edge fixed, and
  the size is clamped to a minimum and an optional maximum.
- `neptune` no longer merely sits beside `neptune-scene`; it depends on it.

### `crates/neptune`

- **`window.rs`** is rebuilt around the scene. `WindowData` holds the
  `ToplevelSurface`, the outer size and the configure bookkeeping; a `Window`
  view joins it to the scene geometry for readers. `Windows` exposes stacking,
  `raise`, `topmost`, hit-testing (`window_under`, `edge_under`, `surface_under`),
  `move_to`, `resize`, `set_resizing`, `send_close`, `id_of`/`surface_of`,
  `minimum_outer_size`/`maximum_outer_size`, and `outer`. Mapping inserts the
  window into the scene and logs `window mapped`.
- **`input.rs`** owns the interaction: an edge band starts a resize, the primary
  button on the decorated band starts a move, and a click over the client raises
  and focuses. Motion drives the grab or updates focus and the edge cursor;
  release ends it. `forget_window` removes a destroyed window from the scene and
  any grab, and moves keyboard focus to the new topmost window or clears it.
- **`state.rs`** holds `Grab` (move or resize), the remembered pointer-press
  serial, and the seat focus handshake.
- **`handlers.rs`** answers `move_request` and `resize_request` after the serial
  check, and routes `toplevel_destroyed` to `forget_window`.
- **`cursor.rs`** loads the `ew`/`ns`/`nwse`/`nesw` resize shapes from the
  Xcursor theme, alongside the default, and the draw path picks the shape
  Smithay reports. A missing shape or theme falls back to the default arrow.
- **`handlers.rs`** also serves `zxdg_decoration_manager_v1` and always configures
  server-side, so a client stops drawing its own chrome (ADR 0017).

### Closing

`Windows::send_close` sends `xdg_toplevel.close`, and a client that exits
tears its window out through `toplevel_destroyed`. Neptune draws no close
button and binds no gesture to close; a compositor-initiated close is left for a
later block, so closing is driven by the client (or by killing it).

## Decisions

- **ADR 0015** — window layout lives in `neptune-scene`; Smithay's
  `desktop::Space` is not adopted. Block 4 is the first consumer of that decision.
- **ADR 0016** — interactive move and resize are compositor-owned state
  (`State::grab`), not Smithay `PointerGrab`s, and client requests are
  serial-checked against the last pointer press.

## Verification

The machine-checked acceptance is a command whose exit code is the result:

- **`./dev.sh block4`** — launches a compositor and three batches of the
  `window_stress` client (fifty decorated toplevels with shm buffers, then
  destroyed). It asserts that the live-object counts return to the startup
  baseline after every batch (no orphaned clients, `wl_surface`s or toplevels),
  and that resident memory does not grow from batch 2 to batch 3 by the stated
  threshold. Observed:

  ```
  block4: batch 1 settled at 89680 kB
  block4: batch 2 settled at 89720 kB
  block4: batch 3 settled at 89720 kB
  block4: 150 windows removed over 3 batches, live objects restored to baseline; resident memory warm-up 89680 kB, batch 2 89720 kB, batch 3 89720 kB, batch 2->3 growth 0 kB (asserted below 1024 kB)
  block4: PASS
  ```

  The first batch is warm-up. The threshold is 1 MiB: after warm-up a batch
  should reuse the same allocations, so the growth is allocator noise (tens of
  kB), while a per-window leak would be far larger — even ~20 kB per window over
  50 windows is ~1 MB. Live-object restoration and the memory threshold are both
  asserted. The leak check is proved fallible: with `NEPTUNE_TEST_LEAK_WINDOW`
  set, the compositor deliberately leaves one window behind and `block4` fails
  with `last seen Some(LiveObjects { clients: 0, surfaces: 0, toplevels: 1 })`.
- **`./dev.sh move`** — launches a compositor with `NEPTUNE_TEST_INPUT=1` and a
  `move_client` probe. The compositor injects the button presses a probe cannot
  make, so the client can send `xdg_toplevel.move` and `xdg_toplevel.resize` with
  valid serials, then repeats both with a fabricated serial. The harness asserts
  the compositor accepted the valid requests and rejected the fabricated ones,
  and the client asserts it received a configure carrying the `resizing` state.
  Observed:

  ```
  move_client: sent move with press serial 4
  move_client: sent resize with press serial 7
  move_client: sent move/resize with fabricated serial 3735879681
  move_client: received a configure with the resizing state
  move_client: PASS
  move_probe: PASS
  ```
- **`./dev.sh check`** — carries the unit tests: the scene's stacking,
  hit-testing, edge zones and `resized` clamping; the one-shot pointer-press
  serial (`consume`); and the edge-to-cursor mapping.
- **`./dev.sh smoke`** — asserts the mapping sequence and that a client
  (`foot`) receives the server-side decoration configure before its first
  buffer attach, so the rebuild of `window.rs` did not regress mapping and the
  decoration mode is settled in time.

Not verified by a command, and left to hand-checking: drag feel and whether the
window keeps the grab offset under the pointer; resize visuals and whether the
minimum and maximum feel right; the cursor shape actually changing as the
pointer crosses an edge; and click-to-focus and stacking as seen.

## Open questions

1. **No movement constraints.** A window can be dragged beyond the output and
   recovered only by dragging it back; tiling and snapping are not modelled.
2. **Maximized and fullscreen are not special.** They still get a titlebar and
   edge zones, so a maximized window can be dragged. The brief stops at ordinary
   windows.
3. **Focus does not follow the pointer.** It changes on click and on the focused
   window's destruction only.
4. **The grab and the cursor override interact.** A resize cursor is only
   restored to the default when the active status is `Named`; a client that set
   its own surface cursor keeps it, which is correct over its content but not
   over Neptune's decoration.
5. **`NEPTUNE_TEST_INPUT` is test-only but lives in production code.** The
   compositor injects a pointer press on map and after an accepted move, so a
   probe can exercise the client-initiated path. It is off unless the variable
   is set, and it is the one test seam in the compositor itself rather than in
   the dev-only probes.

## Decision records

- `docs/decisions/0015-desktop-space-and-neptune-scene.md` — the scene owns
  window layout.
- `docs/decisions/0016-interactive-move-and-resize.md` — moves and resizes are
  the compositor's own state, with serial validation.
- `docs/decisions/0017-xdg-decoration-is-server-side.md` — xdg-decoration is
  served and always server-side.
