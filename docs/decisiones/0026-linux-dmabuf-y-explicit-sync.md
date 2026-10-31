# 0026 — linux-dmabuf y explicit sync

Estado: **aceptado** (Bloque 7a y 7b).

## Contexto

Mesa necesita `zwp_linux_dmabuf_v1` para inicializar EGL/Vulkan en Wayland; sin
él, los clientes GPU fallan o caen a software. La copia por CPU a `wl_shm` era
el cuello de botella en la iGPU Vega.

## Decisión

1. `zwp_linux_dmabuf_v1` v5 con default feedback. El `main_device` es el render
   node del renderer de winit (`EGLDevice::device_for_display` +
   `try_get_render_node`) y los formatos son los que reporta el renderer. Un solo
   camino, sin fallback a v3: sin render node el arranque falla.
2. Un buffer no importable se rechaza en el commit (no al renderizar); para
   `create_immed` se postea `InvalidWlBuffer`.
3. No se expone `wl_drm`; dmabuf v4 con feedback basta.
4. `wp_linux_drm_syncobj_manager_v1` solo si el dispositivo soporta
   `syncobj_eventfd`; si no, el global no se anuncia.
5. Un solo camino de blocker en el commit: acquire point explícito o fence
   implícita; la reanudación ocurre desde calloop, nunca bloqueando el loop.
6. El release point se aplaza hasta que la GPU termina de leer: se retienen los
   buffers dibujados y se sueltan cuando la fence EGL del frame (exportada como
   fd nativo) señala. El arranque exige `EGL_ANDROID_native_fence_sync`.
7. La feature `backend_drm` de Smithay se activa aunque no haya backend DRM:
   es la que expone el render node, el módulo syncobj y el release point.

## Consecuencias

- wezterm, Firefox, vkcube (RADV) y mpv (gpu-next) corren acelerados; los
  clientes SHM siguen funcionando.
- Matar clientes con commits bloqueados no deja blockers, fds ni syncobjs
  huérfanos; el release diferido no filtra fds.
- El daño, las decoraciones y los efectos no cambian.

## Alternativas descartadas

- `wl_drm`: protocolo legacy innecesario con dmabuf v4 + feedback.
- Fallback a dmabuf v3: duplica caminos y oculta la falta de render node.
- Liberar el buffer en el siguiente commit (comportamiento de Smithay): señal
  antes de que la GPU termine de leer.
