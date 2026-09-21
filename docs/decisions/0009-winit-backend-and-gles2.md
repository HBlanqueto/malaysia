# 0009 — The nested backend is winit with the GLES2 renderer

Status: accepted (nested session, Part 1)

## Context

The nested session has to show a real client in a window while Neptune is being
built. That needs an event source for the window, an input source, and a
renderer. Smithay offers several backends and three renderers, and the choice
shapes what can be tested on the development machine (a single desktop running a
Wayland session).

The requirements point in one direction: the compositor must run *inside* the
existing session, not take over the machine. Taking over the DRM device would
make every test a logout, and would need a seat and elevated privileges.

## Options considered

1. **udev/DRM backend.** The shipping path: real hardware, real scanout,
   libinput input. But it takes over a seat,    cannot run as a window, and cannot
   be exercised from an SSH or nested session. Wrong tool for the nested
   session.
2. **X11 backend.** Smithay's `backend_x11` runs as an X11 client. The host
   session is Wayland but does have Xwayland; still, it is an extra indirection
   and the winit path is the one Smithay documents for development.
3. **winit backend.** Runs as a normal Wayland or X11 client of the host
   session, opens one window, and provides both the input source and an EGL
   context. Loses hardware scanout and multi-output realism, which the nested
   session explicitly does not need. Smithay warns that winit "does not integrate into
   calloop too well", but it does expose `WinitEventLoop` as an `EventSource`.
4. **Renderer: pixman.** Pure software, no GPU, no shaders. Simple, but slower
   and off the path the project wants for real scanout later.
5. **Renderer: GLES2 via `GlesRenderer`.** The renderer the winit backend
   returns by default and the one smallvil and anvil both use. Requires no
   shader code from us; Smithay's shaders are internal.

## Decision

Use the winit backend with Smithay's GLES2 (`GlesRenderer`) renderer. The
dependency enables only `backend_winit`, `renderer_gl` and `wayland_frontend`,
with `default-features = false`, so the DRM, libinput, udev, x11 and vulkan
paths are not compiled. One window, one output.

## Consequences

- Development and testing happen nested, alongside the host session; the
  compositor can be started and stopped like any other client (ADR 0007's
  `dev.sh` wrapper still applies).
- The udev/DRM and libinput paths are untested by this milestone and are
  recorded as open questions in `docs/blocks/01-session.md`.
- The renderer is GLES2 behind Smithay's abstraction; no shader is written by
  Neptune in the nested session, matching the scope. A later milestone that
  needs custom drawing may revisit this.
- Because `desktop` is not enabled, Smithay's `Space` window-management helper
  is not available. That is a deliberate choice for the nested session, not an
  accident of the feature list: `Space` is replaced by `neptune-scene` in the
  Direct Session milestone, so enabling it now would add a dependency that gets
  deleted almost immediately. `Space` earns its keep with many windows across
  several outputs; the nested session has one window on one output, and ADR
  0012's state split already assumes the toplevel is tracked directly. The
  toplevel is therefore tracked by hand, behind a module boundary, so the Direct
  Session milestone rewrites one module rather than every call site. The
  `desktop` feature stays off through the nested session.
- The default-feature trimming is a build-surface decision that can be widened
  one feature at a time when a milestone needs another backend.
