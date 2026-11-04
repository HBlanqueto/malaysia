# 0013 — Client dispatch polls a duplicated display descriptor

Status: accepted (nested session, part 2).

## Context

Smithay's canonical pattern wraps the `Display` in a `Generic` and uses `unsafe { get_mut() }`; this crate forbids `unsafe` (ADR 0003).

## Decision

The loop data is `Session { display, state }`. A `Generic` is inserted over a second display fd (`BorrowedFd::try_clone_to_owned`); when it becomes readable, the closure destructures `Session` and calls `display.dispatch_clients(state)`. The display keeps the original and the poller watches the copy. Dispatch and flush are non-blocking; an error is logged and the loop continues.

`WAYLAND_DISPLAY` is not mutated in the compositor's environment: the socket is registered at startup and passed to children with `Command::env`.

## Consequences

`State` remains protocol only; `Session` is private to `session.rs`.

## Rejected alternatives

- `calloop-wayland-source`: it is for clients, not for `wayland_server::Display`.
