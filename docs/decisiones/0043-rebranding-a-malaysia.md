# 0043 — Rebranding a Malaysia

Estado: **aceptado** (2026-10-26).

## Contexto

El proyecto nació como **Yuki**. El repo ya se llama
`github.com/HBlanqueto/malaysia`, así que renombramos todo el escritorio a
**MalaysiaDreams** (proyecto **Malaysia**) para que los nombres de paquetes,
binarios, librerías, rutas, variables de entorno y documentación coincidan con
la nueva identidad.

## Decisión

1. **Nombres de paquete, librería y binario.** La tabla siguiente resume el
   mapeo:

   | Antes | Después (paquete / lib) |
   |---|---|
   | `crates/yuki` | `crates/malaysia-compositor` (`malaysia_compositor`) |
   | `crates/yuki-scene` | `crates/malaysia-scene` (`malaysia_scene`) |
   | `crates/yuki-design` | `crates/vow-design` (`vow_design`) |
   | `crates/yuki-appearance` | `crates/vow-appearance` (`vow_appearance`) |
   | `crates/celestial-kit` | `crates/barrier-kit` (`barrier_kit`) |
   | `crates/yuki-shell` | `crates/malaysia-shell` (`malaysia_shell`) |
   | `crates/yuki-settings` | `crates/malaysia-settings` (`malaysia_settings`) |
   | `crates/xdg-desktop-portal-yuki` | `crates/xdg-desktop-portal-malaysia` |
   | binario `yuki` | `malaysia-compositor` (`--winit` / `--drm` igual) |
   | binario `yuki-shell` | `malaysia-shell` |
   | `yuki::spawn` | `malaysia_compositor::spawn` |
   | D-Bus `org.yuki.Compositor1` | `org.malaysia.Compositor1` |
   | `YUKI_CONFIG_DIR`, `YUKI_DATA_DIR` | `MALAYSIA_CONFIG_DIR`, `MALAYSIA_DATA_DIR` |
   | `~/.config/yuki`, `~/.local/share/yuki` | `~/.config/malaysia`, `~/.local/share/malaysia` |
   | `yuki-session.target`, `yuki-*` | `malaysia-session.target`, `malaysia-*` |
   | `/etc/pam.d/yuki-lock` | `/etc/pam.d/malaysia-lock` |
   | módulo NixOS `programs.yuki.*` | `programs.malaysia.*` |
   | sesión display manager | `Name=MalaysiaDreams`, `DesktopNames=Malaysia` |
   | `XDG_CURRENT_DESKTOP` | `Malaysia` |
   | `yuki.portal` | `malaysia.portal`, `UseIn=Malaysia` |
   | prefijo `yuki_` en Infinity | `malaysia_` |
   | `CelestialKit` | `BarrierKit` |

2. **No hay migración automática.** No se leen las rutas viejas ni las
   variables `YUKI_*` como respaldo. La migración es manual:

   ```sh
   mv ~/.config/yuki ~/.config/malaysia
   mv ~/.local/share/yuki ~/.local/share/malaysia
   ```

3. **Documentación histórica intacta.** Los ADRs anteriores, los informes de
   bloques y la investigación conservan los nombres antiguos porque son
   registro histórico. Solo este ADR, el README principal, AGENTS.md,
   `docs/README.md`, `docs/roadmap.md`, el glosario y `docs/malaysia.md`
   (anteriormente `docs/yuki.md`) se actualizan.

## Consecuencias

- Todos los nombres nuevos son coherentes con el repo `malaysia`.
- Los usuarios que tengan configuración en `~/.config/yuki` deben moverla a
  mano.
- Los ADRs antiguos quedan como referencia del nombre en el momento en que se
  tomaron las decisiones.

## Referencias

- `README.md` — instrucciones de instalación y uso con los nombres nuevos.
- `AGENTS.md` — guía para agentes de IA con la política de diseño actualizada.
- `docs/malaysia.md` — visión general del proyecto renombrado.
- `docs/roadmap.md` — roadmap con este bloque de rebranding.
- `flake.nix` y `nix/malaysia.nix` — empaquetado Nix y módulo NixOS.
