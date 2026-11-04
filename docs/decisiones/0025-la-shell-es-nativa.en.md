# 0025 — The shell is native (`yuki-shell`)

Status: accepted. Replaces and removes ADR 0022 ("The shell is Quickshell").

## Context

Quickshell (QtQuick/QML) added nothing to the real work: each component is small and our own toolkit already covers geometry, text and presentation.

## Decision

1. The shell is `yuki-shell`: one binary, one process per component (`wallpaper`, `menubar`, `dock`, `lock`, `launcher`), each its own Wayland client (ADR 0023), drawing with CelestialKit (ADR 0024).
2. The compositor serves layer-shell, foreign-toplevel, session-lock, viewporter and fractional-scale, and exposes `dock.*`, `menubar.*`, `appearance.*` through `.domain` and the state in `~/.local/share/yuki/`.
3. The design is Yuki's: tokens from `yuki-design` and the ADR 0020 policy.
4. No external shell dependencies; no code is copied from other shells.

## Consequences

Quickshell is removed from the code, flake, configuration and documentation.

## Rejected alternatives

- Staying with Quickshell: a large dependency and duplicated configuration.
