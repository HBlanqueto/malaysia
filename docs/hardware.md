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

## Por qué existe

La sesión anidada usa winit, que deja el contexto EGL/GLES a la sesión
anfitriona. Direct Session (DRM/KMS) es dueño del dispositivo y cambia a una
superficie GBM/DRM. Este archivo es el punto conocido-bueno para distinguir un
fallo de hardware de un fallo de la ruta nueva.

## Qué comparar en DRM/KMS

- **Plataforma EGL:** GBM (`EGL_MESA_platform_gbm`) en vez de Wayland.
- **Renderer:** se espera el mismo `GlesRenderer`; registra versión GL y Mesa
  si difieren.
- **Scanout/formatos:** el backend elige formatos y modificadores; mantén Mesa
  como constante.
- **Input y seat:** libinput directo en vez de winit.

Las medidas de memoria del README/roadmap se tomaron con esta GPU real
(radeonsi), no con llvmpipe.
