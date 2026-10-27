# 0045 — GTK `gtk_shell1` y `org.gtk.Menus`

Estado: **aceptado** (2026-10-27). Implementa 14f-1; 14f-2 queda pendiente.

## Contexto

El Bloque 14 ya sirve menús globales por dos vías:

1. `org_kde_kwin_appmenu` + `com.canonical.dbusmenu` (Qt/KDE, Firefox y apps propias).
2. `com.canonical.AppMenu.Registrar` (GTK con `appmenu-gtk-module`, vía XWayland).

Falta la tercera fuente acordada en el [ADR 0040](0040-menu-global-real.md): GTK 3/4
nativo anuncia su `GMenuModel` a través de `gtk_shell1` / `gtk_surface1.set_dbus_properties`
y lo exporta por D-Bus como `org.gtk.Menus`. Esta decisión cierra el diseño de 14f.

## Decisión

### 1. Precedencia de fuentes

Para una misma superficie, el menú global se resuelve en este orden:

1. **dbusmenu** sobre `org_kde_kwin_appmenu` (ya implementado en 14d).
2. **`org.gtk.Menus`** anunciado por `gtk_shell1` (este ADR).
3. Respaldo de 14c (`focused.rs`).

La prioridad la aplica la barra: si una ventana tiene un menú dbusmenu, se usa ese;
si no, se prueba `org.gtk.Menus`; si tampoco, el respaldo. El compositor solo publica
las direcciones; nunca fusiona árboles.

### 2. `gtk_shell1` en el compositor

Se anuncia `gtk_shell1` versión 5 a todos los clientes, como hace `org_kde_kwin_appmenu`.
Al bindear se envía `capabilities(0)` en 14f-1. En 14f-2 se enviará `capabilities(3)`
(`global_app_menu | global_menu_bar`).

La enumeración de GTK numera las capacidades 1, 2 y 3, pero GTK las lee como una
máscara de bits; el valor 3 activa tanto `global_app_menu` como `global_menu_bar`,
y de paso fija `gtk-shell-shows-desktop`. Eso no cambia el comportamiento de
MalaysiaDreams, así que 14f-2 usará 3.

El compositor maneja todas las peticiones de `gtk_shell1` / `gtk_surface1` sin
panicar, pero en 14f-1 solo se actúa sobre:

- `set_dbus_properties`: guardar las seis cadenas (NULL → `""`). Si todas están
  vacías se borra la entrada.
- `present` / `request_focus`: activar la ventana por el mismo camino que
  `xdg_activation_v1` (`State::activate_window`).
- `titlebar_gesture`: `double_click` ejecuta la acción de doble clic configurada
  de la titlebar; `right_click` va al menú de ventana si existe (hoy no existe en
  el compositor, así que solo se loguea); `middle_click` solo se loguea; cualquier
  otro valor produce un error de protocolo `invalid_gesture`.

No se envían nunca `configure` ni `configure_edges`.

Las propiedades se guardan por `wl_surface`, reutilizando el mismo almacén genérico
que `org_kde_kwin_appmenu`. Se borran con `release`, al destruir el objeto o al
morir la superficie.

### 3. De la app a la barra: Infinity v3

`infinity_window_manager_v1` sube a **versión 3** con un nuevo evento:

```xml
<event name="gtk_menus" since="3">
  <arg name="toplevel" type="object" interface="zwlr_foreign_toplevel_handle_v1"/>
  <arg name="unique_bus_name" type="string"/>
  <arg name="application_id" type="string"/>
  <arg name="menubar_path" type="string"/>
  <arg name="app_menu_path" type="string"/>
  <arg name="application_object_path" type="string"/>
  <arg name="window_object_path" type="string"/>
</event>
```

Cadenas vacías significan "esta ventana no tiene menú GTK". El evento se emite
bajo las mismas reglas que `app_menu`: al cambiar la dirección, al crear el handle
de un toplevel y al bindear el manager. Nunca al cambiar el foco.

En 14f-1 la barra solo loguea el evento:

```text
menubar: gtk_menus app_id=<id> bus=<name> menubar=<path> appmenu=<path>
menubar: gtk_menus app_id=<id> ... none
```

El menú GTK propiamente dicho se dibujará en 14f-2.

### 4. Limitaciones conocidas

- El compositor no lee `org.gtk.Menus` por D-Bus; solo reenvía la dirección. La
  lectura del árbol llega en 14f-2.
- `capabilities` es 0 en 14f-1, por lo que GTK 3/4 no oculta su propia barra de
  menús. La aceptación de 14f-1 verifica que el demo aún muestre su menú interno.
- Si en 14f-2 se anuncia `capabilities(3)` y el proceso `malaysia-shell menubar`
  no está corriendo, las apps GTK anunciadas no tendrán visible su menú. Es la
  misma limitación que con `org_kde_kwin_appmenu` en 14d.
- Los atajos solo viajan si el modelo de menús GTK los lleva; `set_accels_for_action`
  no se publica por D-Bus.

## Consecuencias

- El compositor enlaza una dependencia más de `wayland-scanner` (ya era
  dependencia del workspace) y nada de GTK.
- `ToplevelInfo` en la shell gana un campo `gtk_menus` separado de `app_menu`.
- El protocolo `infinity-window-manager-v1.xml` pasa a versión 3; la shell bindea
  1..=3 y sigue funcionando contra un compositor v2.

## Referencias

- [ADR 0040](0040-menu-global-real.md) — arquitectura del menú global (fuentes y
  precedencia).
- [ADR 0039](0039-infinity-v1-gestion-de-ventanas-para-la-shell.md) — Infinity v1.
- `protocols/gtk-shell.xml` — copiado sin modificar del repositorio GTK en el
  commit `efcc16deb0e0373655e2e4aed139ff7de3096134`.
- `docs/bloques/14-menu-global.md` — estado de 14f.
