# 0029 — The backend is chosen with an explicit flag

Status: accepted (Block 8a).

## Context

There are two backends: nested winit (development) and direct DRM/KMS (takes the seat and the real screen).

## Decision

They are chosen with `--winit` and `--drm`, without autodetection. The default is `--winit`. `--drm` fails with a clear error if there is no seat, GPU or internal connector; it does not fall back to winit.

## Consequences

The session file and a manual start from a TTY pass `--drm`; `dev.sh` and the tests use the default.

## Rejected alternatives

- Autodetection: a failure on a TTY could take the seat or the screen by surprise.
