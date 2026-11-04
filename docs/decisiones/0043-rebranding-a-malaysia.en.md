# 0043 — Rebranding to Malaysia

Status: accepted (2026-10-26).

## Context

The project started as **Yuki**.

## Decision

1. New names:

| Before | After |
|---|---|
| `crates/yuki` / `yuki-scene` | `malaysia-compositor` / `malaysia-scene` |
| `crates/yuki-design` / `yuki-appearance` | `vow-design` / `vow-appearance` |
| `crates/celestial-kit` | `barrier-kit` |
| `crates/yuki-shell` / `yuki-settings` | `malaysia-shell` / `malaysia-settings` |
| `xdg-desktop-portal-yuki` | `xdg-desktop-portal-malaysia` |
| `org.yuki.Compositor1` | `org.malaysia.Compositor1` |
| `YUKI_CONFIG_DIR`, `YUKI_DATA_DIR` | `MALAYSIA_CONFIG_DIR`, `MALAYSIA_DATA_DIR` |
| `~/.config/yuki`, `~/.local/share/yuki` | `~/.config/malaysia`, `~/.local/share/malaysia` |
| `yuki-session.target`, `/etc/pam.d/yuki-lock` | `malaysia-session.target`, `malaysia-lock` |
| `programs.yuki.*` | `programs.malaysia.*` |
| `XDG_CURRENT_DESKTOP=Yuki` | `Malaysia` |
| `yuki.portal` / `UseIn=Yuki` | `malaysia.portal` / `UseIn=Malaysia` |
| `CelestialKit` | `BarrierKit` |

2. **No automatic migration.** Old paths and `YUKI_*` are not read as a fallback:

   ```sh
   mv ~/.config/yuki ~/.config/malaysia
   mv ~/.local/share/yuki ~/.local/share/malaysia
   ```

3. **Historical documentation stays intact.** Earlier ADRs, block reports and the research keep the old names. Only this ADR, the README, AGENTS.md, `docs/README.md`, `docs/roadmap.md`, the glossary and the overview are updated.

## Consequences

All new names are consistent with the repo.

## Rejected alternatives

- Keeping Yuki in packages and paths: it contradicts the repo and the rebranding.
