# 0023 — Los componentes del shell corren como procesos separados

Estado: aceptado. La supervisión queda como propuesta; la ubicación del overview sigue abierta.

## Contexto

Dock, menubar, notificaciones, launcher y lock deben sobrevivir los unos a los otros. Un daemon central reintroduciría un punto único de falla.

## Decisión

1. Cada componente es su propio proceso y cliente Wayland. Ninguno depende de otro; tolera la ausencia de los demás.
2. El código compartido vive en un crate de toolkit que lee los tokens de diseño; las apps propias usan el mismo toolkit.
3. Un binario con subcomandos (`yuki-shell dock`, `menubar`, ...), muchos procesos. `panic = "abort"` en release.
4. Supervisión por unidades de usuario de systemd: `Restart=on-failure`, `RestartSec` corto, límite de reintentos, logs y `yuki-session.target` tras importar `WAYLAND_DISPLAY`. En `--drm` el compositor arranca y detiene el target. El compositor **no** supervisa procesos.
5. El compositor sirve layer-shell, `ext-session-lock-v1`, foreign-toplevel list/management, xdg-activation e idle-notify. El estado del sistema va por D-Bus directo.

## Casos de falla

El compositor sigue siendo el único punto inevitable. Si muere el lock, la sesión sigue bloqueada (`ext-session-lock-v1`, color sólido). Si muere el dock, se reacomodan ventanas y se acepta el parpadeo. Un reinicio de GPU afecta a todos los procesos GL.

## Consecuencias

Aislamiento por componente; la única caída irrecuperable es la del compositor. Complementa al ADR 0025.

## Alternativas descartadas

- Shell dentro del compositor: una caída del shell tumba la sesión.
- Un solo proceso de shell: un bug del dock tumba la barra y el launcher.
- Que el compositor supervise: agranda el proceso que más debe ser pequeño.
