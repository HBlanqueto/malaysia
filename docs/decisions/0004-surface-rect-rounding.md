# 0004 — Rounding logical rectangles into integer space

Status: **open** — decision deferred until surface placement exists

## Context

`crates/neptune-scene/src/geometry.rs` currently converts logical rectangles
into integer Physical/Buffer space by rounding the corner and the extents to
the nearest pixel. That is correct for a single, isolated rectangle. It is
wrong for the case that matters most in a compositor: two rectangles that
share an edge.

If two adjacent surfaces are at logical positions that both land on `x.5`,
round-to-nearest moves one left and one right, opening a one-physical-pixel
seam between them. With fractional output scales (for example 1.25) misaligned
logical edges are the norm, not the exception, so the seams would appear
regularly rather than as an edge case.

The conventional fix is to round asymmetrically:

- **position: floor**
- **size: ceil**

so a rectangle always covers the pixels it logically overlaps and adjacent
rectangles share a boundary pixel instead of leaving a gap. This is what
several compositors and renderers do when turning logical rectangles into a
pixel damage or scissor region.

## Options considered

1. **Round-to-nearest for position and size** (current implementation).
   Simple and symmetric per rectangle, but produces one-pixel seams between
   adjacent surfaces under fractional or non-integer logical positions.
2. **Floor position, ceil size.** No seams between adjacent surfaces and never
   under-covers, at the cost of occasionally drawing one pixel into a
   neighbour's space. This is a well-established convention.
3. **Caller-supplied rounding.** Each conversion takes the rounding mode it
   wants. Most flexible, but pushes a decision that has one correct default
   into every call site, and invites inconsistency.
4. **Keep logical values fractional all the way and let the renderer
   rasterise analytically.** Avoids the rounding question entirely, but is not
   possible with integer scissor/viewport APIs and adds cost that a
   desktop-scale compositor does not need.

## Decision

**Open.** Block 0 keeps round-to-nearest because nothing places surfaces yet
and there is no evidence to act on. The known rendering requirement is
recorded here so that the first block which places or damages surfaces
evaluates option 2 (floor position, ceil size) rather than rediscovering the
seam problem.

The API is deliberately not built yet: this is a rendering requirement, not
speculation, but it has no caller, and the no-speculative-API rule still
applies.

## Consequences

- Round-to-nearest in `geometry.rs` is a placeholder, not a final decision.
- When the first damage/scissor rectangle is produced, this ADR must be
  revisited and accepted or replaced before that code ships.
- The conversion API will likely need to express position and size rounding
  separately; the current `to_physical`/`to_buffer` signatures may change.
- No code changes are implied by this ADR today.

## References

- Rounding of logical rectangles to integer damage regions is discussed in
  wlroots/Smithay rendering code and in the fractional-scale protocol
  discussions; the floor/ceil convention is the common resolution.
