# 0015 — Window layout stays in `yuki-scene`; `desktop::Space` is not adopted

Status: accepted.

## Context

`desktop::Space` sits behind the `desktop` feature, which is disabled (ADR 0009).

## Decision

`desktop` is not enabled and `Space` is not adopted. `yuki-scene` is the layout authority: ordered set of windows, outer geometry, hit-testing, stacking and raises; it derives the client rectangle, the input region and the visual bounds from the insets. The `wayland_frontend` types (`ToplevelSurface`, `PopupSurface`, `PositionerState`) are used directly. For popups, the scene computes the target rectangle and passes it to `PositionerState::get_unconstrained_geometry`; the compositor owns the popup tree and the keyboard/pointer grabs.

## Consequences

`window.rs` is the seed of the layout, not a placeholder.

## Rejected alternatives

- Adopting `Space` with a side table of insets: it recreates the problem inside `Space` and forces a rewrite of the consumers if reverted.
