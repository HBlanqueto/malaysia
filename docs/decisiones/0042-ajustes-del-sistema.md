# 0042 — Ajustes del sistema (`malaysia-settings`)

Estado: **aceptado** (2026-10-26).

## Contexto

El escritorio necesita una app de preferencias. La primera sección, Apariencia,
cambia esquema de color, acento, modo de color de ventanas y estilo de semáforos;
debe integrarse con el menú del sistema, Nix y Tengen.

## Decisión

1. App propia sobre BarrierKit: binario `malaysia-settings`, crate
   `malaysia-settings`, dibujada con `barrier-kit` sobre Freya (ADR 0034).
2. `app_id` fijo: prefijo `io.github.hblanqueto.malaysia.` y `"settings"` →
   `io.github.hblanqueto.malaysia.settings`.
3. Ventana CSD: `Decorations::Client` y titlebar propia con semáforos; tamaño y
   límites de `vow-design::SETTINGS`.
4. Se abre desde el menú ❄ con `spawn_app("malaysia-settings", &[])`, que
   preserva `WAYLAND_DISPLAY` y elimina el entorno Mesa privado.
5. Persistencia en `appearance.domain` vía `vow_appearance`; solo edita campos
   de apariencia.
6. Nix instala el binario, lo envuelve con el wrapper Mesa y copia
   `share/applications/io.github.hblanqueto.malaysia.settings.desktop`.

## Consecuencias

- Hoy solo existe Apariencia; más dominios llegan con el catálogo y el D-Bus de
  configuración (ADR 0021).
- No es un componente `malaysia-shell`; es un cliente Wayland independiente.
- El lanzador del menú no requiere portal ni activación D-Bus.
- El `.desktop` usa `StartupWMClass` con el `app_id`.

## Alternativas descartadas

- Usar una app GTK genérica: no comparte tokens ni el chrome de la shell.
- Ajustes dentro del compositor: agranda el proceso que debe ser pequeño.
