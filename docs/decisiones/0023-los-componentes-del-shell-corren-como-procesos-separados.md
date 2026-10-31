# 0023 — Los componentes del shell corren como procesos separados

Estado: **aceptado.** La supervisión queda como propuesta; la ubicación del
overview sigue abierta.

## Contexto

Dock, menubar, notificaciones, launcher y lock deben sobrevivir los unos a los
otros. Un daemon central de shell reintroduciría un punto único de falla.

## Decisión

1. Cada componente es su propio proceso y cliente Wayland. Ningún componente
   depende de otro; cada uno tolera la ausencia de los demás.
2. El código compartido vive en un crate de toolkit que lee los tokens de
   diseño. Las apps propias usan el mismo toolkit.
3. Empaquetado: un binario con subcomandos (`malaysia-shell dock`, `menubar`,
   ...), un artefacto y muchos procesos. `panic = "abort"` en release.
4. Supervisión por unidades de usuario de systemd incluidas en el flake:
   `Restart=on-failure`, `RestartSec` corto, límite de reintentos, logs por
   componente y `malaysia-session.target` tras importar `WAYLAND_DISPLAY`. En
   `--drm` el compositor arranca y detiene el target. El compositor **no**
   supervisa procesos él mismo.
5. El compositor implementa los protocolos del modelo: layer-shell,
   `ext-session-lock-v1`, foreign-toplevel list/management, xdg-activation,
   idle-notify. El estado del sistema va por D-Bus directo.

## Casos de falla

- El compositor sigue siendo el único punto de falla inevitable.
- Si muere el lock, la sesión **sigue bloqueada** (ext-session-lock-v1); el
  compositor muestra un color sólido y no desbloquea.
- Si muere el dock, se reacomodan las ventanas; se acepta el parpadeo.
- Un reinicio de GPU afecta a todos los procesos con contexto GL.

## Consecuencias

- Aislamiento de fallas por componente; la única caída irrecuperable es la del
  compositor.
- La shell no es UI del compositor ni un daemon central.
- Complementa al ADR 0025: cada componente de `malaysia-shell` es uno de estos
  procesos.

## Alternativas descartadas

- Shell dentro del compositor: una caída del shell tumba la sesión.
- Un solo proceso de shell: un bug del dock tumba la barra y el launcher.
- Que el compositor supervise los procesos: agranda el proceso que más debe ser
  pequeño.
