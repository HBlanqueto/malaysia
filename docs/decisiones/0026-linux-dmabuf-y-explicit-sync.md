# 0026 — linux-dmabuf y explicit sync

Estado: aceptado (Bloque 7a y 7b).

## Contexto

Mesa necesita `zwp_linux_dmabuf_v1` para inicializar EGL/Vulkan en Wayland; sin él, los clientes GPU fallan o caen a software. La copia a `wl_shm` era el cuello de botella en la iGPU Vega.

## Decisión

1. `zwp_linux_dmabuf_v1` v5 con default feedback. El `main_device` es el render node del renderer de winit y los formatos son los del renderer. Sin fallback a v3: sin render node el arranque falla.
2. Un buffer no importable se rechaza en el commit; para `create_immed` se postea `InvalidWlBuffer`.
3. No se expone `wl_drm`; dmabuf v4 con feedback basta.
4. `wp_linux_drm_syncobj_manager_v1` solo si el dispositivo soporta `syncobj_eventfd`.
5. Un solo camino de blocker: acquire point explícito o fence implícita; la reanudación ocurre desde calloop.
6. El release se aplaza hasta que la GPU termina: se retienen los buffers y se sueltan cuando la fence EGL (fd nativo) señala. El arranque exige `EGL_ANDROID_native_fence_sync`.
7. La feature `backend_drm` se activa aunque no haya backend DRM: expone render node, syncobj y release point.

## Consecuencias

wezterm, Firefox, vkcube y mpv corren acelerados; los clientes SHM siguen. Matar clientes con commits bloqueados no deja blockers/fds huérfanos.

## Alternativas descartadas

- `wl_drm`: legacy innecesario.
- Fallback a v3: duplica caminos y oculta la falta de render node.
- Liberar en el siguiente commit: señala antes de que la GPU lea.
