# Modelo de procesos y reparto compositor ↔ shell

ADR 0023: cada componente del shell es un proceso y un cliente Wayland propio. El diagrama está en [`malaysia-y-shell.svg`](malaysia-y-shell.svg); el mapa de la UI, en [`interfaz.md`](interfaz.md).

## Distribución

Un proceso por componente: `malaysia-wallpaper`, `malaysia-shell menubar`, `dock`, `lock` y `launcher`. Ninguno depende de otro; no hay daemon central. `xdg-desktop-portal-malaysia` es un backend D-Bus aparte que puede caer sin tumbar la sesión.
## Reparto de trabajo

| Aspecto | Quién decide |
|---|---|
| Protocolos y orden de composición | compositor |
| Geometría (posición, tamaño, anchors, zona exclusiva) | compositor |
| Efectos de contorno (sombra, borde, radio, blur futuro) | compositor |
| Contenido de las superficies de shell | shell |
| Fondo | `malaysia-wallpaper` |
| Titlebar SSD | compositor |
| Capturas de pantalla | compositor (sin proceso de shell) |

La shell **pide** colocación; el compositor la valida y decide. Cada uno manda en su capa, como el Dock y WindowServer en macOS.

## Comunicación

Cada componente lee `~/.config/malaysia/` y `~/.local/share/malaysia/`; el compositor publica la app activa (`window.domain`), sirve layer-shell, foreign-toplevel, session-lock y activation, y envuelve las superficies de shell con los efectos del ADR 0020.