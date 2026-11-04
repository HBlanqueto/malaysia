# 0004 — Round logical rectangles to integer space

Status: accepted — implemented in `yuki-scene` (2026-10-21).

## Context

Rounding to nearest opens one-pixel seams between adjacent surfaces with fractional positions; at scale 1.25 this is the norm, not an edge case.

## Decision

The position is rounded with `floor` and the size with `ceil`. Two adjacent rectangles share the edge pixel and neither under-covers. `scale_up` was split into `scale_floor` and `scale_ceil`; rounding to nearest disappears from the API.

## Consequences

The compositor converts with Smithay's types (ADR 0001): `to_i32_floor` for the position and `to_physical_precise_up` for the size. `malaysia-scene` no longer has its own conversions.

## Rejected alternatives

- Rounding to nearest: it leaves seams.
