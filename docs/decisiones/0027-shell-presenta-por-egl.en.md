# 0027 — The shell presents through EGL

Status: accepted (Block 7c).

## Context

The shell drew offscreen and presented through `wl_shm`: a CPU-to-CPU copy and a re-upload to a texture.

## Decision

1. It presents through EGL on `wl_egl_window`; Mesa allocates the dmabuf and the compositor textures it without copies. It is the only path.
2. The toolkit receives opaque pointers (`wl_egl_window*`) and stays Wayland-agnostic; the offscreen readback is removed.
3. Menubar, dock and lock compose their RGBA on the CPU and upload it to a reused texture, only in the damaged rectangle.
4. The lock keeps one presenter per surface.
5. `wl_shm` is removed from the shell; the compositor still serves it to clients.
6. Animations must render on the GPU; a full frame is not uploaded per frame.

## Consequences

A single path; the menubar/dock upload is partial.

## Rejected alternatives

- Keeping `wl_shm`: CPU copy and re-upload to the GPU.
