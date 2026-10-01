# 0026 — linux-dmabuf y explicit sync

Estado: **aceptado** (Bloque 7a y 7b).

## Contexto

Yuki solo exponía `wl_shm`. Mesa necesita `zwp_linux_dmabuf_v1` para
inicializar EGL/Vulkan en Wayland: sin él, cualquier cliente con GPU (wezterm,
Firefox, mpv, vkcube) falla o cae a software. Además, la shell renderizaba con
GLES offscreen y copiaba cada frame a `wl_shm` — la copia es el cuello de
botella en la iGPU Vega de memoria unificada, y el bloque 7c la elimina.

Este ADR fija cómo el compositor sirve buffers dmabuf y sincroniza su acceso,
y por qué no usa `wl_drm`.

## Decisión (Aceptada)

1. **`zwp_linux_dmabuf_v1` (versión 5) con default feedback.** El `main_device`
   del feedback es el **render node del dispositivo que usa el renderer de
   winit**, derivado vía `EGLDevice::device_for_display` +
   `try_get_render_node` (anvil 0.7.0 hace lo mismo), y los formatos son los
   que reporta el propio renderer (`dmabuf_formats()`), sin inventar ni
   filtrar ninguno. **Un solo camino, sin fallback a v3**: si no se puede
   derivar un render node o construir el feedback, el arranque falla con un
   error claro.
2. **Un buffer que no se puede importar se rechaza al commit, no al
   renderizar.** La importación de prueba ocurre en el drenado de la cola
   (ADR 0012: el renderer no llega a los handlers); para `create_immed` un
   fallo posta el error de protocolo `InvalidWlBuffer` y mata al cliente,
   nunca una ventana negra.
3. **No se expone `wl_drm`.** Mesa actual descubre el dispositivo por dmabuf v4
   con feedback; `wl_drm` (más su API de importación de buffers EGL) no hace
   falta y arrastra un protocolo legacy. Se documenta por si un cliente
   objetivo lo exigiera en el futuro.
4. **Explicit sync (`wp_linux_drm_syncobj_manager_v1`) solo si el dispositivo
   lo soporta.** El render node se abre como `DrmDeviceFd` y se comprueba
   `supports_syncobj_eventfd`; sin soporte el global no se anuncia y los
   clientes usan el sync implícito.
5. **Un solo camino de blocker en el commit.** Un commit que adjunta un dmabuf
   se retiene hasta que el buffer es legible: con el **acquire point** explícito
   si el cliente puso uno, con la **fence implícita** del dmabuf si no
   (`pending_blocker`). La reanudación ocurre desde el loop de calloop
   (source one-shot + `blocker_cleared`), de modo que el compositor nunca
   bloquea esperando a un cliente y nunca muestra un frame a medio renderizar.
   El **release point** se señala cuando el compositor deja de usar el buffer
   (una sola vez), a través del manejo de buffers de Smithay que el compositor
   ya usa para shm.

## Por qué la feature `backend_drm` de Smithay

`EGLDevice::try_get_render_node` está `#[cfg(feature = "backend_drm")]` en
Smithay 0.7.0 (es la única forma de obtener el render node desde el EGL
device), y el módulo `wayland::drm_syncobj` (explicit sync) y el señalado del
release point en el manejo de buffers también viven detrás de esa feature. Por
eso `Cargo.toml` la activa, **incluso sin backend DRM**: `backend_drm` añade
los crates de espacio de usuario `drm`/`drm-ffi` (que abren el render node vía
libc/rustix), no el driver ni un backend de scanout. El camino winit del
compositor no cambia; el Bloque 8 (Direct Session) usará DRM/KMS de verdad.

## Cómo se elige el dispositivo

El dispositivo lo elige el renderer de winit (el que creó el contexto EGL); el
compositor le pregunta cuál es vía `EGLDevice::device_for_display(display)` y
`try_get_render_node`, y ese render node es a la vez el `main_device` del
feedback de dmabuf y el fd de importación de los timelines de syncobj. No hay
heurística propia: es exactamente el dispositivo que dibuja.

## Consecuencias

- Los clientes GPU (Mesa EGL/Vulkan) inicializan hardware dentro de Yuki:
  verificado con `eglinfo -B` (radeonsi), wezterm (OpenGL, buffers `Dma`),
  Firefox, `vkcube` (RADV) y `mpv` (gpu-next).
- El seguimiento de daño, las decoraciones y los efectos no cambian: el render
  sigue dibujando el buffer a la geometría declarada de la superficie, nunca al
  tamaño del buffer.
- Matar a un cliente con commits bloqueados no deja blockers, fds ni syncobjs
  huérfanos (el callback del blocker tolera la muerte del cliente).
- El bloque 7c puede presentar la shell por EGL/dmabuf sin copias por CPU
  (ADR 0027).

## Relación

- ADR 0012 (el backend vive junto al `State`, no dentro: la importación dmabuf
  ocurre en la sesión; el `loop_handle` del blocker es una extensión pequeña
  documentada en `state.rs`).
- ADR 0024 / 0025 (la shell es nativa; 7c presenta por EGL — ADR 0027).
- `docs/bloques/07-linux-dmabuf-y-explicit-sync.md` — el registro por bloque.