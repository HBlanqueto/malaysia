# El dock (`malaysia-shell dock`)

> Mapa general: [`interfaz.md`](interfaz.md).

Componente nativo (ADR 0023): dock inferior que lista las apps abiertas y las activa.

## Cómo funciona

```
ext-foreign-toplevel-list-v1 ──► lista de ventanas (tiles)
zwlr_foreign_toplevel_management_v1 ──► activar / cerrar / minimizar
dock.domain ──► tilesize, orientation
window.domain ──► app activa resaltada
        ▼
frame RGBA (barrier-kit) → layer-shell Top → EGL/dmabuf
```

1. Superficie en capa `Top`, anclada abajo, alto `tilesize + 2×padding` (64 px por defecto) y `exclusive_zone` igual; las ventanas no se meten debajo.
2. `ext-foreign-toplevel-list-v1` da un tile por ventana mapeada. Cada tile muestra el título sobre la `ShellPalette` de Tengen (`tile`, activo `tile_active`), elegida por `appearance.color_scheme`.
3. Un clic envía `activate` por `zwlr_foreign_toplevel_management_v1`; el compositor restaura si estaba minimizada, eleva y enfoca. `close`, `set_rectangle` y `set_minimized` se manejan; `set_maximized` y `set_fullscreen` se reconocen pero no se implementan.
4. La app activa se resalta desde `window.domain`.

## Lado del compositor

`ext-foreign-toplevel-list-v1` usa el estado de Smithay; `zwlr_foreign_toplevel_management_v1` se sirve a mano (Smithay 0.7 no lo trae), anuncia las ventanas existentes al bindear y cada request actúa sobre su ventana.
## Divergencias

- `zwlr_foreign_toplevel_management_v1` a mano (ADR 0023 lo contempla).
- `set_rectangle` se guarda en `State::dock_rectangle`; la animación de minimizar (genie/scale) sigue pendiente. Los tiles muestran el título; los iconos SVG del kit aún no se usan.
- `activate` no lleva serial de pulsación en esta versión del XML, así que no se valida contra el grab implícito.
