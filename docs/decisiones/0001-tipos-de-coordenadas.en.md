# 0001 — Coordinate types: Smithay's in the compositor, ours in `yuki-scene`

Status: accepted, revised.

## Context

Mixing logical, physical and buffer space is a classic source of bugs; Smithay already offers types that turn the mix into a compile error.

## Decision

The compositor uses `smithay::utils` without a wrapper. Our own types live in `malaysia-scene/src/geometry.rs` and are logical only. The step to physical and buffer space uses Smithay's types, where the compositor feeds the renderer.

## Consequences

Handlers and rendering use a single set of types; a crate boundary separates the two geometries and the conversion surface is small.

## Rejected alternatives

- Our own types in the compositor and a conversion in every handler: it replicated the geometry across the whole Smithay API (it was reverted).
