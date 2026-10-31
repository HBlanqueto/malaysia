# 0042 — Ajustes del sistema (`yuki-settings`)

Estado: aceptado (2026-10-26).

## Contexto

El escritorio necesita una app de preferencias.

## Decisión

1. App propia sobre CelestialKit: binario `yuki-settings`, crate `yuki-settings`, dibujada con `celestial-kit` sobre Freya (ADR 0034).
2. `app_id` fijo: prefijo `io.github.hblanqueto.yuki.` y `"settings"` → `io.github.hblanqueto.yuki.settings`.
3. Ventana CSD: `Decorations::Client` y titlebar propia con semáforos; tamaño y límites de `yuki-design::SETTINGS`.
4. Se abre desde el menú ❄ con `spawn_app("yuki-settings", &[])`, que preserva `WAYLAND_DISPLAY` y elimina el entorno Mesa privado.
5. Persistencia en `appearance.domain` vía `vow_appearance`; solo edita campos de apariencia.
6. Nix instala el binario, lo envuelve con el wrapper Mesa y copia el `.desktop`.

## Consecuencias

Hoy solo existe Apariencia; más dominios llegan con el catálogo y el D-Bus de configuración (ADR 0021).

## Alternativas descartadas

- Una app GTK genérica: no comparte tokens ni chrome.
