# Expansion

Expansion is malaysia's Mission Control: the windows of the current Desktop
shrink into one layer with no overlap, a strip at the top lists the Desktops
and the full-screen Spaces, and the user picks a window or a Space.

This file is a proposal. Steps C, Expansion on a key, D, the strip, E,
the drag of a window to a Space, and F, the hover ring and Control+arrows,
are built, in `mod.rs`, `layout.rs` and `strip.rs` of this folder and in
`effects/strip.rs` and `effects/badge.rs`; none of the rest is. It was written on 2026-11-26 on
a Mac, which cannot build the compositor, from measurements of macOS 27.2.1 and
from reading gnome-shell, KWin, niri and Smithay at the pinned rev. The row of
workspaces it sits on, and the order of the steps, are in
[../README.md](../README.md).

The measured tables, their method and the sources are in barrierkit,
[`docs/research/macos-mission-control-2026-11-26.md`](https://github.com/HBlanqueto/barrierkit/blob/main/docs/research/macos-mission-control-2026-11-26.md)
(called "the measurements" below). The probes are in
`scripts/macos-probes/mission-control/` of the same repository.

## Where it is drawn

In the compositor, with GLES, like the window frame. On macOS the Dock asks the
window server to transform the real windows; a thumbnail is the live window
with its own corners and shadow, scaled. A client cannot do that, so Expansion
is not a `bk` component. BarrierKit gives it its values: every size, colour and
spring below becomes a `ctdesign` token, added by the step that first reads it.
Text (the names of the Desktops, the title of a hovered window) needs
`cosmic-text`, which is a step of its own.

## What the user sees

Each line is measured unless it says otherwise. "Documented" is Apple's
own text and "reported" is the press; the measurements give the source.

1. **Open:** Control+Up. Later a three-finger swipe up and a hot corner.
2. **Windows:** every window of the current Desktop moves to its slot with one
   critically damped spring, ω0 25. Counted from the first frame that changes,
   macOS is half way at 53 to 69 ms and at 90 % at 142 to 158 ms; that spring
   started from rest gives 67 and 156, and those are the numbers a test must
   use. Position and size use the same spring. All windows share one scale,
   never above 1.
3. **Backdrop:** the wallpaper does not change. No dim, no blur, no scale.
4. **Strip:** a bar 40 high slides down from the top edge and takes the place
   of the menu bar. It holds one name per Desktop in cells 95 wide, centred,
   the current one on a pill, and a "+" at the right. When the pointer enters
   it, it grows to 146 with a spring of ω0 16 and shows a live thumbnail of
   each Space, 90 high (1/16 only on a display 1440 high), 32 apart. It stays grown until Expansion
   closes. The window area starts 40 below the strip (about 38 once it has
   grown), so growing the strip computes the layout again. From rest the
   spring of 16 is half way at 105 ms and at 90 % at 243 ms; macOS, counted
   from the first changed frame, shows 92 and about 230.
5. **Strip material:** blur σ 27.5 with a gray of 44 at alpha .41 (dark), a
   line of alpha .224 on its last row and a soft shadow of 48 below.
6. **Hover:** a window under the pointer gets a ring 4 wide in the accent
   colour, at once, with no fade, and its title in a capsule at its centre.
7. **Pick:** releasing the button on a window raises it at the first frame and
   every window returns with the same spring. Releasing it on the background
   closes Expansion (documented). A click on a Space makes it current and
   closes Expansion (documented).
8. **Close:** Control+Up again, with the same spring. Escape and
   Control+Down also close it (reported).
9. **Switch while open:** Control+Left and Right slide the window area with
   the Desktop switch spring; the strip stays.
10. **Strip actions:** "+" adds a Desktop (16 at most, documented). A close
    button, a white circle of 22 at the top-left corner of a hovered
    thumbnail, removes a Desktop; its windows go to another Space
    (documented), the Desktop before it (reported). A window dragged onto a
    thumbnail moves to that Space (documented), and onto "+" gets a new
    Desktop (reported).
11. **The Dock** slides up while Expansion is open, even when it hides itself.
    malaysia has no Dock yet.

## Layout

macOS was measured on 116 arrangements of 1 to 9 windows. The rule for a few
overlapping windows is found; the rule for the scale and for dense
arrangements is not, so this part is not 1:1 yet.

What holds in every arrangement, and what the tests of step C must assert:

- one scale for all windows, never above 1; a small window is never enlarged;
- a window that fits and overlaps nothing stays at scale 1, where it is but
  for a quarter of a point per stacking place (the tie offset);
- a window that does not fit shrinks about its own centre and is then moved
  inside the area;
- an overlapping pair is pushed apart along the line between the two centres
  until it is clear on one axis; both windows move the same distance,
  whatever their sizes;
- two windows with the same centre split at 45°: the one behind goes up and
  left, the one in front down and right. Stacking order decides, not the
  order of creation;
- a pushed pair ends at least 12.5 apart on one axis, at every scale;
- the scale drops only when a window is held by two walls at once; a window
  held by one wall slides along it;
- the area: 20 from each side, 40 below the strip, 15 above the Dock. With
  one window it is 17 tighter at the top and at the bottom, and a lone window
  that must shrink is capped at a height of 1220 on a display 1440 high,
  which is the area of several windows minus 41.

The algorithm to build is section 8 of the measurements, with its constants:
sweeps that move each window three quarters of the way out of every overlap
at once, a tie offset of a quarter of a point per stacking place, walls that
give 2 and 3, and the scale times .995 in each sweep where a window is
cornered. It is within 3 of macOS in 78 of the 116 arrangements, and it
predicted three fresh ones to 1 to 3 before they were measured. That covers
windows alone, without overlap, pairs, and chains of three. It is 11 to 33
off for a pair in a corner, where its scale is a stand-in, and hundreds off
for cascades of four and more and for two rows that overlap.

`layout.rs` ports that script. Where the script and the text of the
measurements differ, it follows the script:

- when 400 sweeps do not settle, the start scale is bisected 18 times between
  .05 and its first value, and the largest scale that settles is kept;
  section 8 leaves that step out;
- a lone window is moved inside the one-window area after it shrinks, so the
  centre of `n1full` is at 714, not 711;
- a pair 12 apart is closer than the margin of 12.5, so it is still pushed,
  about 1.2 with the tie offset.

Cascade placement makes a cascade the usual case, so the dense case matters.
The layout is one pure function, so replacing it later touches nothing else.

## State

One progress value `p` from 0 (Desktop) to 1 (Expansion) and one enum:
`Closed`, `Moving` (a spring and its start time), `Tracking` (the fingers set
`p`) and `Open`. Opening and closing are both `Moving`, towards 1 or 0. A
window is drawn at the mix of its own rectangle and its slot by `p`.

The spring and the clock are those of the slide between Desktops
(`spring.rs` and `desktops.rs`). The spring is the closed-form solution, so it can start from
any position and velocity: a key pressed while the windows are still moving,
or fingers lifted, continue without a jump. It lives in `spring.rs` with ω0
and ζ as arguments, overdamped and, since step C, critically damped.

The strip has one value of its own, its growth (40 to 146), on the same
spring type; its slide is Expansion's progress `p`.

The layout is a pure function from the area and the window rectangles to the
slots. It runs when Expansion opens, when a window is mapped, unmapped or
changes size, when the strip grows and when Control+arrows reach another
Desktop. Its result is kept in a `Vec` that is cleared and filled again,
never allocated per frame. Each window keeps its slot with the scale it was
arranged with, so the Desktop left by Control+arrows slides out spread.

## Drawing

- `Effects::push_window` already builds every element of a window for both
  backends. It takes the window's placement as one value, `Placement`: the
  `geometry`, the `desktop_offset` that the slide sets and the factor of
  Expansion, under a name of its own because `scale` is already the output's.
  `state::elements` gives it the mix of the window's own rectangle and its
  slot.
- The corner shader of `Clipped` takes its size from `Shape.bounds` and only
  its position from the drawn rectangle, so a scaled window cannot be made by
  wrapping `Clipped` in Smithay's `RescaleRenderElement`: the rectangle would
  shrink and the corner would still be computed for the full size.
  What was built instead: `Clipped` takes the factor and scales its own
  rectangle and damage about the window's top-left, with no rescale wrapper,
  and the `Shape` of the clip, the lines and the shadow is built for the
  scaled rectangle with the radius times the factor. The shadow gets the
  scale times the factor, so its reach shrinks with the window and its alpha
  does not, as measured.
- Scaling everything in one wrapper around the finished elements, as niri does,
  was rejected: it also shrinks the one-pixel lines and the half-pixel edge of
  the corner, which then flicker.
- Strip thumbnails are the same live surfaces drawn again, small, without
  lines or shadow. Smithay's damage tracker accepts the same element twice in
  a frame. The window's lines and shadow keep one size per window, so drawing
  them at two sizes in one frame would damage them every frame.
- Smithay has no mipmaps. A window shrunk to 1/16 with linear filtering will
  shimmer when its content moves. The step ships it so and it is judged on
  screen; a chain of halvings is a later step if it is needed.
- Every window that is drawn gets frame callbacks. `State::frame_done` feeds
  the Desktops that have the output mapped, which the slide uses for the
  Desktop it leaves. The strip maps the output on every Space while its
  thumbnails show, lifts `suspended` from their windows, and undoes both when
  Expansion closes, as `advance_slide` does at the end of a slide.
- While windows move, the whole output is damaged each frame. When `p` rests
  the frame is static again and nothing is drawn.

## Input

Expansion takes no pointer grab. When it opens:

- the pointer grab is ended and the popups are closed, as `switch_desktop`
  does when no window is being dragged; a drag in progress is cancelled;
- the keyboard focus is set to none, so clients get no keys and no stale
  modifiers;
- `State::surface_under` returns nothing while it is open, so clients get a
  leave and no motion;
- the button handler branches before the click reaches a window: it tests the
  strip, then the slots from the top window down, then the background;
- the key filter handles Escape, Control+Up, Control+Down and the arrows.

When it closes, the focus goes to the picked window, or back to the window
that had it.

Two parts of the slide undo Expansion's state and need a branch while it is
open: `switch_desktop` gives the focus to the top window of the Desktop it
reaches, and `advance_slide` suspends the windows of every other Desktop and
unmaps the output from it when the slide rests.

## Steps

The letters are those of the table in [../README.md](../README.md). They
follow cascade placement, which is done (step 21): before it every window opens at the top left
corner, and the layout has nothing to keep. Each step is one visible result.

- **C. Expansion on a key (built).** Control+Up spreads the windows, a click picks
   one, Escape closes. No strip, no text. Adds the `EXPANSION_*` spring and
   margin tokens (the gap is a constant of the fit in `layout.rs`), the critically damped case in `spring.rs`, the factor in `push_window`, the layout function with its tests. Until
   the strip exists the area starts 80 below the top, as if it were there.
- **D. The strip (built).** It slides in with the windows, grows under the pointer and
   shows live thumbnails with the ring on the current one; a click on a
   thumbnail switches; "+" adds a Desktop and the close button removes one.
   From this step Desktops are no longer added by Control+Right or dropped
   when left empty: the "+" owns them, as on macOS. Adds the `SPACES_*`
   tokens and frame callbacks for the windows in thumbnails. The strip's
   blur, the first blur of the rebuild, is a step of its own after the
   wallpaper.
- **E. Drag a window to a Space (built).** A window dragged onto the
   thumbnail of another Desktop moves there. It is plain state in
   `Expansion` with a movement threshold, not a pointer grab. A drop on "+"
   is not built.
- **F. Hover ring and Control+arrows while open (built).** The ring has no
   state: it is drawn on the window under the pointer when the frame is
   built. Control+Left and Control+Right are ignored while a window is
   dragged.
- **H. Swipes.** Three fingers up and down for Expansion, sideways for the
   slide, following the fingers. libinput's swipe events are read directly;
   the winit backend reports pinch and hold but no swipe, so this is checked
   only directly on a display with a touchpad. The slide needs a state for
   the fingers: `State.slide` is `None` at rest and `advance_slide` writes
   its position on every frame.
- **I. Text.** The names in the strip, the title capsule, the "No Available
   Windows" pill. Adds `cosmic-text`. One texture per label, made when the
   label changes.

How each is proved:

- Step C: unit tests for the layout (the measured arrangements, ties
  between windows on the same frame) and for the spring at a retarget. A
  screenshot with four round corners on every scaled window. After Escape the
  screenshot equals the one before. `WAYLAND_DEBUG=1` on a client shows a
  leave at open and no key. With `p` at rest nothing is rendered.
- Step D: a client on another Desktop animates inside its thumbnail and its
  frame callbacks stop on close. A Desktop made with "+" survives a switch
  away from it. No damage while idle.

## What will not match macOS yet

- No wallpaper: the backdrop is the clear colour, and the thumbnail of an
  empty Desktop cannot be told from the strip except by its ring.
- No Dock, so no Dock slide, and the bottom margin of the area is a guess: 99
  above the bottom edge, where it was measured with a Dock of tile 59.
- No menu bar for the strip to replace.
- Not planned here: grouping by application, the Space bar preview, App
  Exposé, Show Desktop, hot corners, more than one display, reordering
  Spaces, "rearrange by most recent use", Reduce Motion.

## Not measured

- Whether the swipe follows the fingers, its threshold and its velocity rule.
  A trackpad gesture cannot be posted by a probe. Until someone films it, the
  rule is gnome-shell's: progress is distance over 300, a slow release goes to
  the nearest end, a fast one is projected.
- The margins on another display size or with the Dock elsewhere; every
  number comes from 2560×1440 at scale 1. The thumbnail is taken as a fixed
  height of 90; it may be 1/16 of the display instead.
- The fonts (sizes are matched by the width of the ink only), the radii of
  the thumbnail ring and of the pill, the hover ring at a small scale.
- Light mode. Every tone here is dark.
- The Dock slide and Show Desktop.

## Sources

- The measurements and their sources: barrierkit
  `docs/research/macos-mission-control-2026-11-26.md`.
- Smithay at the pinned rev,
  https://github.com/Smithay/smithay/tree/65aa86fa5236e41775c313408102a5d5c2d1e2ca :
  `src/backend/renderer/element/utils/elements.rs` (`RescaleRenderElement`
  scales the geometry, the damage and the opaque regions, and passes the
  drawn rectangle through), `src/backend/renderer/damage/mod.rs`
  (`last_instances`: one element may appear several times in a frame),
  `src/backend/renderer/gles/mod.rs` (a downscale filter, no mipmaps),
  `src/backend/input/mod.rs` and `anvil/src/input_handler.rs` (the swipe
  events), `src/backend/winit/mod.rs` (no swipe).
- Read for behaviour only, never copied: gnome-shell `js/ui/workspace.js` and
  `js/ui/swipeTracker.js`, KWin `src/plugins/private/expolayout.cpp`, mutter
  `src/compositor/meta-shaped-texture.c` (mipmaps below half size) and
  `src/compositor/meta-surface-actor-wayland.c` (frame callbacks for clones),
  niri `src/layout/monitor.rs` and its Overview wiki page.
