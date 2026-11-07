# 0012 — The backend lives next to the compositor State

Status: accepted (nested session, part 1).

## Context

Direct Session will replace winit with DRM/KMS.

## Decision

`State` holds no backend types: only protocol, scene and the loop signal. The backend (window, renderer, damage tracker) lives in `session.rs`, captured by the event sources. Handlers mutate `State` and request a redraw; the loop reads the state when drawing.

Amendment 7b: `State` holds a `LoopHandle` so that explicit sync blockers and deferred release can register one-shot sources. It is not a backend type.

## Consequences

Changing the backend touches `session.rs` and the scene, not the handlers; handlers are testable without a graphics stack.

## Rejected alternatives

- Concrete winit fields in `State`: it couples every handler to the backend.
