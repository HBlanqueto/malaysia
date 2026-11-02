# La interfaz de MalaysiaDreams

Mapa de lo visual y de interacción; el detalle vive en cada documento.

## Principios

1. **Tengen es el diseño** (ADR 0032, hoy en barrierkit): colores, radios, espacios y sombras salen de `vow-design`; nadie inventa valores.
2. **Chrome separado, efectos para todos** (ADR 0020): GTK/Libadwaita conservan su headerbar; el resto recibe el chrome del compositor. Sombra, bordes y radios son del compositor y se aplican a todas las ventanas.
3. **Shell nativa y separada** (ADR 0023/0025): un proceso por componente.
4. **Skia solo en clientes** (ADR 0034): la shell usa BarrierKit (Freya + Skia); el compositor sigue con GLES, `grade-graphics` y `cosmic-text`.

## Reparto

La shell dibuja su UI con BarrierKit; `malaysia-wallpaper` el fondo con wgpu/Vulkan; las apps propias su UI (CSD de 44 px o SSD de 32 px); el compositor compone, dibuja chrome, esquinas, sombras y animaciones.
## Capas

```
Overlay   launcher, capturas, popups de la shell
Top       menubar (arriba) · dock (abajo) · notificaciones (futuro)
Windows   cliente + titlebar SSD + popups xdg
Bottom    layer-shell Bottom (futuro)
Background wallpaper
```

Las zonas exclusivas de menubar y dock recortan el workspace. El lock sustituye todo: solo se dibujan sus superficies y el cursor.

## Diseño

Temas `light`/`dark`/`dark-tint`; radio 16, semáforos 12 (radio 4), titlebar SSD 32 y CSD 44; Geist 13/600 y 13/400. Sombras `SHADOW`/`SHADOW_DARK` por shader y recorte al radio 16. `appearance.window_color` (`neutral`/`accent`/`wallpaper`) y `traffic_lights` tiñen la titlebar; `window_tint_scope = "desktop"` tiñe menubar y dock. El blur de fondo está en el catálogo, pero aún no se dibuja.

## Shell

`malaysia-wallpaper` (Background), `malaysia-shell menubar` (Top, 28 px), `dock` (Top, abajo), `lock` (lock surfaces) y `launcher` (Overlay). Leen `~/.config/malaysia/` y `~/.local/share/malaysia/`; no escriben configuración.
## Ventanas

- **Apps propias (ADR 0035):** `app_id` con prefijo `io.github.hblanqueto.malaysia.`; su modo de decoración se respeta antes del primer commit y `force_server`/`force_client` no las toca.
- **SSD:** titlebar de 32 px, semáforos, título centrado; cerrar, minimizar, maximizar, arrastrar y redimensionar.
- **CSD:** GTK/Libadwaita, wezterm y apps que lo piden conservan su chrome; `force_server` puede forzar la barra (con riesgo de duplicarla).
- **Marco:** esquinas de 16, sombra, borde exterior y hairline para todas; fullscreen lo elimina, maximizado lo conserva. Diálogos centrados sobre el padre.
- **Popups del kit** (app-id nativo o capa de la shell, ADR 0035): el compositor les da el mismo marco (borde exterior, hairline y sombra) con esquinas de 12 (`vow_design::POPUP`), recortado a su geometría xdg. El cliente solo pinta el fondo y las filas. Los popups de otras apps (GTK, Firefox, Qt) se dibujan tal cual.
