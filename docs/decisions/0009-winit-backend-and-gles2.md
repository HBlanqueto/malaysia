# 0009 — The nested backend is winit with the GLES2 renderer

Status: accepted (nested session, part 1).

## Context

The nested session must show a client in a window without taking the seat or the screen.

## Decision

winit backend with `GlesRenderer`, with `default-features = false` and only `backend_winit`, `renderer_gl` and `wayland_frontend`. One window, one output. The DRM, libinput, udev, X11 and Vulkan paths are not built. The `desktop` feature stays disabled (ADR 0015).

## Consequences

The compositor runs as a client of the host session and starts with `dev.sh`.

## Rejected alternatives

- udev/DRM backend: it takes the seat and the screen; it does not work for development.
