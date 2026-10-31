# Hardware y línea base gráfica

Punto de referencia de la máquina donde se probó MalaysiaDreams, no un
requisito. Sirve para comparar cuando algo se comporte distinto bajo DRM/KMS.

## Máquina

| | |
|---|---|
| CPU/GPU | AMD Ryzen 5 PRO 5650U con Radeon Vega («renoir») |
| Kernel | 7.2.4-cachyos-lto |
| Mesa | 26.2.3 |
| GL renderer | `AMD Radeon Graphics (radeonsi, renoir, ACO, DRM 3.64, 7.2.4-cachyos-lto)` |
| GL version | OpenGL ES 3.2 Mesa 26.2.3 |
| EGL | 1.5, `PLATFORM_WAYLAND_KHR` en anidado |



En DRM/KMS compara la plataforma EGL (GBM), el `GlesRenderer`, los formatos/modificadores y libinput; mantén Mesa como constante.
