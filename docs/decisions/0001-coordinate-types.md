# 0001 — Coordinate types: Smithay's in the compositor, ours in `neptune-scene`

Status: **accepted, revised** (original Block 0 decision reversed on review)

## Context

Neptune manipulates three different notions of position:

- **Logical** — desktop coordinates, independent of output scale.
- **Physical** — real pixels on one output, after its scale factor.
- **Buffer** — pixels inside a client's buffer, before the compositor maps
  them onto an output.

Mixing them is a common source of compositor bugs, so Block 0 required types
that make the mix a compile error. Smithay already ships such types, and both
reference projects use them directly:

- **niri** imports `smithay::utils::{Logical, Physical, Raw, Size, ...}` and
  adds only monitor-scale selection (`src/utils/scale.rs`); it has no geometry
  module of its own.
- **Otto** likewise uses `smithay::utils::Point<f64, Physical>` in its
  rendering code.

Smithay models this with `Point<N, Kind>`, `Size<N, Kind>` and
`Rectangle<N, Kind>`, where `Kind` is one of `Logical`, `Physical`, `Buffer`,
`Raw` (plus an internal `Client`). Arithmetic is locked to a single `Kind`, and
conversions are explicit methods that take a `Scale<N>`:
`to_physical(scale)`, `to_logical(scale)`, `to_buffer(scale, transform)`.
`Logical` is `f64`; physical and buffer coordinates are integer.

## Original decision (superseded)

Define our own `Point<Space>`, `Size<Space>` and `Rect<Space>` in
`crates/neptune/src/geometry.rs` and convert at the Smithay boundary once
Smithay is introduced. This was justified as keeping the core free of
Smithay's feature flags.

## Why it was reversed

The original decision scored the cost wrong. It is not "a conversion layer
later". Every Smithay API a compositor touches — protocol handlers, the
positioner, `Space`, the seat, input events, output geometry — speaks
`smithay::utils::Point<i32, Logical>` and friends. Owning a parallel type means
converting in every one of those callbacks, which is exactly the redundancy the
project's "no speculative abstraction" rule exists to prevent. With zero real
implementations there was no evidence to justify a wrapper, and the reference
projects that have already paid this cost (niri, Otto) both chose Smithay's
types.

The split is by crate, not by taste:

- **`crates/neptune`** (the compositor) uses `smithay::utils` directly. No
  wrapper, no conversion layer.
- **`neptune-scene`** (created in Block 0 for this geometry; the scene graph
  itself is a later block) owns its own geometry, because the scene graph must
  not depend on Smithay.
- **Conversion happens at exactly one boundary**: where the compositor feeds
  the scene graph.

## Decision

1. The compositor uses `smithay::utils` for all coordinate types. A Smithay
   dependency is added when the first module actually needs geometry, not
   before, so Block 0/Part 3 introduces none.
2. The homegrown types move to `crates/neptune-scene/src/geometry.rs` now,
   with `neptune-scene` created as a workspace member holding only that
   module. The crate is declared from the start so the types are compiled and
   their tests run on every `./dev.sh check`; leaving the file outside the
   module tree would be equivalent to `#[allow(dead_code)]`.
3. In that module, logical coordinates are `f64` and physical/buffer
   coordinates are `i32`, so spring animation and fractional scales are not
   quantised. Conversion to integer happens only when entering an integer
   space.

## Consequences

- Protocol handlers and rendering code use one set of types with no
  conversions, which is the whole point.
- Two notions of geometry exist across the project (Smithay's and
  `neptune-scene`'s), but they are separated by a crate boundary rather than
  scattered through the compositor, so the conversion surface is small and
  locateable.
- `neptune-scene` exists before the scene graph does, with no dependencies,
  so that its geometry is validated while Block 2 is still being designed.
- The compositor has no geometry of its own, so it cannot accidentally depend
  on the scene crate's types instead of Smithay's.
- When the first Smithay-consuming module lands, the dependency and its
  feature set must be chosen and justified then.
- This is recorded as an open question in `docs/blocks/00-foundations.md`.

## References

- Smithay `src/utils/geometry.rs` — `Logical`, `Physical`, `Buffer`, `Raw`,
  `Point<N, Kind>`, `Size<N, Kind>`, `Rectangle<N, Kind>`, `to_physical`,
  `to_logical`, `to_buffer`.
- niri `src/utils/scale.rs` — uses Smithay's types directly.
- Otto `Cargo.toml` and rendering code — uses `smithay::utils` types directly.
