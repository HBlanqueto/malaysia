# La interfaz de MalaysiaDreams

Mapa de todo lo visual y de interacción. El detalle vive en cada documento.

## Principios

1. **Tengen es el diseño** (ADR 0032, hoy en barrierkit). Colores, radios, espacios, tipografía y sombras salen de `vow-design`; nadie inventa un valor en el código.
2. **Chrome separado, efectos para todos** (ADR 0020). GTK/Libadwaita conservan su headerbar; el resto recibe el chrome del compositor. Sombra, bordes y radios son del compositor y se aplican a todas las ventanas.
3. **Shell nativa y separada** (ADR 0023/0025): un proceso por componente.
4. **Skia solo en clientes** (ADR 0034): la shell y las apps propias usan BarrierKit (Freya + Skia); el compositor sigue con GLES, `grade-graphics` y `cosmic-text`, y nunca enlaza Skia ni `freya-*`.
5. **macOS como referencia de UX**, no de código.

## Reparto de dibujo

| Quién | Qué dibuja |
|---|---|
| `malaysia-shell` (menubar/dock/lock/launcher) | UI de la shell con BarrierKit |
| `malaysia-wallpaper` | Fondo con wgpu/Vulkan (ADR 0049) |
| Apps propias con `Decorations::Client` | Su UI y su titlebar de 44 px |
| Apps propias con `Decorations::Server` | Su UI; el compositor pone la titlebar SSD de 32 px |
| Apps ajenas SSD | Su contenido; el compositor añade titlebar |
| Compositor | Composición, esquinas, sombras, bordes y animaciones con GLES |

La shell nunca usa la titlebar del compositor. Las apps propias eligen CSD de 44 px (apps con toolbar/sidebar, como Ajustes) o SSD de 32 px (utilidades simples).

## Capas

```
Overlay   launcher, capturas, popups de la shell
Top       menubar (arriba) · dock (abajo) · notificaciones (futuro)
Windows   cliente + titlebar SSD + popups xdg
Bottom    layer-shell Bottom (futuro)
Background wallpaper
```

Las zonas exclusivas de menubar y dock recortan el workspace. El lock sustituye todo: solo se dibujan sus superficies y el cursor.

## Sistema de diseño

- Temas `light` (default), `dark` y `dark-tint`; `appearance.color_scheme` elige.
- Radio de ventana 16; semáforos de 12 con radio 4; titlebar SSD de 32 px. Las apps propias CSD usan 44 px.
- Tipografía Geist; título 13/600 y cuerpo 13/400.
- Sombras de Tengen (`SHADOW`/`SHADOW_DARK`) dibujadas por shader detrás de cada ventana; todas las superficies se recortan al radio 16.
- `appearance.window_color`: `neutral`, `accent` (6 %/8 %), `wallpaper` (7 %/9 %); `traffic_lights` puede forzar `standard`, `accent` o `wallpaper`. `window_tint_scope = "desktop"` tiñe menubar y dock.
- El blur de fondo está en el catálogo, pero **aún no se dibuja**.

## Shell

| Componente | Proceso | Capa | Estado |
|---|---|---|---|
| Wallpaper | `malaysia-wallpaper` | Background | Vulkan/wgpu + dmabuf, recarga en vivo |
| Menubar | `malaysia-shell menubar` | Top, 28 px | app activa, reloj y menús |
| Dock | `malaysia-shell dock` | Top, abajo | lista de ventanas y activar |
| Lock | `malaysia-shell lock` | lock surfaces | PAM, auto-lock, recuperación |
| Launcher | `malaysia-shell launcher` | Overlay | búsqueda, ranking, iconos, activación |
| Notificaciones | futuro | Top/Overlay | Bloque 12b |

Leen preferencias de `~/.config/malaysia/` y estado de `~/.local/share/malaysia/`; no escriben configuración. Ver [`modelo-de-procesos.md`](modelo-de-procesos.md), [`wallpaper.md`](wallpaper.md), [`menubar.md`](menubar.md), [`dock.md`](dock.md), [`lock.md`](lock.md) y [`capturas.md`](capturas.md).

## Ventanas

- **Apps propias (ADR 0035):** `app_id` con prefijo `io.github.hblanqueto.malaysia.`; su modo de decoración se respeta antes del primer commit y `force_server`/`force_client` no las toca.
- **SSD:** titlebar de 32 px, semáforos, título centrado; cerrar, minimizar, maximizar, arrastrar y redimensionar.
- **CSD:** GTK/Libadwaita, wezterm y apps que lo piden conservan su chrome; `window.decorations = "force_server"` puede forzar la barra (con riesgo de duplicarla).
- **Marco:** esquinas de 16, sombra, borde exterior y hairline para todas las ventanas; fullscreen lo elimina, maximizado lo conserva. Diálogos centrados sobre el padre.
- **Foco:** al enfocar se eleva; el render va de arriba abajo. Detalle en [`ventanas.md`](ventanas.md).

## Capturas

`Shift+Super+3` (pantalla), `Shift+Super+4` (región; `Espacio` ventana, `Esc` cancela) y `PrintScreen` (ventana enfocada). `screenshot.domain` decide puntero, destino, formato, fecha, miniatura, temporizador y sombra; hoy solo están activos `show_pointer` y `target` (`desktop`/`clipboard`). Detalle en [`capturas.md`](capturas.md).

## Launcher

`Super+Space` abre `malaysia-shell launcher` en Overlay exclusiva. Busca apps `.desktop` con ranking por prefijo, palabra, subcadena y palabras clave; `↑`/`↓` navegan, `Enter` o clic lanzan con `xdg-activation`, `Esc` cierra.

## Cursor, input y popups

- `wp_cursor_shape_v1` (35 formas), tema Xcursor, HiDPI y shake-to-locate (3×).
- Move/resize con banda exterior de 10 px y esquina de 20 px; popups xdg con grabs, cierre en cascada y `Esc`; pointer constraints y relative-pointer.
- La banda de la titlebar es chrome del compositor, no superficie del cliente.
- El launcher recibe foco de teclado al mapearse y lo devuelve al cerrarse.

## Configuración y portal

Catálogo canónico: [`../configuracion/opciones.md`](../configuracion/opciones.md). Dominios de interfaz: `appearance.domain`, `window.domain`, `dock.domain`, `menubar.domain`, `screenshot.domain` y `effects.domain`.

`xdg-desktop-portal-malaysia` publica `org.freedesktop.appearance` (`color-scheme`, `accent-color`, `contrast`, `reduced-motion`) desde `appearance.domain`. El tema se reconstruye en vivo al cambiar `appearance.domain` o `wallpaper.state`.

## Pendiente de UI

Modo foco real, captura de ventana con marco, iconos del dock, notificaciones y menús GTK (`14f-2`). El blur de fondo sigue sin implementar.
