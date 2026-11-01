# 0045 — GTK `gtk_shell1` y `org.gtk.Menus`

Estado: aceptado (2026-10-27). 14f-1 aceptado el 2026-10-28; 14f-2 aceptado el 2026-11-01 en sesión anidada (gtk3-demo-application).

## Contexto

El Bloque 14 ya sirve menús por `org_kde_kwin_appmenu` + dbusmenu y por `com.canonical.AppMenu.Registrar`.

## Decisión

1. **Precedencia:** dbusmenu → `org.gtk.Menus` → respaldo 14c. La barra decide; el compositor solo publica direcciones.
2. **`gtk_shell1` v5** en el compositor. Al bindear se envía `capabilities(0)` en 14f-1; en 14f-2, `capabilities(3)` (`global_app_menu | global_menu_bar`). Se guardan las seis cadenas de `set_dbus_properties` por `wl_surface`; se borran con `release`, al destruir el objeto o al morir la superficie.
3. Se atienden `present`/`request_focus` (activación como xdg-activation) y `titlebar_gesture` (`double_click` aplica la acción configurada; `right_click`/`middle_click` solo se registran); valores desconocidos dan `invalid_gesture`. Nunca se envían `configure` ni `configure_edges`.
4. **Infinity v3:** evento `gtk_menus(toplevel, unique_bus_name, application_id, menubar_path, app_menu_path, application_object_path, window_object_path)`. Vacío = sin menú GTK. Se emite al cambiar la dirección, al crear el handle y al bindear; nunca al cambiar el foco. En 14f-1 la barra solo lo registra.
5. **14f-2:** cliente `org.gtk.Menus` en la barra y dibujo del menú GTK.
   - Vive en el hilo zbus de dbusmenu (`menubar/gtkmenus.rs`). `Start(au)` del
     grupo 0 y después de cada grupo enlazado por `:submenu`/`:section` hasta
     cerrarlos todos (una llamada por ronda); `Changed` empalma los menús
     guardados; `End(au)` al cambiar de foco, al desactivar `app_menus` o al
     llegar dbusmenu.
   - El modelo se traduce a `RawItem` con nombres de propiedad de dbusmenu y
     pasa por `dbusmenu_map::menu_items`: un solo mapeo (mnemónicos,
     separadores, atajos, ✓ y radio).
   - `org.gtk.Actions`: `DescribeAll` en `application_object_path` (`app.*`) y
     `window_object_path` (`win.*`), `Changed` para habilitado y estado,
     `Activate` con el `target` del elemento. Acción con estado booleano y sin
     `target` = ✓; con `target` = radio. Sin acción = gris; `hidden-when`
     `action-missing`/`action-disabled` ocultan.
   - `accel` (`<Primary>s`) pasa a la lista de teclas de dbusmenu; `<Primary>`
     es Ctrl (⌃).
6. **Capabilities:** el compositor envía `3` si `menubar.app_menus` es `true`
   y `0` si no, para que GTK no esconda su menubar cuando la barra no lo va a
   mostrar. GTK 3.24.52 y 4.22.4 comprueban cada ajuste con
   `(caps & bit) == bit` (`gdkscreen-wayland.c`, `gdksettings-wayland.c`), así
   que `3` activa `gtk-shell-shows-app-menu`, `gtk-shell-shows-menubar` y, por
   la numeración del enum (`desktop_icons = 3`, bug GNOME 787407), también
   `gtk-shell-shows-desktop`: `GtkPlacesSidebar` muestra «Escritorio». Con
   `shows-menubar` tanto GTK 3 como GTK 4 quitan el menubar interno de
   `GtkApplicationWindow`; GTK 4 ya no tiene app menu propio.

## Consecuencias

El compositor añade `wayland-scanner` (ya presente), no GTK. Un cliente que bindeó `gtk_shell1` antes de cambiar `menubar.app_menus` conserva el valor anterior hasta reconectar: el protocolo no reenvía `capabilities`.

## Alternativas descartadas

- Usar solo dbusmenu: GTK 3/4 nativo no lo exporta sin módulos externos.
