# 0025 — La shell es nativa (`yuki-shell`)

Estado: aceptado. Reemplaza y elimina el ADR 0022 ("La shell es Quickshell").

## Contexto

Quickshell (QtQuick/QML) no aportaba al trabajo real: cada componente es pequeño y el toolkit propio ya cubre geometría, texto y presentación. Mantenerlo añadía una dependencia grande y su configuración.

## Decisión

1. La shell es `yuki-shell`: un binario, un proceso por componente (`wallpaper`, `menubar`, `dock`, `lock`, `launcher`), cada uno cliente Wayland propio (ADR 0023), dibujando con CelestialKit (ADR 0024).
2. El compositor sirve layer-shell, foreign-toplevel, session-lock, viewporter y fractional-scale, y expone `dock.*`, `menubar.*`, `appearance.*` por `.domain` y los estados en `~/.local/share/yuki/`.
3. El diseño es de Yuki: tokens de `yuki-design` y política del ADR 0020.
4. Sin dependencias de shell externas; no se copia código de otras shells.

## Consecuencias

Se elimina Quickshell del código, flake, configuración y documentación. `shell.engine` y `shell.config` salen del catálogo. La supervisión usa las unidades systemd del flake.

## Alternativas descartadas

- Seguir con Quickshell: dependencia grande y configuración duplicada.
- Shell dentro del compositor: rompe el aislamiento del ADR 0023.
