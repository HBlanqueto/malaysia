# 0016 — Interactive move and resize are the compositor's own state

Status: accepted (Block 4).

## Context

Move and resize arrive through `xdg_toplevel.move`/`resize` and through the decoration.

## Decision

`State::grab` is `Option<Grab>` (`Move` or `Resize`), started by a press on the decoration or a client request, advanced by motion and cleared on release or when the window is destroyed. Requests are validated against the pointer's implicit grab: the serial must name a press still held over the requesting surface (`has_grab(serial)` + start data). No Smithay grabs are installed for move/resize.

Edge zones are measured against the decorated rectangle (`Scene::edge_at`), not against the client surface. Minimums/maximums are respected and the opposite edge stays fixed.

## Consequences

The pointer is not forwarded to the client during the grab; `./dev.sh move` exercises the live path, including released and fabricated serials, and `./dev.sh decoration` covers real apps.

## Rejected alternatives

- A Smithay `PointerGrab` per operation: it was viable, but our own state is simpler and does not drag in the framework.
