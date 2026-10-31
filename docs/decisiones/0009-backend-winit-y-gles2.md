# 0009 — El backend anidado es winit con el renderizador GLES2

Estado: aceptado (sesión anidada, parte 1).

## Contexto

La sesión anidada debe mostrar un cliente en una ventana sin tomar el seat ni
la pantalla. Smithay ofrece varios backends y renderizadores; la elección
condiciona lo que se puede probar en la máquina de desarrollo.

## Decisión

Backend winit con `GlesRenderer` de Smithay, con `default-features = false` y
solo `backend_winit`, `renderer_gl` y `wayland_frontend`. Una ventana, un
output. Las rutas DRM, libinput, udev, X11 y Vulkan no se compilan. La feature
`desktop` permanece desactivada (ADR 0015).

## Consecuencias

- El compositor corre como cualquier cliente de la sesión anfitriona y se
  arranca/detiene con `dev.sh`.
- Las rutas udev/DRM y libinput no se prueban en este hito.
- Sin `desktop`, no hay `Space`: los toplevels se rastrean a mano en `window.rs`
  detrás de un límite de módulo.
- El recorte de features se amplía una a una cuando un hito necesita otro
  backend.

## Alternativas descartadas

- Backend udev/DRM: toma el seat y la pantalla; no sirve para desarrollo.
- Backend X11: indirección extra sobre la sesión anfitriona.
- Renderizador pixman: software y fuera del camino de GPU.
