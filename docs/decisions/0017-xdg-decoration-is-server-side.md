# 0017 — xdg-decoration is served, and always server-side

Status: **accepted** (Block 4 follow-up)

## Context

Neptune draws its own window chrome — titlebar, border, shadow (ADR 0014).
A client only lets the compositor decorate it if it knows the compositor wants
to: `zxdg_decoration_manager_v1` is the protocol for that. Without the global, a
decorating client assumes no one will draw chrome for it and falls back to
**client-side decorations**.

That is exactly what happened with `foot`: its trace under an earlier Neptune
showed no `zxdg_decoration_manager_v1` global and the message `no decoration
manager available - using CSDs unconditionally`. A `foot` window could therefore
show its own titlebar inside Neptune's decorations: double chrome.

The protocol permits the compositor to keep server-side even when the client
requests client-side, as long as it says which mode it chose in the decoration
configure.

## Decision

Advertise `zxdg_decoration_manager_v1` and configure **server-side** mode
always. `new_decoration` and `request_mode`/`unset_mode` are all answered with a
configure carrying `ServerSide`; a client that requests client-side is still
told server-side. Every xdg configure already carries the mode, so the client
learns it before it attaches its first buffer.

A client that never binds the global is unaffected: it keeps whatever
decorations it had, which for a non-decorating client means none.

## Consequences

- A decorating client stops drawing its own chrome, so Neptune's titlebar and
  border are not doubled. `foot` now binds the global, requests server-side, and
  receives `configure(server_side)` before its first `wl_surface.attach`; the
  smoke harness asserts that ordering.
- Neptune must supply a close affordance itself; it draws no close button yet,
  and compositor-initiated close has no gesture (Block 4 removed the
  middle-click). That is a decoration gap, not a protocol one.
- The mode is settled in the initial configure, so there is no window that maps
  first and switches decoration later.

## References

- Smithay `src/wayland/shell/xdg/decoration.rs` — `XdgDecorationState`,
  `XdgDecorationHandler`.
- `crates/neptune/src/state.rs` — `decoration_state`.
- `crates/neptune/src/handlers.rs` — `XdgDecorationHandler`.
- `crates/neptune/src/window.rs` — `send_configure` sets `ServerSide`.
- `tools/sctk-probe/src/bin/smoke.rs` — the server-side-before-attach assertion.
