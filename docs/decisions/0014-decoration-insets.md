# 0014 — Decoration insets are a function of window state

Status: accepted (Block 1, Part 4)

## Context

Before Neptune maps its first window it must know how much room its own chrome
takes around a client's surface. The number is needed earlier than it looks:
the configure a toplevel receives carries its *usable* size, which is the
window size minus the insets. If the insets do not exist when that configure is
written, it is written with the full size and every later size has to be
corrected.

Neptune's chrome is decided in `neptune-design`: a titlebar with a position
(left, right, top, bottom or none) and a thickness. Not every window is
decorated the same way, and what a window gets can change while it lives.

## Decision

`crate::decoration` owns an `Insets { top, right, bottom, left }` in logical
pixel `f64`, and `Insets::for_window(role, window, chrome, titlebar)` is the
single place the decision is resolved. Its precedence is:

1. **Role.** A popup is never decorated. A dialog — a toplevel with a parent —
   is never decorated. A normal toplevel may be.
2. **Window state.** Fullscreen reserves nothing. Maximized keeps its titlebar
   today; that may change when tiling exists.
3. **Client negotiation.** A client that draws its own chrome, or that asked
   through `xdg_decoration` not to be decorated, reserves nothing.

Anything that is not a normal or maximized toplevel with server-side chrome
gets [`Insets::ZERO`]. The titlebar's edge and thickness come from
`neptune_design`; no caller passes a width, and no code may assume a titlebar
is a particular number of pixels tall.

## The four constraints

1. **Insets are a function of current state, not a field.** `for_window` is
   called on every configure and reads the window's state each time. Nothing is
   captured at map time. Fullscreen zeroes the insets today because the
   function is re-evaluated; a maximized or tiled window can change them later
   without moving data.
2. **One place resolves precedence.** Role, then state, then client
   negotiation, in the one `match` in `Insets::for_window`. Adding a role or a
   state means editing that match, not scattering conditions through the
   mapping code.
3. **The input region does not match the visual region.** Resize borders extend
   a few pixels *outside* the visible window, so the input region can be larger
   than the window, which the visible insets never express. This is not needed
   until move and resize land, but it is what could force this type's shape to
   change, so it is recorded now: the input region will be a sibling
   calculation, and its insets may be negative (extending outward). The `f64`
   fields already allow that.
4. **Shadows are not insets.** They draw outside the window and reserve no
   layout space; adjacent windows' shadows overlap and do not push each other
   apart. Shadows belong to the scene graph, and `neptune-design`'s `SHADOW`
   values are not read here.

## Consequences

- Part 5's initial configure computes usable size as window size minus
  `horizontal()` and `vertical()`, so a decorated window asks its client for a
  surface smaller than the window by exactly the titlebar.
- Nothing is drawn in this part. When rendering lands (Part 6), the reserved
  area shows as the compositor background behind an offset surface: a normal
  window has a visible strip where the titlebar will be drawn, a dialog has
  none. That is the verification that the insets are in the size path.
- The client's `xdg_decoration` negotiation feeds `Chrome`; until it is wired,
  every toplevel is treated as `Chrome::Server`, which is the protocol default.
- The type is pure and unit-tested against `neptune_design` values; it has no
  dependency on Smithay or on a live surface, so it can be exercised before any
  window exists.
