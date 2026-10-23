# La menubar (`yuki-shell menubar`)

El segundo componente nativo del shell (ADR 0023/0034): la barra superior,
como la menu bar de macOS. Primera versión: **nombre de la app activa** a la
izquierda y **reloj** a la derecha. El menú global por app está planeado
(Bloque 14); ver [Menú global](#menú-global-planeado-bloque-14).

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
   texto en un frame RGBA sobre la `ShellPalette` de Neptunia (barra
   translúcida, `#f2f4f7e6` en claro) y la textura se presenta
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
  (blur de la barra, ADR 0020/0034, futuras) deben renderizarse en GPU, nunca
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

## Menú global (planeado, Bloque 14)

Todavía no hay código. La decisión está en el
[ADR 0036](../decisiones/0036-menu-global.md) y la investigación en
[`global-menu.md`](../investigacion/global-menu.md).

### Flujo

```
app ──org_kde_kwin_appmenu──► compositor ──window.domain──► menubar
                                                              │
                                              com.canonical.dbusmenu
                                                              ▼
                                                     popups CelestialKit
```

El cliente anuncia el servicio y la ruta de su menú por superficie. El
compositor guarda el par y publica el de la ventana enfocada en `window.domain`.
La menubar pide el árbol dbusmenu y lo dibuja en popups de CelestialKit.

### Los cuatro modos

1. **Escritorio** (sin ventana enfocada): menú «Yuki».
2. **Menú real**: la app entrega su menú por dbusmenu.
3. **Respaldo**: la app no entrega menú; se usan nombre y acciones del
   `.desktop`, los controles reales de la ventana y Salir.
4. **App propia**: una app de CelestialKit exporta su menú por la misma vía
   dbusmenu.

### Menú «Yuki» aprobado (sin ventana enfocada)

Título «Yuki». Las acciones de carpetas usan el gestor de archivos por defecto
(`xdg-open`, carpetas de `xdg-user-dirs`).

- **Sistema** (glifo de copo): Acerca de este equipo · Ajustes del sistema… ·
  Suspender · Reiniciar… · Apagar… · Bloquear pantalla · Cerrar sesión…
- **Yuki**: Acerca de Yuki · Ajustes… · Vaciar papelera…
- **Archivo**: Nueva ventana (gestor de archivos en ~) · Nueva carpeta en el
  escritorio · Abrir… · Buscar (abre el launcher)
- **Edición**: Deshacer, Cortar, Copiar, Pegar, Seleccionar todo (deshabilitados
  sin selección)
- **Visualización**: Mostrar/ocultar dock · Cambiar fondo de pantalla…
- **Ir**: Atrás/Adelante (deshabilitados) · Inicio · Escritorio · Documentos ·
  Descargas · Imágenes · Música · Vídeos · Papelera · Ir a la carpeta…
- **Ventana**: Minimizar todo · Mostrar escritorio · lista de ventanas
- **Ayuda**: Atajos de teclado · Documentación de Yuki

### Medidas de Neptunia

Menubar 28 px, menús con radio 12, elementos de menú de 24 px con radio 8 y
texto 13. Se dibujaron como maquetas en el lienzo de diseño «Yuki — Global
menu».

### Glifos de atajos (decidido)

Los atajos se muestran con glifos de teclas reales, como macOS, en el orden
`⌃` `⌥` `⇧` `❖` seguido de la tecla: `⌃` Ctrl, `⌥` Alt, `⇧` Shift, `❖` Super.
Sin `+` entre ellos y sin el glifo de Command en ningún sitio. El control recibe el atajo como
texto (`Ctrl+Shift+G`, `Alt+Inicio`) y lo formatea; ningún menú escribe glifos a
mano. Ejemplos: `Ctrl+Shift+G` → `⌃⇧G`, `Alt+Inicio` → `⌥Inicio`,
`Super+L` → `❖L`.

### Decisiones abiertas

- Menús sólidos o translúcidos (Neptunia reserva el cristal para el primer
  arranque).
- Desbordamiento de la barra, navegación con teclado (F10/Alt) y comportamiento
  con una app sin respuesta.

## Relación

- [`interfaz.md`](interfaz.md) — el mapa completo de la UI.
- ADR 0023 (procesos), ADR 0034 (CelestialKit sobre Freya), ADR 0032
  (Neptunia).
- `wallpaper.md` — componente hermano (presentación EGL, ADR 0027).
- `modelo-de-procesos.md` — la distribución de procesos.
- `configuracion/opciones.md` — `menubar.autohide` (catálogo).