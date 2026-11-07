# 0018 — The winit output is flipped at render time, never in `wl_output`

Status: accepted (Block 4 follow-up).

## Context

The frame came out upside down: the winit framebuffer has a bottom-left origin and Smithay's rendering is y-down.

## Decision

Keep `Output` at `Normal` and give the damage tracker a static `Flipped180` mode source (`OutputModeSource::Static`). The flip happens only in the projection matrix. A resize builds a new tracker because the source is static.

## Consequences

Render y = 0 is the top; damage, cursor, hit-testing and geometry stay in logical y-down.

## Rejected alternatives

- Setting `Output` to `Flipped180`: clients would pre-rotate their buffers.
