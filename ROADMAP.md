# Roadmap

Malaysia is being rebuilt from its foundation. Each step adds one dependency
or one Smithay feature and one result that can be seen. A step is done when it
passes `cargo fmt`, `cargo clippy -D warnings` and its run check, and is pushed
to `the-culling-branch`.

Last update: 2026-11-25.

## Where we are

**Now:** step 8b, the cursor.

The display after the last finished step (step 8a: on a NixOS machine, text
typed on the keyboard reached the terminal before and after Ctrl+Alt+F2 and
back):

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
| 8a | Devices with udev and libinput | real keyboard and mouse, the GPU found without an argument, Ctrl+Alt+Fn and Ctrl+Alt+Backspace | done |
| 8b | Cursor | a pointer that can be seen on a real display | next |

When step 8b is done the compositor runs directly on hardware and nested in
another session. That is the goal of this stage.

## After this stage

In the order they will be taken, one at a time: hotplug and several displays,
design tokens from `ctdesign`, window management (move, resize, focus rules),
layer shell, the shell and the apps.

## How each step is checked

Everything is built and run in a Linux box on a Mac (`vm.sh`): Nix, a virtual
KMS display (`vkms`) for the direct backend and a headless desktop for the
nested one. `./vm.sh shot file.png` captures the virtual display.

With a device argument the compositor runs directly on that device. With no
argument it runs nested when `WAYLAND_DISPLAY` or `DISPLAY` is set.

The box has no seat, no udev and no real GPU. Steps 7 and 8 are checked in a
NixOS virtual machine with a full Linux session (`./vm.sh test dir` runs it
and writes its screenshots to `dir`; it is the flake's `checks.<system>.vm`), and what only real hardware can
prove is marked here as **not verified on hardware** until someone runs it.

## Known gaps

- Popups (menus) are not configured or drawn yet.
- Every window opens at the top left corner, over the others.
- Scroll, drag and drop and the clipboard are not tested.
- Directly on a display the pointer works but nothing draws it until step 8b.
- Under logind, switching VT with Ctrl+Alt+Fn needs polkit; without it the
  switch is refused and logged.
- Displays and GPUs are read once at start: no hotplug.
- The compositor must be started from the active VT; starting it from an
  inactive one is an error.
- There is no signal handling: when the process is killed, logind or seatd
  give the VT back.

## Not verified on hardware

- Everything: so far the compositor has only run on virtual displays (vkms and
  QEMU's virtio GPU) with software rendering, and with QEMU's keyboard and
  mouse. A real GPU driver, a touchpad and several GPUs are untested.
