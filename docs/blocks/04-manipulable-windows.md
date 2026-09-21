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
- **`input.rs`** owns the interaction: an edge band starts a resize, the
  decorated band starts a move (the middle button asks the client to close), and
  a click over the client raises and focuses. Motion drives the grab or updates
  focus and the edge cursor; release ends it. `forget_window` removes a destroyed
  window from the scene and any grab, and moves keyboard focus to the new
  topmost window or clears it.
- **`state.rs`** holds `Grab` (move or resize), the remembered pointer-press
  serial, and the seat focus handshake.
- **`handlers.rs`** answers `move_request` and `resize_request` after the serial
  check, and routes `toplevel_destroyed` to `forget_window`.
- **`cursor.rs`** loads the `ew`/`ns`/`nwse`/`nesw` resize shapes from the
  Xcursor theme, alongside the default, and the draw path picks the shape
  Smithay reports. A missing shape or theme falls back to the default arrow.

### The compositor-initiated close

`xdg_toplevel.close` is sent by `Windows::send_close`. Neptune draws no close
button, so the gesture bound to it is the **middle button on the titlebar**;
this is the one concrete interaction the block chooses without a brief.

## Decisions

- **ADR 0015** — window layout lives in `neptune-scene`; Smithay's
  `desktop::Space` is not adopted. Block 4 is the first consumer of that decision.
- **ADR 0016** — interactive move and resize are compositor-owned state
  (`State::grab`), not Smithay `PointerGrab`s, and client requests are
  serial-checked against the last pointer press.

## Verification

The machine-checked acceptance is a command whose exit code is the result:

- **`./dev.sh block4`** — launches a compositor and the `window_stress` client
  (fifty decorated toplevels with shm buffers, then destroyed), asserts the
  compositor reported at least fifty `window removed` lines and then `scene
  empty`, and reports the compositor's resident-memory delta. Observed:

  ```
  block4: 50 windows removed, scene empty; resident memory 88988 kB -> 90340 kB (delta 1352 kB, measured, not asserted)
  block4: PASS
  ```

  The scene count, stacking and focus state returning to empty are asserted. The
  memory delta is **measured, not asserted** — a threshold would be flaky.
- **`./dev.sh check`** — carries the unit tests: the scene's stacking,
  hit-testing, edge zones and `resized` clamping; the one-shot pointer-press
  serial (`consume`); and the edge-to-cursor mapping.
- **`./dev.sh smoke`** — still asserts the mapping sequence, so the rebuild of
  `window.rs` did not regress mapping.

Not verified by a command, and left to hand-checking: drag feel and whether the
window keeps the grab offset under the pointer; resize visuals and whether the
minimum and maximum feel right; the cursor shape actually changing as the
pointer crosses an edge; click-to-focus and stacking as seen; the middle-button
close; and the live client-initiated `move`/`resize` path, which no probe calls
yet.

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
5. **Client-initiated move/resize is untested live.** The serial check is unit
   tested and the request handlers compile against Smithay's trait, but no probe
   sends the requests, so the path is exercised only by a real application's
   titlebar.

## Decision records

- `docs/decisions/0015-desktop-space-and-neptune-scene.md` — the scene owns
  window layout.
- `docs/decisions/0016-interactive-move-and-resize.md` — moves and resizes are
  the compositor's own state, with serial validation.
