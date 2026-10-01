# Modelo de procesos del shell

Descrito por el ADR 0023: cada componente del shell corre como su propio proceso y
su propio cliente Wayland. Este documento describe la distribución de procesos
resultante y cómo se relacionan.

## La distribución

```
COMPOSITOR (un proceso)
  ┌────────────────────────────────────────────┐
  │ YUKI — display server                      │
  │ scene · capas · geometría · input ·        │
  │ efectos · almacén .domain · lock garantía  │
  └────────────────────────────────────────────┘
      ▲              ▲              ▲             ▲
      │layer-shell   │layer-shell   │layer-shell  │ext-session-lock-v1
      │              │              │             │
  ┌───┴────┐    ┌────┴────┐    ┌───┴────┐   ┌─────┴─────┐
  │ yuki-  │    │ yuki-   │    │ yuki-  │   │ yuki-     │
  │ shell  │    │ shell   │    │ shell  │   │ shell     │
  │ dock   │    │ topbar  │    │ notify │   │ lock      │  (+ launcher, …)
  └────────┘    └─────────┘    └────────┘   └───────────┘
  SHELL (un proceso por componente, ninguno depende de otro)
```

Un proceso por componente — dock, barra superior, notificaciones, launcher,
pantalla de bloqueo, ... — todos clientes Wayland de Yuki. Ninguno depende de otro:
si el dock muere, la barra sigue funcionando.

**Estado**: cuatro componentes nativos funcionan — **`yuki-shell wallpaper`**
(render GLES de `celestial-kit`, offscreen, `wl_shm`), **`yuki-shell menubar`**
(capa `Top`, app activa + reloj), **`yuki-shell dock`** (capa `Top` abajo,
lista de apps + activar al clic) y **`yuki-shell lock`** (`ext-session-lock-v1`,
con la garantía de que la sesión no se desbloquea si el cliente muere).
La shell es nativa (ADR 0025); no hay dependencias de shell externas.

## Qué significa en la práctica

- **Aislamiento de fallas por componente**: la muerte de uno no tumba a los demás.
  Es la motivación del modelo (ADR 0023).
- **Tolerancia a la ausencia**: cada componente trata a los demás como opcionales
  (p. ej. la barra oculta el indicador de notificaciones mientras el servidor de
  notificaciones está caído).
- **El compositor es el único punto de falla inevitable**; mantener el shell fuera
  lo mantiene más chico y, por tanto, más fiable.
- **No hay "daemon del shell"** central: reintroduciría el punto único de falla.

## Cómo se comunican

### Shell → compositor (por protocolos Wayland)

| Componente | Protocolo | Superficie |
|---|---|---|
| dock | `zwlr_layer_shell_v1` | capa Bottom/Top con exclusive zone |
| barra superior | `zwlr_layer_shell_v1` | capa Top con exclusive zone |
| notificaciones | `zwlr_layer_shell_v1` | capa Top/Overlay |
| launcher | `zwlr_layer_shell_v1` | capa Overlay |
| pantalla de bloqueo | `ext-session-lock-v1` | lock surface por output |

Común a dock y barra: `ext-foreign-toplevel-list-v1` (lista de ventanas) y
`zwlr_foreign_toplevel_management_v1` (activar/cerrar/minimizar; su `set_rectangle`
da la posición del ícono del dock para la animación de minimizar). El launcher y el
dock usan `xdg-activation-v1` para dar el foco a las apps que lanzan;
`ext-idle-notify-v1` dispara el bloqueo automático.

### Componentes → sistema (por D-Bus directo)

El estado del sistema — batería, red, audio, brillo — va por D-Bus desde cada
componente (UPower, NetworkManager, PipeWire), no por Wayland. Un protocolo privado
de Yuki solo se introduce cuando aparezca una necesidad concreta que los protocolos
estándar no cubran.

### Estado del arte de la lista de protocolos

Los protocolos de la tabla son de la integración del modelo de procesos, no de
los bloques actuales del roadmap. Lo que el compositor **ya sirve** hoy:

- `zwlr_layer_shell_v1` — hecho (la shell lo usa; ADR 0023).
- `wp_viewporter` y `wp_fractional_scale_manager_v1` — hechos (los necesita el
  render HiDPI de la shell; el wallpaper y los futuros paneles los usan).
- `xdg_popup` con padre layer-surface (`get_popup`) — hecho (Bloque 5).
- `ext-session-lock-v1` — hecho (el lock, ADR 0023; garantía de no desbloquear
  si el cliente muere).
- `ext-foreign-toplevel-list-v1` y `zwlr_foreign_toplevel_management_v1` —
  hechos (el dock).
- `zwp_linux_dmabuf_v1` — hecho (Bloque 7a): v5 con feedback del render node y
  los formatos del renderer GLES, para clientes con GPU (Mesa EGL/Vulkan).

Lo que sigue siendo **trabajo futuro**: `xdg-activation-v1`,
`ext-idle-notify-v1` y, más adelante, el sync explícito `wp_linux_drm_syncobj_v1`
(Bloque 7b).

## Fallos esperados y su tratamiento

| Fallo | Qué pasa | Cómo se recupera |
|---|---|---|
| cae un componente del shell | solo ese componente | la supervisión lo relanza (propuesta ADR 0023) |
| cae la pantalla de bloqueo | la sesión **sigue bloqueada** | el compositor muestra un color sólido y nunca desbloquea (`ext-session-lock-v1`) |
| cae el dock y su zona exclusiva | las ventanas maximizadas se reacomodan | se vuelven a reacomodar al reiniciar el dock; se acepta el parpadeo |
| cae el compositor | mueren todos los clientes | es el único punto de falla inevitable |
| reinicio de GPU (amdgpu) | afecta a todos los procesos con contexto GL | separar procesos no protege contra esto |

## Relación con el resto de la documentación

- ADR 0023 — la decisión (este documento es su consecuencia en forma de distribución).
- ADR 0025 — la shell es nativa: los componentes `yuki-shell` son estos procesos.
- `yuki-y-shell.md` — la división de trabajo compositor ↔ shell (quién decide
  cada cosa).