# 0012 — The backend lives beside the compositor State

Status: accepted (Block 1, Part 1)

## Context

Block 1 turns the private `State` stub in `session.rs` into the real
compositor state. It currently holds one thing, the calloop `LoopSignal`, and
the winit window, the `GlesRenderer` and the damage tracker live in locals
inside `session::run`.

Block 2 replaces winit with DRM/KMS. The question this record settles is where
the backend's handles go: inside `State`, where protocol handlers can reach
them, or beside it, where they cannot. Get it wrong and every handler has to be
edited when the backend changes.

Two references in the same Smithay version show the two poles:

- **niri** (`src/backend/mod.rs`) puts a three-variant `Backend` enum inside its
  `State` (`State { backend, niri }`), with one method per capability
  (`render`, `seat_name`, `import_dmabuf`, ...) and an `&mut` accessor per
  variant. Handlers call through the enum and never name a concrete backend.
- **smallvil** (`src/state.rs`, `src/winit.rs`) keeps protocol state in
  `Smallvil` and the winit backend in a local captured by the loop closure. The
  compositor state never sees the backend at all.

## Options considered

1. **Concrete backend fields in `State`.** `state.backend: WinitGraphicsBackend<GlesRenderer>`,
   `state.renderer`, `state.damage_tracker`. Simplest today, but every handler
   that requests a frame or imports a buffer names the winit types, so Part 2 of
   Block 2 edits handlers across the tree. Rejected.
2. **niri's `Backend` enum in `State`.** The swap point is a type, and the
   compiler keeps handlers honest across variants. But niri needs it because it
   ships TTY, winit and headless *simultaneously*; Neptune has exactly one
   backend per run. An enum with a single variant today is the speculative
   abstraction the project's rules forbid, and it would still have to grow a
   method for every backend operation we discover. The layering is right; the
   enum is not due yet.
3. **Backend beside `State` (smallvil's shape).** `State` is protocol and scene
   bookkeeping only; the loop's owner (`session.rs`) holds the window, renderer
   and damage tracker in its own scope. Handlers receive `&mut State` and
   physically cannot name a backend type. Adopted.

## Decision

`State` stores no backend type. It lives in its own public module
(`crate::state`) and holds the protocol state structs and the loop signal —
enough to mutate compositor state and wake the loop. The backend stays in
`session.rs`, captured by the event sources that own it.

The interface from `State` to the backend is narrow and backend-independent:
handlers change state and call `State::stop` (or, later, set a redraw flag and
wake the loop), and the loop reads the state when the backend asks it to draw.
No winit, GLES, DRM or scanout type appears in `State` or in any handler.

`State` will gain `DisplayHandle`, `CompositorState`, `ShmState`, `SeatState`,
`XdgShellState`, `OutputManagerState`, the `Seat`, the scene wrapper and the
start time as the parts that create them land. The protocol structs cannot be
constructed any earlier: Smithay's constructors (`CompositorState::new`,
`ShmState::new`, `XdgShellState::new`, `OutputManagerState::new_with_xdg_output`)
require the state type to implement their `Dispatch`/`GlobalDispatch` traits
first, and those impls are the handlers themselves.

## Consequences

- Swapping winit for DRM/KMS is surgery on `session.rs` (and, when it grows,
  a `backend` module beside `state.rs`) plus the scene. No handler changes.
- `session.rs` remains the single owner of the loop and the window. When Block 2
  arrives and there are two real backends, option 2 can be reconsidered from a
  position of evidence rather than guessing.
- Handlers are testable and readable without a graphics stack, because they
  depend only on `State`.
- The cost is one indirection in the draw path: the loop must read what changed
  from `State` instead of handlers rendering directly. That indirection is the
  redraw cycle Part 6 needs anyway, so it is not extra.
- `State` is public, so its fields are part of the crate's API. That is
  deliberate — handlers in `crates/neptune` are the consumers.
