# 0043 — Rebranding a Malaysia

Estado: aceptado (2026-10-26).

## Contexto

El proyecto nació como **Yuki**.

## Decisión

1. Nombres nuevos:

| Antes | Después |
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

2. **Sin migración automática.** No se leen rutas viejas ni `YUKI_*` como respaldo:

   ```sh
   mv ~/.config/yuki ~/.config/malaysia
   mv ~/.local/share/yuki ~/.local/share/malaysia
   ```

3. **Documentación histórica intacta.** Los ADR anteriores, los informes de bloques y la investigación conservan los nombres antiguos. Solo este ADR, el README, AGENTS.md, `docs/README.md`, `docs/roadmap.md`, el glosario y la visión general se actualizan.

## Consecuencias

Todos los nombres nuevos son coherentes con el repo.

## Alternativas descartadas

- Mantener Yuki en paquetes y rutas: contradice el repo y el rebranding.
