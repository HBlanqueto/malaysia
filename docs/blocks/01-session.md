# Block 1 — The nested Wayland session

Status: **in progress.** This document is completed when the block is.

## Purpose

Block 1 makes Neptune a compositor: a Wayland client can connect, and its window
appears inside the winit window Block 0 set up. It is deliberately nested — the
compositor runs as a client of the host session — so no test is a logout and no
seat or elevated privilege is needed (ADR 0009).

The block is built in parts, each reviewed on its own:

1. The compositor `State`, backend beside it (ADR 0012).
2. Display, listening socket and client dispatch (ADR 0013).
3. Globals: `wl_compositor`, `wl_subcompositor`, `wl_shm`, `wl_output`,
   `wl_seat` and `xdg_wm_base`.
4. The decoration type, written before the first window is mapped.
5. `xdg_shell` toplevel lifecycle: initial configure, ack, and map on the
   first buffered commit — *not* on the ack. The block spec's Part 5 wording
   ("ack_configure → map") is wrong; see Divergences 1, which is authoritative.
6. Rendering client surfaces and forwarding input from winit to the seat.

Out of scope: popups, move/resize, clipboard, XWayland, DRM/KMS.

## Divergences from the block spec

1. **Mapping is on the first buffered commit, not on `ack_configure`.** Part 5's
   wording says "ack_configure → map". The protocol maps a surface when it has a
   committed non-null buffer, not when a configure is acked; mapping on the ack
   would show a surface with no content. The window is marked ready by the ack
   (and only by the matching serial, see ADR/Part 6) and mapped by the first
   commit that carries a buffer. The block spec's literal wording is wrong on
   this point and is overridden here, deliberately.

## Open questions

1. **The `deny(unsafe_code)` policy must be redefined before Block 2.** It is
   currently absolute ("forbidden"), which already bent Block 1 in two places:
   the client-dispatch source cannot use calloop's `Generic::get_mut` and had to
   poll a duplicated descriptor instead (ADR 0013), and the compositor cannot
   call `std::env::set_var` to export `WAYLAND_DISPLAY`, so it passes the
   variable to spawned children with `Command::env` instead. Block 2 brings
   dmabuf, GBM and EGL, where unsafe is not avoidable — the raw fd and mapping
   APIs have no safe wrapper. The policy should become "fenced": unsafe is
   allowed only in modules explicitly marked for it, never in protocol logic,
   and every use carries a `// SAFETY:` comment stating the invariant it upholds.
   That is what niri and Smithay do. Decide this at the Block 1/Block 2
   boundary, not mid-block. *(Raised in Part 2.)*
