# 0045 — GTK `gtk_shell1` y `org.gtk.Menus`

Estado: aceptado (2026-10-27). Implementa 14f-1; 14f-2 queda pendiente.

## Contexto

El Bloque 14 ya sirve menús por `org_kde_kwin_appmenu` + dbusmenu y por `com.canonical.AppMenu.Registrar`.

## Decisión

1. **Precedencia:** dbusmenu → `org.gtk.Menus` → respaldo 14c. La barra decide; el compositor solo publica direcciones.
2. **`gtk_shell1` v5** en el compositor. Al bindear se envía `capabilities(0)` en 14f-1; en 14f-2, `capabilities(3)` (`global_app_menu | global_menu_bar`). Se guardan las seis cadenas de `set_dbus_properties` por `wl_surface`; se borran con `release`, al destruir el objeto o al morir la superficie.
3. Se atienden `present`/`request_focus` (activación como xdg-activation) y `titlebar_gesture` (`double_click` aplica la acción configurada; `right_click`/`middle_click` solo se registran); valores desconocidos dan `invalid_gesture`. Nunca se envían `configure` ni `configure_edges`.
4. **Infinity v3:** evento `gtk_menus(toplevel, unique_bus_name, application_id, menubar_path, app_menu_path, application_object_path, window_object_path)`. Vacío = sin menú GTK. Se emite al cambiar la dirección, al crear el handle y al bindear; nunca al cambiar el foco. En 14f-1 la barra solo lo registra.
5. **14f-2:** cliente `org.gtk.Menus` en la barra y dibujo del menú GTK.

## Consecuencias

El compositor añade `wayland-scanner` (ya presente), no GTK.

## Alternativas descartadas

- Usar solo dbusmenu: GTK 3/4 nativo no lo exporta sin módulos externos.
