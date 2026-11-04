# 0044 — Appearance cross-fade on the GPU

Status: accepted (2026-10-26).

## Context

When switching light/dark, each surface changes on its own commit and produces a staggered flicker.

## Decision

1. Full-output cross-fade on the GPU. Before applying the theme, the compositor renders the current output (without the cursor) to a GLES texture; it updates the theme and starts two phases:
   - **Waiting:** the snapshot is drawn opaque on top of everything except the cursor until the mapped surfaces commit, or 150 ms.
   - **Fading:** decreasing alpha with a cubic ease-in-out for 420 ms (`duration.slow`); at the end the texture is released.
2. No CPU readbacks: `Offscreen::<GlesTexture>::create_buffer` and the same `OutputDamageTracker`; it is drawn as a `TextureRenderElement`.
3. No CPU compositing and no new shader: the element alpha is enough.
4. The cursor is not part of the snapshot and keeps being drawn on top.
5. With `appearance.reduced_motion` or a locked session, the change is immediate.
6. If another change arrives during the fade, the current frame is captured and the fade restarts in **Waiting**.
7. Duration and maximum wait live in `vow-design`; no literals.

## Consequences

The change feels like a global cross-fade.

## Rejected alternatives

- Per-client fade: it needs a private protocol and does not guarantee synchronization.
