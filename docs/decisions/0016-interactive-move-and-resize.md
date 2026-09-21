# 0016 — Interactive move and resize are the compositor's own state

Status: **accepted** (Block 4)

## Context

Block 4 adds interactive move and resize. A client asks for them with
`xdg_toplevel.move` and `xdg_toplevel.resize`, and the user can also start them
from Neptune's own decoration (titlebar and edge zones). Smithay offers a grab
framework — `smithay::input::pointer::PointerGrab`, installed with
`PointerHandle::set_grab` — which is available under the `wayland_frontend`
feature, so this is a real choice, not a forced one.

There is a second, later consumer: Block 5 (popups) needs keyboard and pointer
grabs for dismissal, and those *are* behind the `desktop` feature
(`PopupKeyboardGrab`/`PopupPointerGrab`), which ADR 0015 keeps off. Whatever is
decided here should not be mistaken for that.

## Options considered

1. **A Smithay `PointerGrab` per operation.** `MoveGrab` and `ResizeGrab` types
   implementing `PointerGrab`, installed on the seat for the duration. This is
   the framework's intended use and would stop motion from reaching the client
   automatically.
2. **A compositor-owned grab enum, driven from the existing input path.** Every
   pointer event already arrives in `input.rs`; while a grab is set, motion
   updates the window and is not forwarded. The grab is a small enum in `State`.

## Decision

Option 2. Block 4 does not install Smithay grabs. `State::grab` is
`Option<Grab>` (`Move` or `Resize`), started on a decoration press or a client
request, advanced by pointer motion, and cleared on pointer release or when the
window is destroyed.

The client requests are serial-checked, not trusted: `xdg_toplevel.move` and
`resize` are honoured only when their serial is the last pointer button press
`input.rs` generated from the single `SERIAL_COUNTER`, and that press is
consumed so it cannot authorise a second request.

## Consequences

- Motion during a grab is not forwarded to the client; the compositor owns the
  pointer for the operation. There is one place that decides this (`input.rs`),
  next to the motion source.
- Resize sends a configure carrying the xdg `resizing` state for the duration.
  The client's minimum and maximum, read from `SurfaceCachedState`, are honoured,
  and the scene's `resized` keeps the opposite edge fixed when a leading edge is
  dragged and a size clamp bites.
- Edge zones are measured against the window's **decorated** bounds
  (`Scene::edge_at`), not the client surface, because the two do not coincide
  (ADR 0014). The resize cursor shapes are drawn from the Xcursor theme
  (`ew`/`ns`/`nwse`/`nesw`), with the default arrow as fallback.
- The serial check is one function, `consume`, with unit tests for a matching
  press, a foreign serial, no press, and single use. `./dev.sh move` exercises
  the live path: `NEPTUNE_TEST_INPUT` makes the compositor inject the button
  presses a probe cannot make, so `move_client` can send valid requests and then
  fabricated ones.
- Block 5 cannot reuse this for popups: a popup grab must move keyboard and
  pointer focus and restore it, which is exactly what the `desktop`-gated
  `PopupKeyboardGrab`/`PopupPointerGrab` do. Block 4's grab deliberately does not
  try to be that; it only moves and resizes one window.

## References

- `crates/neptune/src/input.rs` — `pointer_press`, `drag`, `consume`, the edge
  cursor.
- `crates/neptune/src/state.rs` — `Grab`.
- `crates/neptune/src/window.rs` — `resize`, `set_resizing`, `send_close`,
  `minimum_outer_size`/`maximum_outer_size`.
- `crates/neptune-scene/src/scene.rs` — `Scene::edge_at`, `resized`.
- Smithay `src/input/pointer/mod.rs` — `PointerGrab`, the framework not adopted.
