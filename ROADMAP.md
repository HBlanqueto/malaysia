# Roadmap

Malaysia is being rebuilt from its foundation. Each step adds one dependency
or one Smithay feature and one result that can be seen. A step is done when it
passes `cargo fmt`, `cargo clippy -D warnings` and its run check, and is pushed
to `the-culling-branch`.

Last update: 2026-11-26.

## Where we are

**Now:** the compositor runs directly on a display and nested in a Wayland
session, apps show their menus, a HiDPI display gets a scale, whole or
fractional, a BarrierKit app leaves its frame to the compositor, a
signal stops the compositor cleanly, and every window gets the continuous
corner of macOS with an outline outside, an inner line inside and the macOS
shadow, Ctrl+Left and Ctrl+Right slide between Desktops, and a window is
dragged by its own title bar and carried along to the next Desktop, a new
window opens in a cascade, a window is resized by its own edges, and
Ctrl+Super with F, C, an arrow or R tiles the focused window at once, and
Ctrl+Up spreads the windows of the Desktop (Expansion on a key) under a strip
that shows every Desktop live, adds one with "+" and removes one with its
close button, a window spread there is dragged onto the thumbnail of
another Desktop and moves there, the window under the pointer gets a ring in
the accent colour, and Ctrl+Left and Ctrl+Right slide the spread windows to
the next Desktop while the strip stays, and the windows of Expansion move
to their slots with its spring whenever the arrangement changes, and the
strip names every Desktop and the window under the pointer shows its
title, and the strip is a material that blurs what is behind it. Of
Expansion only the swipes are left, which need a real touchpad. A window
that asks for full screen gets a Space of its own in the row, and
`malaysia-expansion` opens or closes Expansion for a launcher, and Ctrl+Alt+F
sends the window in front to full screen and back, growing in place while
the row slides, and Ctrl+Alt+Left or Right puts two windows side by side in
a Split View whose divider is dragged, and three fingers on a touchpad
open Expansion and slide the row. The row of workspaces of the proposal is
built; what is left of it is in «Known gaps». The proposal is in
[crates/malaysia-compositor/src/workspaces/README.md](crates/malaysia-compositor/src/workspaces/README.md). `linux-dmabuf` waited for a real GPU, which the
development laptop now has. Nested, the compositor has run on that laptop's GPU; directly
on a real display it has not run yet.

The display after step 11 (on a NixOS machine whose display reports a
180x100 mm panel at 1600x900, directly on the display, a terminal drawn at a
scale of 5/3):

![The display after step 11](screen.png)

| Step | What it adds | What can be seen | State |
|---|---|---|---|
| 1 | Smithay, event loop, Wayland socket | a window with one color, nested | done `8bbad9a` |
| 2 | DRM/KMS output through GBM and GLES | the same color, directly on a display | done `da8a2c9` |
| 3 | Logging with `tracing` | Smithay's warnings and the compositor's own messages | done `017f0a4` |
| 4 | Windows: `wl_compositor`, `wl_shm`, `xdg_shell`, `wl_output` | an app drawn on the screen | done `80a5b31` |
| 5 | Nested session: the winit backend returns and shares the frame code with DRM | the compositor inside another desktop | done `7f75682` |
| 6 | Seat: keyboard and pointer | typing in a window | done `9427c17` |
| 7a | A Nix package and a NixOS test machine | the compositor on a virtual GPU in a full Linux session | done `8c058c3` |
| 7b | Session with libseat | start from a TTY as a normal user, pause and resume on VT switch | done `447a64a` |
| 8a | Devices with udev and libinput | real keyboard and mouse, the GPU found without an argument, Ctrl+Alt+Fn and Ctrl+Alt+Backspace | done `983e166` |
| 8b | Cursor | a pointer that can be seen on a real display | done `aa62ca9` |
| review | The whole code read against Smithay's source | fixes for focus, hidden errors and a retried frame | done `afa81d4` |
| 9 | Popups: `xdg_popup` with its keyboard and pointer grab | a menu, its submenus and a context menu that stays inside the display | done `5c5d116` |
| 10 | Output scale: 1 to 4, chosen from the display's physical size and mode with mutter's rule; nested, the host's scale | windows, menus and the cursor twice as large on a HiDPI panel | done `17d9fe9` |
| 11 | Fractional scale: `wp_fractional_scale_v1` and `wp_viewporter`, and mutter's fractional rule (steps of a half, a third and a quarter that divide the mode) | a terminal drawn sharp at 5/3 | done `44fba23` |
| 12 | `limitless`, the protocol shared with BarrierKit: the `limitless_window_style_v1` global at version 1 | a BarrierKit app leaves its frame and its round corners to the compositor, which does not draw them yet | done `da4b026` |
| 13 | Signals: `calloop`'s `signals` feature, so SIGTERM, SIGINT and SIGHUP stop the event loop | `./dev.sh demo` ended with Ctrl-C, or the compositor ended with `kill`, logs the signal and leaves no socket behind | done `6c0389c` |
| review | An audit of the code on the Linux laptop | nested, the compositor stops when its host goes away and releases the keys when the host takes the keyboard; a menu whose grab is refused is closed | done |
| 14 | `ctdesign`, BarrierKit's design tokens, and the compositor's own effects module | every window cut to the continuous corner of a macOS window, with the outline outside it and the inner line inside it, in light and dark and for the main window and the others; a BarrierKit app gets the corner of its bar (all four bars are 16 in barrierkit's data, so `set_bar` gives the default 16.8 for every bar and changes nothing visible today) | done `e43b9a5` |
| 15 | the window shadow from `WINDOW_SHADOW_*` | every window casts the macOS shadow, stronger under the main window | done `2ed869e` |
| 16 | Desktops: one `Space` per Desktop | Ctrl+Left and Ctrl+Right move between them; until step 25, Ctrl+Right on the last Desktop with windows opened an empty one and an empty last Desktop went away when left | done `5837b11` |
| 17 | The slide between Desktops: an animation clock and macOS's spring from `DESKTOP_SWITCH_*` | the Desktops slide past with a fast start and a slow end, a black band of 64 between them, and a second key press mid-slide carries on from where the strip is | done `174f353` |
| 18 | `suspended` for the windows of a hidden Desktop | a client on a hidden Desktop is told it is suspended when the slide ends, gets its pending frame callbacks once, and is told it is shown again at the switch back | done `8b5cf44` |
| 19 | Interactive move: `xdg_toplevel.move` and a pointer grab | a window dragged by its own title bar | done `bbbedb9` |
| 20 | A dragged window goes along to the next Desktop | holding a window by its title bar and pressing Ctrl+Left or Ctrl+Right takes it to that Desktop, still under the pointer | done `ca36aeb` |
| 21 | Cascade placement: `WINDOW_CASCADE_STEP` and `WINDOW_CENTER_TOP` | the first window of a Desktop opens centred with a quarter of the free height above it, each next one 29 right and 29 down from the one in front, and a dialog over its parent | done `f0df1b9` |
| 22 | Interactive resize: `xdg_toplevel.resize` and a pointer grab | a window resized by dragging its own edges and corners, the opposite edge staying still | done `1ee5505` |
| 23 | Tiling by keyboard: Fill, Center, the halves and Return to Previous Size | Ctrl+Super with F, C, an arrow or R fills the display, centres the window, puts it in a half, or returns it to the frame it had before | done `219f8d3` |
| 24 | Expansion on a key: the layout fitted to macOS, the spring of ω0 25, the window drawn at a factor | Ctrl+Up spreads the windows of the Desktop with no overlap and one scale, a click picks one, Escape or Ctrl+Up again puts them back | done `6b8fa7c` |
| 25 | Expansion's strip: thumbnails of every Space, "+" and close | with Expansion open a bar slides in at the top; the pointer makes it grow and show every Desktop live; a click on one goes there, "+" adds a Desktop and the close button of a thumbnail removes one | done `dc70f98` |
| 26 | Drag a window to a Space in Expansion | a window spread in Expansion, dragged onto the thumbnail of another Desktop, moves there | done `1d0bd89` |
| 27 | The hover ring, and Ctrl+arrows inside Expansion | the window under the pointer gets a ring in the accent colour, and Ctrl+Left and Ctrl+Right slide the spread windows to the next Desktop while the strip stays | done |
| 28 | Nothing jumps in Expansion | a window released from a drag returns to its slot, the windows left move to their new slots, and closing during Ctrl+arrows shrinks both Desktops back, all with Expansion's spring; a window dropped on "+" gets a new Desktop | done |
| 29 | Text in Expansion, with `cosmic-text` | the strip names every Desktop, in its cell and under its thumbnail, and the window under the pointer shows its title in a capsule at its centre | done |
| 30 | The blur under Expansion's strip | the strip is macOS's material: what is behind it is blurred, its colour raised, a veil over it; the black band between Desktops passes under it soft | done |
| 31 | Full screen from a client's request | `foot --fullscreen`, or an app's own full-screen key, fills the display on a Space of its own right after its Desktop, with square corners, no shadow and black behind; leaving returns it to its Desktop and its frame | done |
| 32 | Expansion from a launcher | `malaysia-expansion` opens Expansion, or closes it, through the private protocol `limitless_expansion_v1`; the package installs it with a desktop entry named Expansion, for the launcher and the Dock to come | done |
| 33 | A key for full screen | Ctrl+Alt+F sends the window in front to full screen and back, as macOS's Control+Command+F; the app is not told the key | done |
| 34 | The full-screen transition | a window going full screen grows in place from its frame to the display while the row slides to its Space, and shrinks back to its frame while the row slides back, with macOS's spring | done |
| 35 | Split View with a key | Ctrl+Alt+Left or Right puts the window in front on that half of a full-screen Space and the window below it on the other, with a black divider of 12; when one leaves or closes, the other stays full screen alone | done |
| 36 | The divider of Split View | dragging the divider resizes both tiles, a tile stops at 87, a double-click makes the halves equal, and a handle on it shows which side is active | done |
| 37 | Three-finger swipes | on a touchpad, three fingers up open Expansion and down close it, sideways slide to the next entry of the row, all following the fingers and handing their speed to the springs on release | done |

Steps 1 to 8b were the first stage: the compositor runs directly on hardware
and nested in another session.

## After this stage

In the order they will be taken, one at a time. On 2026-11-26 the user asked to
take the macOS window model first, so these come before the rest:

1. Expansion, the Mission Control of this compositor: steps C to F and I of
   [crates/malaysia-compositor/src/workspaces/README.md](crates/malaysia-compositor/src/workspaces/README.md).
   Steps C, Expansion on a key, D, the strip, E, dragging a window to a
   Space, and F, the hover ring and Ctrl+arrows while Expansion is open, are
   done, and so are I, text with `cosmic-text`, and H, the swipes.
2. The spring of a tile (measured: ω0 45, ζ 1.38, 240 ms), then pairs and
   quarters, then restoring a tiled window when it is dragged away.

Then:

1. `linux-dmabuf`: until then every client draws with software rendering, so a
   real machine looks slower than it is. It is written and kept out of the
   branch: it only turns on with a hardware render node, and the test machines
   have none. The development laptop has one since 2026-11-25, so it can be
   checked there. The steps below go ahead meanwhile.
2. Layer shell, which the shell needs to place its surfaces.
3. `bk` returns, with its first two components: the lock screen and the
   screen capture portal.
4. Hotplug and several displays.
5. What a laptop needs: display power and idle, touchpad settings such as
   tap-to-click.
6. The rest of window management, the shell and the apps.

Proposed on 2026-11-26 and built: full screen as a Space of its own and
Split View (steps A, B, G, J and K of the same file).

## How the desktop is put together

The decisions and the new dependencies are in the «Design» section of
[README.md](README.md). None of it is built yet.

## What macOS does (measured, 2026-11-25)

Measured on macOS 27.2.1, one 2560×1440 display at 180 Hz, with the Dock
visible (autohide off), through `NSScreen`, `CGWindowList`, the accessibility
tree and screenshots. Sizes are points. Modes tried: 2560×1440 and 1920×1080
at scale 1, 1280×720 and 960×540 at scale 2.

- **Menu bar:** 30 in every mode. It is a window of its own, not part of the
  Dock.
- **Dock (tile 59):** the Dock is 79 high and 6 above the bottom edge, and the
  area it reserves is 85, from the same bottom edge, in all modes where it
  fits. At 960×540 it shrinks to 66 high and reserves 72, and its width is the
  screen minus 36 each side. With autohide on it reserves nothing. Its pill top
  edge matches the reserved edge in the pixels.
- **Windows:** the app decides the first position, not the system. A new window
  of the same app goes 29 right and 29 down from the previous one, and a
  Finder window (700×600) and a TextEdit window (603×505) both follow it. After
  a mode change the first window opened at 316,30, that is, clamped under the
  menu bar. Why it started at x 316 is not understood.
- **Dialogs:** a sheet is centered horizontally on its window (Finder's Go to
  Folder, 460 wide, sits 120 in from a 700 wide window and 102 below its top).
  The Open panel with no window is centered horizontally and its top is
  `(screen height − panel height) / 4 + 2`: 250 at 1440 and 160 at 1080 for a
  panel 448 high. Two points only.
- **Spaces:** the switch with Control+arrow is a slide with a fast start and a
  slow end. Its timing is in the later pass below.
- **Frames:** the display link ticks every 5.556 ms (180 Hz) with a spread of
  0.03 ms over 539 frames. Screen capture saw 11.1 ms gaps in about a quarter of
  the frames, and it cannot tell if the compositor or the capture caused them.

Later pass (2026-11-26), with the tables in barrierkit
`docs/research/macos-measurements-2026-11-26.md`:

- **Scale:** every value is in points. 1920×1080 at scale 1 gives the
  2560×1440 numbers, and 1280×720 at scale 2 doubles every pixel value.
- **Window shadow:** a blurred box moved down. The main window has σ 19.5, dy 18
  and alpha .689 dark or .393 light; any other window has σ 7.75, dy 7 and alpha
  .431 or .210. Dark is about twice light.
- **Space switch:** a spring measured at ω0 23 to 24 and ζ 1.72 to 1.75, 64
  of gap between Spaces, half way at 103 to 119 ms, t90 at 319 to 331 ms and
  about 1 s until the last pixel. The token is ω0 23 and ζ 1.72, because that
  pair is half way at 109 ms and at 90 % at 327 ms, inside both ranges. Mission Control is a
  critically damped spring (ω0 25) of about 260 ms.
- **Dock:** magnification 59 to 128 in about 115 ms with a radius of influence
  of 207; auto-hide waits about 250 ms; its glass blur is about 16.
- **Wallpaper tint:** one colour per window, the average of the wallpaper under
  the frame, mixed in encoded sRGB with k about .046 (dark window background)
  and about .045 for blurring materials; light windows are not tinted.
- **Not understood:** the window open shows no scale animation, and the menu
  bar ink and Control Center ignore the light scheme.

- **Desktops and tiling (documented by Apple, not measured):** Desktops are
  added with + in Mission Control, up to 16, and a Desktop that is emptied
  stays. Control+Left and Control+Right move between them. A full-screen app
  or Split View is a Space of its own. A window can be assigned to all
  Desktops, to one, or to none. fn+Control+F, C, the arrows and R fill,
  centre, halve and restore a window. Sources:
  https://support.apple.com/guide/mac-help/mh14112/mac and
  https://support.apple.com/guide/mac-help/mchl9674d0b0/mac.

Mission Control, full screen and Split View (2026-11-26), with the tables in
barrierkit `docs/research/macos-mission-control-2026-11-26.md`:

- **Mission Control windows:** one scale for all of them, never above 1. A
  window that fits stays where it is; overlapping ones are pushed apart,
  about 13 from each other, inside an area 20 from the sides, 40 below the
  strip and 15 above the Dock. One critically damped spring, ω0 25, moves
  every window in, out and on a pick. The wallpaper is not dimmed or blurred.
  A pair is pushed along the line between its centres, both windows the same
  distance, and stacking order breaks a tie. The rule for the scale and for
  cascades is not found: the best algorithm matches 78 of 116 measured
  arrangements.
- **Spaces strip:** 40 high with the names, 146 with thumbnails of 1/16 of the
  display once the pointer enters it (a spring of ω0 16); blur σ 27.5.
- **Full screen:** the Space goes right after the Desktop the window came
  from, which was also the current one in every run. The window grows in place (ω0 25, ζ 1.4) while the row slides, about
  550 ms in all, and returns to its Desktop and its exact frame. Inside, the
  window has square corners and no shadow, on black.
- **Split View:** a black divider of 12 with no margin, a smallest tile of 87,
  no snapping; a double-click makes the halves equal.
- **Not measured:** whether a swipe follows the fingers, and anything in
  light mode or on another display size.

Not measured: banners and Notification Center, glass at slider 0, the system
switches (Reduce Transparency, Increase Contrast, Reduce Motion), mouse drag
and snap, more than one display.

## How each step is checked

On Linux the checks and `./dev.sh demo` (the nested session with a terminal)
run on the host inside `nix develop`. On a Mac, `vm.sh` gives a Linux box: Nix,
a virtual KMS display (`vkms`) for the direct backend and a headless desktop
for the nested one. `./vm.sh shot file.png` captures the virtual display.

With a device argument the compositor runs directly on that device. With no
argument it runs nested when `WAYLAND_DISPLAY` is set.

Nested input is checked without a person: the compositor runs inside a
headless sway with wayvnc, and a script sends pointer and key events over VNC.

Steps 7 and 8 need a seat, udev and a GPU, which the box of a Mac lacks. They
are checked in a NixOS virtual machine with a full Linux session, the flake's
`checks.<system>.vm` (on a Mac, `./vm.sh test dir` runs it and writes its
screenshots to `dir`). What only real hardware can prove is marked here as
**not verified on hardware** until someone runs it.

## Known gaps

- Any client may open or close Expansion through `limitless_expansion_v1`;
  on macOS only the Dock opens Mission Control. Limiting it to the shell
  needs `security-context-v1`, which Smithay has at the pinned rev, about 25
  lines, and it only tells sandboxed apps apart.
- Nothing exports `XDG_CURRENT_DESKTOP`, because the compositor starts no
  session, so the desktop entry of `malaysia-expansion`, shown only in
  `X-Malaysia`, shows in no menu yet. `desktop-file-validate` refuses a bare
  `Malaysia`, which is not a registered desktop name: the session must
  export `X-Malaysia`, or the name be registered with freedesktop.org. The
  entry has no icon, and the tool prints why it fails when the compositor
  does not offer the protocol.
- Menus have no shadow.
- A closing window disappears at once. The close animation needs a snapshot
  of the dead surface, which does not exist yet.
- Every window is cut to its window geometry, so a client's own shadow outside
  it (GTK) is not shown.
- A window whose buffer is rotated or flipped is
  drawn square.
- A window narrower or shorter than 3.5 times its corner gets Figma's shorter
  tails, a circle at twice; macOS is not measured there.
- The scheme of the frame lines and of the shadow comes from
  `MALAYSIA_SCHEME` (`light`, or dark by default), read once at start. It is
  temporary, until the appearance store exists.
- A click outside an open menu closes the menu and also reaches the window
  under it, without giving that window the focus. KWin keeps that click.
- When an app closes its own menu, the keyboard focus returns to the window on
  the next key or pointer event, not at once.
- Placement: macOS cascades per app; here the cascade is per Desktop, for any
  client. Where the cascade starts again is not measured on macOS; ours starts
  each axis again on its own. The first window's quarter of free height comes
  from two points of the Open panel; where macOS opens the first document
  window is not understood. A window keeps its place when it is unmapped and
  mapped again or changes size. There are no reserved areas yet, so the free
  area is the whole display.
- A dialog is centred on its parent when the parent is on the same Desktop and
  already placed; otherwise it joins the cascade. Checked with a test client,
  not with a GTK or Qt dialog.
- The scale follows mutter's rule and cannot be set by hand. A 13.3 inch
  2560x1600 panel and a 14 inch 2880x1800 panel get 5/3 (KWin would give 2,
  niri 1.75), a 16 inch 2560x1600 panel 4/3, a 27 inch 4K monitor 1.5. A
  display that reports a wrong size gets a wrong scale, and one that reports
  none (a TV, a projector) gets 1.
- At 1.25 and 1.75 (a 14 inch 1920x1200 panel, for example) a decoration drawn
  as a subsurface, such as foot's title bar, sits one pixel low and loses a
  row: Smithay rounds a half pixel that way.
- A client without `wp_fractional_scale_v1` (GTK 3) draws at the next whole
  scale and is scaled down: the right size, a little soft.
- Above scale 1 a client that draws with `wl_shm` fills up to four times the
  pixels on the CPU, until `linux-dmabuf`.
- A client is not told which display its cursor is on, so some toolkits draw
  the cursor at scale 1 and it looks soft above it. foot and GTK are right.
- The fallback cursor is a white square: it shows where no client sets its
  own cursor. Cursor themes come later.
- The pointer starts at the top left corner.
- An absolute pointer has run only in the machine check, as QEMU's USB
  tablet; not on a real display.
- Nested, the host draws its own cursor; the client's cursor is not shown.
- Nested works inside a Wayland session only; an X11 host is not supported.
- The compositor starts no program; a client is started by hand.
- Under logind, switching VT with Ctrl+Alt+Fn needs polkit; without it the
  switch is refused and logged.
- Displays and GPUs are read once at start: no hotplug.
- The compositor must be started from the active VT; starting it from an
  inactive one is an error.
- A display mode is trusted as the kernel reports it; one with a zero total or
  a zero refresh would panic the compositor.
- A window on a hidden Desktop gets no frame callback after the one sent with
  `suspended`, as in mutter and KWin, so a client that draws without waiting
  for that state sleeps until it is shown.
- Ctrl+Left and Ctrl+Right no longer reach apps, as on macOS.
- A pointer grab (a held button, an open menu) ends at the switch,
  and every popup of the windows left behind is closed.
- Every new window opens on the current Desktop and takes the focus, also a
  dialog or a window of an app on a hidden one.
- A window held by its title bar goes to another Desktop with Ctrl+Left or
  Ctrl+Right, and in Expansion a window is dragged onto the thumbnail of
  another Desktop; there is no drag to the display edge and no «Assign To». That Ctrl+arrow carries a dragged window is reported by macOS
  users, not documented by Apple and not measured.
- A dragged window released mid-slide stays still until the strip rests; a
  second key press before that makes it jump into its Desktop.
- Only the app starts a move or a resize, from its own title bar and edges:
  there is none started by the compositor (a modifier and a drag, or an edge
  drawn for an app without decorations), no touch or tablet drag, and no
  snapping to the edges; tiling is by keyboard only.
- The compositor tells clients it only does full screen, so an app that
  follows `wm_capabilities` hides its maximize and minimize buttons. A
  request to maximize is answered with a configure that changes nothing.
- Full screen is Ctrl+Alt+F: Alt sits where Command does on a Mac keyboard,
  and Ctrl+Super+F is already Fill, Super standing in for fn. It acts on the
  window in front of the current entry, and does nothing while Expansion
  is open. It takes Ctrl+Alt+F from the apps (Emacs's forward-sexp), and on
  a layout without a latin f, as Ctrl+Super+F, it does nothing. A browser
  sent to full screen by it hides its toolbar too:
  Wayland has one full-screen state, where macOS keeps the toolbar.
  The transition starts when the client commits its new size, so its new
  buffer is the one scaled: uniformly, to cover the rectangle between the
  two frames, and cropped to it; there is no snapshot of the old one and no
  cross-fade. All four edges follow one spring, where macOS's left and top
  edges lead a little, and the row's slide back is the Desktop switch spring
  where macOS's is near a bezier of 380 ms. On entering, the corners go
  square and the shadow goes at the first frame; macOS keeps the shadow
  while the window grows. Leaving starts from the whole display even for a
  window smaller than it. Only the transition of the current entry is
  animated, and Ctrl+Left or Ctrl+Right during the slide back makes the
  black place left by the Space disappear at once. A window that opens while a full-screen Space is current
  opens in that Space, above the full-screen window, whatever its client;
  macOS sends the windows of other apps to a Desktop. A dragged window can
  be carried into a full-screen Space with Ctrl+arrows. All of them go to
  the nearest Desktop when the full-screen window leaves or closes. In
  Expansion a full-screen Space is named by its `app_id`, has no button to
  leave full screen on its thumbnail and takes no dropped window (Split
  View). While Expansion is open the black behind a full-screen window is
  not drawn. The row holds 16 entries of both kinds; with 16, a request for
  full screen is answered without the state. The bars that slide in at the
  top edge wait for the menu bar.
- The margin of tiled windows is not measured on macOS, so
  `WINDOW_TILE_MARGIN` is 0.
- A tile is applied at once, with no spring. There are no pairs, no quarters,
  and a tiled window dragged away keeps its tiled size.
- Whether macOS restores the position as well as the size, and what it does
  with a window that cannot take the size of a tile, are not measured. Ours
  restores position and size, and refuses a tile outside the client's minimum
  and maximum.
- A terminal snaps to its cells and leaves a gap at the right or bottom of its
  tile (foot: up to 5 at the right and 11 at the bottom of a half).
- An answer that arrives after a move begun meanwhile still lands the window
  on its tile.
- A tile does not follow a change of the display's size.
- A tile asked before the client has answered a Return to Previous Size saves
  the tiled frame as the previous one.
- Return to Previous Size goes back to the frame before the first tile, even
  after the window was moved or resized by hand since.
- A focus change between the end of a pointer resize and the commit of a tile
  asked meanwhile tells the client it may pick its size (0x0) before the tile
  is committed.
- A tile sends only a size, no `tiled` or `maximized` state.
- Super stands in for fn, and Ctrl+Super+F is also what macOS's
  Control+Command+F (full screen) would map to.
- The letters are matched by keysym, so a non-Latin layout has no F, C or R.
- A move started before the last commit of a resize can leave the window
  displaced by that last change of size.
- With a Qt app Smithay logs `surface missing from known popups` each time a
  menu closes. The menus work; the cause is not looked into.
- A window can be dragged until only the point held is left on the display.
  macOS keeps the title bar below the menu bar and reachable; how is not
  measured.
- After a drag the pointer focus and its position are those of one motion
  before the last, until the next motion.
- A second key press mid-slide is not measured on macOS. Ours carries the
  speed of the strip and can pass the target by at most 6 % of a Desktop.
- The spring is read when the frame is rendered, not when it is presented.
- A window wider than the display shows over the neighbouring Desktop during
  the slide.
- During the slide the pointer hits the target Desktop at its rest position.
- The gap between Desktops is a black band from `DESKTOP_GAP_COLOR`. Its colour
  was observed on macOS, not sampled. With no wallpaper yet the Desktops show
  the clear colour, so the band has little contrast.
- The slide has no Reduce Motion and no touchpad gesture.
- A slide redraws the whole display on every refresh for about 1.1 s, though
  after 0.6 s the strip moves under 18 pixels. Nested, in a debug build, one
  slide costs 49 to 78 ms of CPU against 4 ms for the instant switch.
- The windows of the Desktop left keep their frame callbacks and the display
  until the strip rests.
- The top window of the Desktop left takes the shadow of a window that is not
  main at the key press; macOS is not measured there.
- Each frame that renders nothing inserts a one-shot timer to send its frame
  callbacks one refresh later; a reusable timer is not done. The last frames
  of a slide move no pixel, so each slide inserts several.
- Expansion's layout is the fit of the Mac's script, not macOS itself: within
  3 points for windows alone, pairs and chains of three, 11 to 33 off for a
  pair in a corner and far off for cascades of four or more (nine cascaded
  windows end at scale .513 where macOS gave .434).
- The margins of Expansion's area come from one display size, and the bottom
  one assumes no Dock.
- A new layout while Expansion is open (a window opened, closed or resized)
  jumps to its slots; it is not animated.
- A popup mapped while Expansion is open is drawn unscaled.
- The frame lines stay one pixel at every scale of Expansion.
- A click during Expansion's closing animation reaches the windows' real
  frames.
- Ctrl+Up, Ctrl+Down and Escape no longer reach apps while Expansion is open,
  and Ctrl+Up never does.
- Expansion has no hot corner.
- The swipes follow gnome-shell's rule, as macOS's is not measured: 300 of
  travel open Expansion all the way and 400 slide one entry, a slow release
  goes to the nearest end and a fast one one entry on, and a swipe moves at
  most one entry. Only three fingers count, and only directly on a display:
  the nested session gets no swipes from its host. Clients get no gestures
  at all, as `pointer-gestures` is not served. A swipe does not start while
  a window or the divider is dragged.
- Expansion's strip blurs what is behind it, but until there is a wallpaper
  almost nothing is: the Desktop background is flat, the windows leave the
  top before the strip covers it, and only the black band between Desktops
  and the shadows of windows pass under it. A dragged window is drawn above
  the strip. The blur works on a copy an eighth of the size, read back from
  the frame with a blit, which needs OpenGL ES 3: without it the strip is
  the flat veil. With Expansion open, twenty Ctrl+arrow slides cost the
  compositor 2.5 s of processor instead of 1.8 in a debug build, nested;
  with nothing moving behind the strip it costs nothing. Only the dark
  material is measured.
- The names in the strip are "Desktop" and a number, in English: the
  rebuild has no translations yet. Their sizes, 14 in the cell and 12 under
  the thumbnail, come from the width of macOS's ink only. The font is
  the desktop's, Adwaita Sans, which the compositor asks for by name;
  another sans font is used when it is missing, and with no font at all
  there is no text. Nothing makes it the default of the apps yet: the
  compositor starts no session, so an app draws the sans font of the
  system's fontconfig. The machine check does not install Adwaita Sans: GTK
  then draws the text editor with it, and the check, which reads that
  editor's menu and text by OCR, no longer finds them. The check's
  compositor draws its names with DejaVu Sans.
  The names in the cells go at once when the strip starts to grow, and the
  thumbnails and their names appear at their final place as it grows; the
  hand-over is not measured.
- The title of the window under the pointer shows only while nothing in
  Expansion moves, with no fade. Its capsule is a flat gray 244 at alpha
  .365, which gives the one pixel measured over yellow; macOS draws a light
  material there. A title wider than its window ends in an ellipsis. An
  empty Desktop shows no "No Available Windows" pill: its look is not
  measured.
- The strip slides with Expansion's spring, half way at 67 ms, where macOS
  measured 47 to 58 on entry.
- The ring of the current thumbnail and the hover ring are BarrierKit's
  accent, not macOS's blue.
  The cross of the close button and the colour of the strip's last line are
  not measured.
- Thumbnails have no shadow and no frame lines.
- More Spaces than fit run off both sides of the strip's row.
- Every client draws at full rate while the thumbnails show.
- With no wallpaper, the strip and an empty thumbnail are 8 levels apart.
- The strip does not grow when the pointer is already at the top as Expansion
  opens, until the pointer moves.
- The windows of a removed Desktop land above the heir's, and the top one
  takes the focus at close.
- Adding or removing a Space is not animated.
- The hover ring is 4 wide at every window scale (macOS was measured at .93
  only), starts at the window's edge over its outer line, and shows while
  windows move. Nested, it stays when the pointer leaves the host's window.
- Ctrl+Left and Ctrl+Right are ignored while a window is dragged in
  Expansion, and there is no Control+digit. A click during their slide picks
  by the slot at rest. A Desktop still sliding out as Expansion opens, or an
  arrow pressed while the strip grows, can show slots of its last Expansion.
  During that slide the black band
  between Desktops passes blurred under the strip, and a Desktop left before the
  strip grew slides out under it.
- The hover ring is drawn with its window: under the shadows of the windows
  above and of the strip, and through the strip while Expansion opens.
- Dragging a window in Expansion is not measured on macOS. The drag starts
  past the kit's drag threshold of 8, GNOME's and GTK's default, on either
  axis. The dragged window keeps its slot's scale and does not shrink near
  the strip. It is drawn above the strip and its ring, without the strip's
  shadow on it. The target is not highlighted and the close
  button hides during a drag. A window released anywhere but another
  Desktop's thumbnail or "+" returns to its slot, and the windows left move
  to their new slots, with the spring of Expansion's opening: macOS's own
  motion there is not measured. A window on its way back is drawn in its
  stacking order, so it can pass under another one, and a change that
  starts while windows still move restarts their spring from rest.
  Expansion stays open after a drop. The window lands at its old
  frame, on top of the other Desktop. Escape mid-drag closes
  Expansion. The window stays in its own thumbnail during the drag.
- Split View is made only by its key: the window in front takes the half
  of the arrow and the next window below it on the same Desktop, not a
  dialog, the other; macOS makes it from the green button's menu or by a
  drag in Mission Control. Each window is told it is tiled on all four
  edges, with its tile's size, and not full screen; a client that keeps
  another width, as `foot` rounding to its cells, moves the gap off 12.
  While one of the two has not answered, the other waits at its own
  place; the key pressed again then sends both back. The strip names the
  Space with both `app_id`s joined by "and", in English. A tile that would
  be narrower than 87, or a window that does not fit its tile, makes the
  key do nothing. A window whose smallest width does not leave half the
  display gets a wider tile. The divider is dragged with no snapping and
  shows no resize cursor, as there is no cursor shape yet; its width is
  kept in pixels, so a change of the display's mode keeps it. A
  double-click on another Split View's divider at the same place within
  the interval also counts.
- There is no drop onto a full-screen Space (Split View) and no reordering of
  thumbnails.
- A resize of the display while Expansion is open keeps the old slots.
- A slot can pass the margin of Expansion's area by 2 at the sides and 3 at
  the top and bottom, which is the slack of the fit.
- `foot` snaps to its cell grid at its first configure after a fractional
  scale, so it loses a few pixels of height the first time its state changes.

## Not verified

- Three-finger swipes (step 37): only in the machine check, with a virtual
  touchpad made with uinput, which swipes up, down, right and left, and by
  the tests of the tracker. Not checked: a real touchpad, its feel, and a
  swipe across a VT switch.
- The divider of Split View (step 36): nested, at scales 1 and 5/3, with
  `foot` (a drag, the stop at 87, the double-click, the handle's two
  colours, a press inside a tile), and the machine check, which drags it.
  Not checked: directly on a display.
- Split View with a key (step 35): nested, at scales 1 and 5/3, with
  `foot` (the tiles, the divider, the strip's name, leaving, closing, a
  partner that never answers, three windows), and the machine check with
  a terminal and the text editor. Not checked: directly on a display, a
  GTK 4 or Qt app in a tile.
- The full-screen transition (step 34): nested, at scales 1 and 5/3, with
  `foot`, where the window's box was followed over time, and the machine
  check. Not checked: directly on a display, where its smoothness can only
  be judged.
- The key for full screen (step 33): nested, at scales 1 and 5/3, with
  `foot`, and the machine check. Not checked: directly on a display.
- Expansion from a launcher (step 32): nested, and the machine check, which
  runs `malaysia-expansion` twice and validates its desktop entry. Not
  checked: directly on a display, from a real launcher.
- Full screen from a client's request (step 31): nested, at scales 1 and
  5/3, with `foot` (its key, `--fullscreen`, `kill -9` in both cases, the
  strip, a second window in the Space), and the machine check, where the
  text editor's own key enters and leaves. Not checked: directly on a
  display, a Qt app, a video player, a dialog of the full-screen window.
- The blur under the strip (step 30): nested, at scales 1 and 5/3, where
  the band between Desktops was seen blurred, and the machine check, which
  finds the strip's colour over the flat background on the virtual GPU.
  Not checked: directly on a real GPU, a window or a wallpaper behind the
  strip, and which way up the copy is on the DRM backend, which a flat
  background cannot show.
- Text in Expansion (step 29): nested, at scales 1 and 5/3, with `foot`,
  with a right-to-left title and with no fonts, and the machine check,
  which finds a name under the second thumbnail. Not checked: directly on a
  display, a title in a script the fonts do not cover.
- The motion of the windows in Expansion after a drop, a close or a new
  window, and the drop on "+" (step 28): nested only, at scales 1 and 5/3,
  with `foot`. Not checked: directly on a display.
- The hover ring and Ctrl+arrows inside Expansion (step 27): nested only, at
  scales 1 and 5/3, with `foot`. Not checked: directly on a display.
- Dragging a window to a Space in Expansion (step 26): nested only, at scales
  1 and 5/3, with `foot`. Not checked: directly on a display.
- Expansion's strip (step 25): nested only, at scales 1 and 5/3, with
  `foot`, and the machine check, which adds a Desktop with "+" and goes to
  it by its thumbnail. Not checked: the rest directly on a display.
- Expansion on a key (step 24): nested only, at scales 1 and 5/3, with
  `foot`. Not checked: directly on a display.
- Tiling by keyboard (step 23): nested, at scale 1, with `foot` and a Qt 6
  app. Not checked: GTK 4, directly on a display, and a fractional scale.
- Interactive resize (step 22): nested, at scale 1, with `foot` and a Qt 6
  app. Not checked: GTK 4, another scale, directly on a display, and the
  frame of a window smaller than about 43 points.
- Cascade placement (step 21): nested, at scales 1 and 1.25, and the machine
  check. Not checked: a dialog of a real toolkit.
- Desktops (step 16): nested, and directly on the virtual display of the
  machine check, which switches to an empty Desktop and back.
- A dragged window carried to the next Desktop (step 20): nested only, at
  scale 1, with `foot` and with a Qt 6 app. Not checked: directly on a
  display.
- The carried window gets `wl_surface.leave` at the key press and
  `wl_surface.enter` right after, because it is taken out of one Desktop and
  put on the other.
- The slide between Desktops (step 17): nested at scale 1, on the laptop, and
  the machine check, which waits for the strip to rest. Not checked: the
  slide on a real GPU directly on a display, a fractional scale, and a VT
  switch mid-slide.
- Real hardware: nested, the compositor has run on a laptop with an AMD GPU,
  with clients that draw through `wl_shm`. Directly on a display it has only
  run on virtual ones (vkms and QEMU's virtio GPU) with software rendering,
  and with QEMU's keyboard and mouse. A real GPU driver on the direct backend,
  a touchpad and several GPUs are untested.
- The hardware cursor plane: built without `renderer_pixman`, Smithay puts a
  client's cursor on the plane only when it is an ARGB8888 `wl_shm` buffer
  drawn at exactly the output's scale, uncropped, on an untransformed output
  and no larger than the plane; the white fallback square and every other
  cursor are drawn into the frame. Which path a real display takes has not
  been observed.
- Signals on the direct backend: the stop was checked nested only.
- A second signal while the compositor is stopping: by calloop's code it stays
  blocked until the signal source is dropped at the end of `main`, and then
  ends the process with that signal's status. Not exercised.
- A refused menu grab that closes the menu: it follows the protocol and
  mutter, and was not exercised, because no client here reaches the refused
  path.
- A page flip that the kernel never reports freezes the screen until the next
  VT switch; there is no page-flip watchdog.
- If the session cannot be resumed, drawing is retried on every refresh with a
  warning each time.
- A frame lost while the session is paused, and starting from a VT that is not
  the active one.
- Scroll, drag and drop and the clipboard.
- Scale on a real panel: the rule was checked with mutter's own table of
  displays and on a virtual display made to report a small panel, at 5/3. A
  client's own cursor above scale 1 on the direct backend, GTK 4 and Qt 6
  were not checked.
- Menus were checked with one GTK 3 app and, nested, one Qt 6 app (FeatherPad:
  its window, menus, drag, Desktop switch and close). A popup that asks to be
  moved after it is shown is untested.
- The window frame (step 14) was checked nested only, in a headless sway at
  scales 1, 1.25 and 2; nothing of it ran directly on a display. On a Mac, two
  things are still to be measured: a capture at 2x of the uncovered pixels in row 0 of the window
  corner (15.15 computed against 16.00 measured), and whether the light inner line exists, since it cannot be seen on
  white.
- A BarrierKit app that sets its bar: the corner per bar follows the protocol
  and was not exercised with a real `bk` app. All four `WINDOW_CORNER` bars are
  16 in barrierkit's data, so `set_bar` gives the default 16.8 for every bar and
  changes nothing visible today.
- Cost of the lines element: it covers the whole window, so any damage inside
  it pays a blended shader pass. Only CPU was measured (about 52.4 to 54.6 ms
  per second, nested, 1280x800, debug build); GPU time on the laptop's panel was
  not.
- The window shadow (step 15) was checked nested only, in a headless sway at
  scales 1, 1.25 and 2, light and dark: its right side, bottom and bottom-right
  corner are within 1/255 of its mirror. Not checked: the top and left sides
  on screen (windows open at 0,0), the corner region of the shadow and the
  spread against a Mac capture, a run directly on a display, and GPU time. Its CPU
  cost under a translucent terminal scrolling below the main window was
  about 4 % (108.9 to 113.6 ms per second, debug build); idle did not change.
