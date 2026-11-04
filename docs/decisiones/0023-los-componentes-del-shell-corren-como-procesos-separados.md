# 0023 — Los componentes del shell corren como procesos separados

Estado: aceptado. La supervisión queda como propuesta; la ubicación del overview sigue abierta.

## Contexto

Dock, menubar, notificaciones, launcher y lock deben sobrevivir los unos a los otros.

## Decisión

1. Cada componente es su propio proceso y cliente Wayland. Ninguno depende de otro; tolera la ausencia de los demás.
2. El código compartido vive en un crate de toolkit que lee los tokens de diseño; las apps propias usan el mismo toolkit.
3. Un binario con subcomandos (`yuki-shell dock`, `menubar`, ...), muchos procesos. `panic = "abort"` en release.
4. Supervisión por unidades de usuario de systemd (`Restart=on-failure`, `RestartSec` corto, límite de reintentos, logs y `yuki-session.target` tras importar `WAYLAND_DISPLAY`). En `--drm` el compositor arranca y detiene el target; no supervisa procesos. Las unidades arrancan el binario de `MALAYSIA_BIN_DIR`, que el compositor importa junto a `WAYLAND_DISPLAY`: la shell es siempre la del compositor en marcha, la única en la que confía Infinity.
5. El compositor sirve layer-shell, `ext-session-lock-v1`, foreign-toplevel list/management, xdg-activation e idle-notify. El estado del sistema va por D-Bus directo.

## Consecuencias

Aislamiento por componente; la única caída irrecuperable es la del compositor.

## Alternativas descartadas

- Shell dentro del compositor: una caída del shell tumba la sesión.
