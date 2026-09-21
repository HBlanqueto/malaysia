# Hardware and graphics baseline

This records the machine Neptune's first GPU contact happened on, and the
graphics stack as it reported itself. It is a reference point, not a
requirement: nothing here is promised to remain true.

## Why this exists

The nested session runs Neptune nested on the winit backend, which lets the host
session's EGL/GLES2 stack drive the window. The Direct Session milestone moves
to the udev/DRM backend, where Neptune owns the device directly and the EGL
platform changes from `PLATFORM_WAYLAND_KHR` to a GBM/DRM surface. When
something behaves differently there, this file is the known-good point to
compare against: the same GPU, the same Mesa, the same kernel, and the exact
renderer string the nested session displayed successfully.

## Machine

| | |
| --- | --- |
| CPU | AMD Ryzen 5 PRO 5650U with Radeon Graphics |
| GPU | AMD Radeon integrated (Vega, "renoir"), Ryzen 5 PRO 5650U |
| Kernel | 7.2.4-cachyos-lto |

## Graphics stack observed by the Part 1 live run

| | |
| --- | --- |
| Mesa | 26.2.3 |
| GL renderer | `AMD Radeon Graphics (radeonsi, renoir, ACO, DRM 3.64, 7.2.4-cachyos-lto)` |
| GL version | OpenGL ES 3.2 Mesa 26.2.3 |
| EGL version | 1.5 |
| EGL platform | `PLATFORM_WAYLAND_KHR` |
| Window system | Wayland client of the host session (winit backend) |

The renderer string is copied verbatim from the Part 1 startup log; it is what
`GlesRenderer` reported.

## What changes in the Direct Session milestone, and what to compare

- **EGL platform.** Nested winit uses `PLATFORM_WAYLAND_KHR` because the window
  is a Wayland surface of the host. Direct DRM/KMS uses a GBM surface and the
  `EGL_MESA_platform_gbm` path. A failure there is not evidence of a hardware
  difference; it is the first time that path is exercised.
- **Renderer.** The same `GlesRenderer` is expected. If the GL renderer or GL
  version differs under DRM, that is worth recording before debugging anything
  else.
- **Scanout and formats.** The nested backend composites through the host; the
  Direct Session milestone picks formats and modifiers itself. The Mesa version
  above is the first thing to hold constant when comparing.
- **Input and seat.** The nested session receives input through winit; the
  Direct Session milestone reads libinput from a seat. Different source, same
  target.
