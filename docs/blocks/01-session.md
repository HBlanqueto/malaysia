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

## Part 5 review rulings

The Part 5 review produced six rulings. Five changed code and are recorded in
their own commits: advertising `wl_data_device_manager`, one accessor for
logical output conversion, matching configure serials before marking a window
ready, mapping on the first buffered commit (Divergences 1), and a live
`set_parent` probe (now `tools/sctk-probe`, see Verification). The sixth changed
nothing, so it is recorded here.

**Ruling 6 — the window tracker keeps `Window::location` and `Windows::iter`.**
Both were questioned as API with no consumer yet. They have consumers today:
the render path reads each `Window::location` to place its surface and iterates
`Windows` to collect the surfaces to draw. The tracker itself is a placeholder
for `neptune-scene` in Block 2 — this module is what gets rewritten then — so
the two methods are the smallest surface that serves the draw path and nothing
is built on top of them.

## Verification

The block's checks are commands whose exit code is the result, not a log read by
eye:

- **`./dev.sh check`** — rustfmt, `clippy -D warnings` across the workspace, and
  every test. It includes the probe harness's protocol-parser tests, which cover
  each mapping assertion below, including that a `wl_display.sync` completion is
  *not* accepted as a frame callback.
- **`cargo run -p sctk-probe --bin parent_probe`** — drives two toplevels against
  a running compositor and asserts that setting and unsetting a parent
  reconfigures the child (`1248×800 → 1280×800 → 1248×800` on a 1280×800
  output). It exits non-zero if the cycle is not reproduced.
- **`cargo run -p sctk-probe --bin smoke`** — launches the compositor and `foot`,
  captures the client's `WAYLAND_DEBUG` trace, and asserts, in order:
  `xdg_toplevel.configure` before the first non-null attach; a matching
  `ack_configure` serial before that attach; a non-null buffered commit that the
  compositor reports as mapped; `wl_keyboard.enter` on that surface after the
  commit; and completion of the callback id resolved from `wl_surface.frame`. It
  exits non-zero on the first missing or out-of-order step.
- **`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`** — the
  documentation build is warning-free.

Both binaries live in `tools/sctk-probe`, a dev-only workspace member excluded
from the default build (see its README for the exact commands). They need the
dev shell and a host display.

Not verified by a command, and left to hand-checking: that typing reaches the
client with the correct layout, that the pointer responds, and that the client's
output is legible.

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
