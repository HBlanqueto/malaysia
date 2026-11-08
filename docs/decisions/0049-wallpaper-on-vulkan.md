# 0049 — Wallpaper on Vulkan

Status: accepted (2026-10-30).

## Context

The wallpaper lived in `malaysia-shell wallpaper`, drawn with BarrierKit/Skia GL.

## Decision

1. The wallpaper moves to `malaysia-wallpaper`, a separate binary and its own Wayland client that draws with `wgpu` on **Vulkan only**. It uses SCTK + calloop, without BarrierKit or Freya.
2. Bounded exception to "the shell draws with the kit": the wallpaper has no widgets, text or accessibility. It does not extend to other components.
3. `wgpu` does not enter the compositor or `malaysia-shell`; Freya/Skia do not enter `malaysia-wallpaper`. `scripts/check-client-only-deps.sh` checks it.
4. The Nix wrapper points the Vulkan loader to the flake's Mesa drivers (all of `share/vulkan/icd.d`) with `MALAYSIA_ORIG_*`, like the EGL wrapper (ADR 0007). `VK_DRIVER_FILES` does not reach the compositor's clients.
5. The contract does not change: `wallpaper-selection.state` stores the selection, `wallpaper.state` publishes the dominant color and the layer-shell namespace is still `malaysia-wallpaper`.

## Consequences

There is one more process to supervise, with `malaysia-wallpaper.service`.

## Rejected alternatives

- Keeping the wallpaper in the kit: it pollutes menubar/dock/lock with dependencies they do not need.
