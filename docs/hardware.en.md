# Hardware and graphics baseline

Reference point of the machine where Malaysia was tested, not a
requirement. It serves for comparison when something behaves differently under DRM/KMS.

## Machine

| | |
|---|---|
| CPU/GPU | AMD Ryzen 5 PRO 5650U with Radeon Vega («renoir») |
| Kernel | 7.2.4-cachyos-lto |
| Mesa | 26.2.3 |
| GL renderer | `AMD Radeon Graphics (radeonsi, renoir, ACO, DRM 3.64, 7.2.4-cachyos-lto)` |
| GL version | OpenGL ES 3.2 Mesa 26.2.3 |
| EGL | 1.5, `PLATFORM_WAYLAND_KHR` when nested |



Under DRM/KMS, compare the EGL platform (GBM), the `GlesRenderer`, the formats/modifiers and libinput; keep Mesa constant.
