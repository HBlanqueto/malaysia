# Hardware y línea base gráfica

Esto registra la máquina donde tuvo lugar el primer contacto con la GPU de MalaysiaDreams y
el stack gráfico tal y como se reportó a sí mismo. Es un punto de referencia, no un
requisito: nada de lo que hay aquí se promete que siga siendo cierto.

## Por qué existe

La sesión anidada ejecuta MalaysiaDreams anidado sobre el backend winit, que deja que el
stack EGL/GLES2 de la sesión anfitriona maneje la ventana. El hito Direct Session
pasa al backend udev/DRM, donde MalaysiaDreams es dueño del dispositivo directamente y la
plataforma EGL cambia de `PLATFORM_WAYLAND_KHR` a una superficie GBM/DRM. Cuando algo
se comporte distinto allí, este archivo es el punto conocido-bueno con el que
comparar: la misma GPU, el mismo Mesa, el mismo kernel, y la cadena del renderer
exacta que la sesión anidada mostró correctamente.

## Máquina

| | |
| --- | --- |
| CPU | AMD Ryzen 5 PRO 5650U con Radeon Graphics |
| GPU | AMD Radeon integrada (Vega, "renoir"), Ryzen 5 PRO 5650U |
| Kernel | 7.2.4-cachyos-lto |

## Stack gráfico observado por la ejecución en vivo de la Parte 1

| | |
| --- | --- |
| Mesa | 26.2.3 |
| GL renderer | `AMD Radeon Graphics (radeonsi, renoir, ACO, DRM 3.64, 7.2.4-cachyos-lto)` |
| GL version | OpenGL ES 3.2 Mesa 26.2.3 |
| EGL version | 1.5 |
| EGL platform | `PLATFORM_WAYLAND_KHR` |
| Window system | Cliente Wayland de la sesión anfitriona (backend winit) |

La cadena del renderer está copiada textualmente del log de arranque de la Parte 1;
es lo que reportó `GlesRenderer`.

## Qué cambia en el hito Direct Session y qué comparar

- **Plataforma EGL.** El winit anidado usa `PLATFORM_WAYLAND_KHR` porque la ventana
  es una superficie Wayland del anfitrión. El DRM/KMS directo usa una superficie GBM
  y la ruta `EGL_MESA_platform_gbm`. Un fallo ahí no es evidencia de una diferencia
  de hardware; es la primera vez que se ejercita esa ruta.
- **Renderer.** Se espera el mismo `GlesRenderer`. Si el renderer GL o la versión GL
  difieren bajo DRM, vale la pena registrarlo antes de depurar cualquier otra cosa.
- **Scanout y formatos.** El backend anidado compone a través del anfitrión; el hito
  Direct Session elige formatos y modificadores por sí mismo. La versión de Mesa de
  arriba es lo primero que hay que mantener constante al comparar.
- **Input y seat.** La sesión anidada recibe input a través de winit; el hito Direct
  Session lee libinput desde un seat. Fuente distinta, mismo destino.