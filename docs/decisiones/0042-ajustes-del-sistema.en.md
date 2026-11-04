# 0042 — System settings (`yuki-settings`)

Status: accepted (2026-10-26).

## Context

The desktop needs a preferences app.

## Decision

1. Own app on CelestialKit: binary `yuki-settings`, crate `yuki-settings`, drawn with `celestial-kit` on Freya (ADR 0034).
2. Fixed `app_id`: prefix `io.github.hblanqueto.yuki.` plus `"settings"` → `io.github.hblanqueto.yuki.settings`.
3. CSD window: `Decorations::Client` and its own titlebar with traffic lights; size and limits come from `yuki-design::SETTINGS`.
4. It opens from the ❄ menu with `spawn_app("yuki-settings", &[])`, which preserves `WAYLAND_DISPLAY` and removes the private Mesa environment.
5. Persistence in `appearance.domain` through `vow_appearance`; it only edits appearance fields.
6. Nix installs the binary, wraps it with the Mesa wrapper and copies the `.desktop` file and its `hicolor/scalable` icon.

## Consequences

Today only Appearance exists; more domains arrive with the catalog and the configuration D-Bus (ADR 0021).

## Rejected alternatives

- A generic GTK app: it shares neither tokens nor chrome.
