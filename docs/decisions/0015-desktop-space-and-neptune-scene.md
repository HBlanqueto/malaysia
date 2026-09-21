# 0015 — Window layout stays in `neptune-scene`; Smithay's `desktop::Space` is not adopted

Status: **accepted.** This record settles a divergence the roadmap left
unexplained: roadmap Block 2's brief asked for "a dedicated module wrapping
`desktop::Space`", the implementation kept the `desktop` feature off and defined
its own types in `neptune-scene` instead, and no document said why. It is
written before Block 4, where reversing the choice would become expensive.

## Context

Smithay ships a window-management helper, `desktop::Space`, behind its
`desktop` feature. Neptune compiles Smithay with `default-features = false` and
does **not** enable `desktop` (ADR 0009, ADR 0010). Toplevels are tracked by
hand in `crates/neptune/src/window.rs`, and the crate `neptune-scene` owns the
project's own coordinate types (ADR 0001).

The roadmap's Block 2 asked for a module wrapping `Space`. The implementation
did not do that; this record makes the choice deliberate. It has to account for
the project's decoration model, which the brief predates: Neptune draws its own
chrome — squircle corners, a dark hairline border, a light inner line, a
two-layer shadow, and a vertical titlebar — so a window is **not** a rectangle
and a window's geometry, its client's surface, and its input region do not
coincide.

## What `neptune-scene` provides today

Concretely, and by type:

- `geometry::Logical` (`f64`), `geometry::Physical` (`i32`) and
  `geometry::Buffer` (`i32`) — type-level markers for the three coordinate
  spaces.
- `geometry::Point<N, Space>`, `geometry::Size<N, Space>` and
  `geometry::Rect<N, Space>` — values parameterised by those markers. Points
  and sizes add and subtract only within one space; conversions are explicit
  `to_physical(scale)` / `to_buffer(scale)` / `to_logical(scale)` methods.
  `Rect` exposes `location`, `size`, `x`, `y`, `width`, `height`.
- The crate has **no dependencies**, is `#![forbid(unsafe_code)]`, and its
  cross-space arithmetic is held in place by a `compile_fail` doctest.

Its responsibility today is the coordinate vocabulary at the one boundary where
the compositor feeds the scene (ADR 0001). It has **no scene graph, no layout,
no z-order and no hit-testing yet** — the graph is a later milestone. In
particular `Rect` has no `contains`/`intersection`, and no code tracks where a
window is except `Window` in the compositor.

## What `desktop::Space` would provide that `neptune-scene` does not

`Space<E>` is an ordered collection of elements with per-element locations. It
provides: `map_element`/`unmap_elem`; `elements` in z-order; `raise_element`;
point hit-testing (`element_under`); `element_location`, `element_geometry` and
`element_bbox`; output mapping (`map_output`, `output_under`, `outputs_for_element`,
`output_geometry`); `refresh`; and render-element generation
(`render_elements_for_region`, `render_elements_for_output`,
`space_render_elements`, `render_output`). Enabling `desktop` also brings the
concrete element `WindowElement`/`Window` (a surface-tree element with
`geometry`, `bbox`, `bbox_with_popups`, `set_activate`, output enter/leave), the
popup tracking helper `PopupManager` with `PopupGrab`, and layer-shell types.

`neptune-scene` provides none of that, and would have to grow a subset of it.

## What Block 4 needs, and what each side offers

Block 4 (move, resize, close, stacking, click-to-focus) needs four things:

1. **Hit-testing** a logical point to the topmost window. `Space::element_under`
   does this from each element's `bbox`. Neptune's `Windows::surface_under`
   already does it from `Window::contains`, back to front.
2. **Stacking order** with raise-on-click. `Space` maintains the order and has
   `raise_element`; a `Vec` with move-to-end does the same for one output.
3. **Focus-on-click**, which is hit-testing plus the existing central serial
   counter (`input.rs`).
4. **Move and resize grabs.** These are Smithay pointer grabs, driven from the
   window's *outer* rectangle and its decoration edges. `Space` can report an
   element's surface geometry, but a decoration edge (the 32 px titlebar, the
   resize bands) is not part of `WindowElement`'s geometry, so the grab math
   would sit outside `Space` either way.

`Space`'s extra machinery — per-output overlap and damage, element activation,
`render_elements_for_output` — is aimed at many windows over several outputs.
Neptune has one output today (ADR 0009), and the draw path already works through
`OutputDamageTracker::render_output` with a plain element slice.

## What Block 5 needs, and what each side offers

Block 5 (popups) needs two things the brief names:

1. **Position against the parent.** This is the parent window's rectangle plus
   the popup's `PositionerState` (anchor, gravity, offset, size). `Space` can
   report the parent's `element_geometry`/`bbox_with_popups`; it does not place
   the popup.
2. **Constraint adjustments** (flip, slide, resize) against the monitor's usable
   area. Neither `Space` nor `desktop`'s `PopupManager` performs this: the
   manager tracks popup trees and manages grabs, and the constraint math is left
   to the compositor using the `PositionerState` and `send_configure` that the
   `wayland_frontend` code already exposes. Neptune would write this logic
   whether or not `desktop` is enabled.

So `desktop` buys Block 5 little: the piece it would add (popup lifecycle and
grabs) is real, but the piece Block 5 is *about* (placement and constraints) is
ours on either path.

## How the decoration requirements affect geometry ownership

A Neptune window has at least four distinct figures, and they do not coincide:

- the **outer** rectangle: titlebar, border and shadow extents plus the client;
- the **client surface** rectangle, inside the insets;
- the **input region**, which excludes shadow pixels and is not the client
  surface when the pointer is over the titlebar;
- the **visual bounds**, where the two-layer shadow extends past the outer
  rectangle.

The insets are a function of window state and are recomputed on every commit
(ADR 0014): a `set_parent` can arrive after mapping, fullscreen can change, so
the outer/inner relationship is never frozen. `Space`'s element geometry is the
client surface tree's — `WindowElement::geometry`/`bbox` come from the surface,
with no place for server-side insets. Any code routing through `Space` would
therefore need a side table of insets to correct placement, hit-testing and
grabs, which recreates the ownership problem *inside* `Space` instead of solving
it.

The alternative is one authority: `neptune-scene` (or a window type it owns)
holds the **outer** rectangle and derives the client rectangle, the input region
and the visual bounds from it using the insets. Every consumer — hit-testing,
grabs, popup placement, drawing — reads that single geometry.

## Cost of reversing the decision after Block 4

If `desktop::Space` were adopted for Block 4 and the decision reversed later,
the reversal touches every place Block 4 introduces: hit-testing and
click-to-focus in `input.rs`, stacking and window bookkeeping in `window.rs`,
the draw path in `session.rs` (which would be feeding `WindowElement`s to
`Space`), and all of Block 5's parent rectangles. The decoration geometry would
have to be re-derived from scratch, because `Space` would have been the thing
holding it. The project would do the layout work twice.

Reversing the other way (starting with `neptune-scene` and moving to `Space`
later) is cheaper only in that `neptune-scene`'s coordinate types survive
either way — but it would still mean rewriting the same consumers. Either
direction is a real rewrite; the cheaper decision is the one made before Block 4
rather than after.

## Decision

Do **not** enable Smithay's `desktop` feature and do **not** adopt
`desktop::Space`. Grow `neptune-scene` into the window-layout authority: an
ordered set of windows, each owning its outer geometry and deriving its client
rectangle, input region and visual bounds from the decoration insets, exposing
point hit-testing, stacking and raises. Use Smithay's `wayland_frontend` types
(`ToplevelSurface`, `PopupSurface`, `PositionerState`) directly for protocol
work, and implement Block 5's constraint math against the output's usable area
as the compositor's own code.

**Recommendation:** keep `desktop` off. `Space`'s central abstraction — an
element whose geometry is its client surface — is the wrong unit for a
compositor whose windows are decorated with chrome that does not coincide with
the client surface; adopting it would add a dependency and an element layer only
to wrap it with the insets it cannot represent, while neither Block 4's move and
resize grabs nor Block 5's popup constraints would come for free. `neptune-scene`
already owns the coordinate types and is the natural single authority for the
outer geometry, so the layout work belongs there, behind one boundary, and it
should be written before Block 4 rather than after it.

## Consequences

- `window.rs`'s hand-tracking is the seed of the `neptune-scene` layout, not a
  placeholder to be deleted; Block 4 extends it behind the existing module
  boundary.
- Block 4 must add to `neptune-scene` what `Space` would have given: an ordered
  window set, outer-rectangle storage, rectangle containment/intersection, and
  the conversion from outer geometry and insets to client and input rectangles.
- Block 5 implements popup constraint adjustment itself, using `PositionerState`
  and `send_configure`; `desktop::PopupManager` and its grabs are not available
  and are replaced by the project's own popup bookkeeping when that block lands.
- The `desktop` feature stays off; if a later milestone needs layer-shell or
  multi-output overlap, this decision can be revisited from the position of a
  concrete, written requirement rather than from the Block 2 brief's wording.

## References

- `crates/neptune-scene/src/geometry.rs` — the coordinate types and their tests.
- `crates/neptune/src/window.rs` — the current toplevel tracking and
  `surface_under` hit-testing.
- `crates/neptune/src/decoration.rs` — `Insets`, `Role`, `WindowState`; the
  insets are recomputed per commit (ADR 0014).
- `docs/decisions/0001-coordinate-types.md` — why `neptune-scene` owns geometry.
- `docs/decisions/0009-winit-backend-and-gles2.md` — the `desktop` feature is
  deliberately off.
- Smithay `src/desktop/space/mod.rs`, `src/desktop/space/element/mod.rs`,
  `src/desktop/wayland/popup/manager.rs` — the API this decision does not adopt.
