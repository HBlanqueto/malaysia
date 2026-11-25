# Roadmap

Malaysia is being rebuilt from its foundation. Each step adds one dependency
or one Smithay feature and one result that can be seen. A step is done when it
passes `cargo fmt`, `cargo clippy -D warnings` and its run check, and is pushed
to `the-culling-branch`.

Last update: 2026-11-25.

## Where we are

**Now:** this stage is done and reviewed. The compositor runs directly on a
display and nested in a Wayland session. Next is the first item of «After this
stage». Nothing has run on real hardware yet.

The display after the last finished step (step 8b: on a NixOS machine, the
terminal's own text cursor drawn where the mouse is):

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
| review | The whole code read against Smithay's source | fixes for focus, hidden errors and a retried frame | done |

When step 8b is done the compositor runs directly on hardware and nested in
another session. That is the goal of this stage.

## After this stage

In the order they will be taken, one at a time:

1. Popups: today a menu is never shown, and every GTK or Qt app has menus.
2. `linux-dmabuf`: until then every client draws with software rendering, so a
   real machine looks slower than it is.
3. Output scale, for HiDPI displays.
4. Hotplug and several displays.
5. What a laptop needs: display power and idle, touchpad settings such as
   tap-to-click, signal handling.
6. Design tokens from `ctdesign`, window management (placement, move, resize),
   layer shell, the shell and the apps.

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

- Popups (menus) are not configured or drawn yet.
- Every window opens at the top left corner, over the others.
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
