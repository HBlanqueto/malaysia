# 0052 — Menú global para libadwaita

Estado: propuesto (2026-11-01). Implementado en el fork
[`HBlanqueto/libadwaita`](https://github.com/HBlanqueto/libadwaita) (rama
`claude/optimistic-bell-p2v9or`, `48d3ff5`) y en la barra; falta empaquetar el fork y aceptarlo con una app real.

## Contexto

14f-2 (ADR 0045) muestra el menubar de GTK por `org.gtk.Menus`, pero las apps
libadwaita no tienen menubar: `AdwApplicationWindow` fuerza
`show-menubar = FALSE` y su menú vive en el botón ☰ del header bar
(`GtkMenuButton` con `primary`), que GTK no publica. La barra caía al respaldo
del `.desktop` (14c). GTK 4 ya no carga módulos, así que no hay forma de
publicarlo sin tocar la librería.

GTK envía la ruta del menubar por `gtk_shell1.set_dbus_properties` solo al
realizar la ventana (`gtkapplication-wayland.c`), y la exporta en
`gtk_application_set_menubar` (`gtkapplication-dbus.c`).

## Decisión

1. **En el fork** (`src/adw-global-menu.c`): `AdwApplicationWindow::realize`,
   antes de encadenar, publica un `GMenu` propio como menubar de la app si
   `gtk-shell-shows-menubar` es `TRUE` (capabilities `3`, ADR 0045 §6), la app
   no tiene menubar propio y no está `ADW_DISABLE_GLOBAL_MENU`. El modelo se
   crea una vez por app y después solo cambia su contenido.
2. El modelo tiene un submenú con `g_get_application_name()` y el atributo
   `x-adw-app-menu = true`, enlazado al `menu-model` del botón `primary` de la
   última ventana activa (el primero mapeado; si no, el primero encontrado).
   Se recalcula en `notify::is-active`, `notify::focus-widget` (en idle) y
   `notify::menu-model` del botón. Sin botón, el menubar queda vacío y la barra
   usa el respaldo 14c.
3. **En la barra** (`menubar/gtkmenus.rs`): un submenú con `x-adw-app-menu`
   toma el nombre de app de la barra, así `remote::compose` lo pliega en el
   título en negrita aunque `g_get_application_name()` no coincida con el
   `.desktop`.
4. El botón ☰ sigue visible en el header bar; la app no cambia.

## Consecuencias

- Solo lo ven las apps que enlazan el fork. Las apps Flatpak usan la
  libadwaita de su runtime y siguen con 14c.
- Falta un overlay de Nix que sustituya `libadwaita` por el fork (recompila
  las apps que dependen de ella) o limitarlo a las apps de la sesión.
- El ☰ es una lista plana: libadwaita da un menú de app, no Archivo/Editar.
- `AdwWindow` no se engancha: no es `GtkApplicationWindow` y GTK no exporta
  sus acciones `win.*`.
- Las acciones `win.*` del menú se activan en el `window_object_path` de la
  ventana enfocada, el mismo que la barra ya usa.

## Alternativas descartadas

- Deducir el menú de `org.gtk.Actions` sin libadwaita: no hay etiquetas, solo
  nombres de acción.
- Ocultar el ☰ cuando la barra lo muestra: rompe apps que lo usan para algo
  más que el menú y no es necesario para 14f.
