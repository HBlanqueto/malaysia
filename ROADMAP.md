# Roadmap

Malaysia is being rebuilt from its foundation. Each step adds one dependency
or one Smithay feature and one result that can be seen. A step is done when it
passes `cargo fmt`, `cargo clippy -D warnings` and its run check, and is pushed
to `the-culling-branch`.

Last update: 2026-11-25.

## Where we are

**Now:** the compositor runs directly on a display and nested in a Wayland
session, apps show their menus, and a HiDPI display gets a whole-number
scale. Next is fractional scale; `linux-dmabuf` waits for a real GPU. Nothing
has run on real hardware yet.

The display after the last finished step (step 10: on a NixOS machine whose
display reports a small 1600x900 panel, directly on the display, a terminal
drawn at scale 2):

![The display after the last finished step](screen.png)

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
| 10 | Output scale: 1 to 4, chosen from the display's physical size and mode with mutter's rule; nested, the host's scale | windows, menus and the cursor twice as large on a HiDPI panel | done |

Steps 1 to 8b were the first stage: the compositor runs directly on hardware
and nested in another session.

## After this stage

In the order they will be taken, one at a time:

1. `linux-dmabuf`: until then every client draws with software rendering, so a
   real machine looks slower than it is. It is written and kept out of the
   branch: it only turns on with a hardware render node, and the test machines
   have none, so it waits for a real GPU to be checked on. The steps below go
   ahead meanwhile.
2. Fractional scale (`wp_fractional_scale_v1` and `wp_viewporter`): the
   displays whose right scale is 1.25, 1.5 or 1.75 stay at 1 until then.
3. Layer shell, which the shell needs to place its surfaces.
4. `bk` returns, with its first two components: the lock screen and the
   screen capture portal.
5. Hotplug and several displays.
6. What a laptop needs: display power and idle, touchpad settings such as
   tap-to-click, signal handling.
7. Design tokens from `ctdesign`, window management (placement, move, resize),
   the rest of the shell and the apps.

## How the desktop is put together

The decisions and the new dependencies are in the «Design» section of
[README.md](README.md). None of it is built yet.

## How each step is checked

Everything is built and run in a Linux box on a Mac (`vm.sh`): Nix, a virtual
KMS display (`vkms`) for the direct backend and a headless desktop for the
nested one. `./vm.sh shot file.png` captures the virtual display.

With a device argument the compositor runs directly on that device. With no
argument it runs nested when `WAYLAND_DISPLAY` is set.

The box has no seat, no udev and no real GPU. Steps 7 and 8 are checked in a
NixOS virtual machine with a full Linux session (`./vm.sh test dir` runs it
and writes its screenshots to `dir`; it is the flake's `checks.<system>.vm`), and what only real hardware can
prove is marked here as **not verified on hardware** until someone runs it.

## Known gaps

- A click outside an open menu closes the menu and also reaches the window
  under it, without giving that window the focus. KWin keeps that click.
- When an app closes its own menu, the keyboard focus returns to the window on
  the next key or pointer event, not at once.
- Every window opens at the top left corner, over the others.
- The scale is a whole number. A 14 inch 1440p panel, a 16 inch 2560x1600
  panel and a 27 inch 4K monitor stay at 1; a 14 inch 2880x1800 panel gets 2.
- The scale cannot be set by hand: a display that reports a wrong size gets a
  wrong scale, and one that reports none (a TV, a projector) gets 1.
- At scale 2 a client that draws with `wl_shm` fills four times the pixels on
  the CPU, until `linux-dmabuf`.
- A client is not told which display its cursor is on, so some toolkits draw
  the cursor at scale 1 and it looks soft at scale 2. foot and GTK are right.
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
- There is no signal handling: when the process is killed, logind or seatd
  give the VT back.

## Not verified

- Real hardware: so far the compositor has only run on virtual displays (vkms
  and QEMU's virtio GPU) with software rendering, and with QEMU's keyboard and
  mouse. A real GPU driver, a touchpad and several GPUs are untested.
- The hardware cursor plane: on the virtual displays the cursor was always
  drawn into the frame.
- x86_64: the package and the machine check evaluate but have only been built
  and run on aarch64.
- A frame lost while the session is paused, and starting from a VT that is not
  the active one.
- Scroll, drag and drop and the clipboard.
- Scale on a real panel: the rule was checked with mutter's own table of
  displays and on a virtual display made to report a small panel. A client's
  own cursor at scale 2 on the direct backend was not checked.
- Menus were checked with one GTK 3 app. A Qt app and a popup that asks to be
  moved after it is shown are untested.
