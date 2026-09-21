# 0013 — Client dispatch polls a duplicated display file descriptor

Status: accepted (Block 1, Part 2)

## Context

Block 1 Part 2 gives Neptune a `Display<State>`, a listening socket and a way
to feed client requests into the calloop loop without ever blocking it. The
calloop bridge is the only real choice in this part, and the obvious answers
are blocked in this tree.

The canonical Smithay pattern, used by both references, wraps the display
itself in a `Generic` and takes it back out to dispatch:

```rust
insert_source(Generic::new(display, Interest::READ, Mode::Level), |_, display, data| {
    unsafe { display.get_mut().dispatch_clients(data) };
    Ok(PostAction::Continue)
})
```

`Generic::get_mut` is `pub unsafe fn` — the mutable reference it hands out
could drop the polled object while it is still registered. `crates/neptune`
denies unsafe code (ADR 0003), so that pattern is unavailable.

## Options considered

1. **`calloop-wayland-source`.** The crate exists for exactly this shape of
   problem, but it adapts a `wayland_client::EventQueue` and `Connection` — it
   is client-side. A compositor has a `wayland_server::Display`. Rejected.
2. **`Generic<Display<State>>` with `unsafe { get_mut() }`.** The community
   pattern (anvil, smallvil). Correct, but violates the crate's unsafe policy
   for the sake of one call. Rejected.
3. **A hand-written `EventSource` wrapping the display.** Its `process_events`
   receives `&mut Self::Metadata`, and the loop's `State` arrives separately as
   the loop data; the source has no way to reach `State`, so it cannot call
   `dispatch_clients(&mut State)`. Rejected.
4. **Loop data carries both, and a `Generic` polls a duplicated fd.** Make the
   loop data `Session { display, state }` and insert a `Generic` over a second
   file descriptor for the display, obtained safely with
   `BorrowedFd::try_clone_to_owned`. When it is readable, the closure
   destructures `Session` into its two disjoint fields and calls
   `display.dispatch_clients(state)`. Adopted.

## Decision

Dispatch through option 4. The display is owned by `Session`, which is the
calloop loop data; `State` stays protocol-only as ADR 0012 requires, and
handlers still receive `&mut State`. No new dependency is added and no unsafe
is used.

The duplicated descriptor refers to the same socket, so poll readiness is
shared with the display's own descriptor; the display keeps its original and
the poller watches the copy. `dispatch_clients` and `flush_clients` are
non-blocking, so the source only runs when there is something to read and a
client that stops reading or writing cannot stall the loop. A dispatch or flush
error is logged and the loop continues: one bad client does not stop the
compositor.

## Consequences

- `State` is no longer the calloop loop data; `Session` is, and it is private
  to `session.rs`. `State` remains the type protocol handlers mutate, so ADR
  0012's guarantee is unchanged.
- Registering the display costs one `fcntl` duplicate at startup. This is the
  price of not using unsafe; it is paid once, not per event.
- When Block 2 replaces the backend, this source is untouched: the backend
  reaches into `Session.state` the same way, and the display is not
  backend-specific.
- `WAYLAND_DISPLAY` is **never set in the compositor's own environment**. In
  edition 2024 `std::env::set_var` is `unsafe` because a concurrent reader is
  undefined behaviour, and this crate forbids unsafe — but mutating the whole
  process environment is also wrong on its own terms: a nested compositor that
  exports its own socket name can then confuse itself or a sibling for the
  host. The socket name is logged at startup, and a child the compositor spawns
  is given the variable with `Command::env`, which is safe and scoped to that
  child. The first spawn (Block 1, Part 6) implements it; until then the test
  harness passes `WAYLAND_DISPLAY` explicitly.
- `wayland-info` (from `wayland-utils`) and `foot` join the dev shell so the
  Part 2 and later acceptance checks run against pinned tools.
- `dev.sh` gives the compositor its own `XDG_RUNTIME_DIR`, so its socket is not
  created next to the host session's. The nested winit client still reaches the
  host by being given the host socket as an absolute `WAYLAND_DISPLAY`.
