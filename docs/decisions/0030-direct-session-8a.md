# 0030 — Direct Session 8a: one primary plane and a single output

Status: accepted (Block 8a).

## Context

8a must start from a TTY on the internal panel without multi-GPU, overlay/cursor planes or hotplug; 8b and 8c add them later.

## Decision

- Atomic DRM, no legacy path.
- One device: the seat's primary, opened by `libseat`; GBM + EGL + `GlesRenderer`. `--drm-device` separates render and scanout for development only.
- One internal connector (eDP) in its preferred mode; if there is no eDP, the first connected one; if there is none, it fails.
- One plane: `DrmCompositor` gets empty overlay and cursor planes; the pointer is composited as an element.
- One frame per vblank with damage tracking; without damage, a timer is rescheduled.
- libinput is the only input, with defaults.
- VT switching with DRM and libinput pause/resume; `Ctrl+Alt+Backspace` is the emergency exit. A locked session stays locked after returning.
- The EGL/GBM initialization is the only `unsafe`, in `udev.rs` (exception to ADR 0003).

`--drm-device` and `--no-input` are for testing only and require `--drm`; the test uses `LIBSEAT_BACKEND=noop` under sudo.

## Consequences

8a does not support hotplug, external monitors, per-output scale, direct scanout or XWayland.

## Rejected alternatives

- Copying multi-GPU/planes from Otto/anvil: outside the scope of a minimal session.
