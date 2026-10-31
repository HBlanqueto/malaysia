# 0025 — La shell es nativa (`malaysia-shell`)

Estado: **aceptado.** Reemplaza y elimina el ADR 0022 ("La shell es Quickshell").

## Contexto

Quickshell (QtQuick/QML) no aportaba al trabajo real: cada componente es pequeño
y el toolkit propio ya cubre geometría, texto y presentación. Mantenerlo
añadía una dependencia externa grande y su sistema de configuración.

## Decisión

1. La shell es `malaysia-shell`: un binario, un proceso por componente
   (`wallpaper`, `menubar`, `dock`, `lock`, `launcher`), cada uno cliente
   Wayland propio (ADR 0023), dibujando con BarrierKit (ADR 0024).
2. El compositor sirve los protocolos que necesitan los componentes
   (layer-shell, foreign-toplevel list/management, session-lock, viewporter,
   fractional-scale) y expone `dock.*`, `menubar.*`, `appearance.*` por el
   almacén `.domain` y los estados en `~/.local/share/malaysia/`.
3. El diseño es de MalaysiaDreams: componentes con los tokens de `vow-design` y
   la política del ADR 0020.
4. Sin dependencias de shell externas; no se copia código de otras shells.

## Consecuencias

- Se elimina Quickshell del código, flake, configuración y documentación.
- `shell.engine` y `shell.config` salen del catálogo de opciones.
- La supervisión usa las unidades systemd del flake; matar un componente no
  afecta al compositor ni a los demás.

## Alternativas descartadas

- Seguir con Quickshell: dependencia grande y configuración duplicada.
- Shell dentro del compositor: rompe el aislamiento de fallas del ADR 0023.
