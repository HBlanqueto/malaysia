# 0010 — Smithay is pinned to the crates.io 0.7.0 release

Status: accepted (nested session, Part 1)

## Context

Smithay is the one large dependency the project is built on. It is published to
crates.io, and its `master` branch (which niri, smallvil in-tree and Otto
follow) moves ahead of the releases. The nested session uses a small, stable
slice of the API: a winit event source, an EGL/GLES2 renderer, and the Wayland
protocol handlers for compositor, shm, seat and xdg-shell. The project builds
inside a single Nix dev shell and values reproducible builds (ADR 0005,
ADR 0006).

## Options considered

1. **crates.io `0.7.0`.** A tagged, released API. Cargo's normal version
   resolution and `Cargo.lock` pin it. No extra flake input, no git fetching in
   the build, and the documentation on docs.rs matches the code.
2. **Git `master` pinned by revision.** Newest fixes, and the same source the
   reference compositors use. But it adds a git dependency to `flake.nix` and
   `Cargo.toml`, tracks a moving API, and makes a version bump a revision bump.
3. **Git `master` unpinned.** Rejected outright: the build would change under
   the project without a commit.

## Decision

Pin Smithay to the crates.io `0.7.0` release. Bump it deliberately when a later
milestone needs something newer, the same way the toolchain is bumped
(ADR 0006).

## Consequences

- Builds are reproducible from `flake.lock` and `Cargo.lock` alone.
- A fix that exists only on `master` is not available until a release or an
  explicit switch to option 2; if the winit integration proves broken, switching
  to a pinned git revision is the recorded fallback.
- The published `0.7.0` winit backend is what the code is written against, so
  the docs on docs.rs are authoritative for this project.
