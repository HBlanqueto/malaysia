# Workspaces

This folder is for the row of workspaces: the Desktops, the full-screen
Spaces, Split View and, in [expansion/](expansion/README.md), Expansion,
malaysia's Mission Control.

This file is a proposal. None of it is built: the folder holds no code yet.
It was written on 2026-11-26 on a Mac, which cannot build the compositor, so
that the session on Linux can check it and build from it. The measured tables
and the sources are in barrierkit,
[`docs/research/macos-mission-control-2026-11-26.md`](https://github.com/HBlanqueto/barrierkit/blob/main/docs/research/macos-mission-control-2026-11-26.md).

## What exists

Steps 16 to 20, in `desktops.rs`, `state.rs` and `move_grab.rs`:

- the row is a `Vec<Space<Window>>` and the index of the current one;
- Control+Left and Right slide between Desktops with macOS's overdamped
  spring and a black band of 64 between them. `Slide` holds a fractional
  position in the row that rendering reads, and a key pressed during a slide
  carries on from where the row is;
- the windows of a hidden Desktop are told they are `suspended`;
- a window is dragged by its title bar, and Control+arrows during the drag
  carry it to the next Desktop;
- Control+Right on the last Desktop with windows adds one, and an empty last
  Desktop is dropped when it is left.

## The row

macOS has one row per display, and Control+arrows and the swipes walk through
all of it with the same slide. A full-screen window is a Space of its own,
placed right after the Desktop it came from (measured; that Desktop was also
the current one in every run), and two windows can share one as a Split View.

The proposal keeps the `Vec` and gives each entry one more field:

- an entry is a `Space<Window>` and, when it is a full-screen Space, the
  frame its window had before, so it can go back;
- an entry with that frame is a full-screen Space; one without is a Desktop;
- there is always at least one Desktop;
- one function removes an entry, and it moves the current index down when the
  removed entry was before it. Leaving full screen, a full-screen window that
  closes and the close button of Expansion all use it.

No identifier per Desktop is needed. A full-screen Space is inserted right
after the current entry, and its window returns to the nearest Desktop,
looking left first and then right. As long as the row is not reordered that
is the Desktop it came from, as on macOS.

Two rules of step 16 end when Expansion's strip brings the "+": Control+Right
no longer adds a Desktop, and an empty Desktop is no longer dropped. On macOS
a Desktop that is emptied stays.

A new window is still mapped on the current entry when its toplevel is
created, because `commit` sends the first configure only to a window that is
in a Space. It is moved, if it must be, at its initial commit, when its
parent and its full-screen request are known. A window with a parent goes to
its parent's Space. While a full-screen Space is
current, another window of the same client opens in that Space, above the
full-screen window, and a window of any other client opens on the nearest
Desktop, which becomes current. macOS does the first; the second it does
only for an app that was just launched, on its first Desktop.

## What the steps share

- **The spring.** `desktops.rs` has it in closed form, with position and
  velocity at a time, started from any position and velocity. It is written
  for the slide alone: its ω0 and ζ are constants of the module and it
  divides by the square root of ζ² − 1, which is zero for the critically
  damped springs of Expansion (ζ 1). The first step that needs a second
  spring (the tiling shortcuts, if they come first) moves the function and
  `rests`, which reads the same constants, to `spring.rs` at the root of the
  crate, gives them ω0 and ζ as arguments and adds the critical case. The tiling shortcuts
  (45, 1.38), full screen (25, 1.4) and Expansion (25, 1 and 16, 1) read it.
  The underdamped case has no user and is not written. BarrierKit's own
  spring cannot be used: it lives in `bk`, which links Freya, and it cannot
  be overdamped.
- **The clock** is `start_time.elapsed()`, taken once per frame. An animation
  is a spring and the time it started, and `needs_redraw` stays set while it
  moves, as `advance_slide` does.
- **The fractional position in the row** of `Slide`, which the full-screen
  transition reads. A sideways swipe cannot set it as it is: `State.slide` is
  `None` at rest and `advance_slide` writes the position on every frame, so
  the swipe step adds a state for the fingers.
- **`desktops.rs` moves into this folder** in step A, the first step here
  that changes it. It was not moved from the Mac: the move could not be
  compiled there, and it would have crossed the work in progress on Linux.

## Full screen

What macOS does, measured unless it says otherwise:

- **Enter:** the green button, Control+Command+F or the Window menu
  (documented). The window grows in place from its frame to the display with
  a spring of ω0 25 and ζ 1.4 while the row slides one place. Counted from
  its first change it is half way at about 97 ms, at 90 % at about 260 ms
  and still at about 440 ms; the fitted spring started from rest gives 84
  and 238, and those are the numbers a test must use. It does not travel
  with its Space. What slides in beside the old Desktop is black. The whole
  transition takes about 550 ms.
- **The Space:** right after the Desktop the window came from. In Mission
  Control its name is the app's name and its thumbnail is the window alone.
- **Inside:** the window is 0, 0 to the size of the display, with square
  corners and no shadow. A window that cannot be that large is centred on
  pure black.
- **Bars:** the pointer within 4 of the top edge shows the menu bar and the
  window's title strip after about 180 ms: they slide 62 down in about
  100 ms, and go back when the pointer is 36 below them.
- **Exit:** the window returns to its Desktop and its exact frame, the Space
  is removed and that Desktop becomes current.
- **Close:** the Space is removed. If it was current, the Desktop to its left
  becomes current; if not, nothing else changes.

How it maps to Wayland:

- `xdg_toplevel.set_fullscreen`, `unset_fullscreen`, the key and, later, a
  button of the title bar all reach one function to enter and one to leave.
  A window that is alone on its Desktop still gets its own Space.
- Enter: remember the window's location and size, send a configure with the
  `fullscreen` state and the size of the output, and when the client commits
  after acknowledging that configure (`with_committed_state` gives the
  acknowledged state; the buffer may be smaller) move the window to a new
  entry right after the current one and make it current.
- Leave: send a configure without the state and with the remembered size,
  map the window on the nearest Desktop at the remembered location, remove
  the entry.
- The output the client names in `set_fullscreen` is ignored while there is
  one output.
- `Effects::push_window` draws a full-screen window without corner, lines or
  shadow, and a black rectangle the size of the output goes under it. xdg-shell
  asks that nothing else show around a full-screen surface smaller than the
  output and gives black as its example; macOS uses black.
- `XdgShellState::new` advertises Maximize, Minimize and the window menu,
  which the compositor does not do. `new_with_capabilities` with Fullscreen
  alone tells clients the truth.
- The bars that slide in at the top wait for the menu bar and for
  compositor-drawn title bars. A GTK app shows its own header bar in full
  screen, or reveals it itself.

The proposal differs from macOS on purpose in two places. Leaving full screen
while another Desktop is current, macOS puts the window on the first Desktop
and the proposal on the nearest one to the left of its Space. And a window of
another app that opens while a full-screen Space is current goes, on macOS,
to the first Desktop without a switch unless the app was just launched; the
proposal always uses the nearest Desktop and switches to it, because a new
window takes the focus in malaysia today.

## Split View

Measured on macOS: two windows share one full-screen Space with a black
divider 12 wide between them and no margin; the tiles have square corners.
The divider has a handle of 4×64. A tile cannot be narrower than 87. The
divider does not snap, and a double-click on it makes both halves equal. When
one window leaves or closes, the other stays alone in the Space at the full
size. The name of the Space is both app names, the left one first.

The proposal: the entry holds two windows instead of one, each with the four
`tiled` states and its tile's size, and without the `fullscreen` state. A
client told it is full screen at half the width may go into its immersive
mode (a browser hides its toolbar, a player its controls), and the tiled
states say what is true: the edges touch something and the size is fixed. On
macOS a second window joins from a picker on the other half of the screen or
by a drag onto the thumbnail in Mission Control; the first step here uses a
key, and the drag comes with Expansion's strip.

## Steps

In this order among themselves. The roadmap has cascade placement, the tiling
shortcuts and interactive resize next; step C needs the first of them, and
steps A and B need none. Each is one visible result.

| Step | What it adds | What can be seen | Needs |
|---|---|---|---|
| A | Full screen from a client's request: `desktops.rs` moves here; `fullscreen_request` and `unfullscreen_request`, the entry with its saved frame, placement at first commit, `wm_capabilities` with Fullscreen only | `foot --fullscreen`, or mpv with F, fills the display on a Space of its own right of its Desktop, with square corners, no shadow and black behind; leaving returns it to its Desktop and its frame | nothing new |
| B | A key for the same: Control+Super+F | any window goes full screen and back | A |
| C | Expansion on a key | Control+Up spreads the windows; a click picks one | cascade placement; `spring.rs` |
| D | Expansion's strip | live thumbnails of every Space, "+" and close | C; the first blur |
| E | Drag a window to a Space in Expansion | windows move between Desktops | D |
| F | The hover ring, and Control+arrows while Expansion is open | a ring on the window under the pointer; the window area slides to the next Desktop | C |
| G | The full-screen transition | the window grows while the row slides | A, `spring.rs` |
| H | Swipes with three fingers | Expansion and the slide follow the fingers | C; a real touchpad |
| I | Text with `cosmic-text` | names in the strip, the title of a hovered window | D |
| J | Split View with a key | two windows side by side in one Space | A |
| K | The divider | dragging it resizes both windows | J |

Steps C, D, E, F, H and I are described in
[expansion/README.md](expansion/README.md).

How step A is proved: `foot --fullscreen` before its first commit, and a
toggle after it; the configure carries the size of the output and the state;
the corners are square after the commit; `kill -9` of a full-screen client
seen from another Desktop keeps the current Desktop current; a dialog of the
full-screen window stays in its Space; all of it again at scale 5/3.

What cannot be checked nested or in a virtual machine: the swipes (the winit
backend reports pinch and hold but no swipe), smoothness, and a full-screen window on a
hardware plane, which also needs `linux-dmabuf`.

## For the owner to decide

- **A key that sends `Fullscreen` to a browser hides its toolbar.** On macOS
  the green button keeps the toolbar and only the page's own full screen hides
  it; Wayland has one state for both. The proposal accepts the difference.
- **Where these steps go** against `linux-dmabuf`, layer shell and `bk` in
  the roadmap. The table above only fixes their order among themselves.
- **Wallpaper before Expansion's strip?** Without one, the thumbnail of an
  empty Desktop is an empty rectangle.

## Risks

- `switch_desktop`, `switch_target`, `advance_slide`, the carried window of a
  drag and `new_toplevel` all assume that every entry is a Desktop. Step A
  changes them: a dragged window is not carried into a full-screen Space.
  `toplevel_destroyed` must remove the entry of a full-screen window and
  choose the focus, and the `output_geometry(..).unwrap()` calls of
  `input.rs` panic if the current entry has no output mapped.
- A client may drop its full-screen state when it is moved before it has
  drawn at the new size. The order is: configure, and move the window to its
  Space when it commits that size (`with_committed_state`).
- This proposal was written against `3fa9ea0`. Function names in it may have
  moved since.

## Sources

- xdg-shell: https://wayland.app/protocols/xdg-shell
- Smithay at the pinned rev, `src/wayland/shell/xdg/mod.rs`
  (`fullscreen_request`, `new_with_capabilities`, `with_committed_state`;
  `xdg_wm_base` at version 7) and `anvil/src/shell/xdg.rs`:
  https://github.com/Smithay/smithay/tree/65aa86fa5236e41775c313408102a5d5c2d1e2ca
- Apple, full screen: https://support.apple.com/guide/mac-help/mchl9c21d2be/mac
- Apple, Split View: https://support.apple.com/guide/mac-help/mchl4fbe2921/mac
- Apple, Spaces: https://support.apple.com/guide/mac-help/mh14112/mac
- `NSWindow.CollectionBehavior`:
  https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct
- Read for behaviour only, never copied: mutter `src/core/window.c`, KWin
  `src/xdgshellwindow.cpp`, Gala `src/Misc/WindowMover.vala` (a full-screen
  window moved to a workspace of its own, the nearest model on Linux),
  cosmic-comp `src/shell/workspace.rs`, niri's Fullscreen and Maximize wiki
  page.
