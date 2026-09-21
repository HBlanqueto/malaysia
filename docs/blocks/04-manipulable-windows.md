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
- **`state.rs`** holds `Grab` (move or resize) and the seat focus handshake.
- **`handlers.rs`** answers `move_request` and `resize_request` after the serial
  check, and routes `toplevel_destroyed` to `forget_window`.
- **`cursor.rs`** loads the `ew`/`ns`/`nwse`/`nesw` resize shapes from the
  Xcursor theme, alongside the default, and the draw path picks the shape
  Smithay reports. A missing shape or theme falls back to the default arrow.
- **`handlers.rs`** serves both decoration protocols: `zxdg_decoration_manager_v1`
  and `org_kde_kwin_server_decoration_manager`. The mode each client negotiates
  is stored per window in `neptune-scene` and read for insets, hit-testing and
  drawing (ADR 0019).

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
- **`./dev.sh move`** — launches a compositor with `NEPTUNE_TEST_INPUT=1` and
  runs three cases. `move_client` repeats a GTK-shaped gesture: press, three
  motions past a drag threshold, a second window that takes focus, then
  `xdg_toplevel.move` and `xdg_toplevel.resize` with the original press serial
  while the button is held. The harness asserts the compositor accepted both,
  moved the window by the injected pointer delta, and rejected the released and
  fabricated serials; the client asserts it received a configure carrying the
  `resizing` state. `hover_client` then asserts the surface-local coordinates of
  a buttonless hover: the drag moves the window to a non-zero origin and the
  release ends the grab, the compositor hovers the pointer, and the harness
  asserts the enter and motion the client read. Finally it drags a real Nautilus
  headerbar and asserts its window moved by the same delta (UNVERIFIED if
  Nautilus is not installed). Observed:

  ```
  move_client: got the press serial 7
  move_client: mapped a second window to take focus
  move_client: sent move/resize with serial 7 after a threshold and a focus change, while held
  move_client: sent move/resize with serial 3735879681 (fabricated)
  move_client: received a configure with the resizing state
  move_client: sent move/resize with serial 7 after the release
  move_client: PASS
  hover_client: got the press serial 7
  hover_client: sent move/resize with serial 7
  hover_client: enter x=32 y=32
  hover_client: motion x=40 y=40
  hover_client: PASS
  move_probe: the hovered client read the pointer at the surface-local positions
  move_probe: nautilus PASS (its headerbar drag moved the window by the pointer delta)
  move_probe: PASS
  ```
- **`./dev.sh check`** — carries the unit tests: the scene's stacking,
  hit-testing, edge zones and `resized` clamping; the held-grab serial check
  (`held_grab`); the edge-to-cursor mapping; the host-pointer corner mapping;
  the decoration mode mappings and scene storage; and the offscreen pixel tests
  that assert the frame is upright and the cursor lands on its hotspot-adjusted
  position (ADR 0018).
- **`./dev.sh smoke`** — asserts the mapping sequence and that a client
  (`foot`) receives the server-side decoration configure before its first
  buffer attach, so the rebuild of `window.rs` did not regress mapping and the
  decoration mode is settled in time.
- **`./dev.sh decoration`** — runs real applications inside Neptune and asserts
  the decoration protocol and mode each one negotiated, plus the mode-change
  client (ADR 0019). See the follow-up below.

Not verified by a command, and left to hand-checking: drag feel and whether the
window keeps the grab offset under the pointer; resize visuals and whether the
minimum and maximum feel right; the cursor shape actually changing as the
pointer crosses an edge; and click-to-focus and stacking as seen.

## Follow-up: the inverted frame and the decoration policy

Two defects found after Block 4 was accepted are fixed here, each with its own
decision record:

- **The frame rendered upside down.** The winit output's transform stays
  `Normal` and the damage tracker's static mode source carries `Flipped180`, so
  the flip happens only in the projection matrix and no client sees
  `flipped_180` in `wl_output.geometry` (ADR 0018). The offscreen pixel tests
  in `./dev.sh check` fail with the old transform and pass with the fix; they
  assert the quadrant orientation and the cursor's hotspot-adjusted position.
- **Forcing server-side mode put Neptune's titlebar on self-decorating
  clients.** ADR 0017 is superseded by ADR 0019: a client that negotiates and
  accepts server-side gets compositor decorations, one that explicitly asks for
  client-side or never negotiates keeps its own chrome and gets no shadow, the
  mode is settled before the first attach, and a change after mapping
  reconfigures mode and geometry. Both decoration protocols are served, and the
  mode lives per window in `neptune-scene`.
- **Pointer hover coordinates were offset by the window origin.** `pointer_motion`
  passed `surface_under`'s surface-local point where Smithay expects the
  surface's origin in global coordinates
  (`smithay-0.7.0/src/input/pointer/mod.rs:220-243`), so a client read the window
  origin for every hover motion — and a press could land on a widget nowhere near
  the pointer. The fix passes `location - local`. `hover_client` through
  `./dev.sh move` asserts the surface-local positions end to end; with the old
  mapping it fails reading `enter x=60 y=40` (the origin) where `x=32 y=32` is
  expected.

`./dev.sh decoration` is the record of the per-application verification, and
`decoration_client` pins the mode-change rule. What it showed:

| Application | Protocol | Mode | Chrome | Result |
| ----------- | -------- | ---- | ------ | ------ |
| nautilus (GTK4 + libadwaita) | KDE | none negotiated | none | verified |
| gtk3-demo, gtk3-widget-factory | KDE | client | none | verified |
| gtk4-demo | KDE | none negotiated | none | verified |
| gtk3-plain, gtk4-plain (built) | KDE | server | 32 px inset | verified |
| qt6 (`qml`) | xdg-decoration | server | 32 px inset | verified |
| gtk4-widget-factory | KDE | — | — | unverified: the demo binary aborts before mapping |
| qt5 | — | — | — | unverified: not installed |
| onlyoffice-desktopeditors | none | none | none | unverified: the bundle ships only the XCB platform plugin |

GTK4 asks through the KDE protocol **before its xdg_toplevel exists**, so a mode
for an untracked surface is remembered and applied when the toplevel is created;
`gtk4-plain` pins that ordering. Whether two titlebars are visible is not
machine-checkable here and stays manual — and is moot until a chrome painter
exists: Neptune reserves the titlebar band but draws no titlebar, hairline or
shadow yet.

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
   compositor presses once, on the first window that maps, holds the button,
   follows the press with motions and applies a drag once a request is
   accepted, so a probe can exercise the client-initiated path. It is off
   unless the variable is set, and it is the one test seam in the compositor
   itself rather than in the dev-only probes. `NEPTUNE_TEST_PRESS_AT` and
   `NEPTUNE_TEST_DRAG` aim the press and size the drag, and
   `NEPTUNE_TEST_HOVER` hovers buttonless points after the release so a client
   can assert the coordinates it receives.

## Decision records

- `docs/decisions/0015-desktop-space-and-neptune-scene.md` — the scene owns
  window layout.
- `docs/decisions/0016-interactive-move-and-resize.md` — moves and resizes are
  the compositor's own state, with serial validation.
- `docs/decisions/0017-xdg-decoration-is-server-side.md` — xdg-decoration is
  served and always server-side. Superseded by 0019.
- `docs/decisions/0018-winit-render-transform.md` — the winit output flips at
  render time through the damage tracker's mode source; `wl_output` stays
  `normal`.
- `docs/decisions/0019-decoration-policy.md` — server-side is the default and
  the answer to an unset mode, but an explicit client-side request is honoured.
