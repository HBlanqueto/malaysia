# 0042 — System settings (`malaysia-settings`)

Status: accepted (2026-10-26).

## Context

The desktop needs a preferences app.

## Decision

1. Own app on BarrierKit: binary `malaysia-settings`, crate `malaysia-settings`, drawn with `bk` on Freya (ADR 0034).
2. Fixed `app_id`: prefix `io.github.hblanqueto.malaysia.` plus `"settings"` → `io.github.hblanqueto.malaysia.settings`.
3. CSD window: `Decorations::Client` and its own titlebar with traffic lights; size and limits come from `ctdesign::SETTINGS`.
4. It opens from the ❄ menu with `spawn_app("malaysia-settings", &[])`, which preserves `WAYLAND_DISPLAY` and removes the private Mesa environment.
5. Persistence in `appearance.domain` through `ctappearance`; it only edits appearance fields.
6. Nix installs the binary, wraps it with the Mesa wrapper and copies the `.desktop` file and its `hicolor/scalable` icon.

## Consequences

Panes: Wi-Fi, Bluetooth, Network, VPN and Energy (ADR 0069), Accessibility, Appearance, Wallpaper (ADR 0072), Notifications and Focus (ADR 0068). More domains arrive with the catalog and the configuration D-Bus (ADR 0021).

## Rejected alternatives

- A generic GTK app: it shares neither tokens nor chrome.
