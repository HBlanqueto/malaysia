# La menubar (`malaysia-shell menubar`)

> Mapa general: [`interfaz.md`](interfaz.md).

Componente nativo (ADR 0023/0034): barra superior de 28 px con la app activa, el reloj y los menús.

## Cómo funciona

```
window.domain ──► app activa (título/app_id)
        ▼
frame RGBA (barrier-kit) → layer-shell Top (28 px) → EGL/dmabuf
```

1. Capa `Top`, anclada arriba/izquierda/derecha, `exclusive_zone = 28`; las ventanas no se meten debajo.
2. El texto se compone por CPU con `barrier-kit::text` sobre `ShellPalette` y se presenta por EGL; la subida a textura es parcial (franja del reloj o del título), no el frame entero (ADR 0027).
3. El reloj muestra `%H:%M` y commitea al borde de minuto.
4. La app activa se lee de `window.domain` (`[active] title`/`app_id`), que publica el compositor al enfocar. La menubar usa el **app_id**, nunca el título de la ventana.

## El nombre de la app

1. `.desktop` cuyo id o `StartupWMClass` coincide → su `Name` localizado (`foot` → «Foot», `org.wezfurlong.wezterm` → «WezTerm»).
2. Sin `.desktop`, el último segmento con mayúscula inicial (`org.example.notes` → «Notes»).
3. Sin app ni ventana, «MalaysiaDreams».

Va en negrita (`MENUBAR_APP_NAME_WEIGHT`, 700), color `ink-strong`, 13 px, con la sans de `vow-design` (Geist); el reloj en peso normal.

## Zona exclusiva

El compositor calcula el workspace como el output menos las zonas exclusivas de capas `Top`/`Overlay` ancladas a los bordes (`Layers::exclusive_top`/`exclusive_bottom`). Las ventanas nuevas se colocan ahí: origen `(0, top)` y tamaño `(w, h − top − bottom)`.

## Menú global

La barra integra `barrier_kit::menu::MenuView`.

- **Sin ventana:** ❄ · MalaysiaDreams (negrita) · Archivo · Edición · Visualización · Ir · Ventana · Ayuda, cada uno con su menú; reloj a la derecha.
- **Con ventana:** ❄ · {App} · menús reales de la app · Ventana · Ayuda.
- **Menú real:** `org_kde_kwin_appmenu` + dbusmenu (ADR 0036/0040); el compositor publica el par por Infinity v2 y la barra pide el árbol.
- **Respaldo:** si no hay menú, nombre y acciones del `.desktop` y los controles reales de la ventana (14c).
- **GTK:** `gtk_shell1` → `org.gtk.Menus`; 14f-1 solo registra el evento, 14f-2 dibuja el menú (ADR 0045).

Habilitados: carpetas de Ir (salvo Atrás/Adelante/Ir a la carpeta), Nueva ventana, Nueva carpeta, Buscar, Minimizar todo, Mostrar escritorio, lista de ventanas, Bloquear pantalla y Ajustes del sistema. El resto se ve deshabilitado.

**Pendiente:** 14b-2 (diálogos Suspender/Reiniciar/Apagar/Cerrar sesión), 14f-2, navegación por teclado (F10/Alt) y desbordamiento de la barra. `menubar.autohide` está en el catálogo sin implementar.

## Medidas y atajos

Barra 28 px; menús radio 12; elementos de 24 px con radio 8; texto 13. Los atajos se muestran como `⌃⌥⇧❖` + tecla, sin `+` ni glifo de Command; el control recibe `Ctrl+Shift+G`, `Alt+Inicio` o `Super+L` y los formatea.

## Verificación

```sh
./dev.sh demo
./dev.sh check
```

El reloj avanza al minuto (1 commit/min, Dma); `kill -9` al proceso no afecta al compositor ni a los demás.
