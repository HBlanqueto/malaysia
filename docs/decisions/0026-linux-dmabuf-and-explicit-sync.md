# 0026 — linux-dmabuf and explicit sync

Status: accepted (Block 7a and 7b).

## Context

Mesa needs `zwp_linux_dmabuf_v1` to initialize EGL/Vulkan on Wayland; without it, GPU clients fail or fall back to software.

## Decision

1. `zwp_linux_dmabuf_v1` v5 with default feedback. The `main_device` is the render node of the winit renderer and the formats are the renderer's. No fallback to v3: without a render node, startup fails.
2. A buffer that cannot be imported is rejected at commit; for `create_immed`, `InvalidWlBuffer` is posted.
3. `wl_drm` is not exposed; dmabuf v4 with feedback is enough.
4. `wp_linux_drm_syncobj_manager_v1` only if the device supports `syncobj_eventfd`.
5. A single blocker path: explicit acquire point or implicit fence; resumption happens from calloop.
6. Release is deferred until the GPU finishes: buffers are retained and released when the EGL fence (native fd) signals. Startup requires `EGL_ANDROID_native_fence_sync`.
7. The `backend_drm` feature is enabled even without a DRM backend: it exposes the render node, syncobj and the release point.

## Consequences

wezterm, Firefox, vkcube and mpv run accelerated; SHM clients keep working.

## Rejected alternatives

- `wl_drm`: unnecessary legacy.
