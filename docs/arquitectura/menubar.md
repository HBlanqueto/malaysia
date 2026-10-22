# La menubar (`yuki-shell menubar`)

El segundo componente nativo del shell (ADR 0023/0024): la barra superior,
como la menu bar de macOS. Primera versión: **nombre de la app activa** a la
izquierda y **reloj** a la derecha. Sin menús globales por app todavía.

## Cómo funciona

```
window.domain ──► título de la app activa (el compositor lo publica)
   (~/.local/share/yuki)                │
                                        ▼
  frame RGBA: fondo + texto (celestial-kit::text, cosmic-text)
                                        │
                                        ▼
  superficie layer-shell Top, 1280×28, exclusive zone 28
                                        │  eglSwapBuffers (wl_egl_window → dmabuf)
                                        ▼
                        Yuki (compositor) lo compone sobre las ventanas
```

1. **La superficie**: capa **`Top`**, anclada a arriba/izquierda/derecha,
   **28 px de alto** con `exclusive_zone = 28` — las ventanas no se meten
   debajo de la barra.
2. **El render** es CPU: `celestial-kit::text` (cosmic-text + swash) compone el
   texto en un frame RGBA sobre el fondo claro de Neptunia
   (`SHELL_BAR`, `#f2f4f7e6`) y la textura se presenta
   por **EGL** sobre un `wl_egl_window` (Bloque 7c, ADR 0027) — el swap es un
   dmabuf que el compositor texturiza directo. La subida a la textura es
   **parcial**: solo la franja del reloj (90 px) o del título (250 px) que
   cambió, con `glTexSubImage2D`.
3. **El reloj**: solo muestra minutos (`%H:%M`), así que commitea **al borde de
   minuto** (timer alineado), no cada segundo.
4. **La app activa**: el compositor publica la ventana enfocada en el estado
   `window.domain` (`[active] title`/`app_id`) — el mismo patrón de almacén que
   el wallpaper — y la menubar lo vigila.

## El estado de la app activa

`ext-foreign-toplevel-list-v1` no lleva el estado "activada" (no existe en el
protocolo), así que la menubar no puede saber qué ventana es la activa por ahí.
El compositor conoce la ventana enfocada, y la publica en el almacén de estado
(`crates/yuki/src/active.rs`): al dar foco (`focus_window`) escribe
`~/.local/share/yuki/window.domain` con el título y el app_id de la ventana; al
quitarlo, vacío. Es el mecanismo de primera versión; el canal de notificación
adecuado (D-Bus, ADR 0021) lo reemplazará cuando llegue. `ext-foreign-toplevel-list`
sigue siendo la vía para la **lista** de ventanas del dock (paso 4).

## La zona exclusiva en el compositor

Para que la barra reserve espacio, el compositor calcula el **workspace** de las
ventanas: el output menos las zonas exclusivas de las capas `Top`/`Overlay`
ancladas al borde (`Layers::exclusive_top`/`exclusive_bottom`). Las ventanas
nuevas se colocan y dimensionan dentro de ese workspace (debajo de la barra):
`State::workspace()` → origen `(0, top)` y tamaño `(w, h - top - bottom)`. Esto
es el caso de falla documentado en el ADR 0023 (muerte del dock → se recalcula
el área útil).

## Divergencias

- **App activa por el almacén, no por protocolo**: `ext-foreign-toplevel-list-v1`
  no expresa "activada"; se usa el estado `window.domain`. Documentado arriba.
- **Render CPU** (cosmic-text a un frame RGBA) en vez de GLES para el texto:
  los glifos se rasterizan con swash de todos modos; el contenido es casi
  estático, así que la subida a la textura es parcial y barata. Las animaciones
  (blur de la barra, ADR 0020/0024, futuras) deben renderizarse en GPU, nunca
  CPU + subida por frame (ADR 0027).
- `menubar.autohide` se lee del catálogo pero aún no se implementa (el
  comportamiento de autohide es trabajo futuro).

## Aceptación (verificada)

- La menubar se mapea en la capa `Top` a 1280×28 con zona exclusiva.
- El compositor coloca las ventanas nuevas debajo de la barra (workspace).
- Al abrir `foot`, `window.domain` pasa a `[active] title = "foot"` y la menubar
  lo muestra; el reloj avanza al minuto (1 commit/min, `Dma`).
- `kill -9` al proceso: el compositor y los demás componentes siguen; el
  supervisor lo relanza.
- El texto se rasteriza (el bug de `layout_runs` vacío se corrigió en 7.1);
  la menubar muestra la app activa y el reloj sobre la paleta clara de Neptunia.
- Todo verde: clippy `-D warnings`, fmt y tests.

## Relación

- [`interfaz.md`](interfaz.md) — el mapa completo de la UI.
- ADR 0023 (procesos), ADR 0024 (texto con cosmic-text), ADR 0032 (Neptunia).
- `wallpaper.md` — componente hermano (presentación EGL, ADR 0027).
- `modelo-de-procesos.md` — la distribución de procesos.
- `configuracion/opciones.md` — `menubar.autohide` (catálogo).