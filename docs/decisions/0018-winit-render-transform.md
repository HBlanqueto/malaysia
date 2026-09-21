# 0018 — The winit output flips at render time, never in `wl_output`

Status: accepted (Block 4 follow-up)

## Context

The whole frame rendered upside down inside the winit window: client content,
the cursor image and the chrome were vertically mirrored, and the default arrow
pointed down. Input was not the cause. Host top-left already mapped to logical
(0, 0) and host bottom-right to the logical output size, so the two coordinate
spaces agreed while the picture did not.

The cause is the framebuffer, not the coordinates. The winit framebuffer is an
OpenGL surface whose origin is bottom-left, while Smithay's render space is
y-down. Smithay's GLES renderer composes the transform it is handed with its own
OpenGL correction (`gles/mod.rs:2087-2089`:
`flip180 * transform.matrix() * renderer`), so a render transform of `Normal`
puts render y = 0 at the bottom of the window, and `Flipped180` puts it at the
top. The damage tracker inverts the transform of its mode source before handing
it to the renderer (`damage/mod.rs:330-335`).

The output's own transform is a different thing: `Output::change_current_state`
sends it to clients in `wl_output.geometry` (`wayland/output/mod.rs:164-177`).
Setting it to `Flipped180` would make every client see `flipped_180` and
pre-rotate its buffers — the same mistake in the other direction.

## Decision

Keep the `Output` transform `Normal`, and give the damage tracker a static mode
source carrying `Flipped180` (`OutputModeSource::Static`, `output.rs:496-536`).
The flip therefore happens only in the projection matrix. A resize builds a
fresh tracker, because a static source pins its size.

The advertised output stays `Normal`, so no client is asked to rotate anything,
and `State::logical_output_size` remains the single physical-to-logical path.

## Consequences

- Render y = 0 is the top of the window. Damage rectangles, element geometry,
  cursor placement and edge-zone hit-testing all stay in logical, y-down
  coordinates and need no further flips.
- The damage history is dropped on resize, which is what a resize needs anyway.
- The pixel tests in `session::tests` assert the orientation through the same
  mode source, so a future backend change cannot silently un-flip the frame.
- A DRM/KMS backend must not copy this: there the output transform is the
  scanout rotation and does belong on the `Output`.

## References

- Smithay `src/backend/renderer/damage/mod.rs:330-335` — the tracker inverts the
  mode source's transform.
- Smithay `src/backend/renderer/gles/mod.rs:2087-2089` — the projection
  composition.
- Smithay `src/output.rs:496-536` — `OutputModeSource::Static`.
- Smithay `src/wayland/output/mod.rs:164-177` — the transform sent to clients.
- `crates/neptune/src/session.rs` — `winit_render_transform`,
  `winit_mode_source`, `render_and_read`.
