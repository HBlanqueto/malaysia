# El dock (`yuki-shell dock`)

El tercer componente nativo del shell (ADR 0023): el dock estilo macOS en el
borde inferior. Lista las apps abiertas desde
`ext-foreign-toplevel-list-v1` y las activa al hacer clic por
`zwlr_foreign_toplevel_management_v1`.

## Cómo funciona

```
ext-foreign-toplevel-list-v1 ──► lista de ventanas (tiles)
zwlr_foreign_toplevel_management_v1 ──► activar / cerrar / minimizar
dock.domain ──► tilesize, orientation
window.domain ──► app activa (resaltada)
        │
        ▼
  frame RGBA: barra + tiles (celestial-kit::text)
        │
        ▼
  superficie layer-shell Top, borde inferior, exclusive zone
        │  eglSwapBuffers (wl_egl_window → dmabuf)
        ▼
       Yuki (compositor)
```

1. **La superficie**: capa **`Top`**, anclada al borde inferior, alto =
   `tilesize + 2×padding` (64 px por defecto), `exclusive_zone` igual — las
   ventanas no se meten debajo. En `Top` para recibir input (el compositor da
   foco de puntero a las capas `Top`/`Overlay` con interactividad).
2. **La lista de ventanas**: `ext-foreign-toplevel-list-v1` (el compositor lo
   sirve con el módulo de smithay). Cada ventana mapeada → un tile; cada tile
   muestra el título de la app.
3. **Activar al clic**: `zwlr_foreign_toplevel_management_v1` (el compositor lo
   sirve manualmente — smithay 0.7 no lo trae). Un clic en un tile envía
   `activate`, y el compositor restaura (si estaba minimizada), eleva y enfoca
   la ventana. `close` y `set_rectangle` (el rectángulo del ícono, para la
   animación de minimizar) se manejan; `set_minimized` oculta la ventana de la
   escena (el ícono del dock queda como su único manejador).
4. **App activa resaltada**: desde `window.domain` (el estado que publica el
   compositor), como la menubar.

## El lado del compositor (`crates/yuki/src/foreign.rs`)

- `ext-foreign-toplevel-list-v1` — estado de smithay + handler + handles por
  ventana (creados al mapear, `closed` al destruir). El protocolo no lleva
  "activada", así que la app activa va por el almacén.
- `zwlr_foreign_toplevel_management_v1` — global servido a mano: en el bind se
  anuncian las ventanas existentes (evento `toplevel` con un handle); cada
  request del handle actúa sobre su ventana (`activate`, `close`,
  `set_rectangle`, `set_minimized`). `set_maximized`/`set_fullscreen` se
  reconocen pero no se implementan aún.

## Divergencias

- **`zwlr_foreign_toplevel_management_v1` a mano**: smithay 0.7 no lo incluye,
  se sirve manualmente (ADR 0023 lo contempla en la lista de protocolos).
- **`set_rectangle` se registra, no se anima**: la animación de minimizar
  (genie/scale) es trabajo futuro; el rectángulo del ícono queda guardado en
  `State::dock_rectangle`.
- **Íconos**: la primera versión muestra el título de la app en cada tile; los
  íconos SVG (`resvg`, ADR 0024) se integrarán en el toolkit cuando toque.
- El `activate` de este protocolo solo lleva el seat (sin serial de pulsación
  en esta versión del XML), así que no se valida contra el grab implícito.

## Aceptación (verificada)

- El compositor anuncia `ext_foreign_toplevel_list_v1` y
  `zwlr_foreign_toplevel_manager_v1` (`wayland-info`).
- El dock se mapea en `Top` en el borde inferior (1280×64) con zona exclusiva;
  el workspace de las ventanas se ajusta (arriba 28 de la menubar, abajo 64 del
  dock).
- Al abrir `foot`: se mapea, aparece como tile en el dock y como app activa en
  la menubar (`window.domain` = `foot`).
- `kill -9` al dock: el compositor y los demás componentes siguen; el
  supervisor lo relanza.
- Todo verde: clippy `-D warnings`, fmt, 115 tests.
- **Bug pre-existente conocido**: el texto (cosmic-text) no se rasteriza en
  este entorno; el dock dibuja los tiles sin el título. Seguimiento en
  `bloques/07`.

## Relación

- ADR 0023 (procesos y protocolos), ADR 0024 (texto).
- `menubar.md` / `wallpaper.md` — componentes hermanos (presentación EGL,
  ADR 0027).
- `configuracion/opciones.md` — `dock.*` (catálogo; `tilesize` en uso).