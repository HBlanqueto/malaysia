# Modelo de procesos y reparto compositor ↔ shell

ADR 0023: cada componente del shell es un proceso y un cliente Wayland propio. El diagrama está en [`malaysia-y-shell.svg`](malaysia-y-shell.svg); el mapa de la UI, en [`interfaz.md`](interfaz.md).

## Distribución

```
COMPOSITOR (un proceso)
  escena · capas · geometría · input · efectos · .domain · garantía de lock
      ▲            ▲             ▲              ▲
 layer-shell  layer-shell   layer-shell   ext-session-lock
      │            │             │              │
  wallpaper     menubar         dock           lock      (+ launcher)
```

Un proceso por componente: **`malaysia-wallpaper`**, **`malaysia-shell menubar`**, **`dock`**, **`lock`** y **`launcher`**. Ninguno depende de otro: si el dock muere, la barra sigue. No hay daemon central de shell.

Fuera de la shell, `xdg-desktop-portal-malaysia` (ADR 0031) es un backend D-Bus aparte; lo arranca `xdg-desktop-portal` por activación y puede caer sin tumbar la sesión.

## Reparto de trabajo

| Aspecto | Quién decide |
|---|---|
| Protocolos (`wl_surface`, `xdg_shell`, layer-shell...) | compositor |
| Orden de composición (Background → Ventanas → Top → Overlay) | compositor |
| Geometría (posición, tamaño, anchors, zona exclusiva) | compositor |
| Efectos de contorno (sombra, borde, radio, blur futuro) | compositor |
| Contenido de las superficies de shell | shell |
| Fondo | `malaysia-wallpaper` |
| Titlebar SSD | compositor |
| Capturas de pantalla | compositor (sin proceso de shell) |

La shell **pide** colocación; el compositor la valida y decide. Cada uno manda en su capa, como el Dock y WindowServer en macOS.

## Comunicación

- **`.domain`:** el compositor es dueño de la configuración; cada componente lee `~/.config/malaysia/` y `~/.local/share/malaysia/`. No escribe su propia configuración.
- **Protocolos:** layer-shell y sus zonas exclusivas; `window.domain` publica la app activa; `ext-foreign-toplevel-list-v1` da la lista; `zwlr` activa/cierra/minimiza; `ext-session-lock-v1` bloquea; `xdg-activation` enfoca.
- **Efectos:** el compositor envuelve las superficies de shell con los mismos efectos que las ventanas (ADR 0020); la shell no los dibuja.

También están hechos `wp_presentation`, `xdg-activation-v1`, `ext-idle-notify-v1` + `zwp_idle_inhibit_manager_v1`, `wp_cursor_shape_v1`, pointer constraints, relative-pointer, explicit sync, dmabuf y la captura. Quedan `keyboard-shortcuts-inhibit` y el portal `Screenshot`/`ScreenCast`.

## Fallos

| Fallo | Qué pasa | Recuperación |
|---|---|---|
| Componente del shell | solo ese componente | systemd lo relanza |
| Lock | la sesión sigue bloqueada | color sólido y lock nuevo |
| Dock | se reacomodan ventanas | se reacomodan al volver |
| Portal | apps pierden Settings | D-Bus lo relanza |
| Compositor | mueren todos los clientes | único punto de falla inevitable |

## Relación

ADR 0023 (procesos), ADR 0025 (shell nativa), ADR 0031 (portal), [`interfaz.md`](interfaz.md) y los documentos de cada componente.
