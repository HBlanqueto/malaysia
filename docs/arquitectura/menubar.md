# La menubar (`malaysia-shell menubar`)

> Mapa general: [`interfaz.md`](interfaz.md).

Componente nativo (ADR 0023/0034): barra superior de 30 px con la app activa, el reloj y los menús.

## Cómo funciona

```
window.domain ──► app activa (título/app_id)
        ▼
frame RGBA (barrier-kit) → layer-shell Top (30 px) → EGL/dmabuf
```

1. Capa `Top`, anclada arriba/izquierda/derecha, `exclusive_zone = 28`; las ventanas no se meten debajo.
2. El texto se compone por CPU con `barrier-kit::text` sobre `ShellPalette` y se presenta por EGL; la subida a textura es parcial (franja del reloj o del título), no el frame entero (ADR 0027).
3. El reloj muestra `%H:%M` y commitea al borde de minuto.
4. La app activa se lee de `window.domain` (`[active] title`/`app_id`), que publica el compositor al enfocar y vacía al arrancar la sesión; `use_watched` la relee cuando inotify avisa de un cambio, sin sondeo. La menubar usa el **app_id**, nunca el título de la ventana.
5. Los diálogos de la shell («Acerca de este PC» y «Acerca de» una app) no cambian la app activa: la barra sigue con la última app que tuvo el foco, como en macOS.

## El nombre de la app

1. `.desktop` cuyo id o `StartupWMClass` coincide → su `Name` localizado (`foot` → «Foot», `org.wezfurlong.wezterm` → «WezTerm»).
2. Sin `.desktop`, el último segmento con mayúscula inicial (`org.example.notes` → «Notes»).
3. Sin app ni ventana, «Malaysia».

Va en negrita (`MENUBAR_APP_NAME_WEIGHT`, 700), color `ink-strong`, 13 px, con la sans de `vow-design` (Geist); el reloj en peso normal.

## Zona exclusiva

El compositor calcula el workspace como el output menos las zonas exclusivas de capas `Top`/`Overlay` ancladas a los bordes (`Layers::exclusive_top`/`exclusive_bottom`). Las ventanas nuevas se colocan ahí: origen `(0, top)` y tamaño `(w, h − top − bottom)`.

## Menú global

La barra integra `MenuView`: sin ventana muestra ❄ · Escritorio · Archivo · Edición · Visualización · Ir · Ventana · Ayuda; con ventana, ❄ · {App} · menús reales · Ventana · Ayuda. El menú real usa `org_kde_kwin_appmenu` + dbusmenu (ADR 0036/0040); si no hay, respaldo de `.desktop` (14c). GTK usa `gtk_shell1`/`org.gtk.Menus` (14f-1 registra; 14f-2 dibuja); un `GtkMenuBar` de GTK 3 llega por la misma vía con `appmenu-gtk-module` y acciones `unity.*` (14f-4). Chromium y Electron parcheado (14f-5) usan `org_kde_kwin_appmenu`. Pendiente: 14b-2, 14f-2, teclado F10/Alt y desbordamiento.

«Acerca de {App}» toma la entrada «Acerca de…»/«About…» de la app, esté en el menú que esté (casi siempre Ayuda), y la quita de allí (`remote::take_about`). Si la app no tiene, abre `malaysia-shell about-app <app_id>`: icono, nombre y `GenericName` del `.desktop`.
