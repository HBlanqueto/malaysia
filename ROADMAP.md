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
shadow, and Ctrl+Left and Ctrl+Right slide between Desktops. Next is
cascade placement. `linux-dmabuf` waited for a real GPU, which the development
laptop now has. Nested, the compositor has run on that laptop's GPU; directly
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
| 16 | Desktops: one `Space` per Desktop | Ctrl+Right on the last Desktop with windows opens an empty one, Ctrl+Left and Ctrl+Right move between them, and an empty last Desktop goes away when left | done `5837b11` |
| 17 | The slide between Desktops: an animation clock and macOS's spring from `DESKTOP_SWITCH_*` | the Desktops slide past with a fast start and a slow end, a gap of 64 between them, and a second key press mid-slide carries on from where the strip is | done |

Steps 1 to 8b were the first stage: the compositor runs directly on hardware
and nested in another session.

## After this stage

In the order they will be taken, one at a time. On 2026-11-26 the user asked to
take the macOS window model first, so these two come before the rest:

1. Cascade placement: a new window goes 29 right and 29 down from the previous
   one, as measured; where the first window goes is still to be measured.
2. The tiling shortcuts: Fill, Center, the halves and Return to Previous Size,
   with a spring of ω0 45 and ζ 1.38, as measured; the margin of tiled windows
   is still to be measured.

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
6. Window management (placement, move, resize), the rest of the shell and the
   apps.

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
- Every window opens at the top left corner, over the others.
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
- A window on a hidden Desktop gets no frame callback and no `suspended`
  state.
- Ctrl+Left and Ctrl+Right no longer reach apps, as on macOS.
- Desktops are created by the key, because there is no Mission Control. An
  empty Desktop in the middle stays and an empty last one is dropped when it
  is left; macOS keeps both.
- A pointer grab (a held button, an open menu, a drag) ends at the switch,
  and every popup of the Desktop left is closed.
- Every new window opens on the current Desktop and takes the focus, also a
  dialog or a window of an app on a hidden one.
- Windows cannot be moved between Desktops.
- A second key press mid-slide is not measured on macOS. Ours carries the
  speed of the strip and can pass the target by at most 6 % of a Desktop.
- The spring is read when the frame is rendered, not when it is presented.
- A window wider than the display shows over the neighbouring Desktop during
  the slide.
- During the slide the pointer hits the target Desktop at its rest position.
- The gap between Desktops shows the clear colour.
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

## Not verified

- Desktops (step 16): nested, and directly on the virtual display of the
  machine check, which switches to an empty Desktop and back. Not checked: a
  drag at the switch, a Qt app.
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
- Menus were checked with one GTK 3 app. A Qt app and a popup that asks to be
  moved after it is shown are untested.
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
