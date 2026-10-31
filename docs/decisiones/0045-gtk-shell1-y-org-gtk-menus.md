# 0045 — GTK `gtk_shell1` y `org.gtk.Menus`

Estado: aceptado (2026-10-27). Implementa 14f-1; 14f-2 queda pendiente.

## Contexto

El Bloque 14 ya sirve menús por `org_kde_kwin_appmenu` + dbusmenu y por `com.canonical.AppMenu.Registrar`. Falta la tercera fuente del ADR 0040: GTK 3/4 anuncia su `GMenuModel` por `gtk_shell1`/`gtk_surface1` y lo exporta como `org.gtk.Menus`.

## Decisión

1. **Precedencia por superficie:** dbusmenu → `org.gtk.Menus` → respaldo 14c. La barra decide; el compositor solo publica direcciones y nunca fusiona árboles.
2. **`gtk_shell1` v5** en el compositor. Al bindear se envía `capabilities(0)` en 14f-1; en 14f-2, `capabilities(3)` (`global_app_menu | global_menu_bar`). Se guardan las seis cadenas de `set_dbus_properties` por `wl_surface`; se borran con `release`, al destruir el objeto o al morir la superficie.
3. Se atienden `present`/`request_focus` (activación como xdg-activation), `titlebar_gesture` (`double_click` aplica la acción configurada; `right_click`/`middle_click` solo se registran) y valores desconocidos dan `invalid_gesture`. Nunca se envían `configure` ni `configure_edges`.
4. **Infinity v3:** evento `gtk_menus(toplevel, unique_bus_name, application_id, menubar_path, app_menu_path, application_object_path, window_object_path)`. Cadenas vacías = «sin menú GTK». Se emite al cambiar la dirección, al crear el handle y al bindear; nunca al cambiar el foco. En 14f-1 la barra solo lo registra.
5. **14f-2:** cliente `org.gtk.Menus` en la barra y dibujo del menú GTK.

## Limitaciones

El compositor no lee D-Bus, solo reenvía la dirección. Con `capabilities(0)` GTK no oculta su barra; con `capabilities(3)`, si la menubar no corre, GTK no tendrá menú visible. `set_accels_for_action` no viaja por D-Bus.

## Consecuencias

El compositor añade `wayland-scanner` (ya presente), no GTK. `ToplevelInfo` gana `gtk_menus`, separado de `app_menu`. Infinity pasa a v3; la shell bindea 1..=3 y sigue funcionando contra v2.

## Alternativas descartadas

- Usar solo dbusmenu: GTK 3/4 nativo no lo exporta sin módulos externos.
- Leer `org.gtk.Menus` en el compositor: rompe el reparto compositor ↔ shell.
