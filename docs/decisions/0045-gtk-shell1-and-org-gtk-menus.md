# 0045 — GTK `gtk_shell1` and `org.gtk.Menus`

Status: accepted (2026-10-27). 14f-1 accepted on 2026-10-28; 14f-2 accepted on 2026-11-01 in a nested session (gtk3-demo-application).

## Context

Block 14 already serves menus through `org_kde_kwin_appmenu` + dbusmenu and through `com.canonical.AppMenu.Registrar`.

## Decision

1. **Precedence:** dbusmenu → `org.gtk.Menus` → 14c fallback. The bar decides; the compositor only publishes addresses.
2. **`gtk_shell1` v5** in the compositor. On bind it sends `capabilities(0)` in 14f-1; in 14f-2, `capabilities(3)` (`global_app_menu | global_menu_bar`). The six strings of `set_dbus_properties` are stored per `wl_surface`; they are cleared on `release`, when the object is destroyed or when the surface dies.
3. It handles `present`/`request_focus` (activation like xdg-activation) and `titlebar_gesture` (`double_click` applies the configured action; `right_click`/`middle_click` are only logged); unknown values give `invalid_gesture`. It never sends `configure` or `configure_edges`.
4. **Infinity v3:** event `gtk_menus(toplevel, unique_bus_name, application_id, menubar_path, app_menu_path, application_object_path, window_object_path)`. Empty = no GTK menu. It is emitted when the address changes, when the handle is created and on bind; never when the focus changes. In 14f-1 the bar only logs it.
5. **14f-2:** `org.gtk.Menus` client in the bar and drawing of the GTK menu.
   - It lives on the dbusmenu zbus thread (`menubar/gtkmenus.rs`). `Start(au)` for
     group 0 and then for each group linked by `:submenu`/`:section` until
     all are closed (one call per round); `Changed` splices the stored
     menus; `End(au)` when the focus changes, when `app_menus` is disabled or when
     dbusmenu arrives.
   - The model is translated to `RawItem` with dbusmenu property names and
     goes through `dbusmenu_map::menu_items`: a single mapping (mnemonics,
     separators, shortcuts, ✓ and radio).
   - `org.gtk.Actions`: `DescribeAll` on `application_object_path` (`app.*`) and
     `window_object_path` (`win.*`), `Changed` for enabled and state,
     `Activate` with the item's `target`. An action with boolean state and no
     `target` = ✓; with `target` = radio. No action = grey; `hidden-when`
     `action-missing`/`action-disabled` hide the item.
   - `accel` (`<Primary>s`) goes into the dbusmenu key list; `<Primary>`
     is Ctrl (⌃).
6. **Capabilities:** the compositor sends `3` if `menubar.app_menus` is `true`
   and `0` otherwise, so that GTK does not hide its menubar when the bar will not
   show it. GTK 3.24.52 and 4.22.4 check each setting with
   `(caps & bit) == bit` (`gdkscreen-wayland.c`, `gdksettings-wayland.c`), so
   `3` enables `gtk-shell-shows-app-menu`, `gtk-shell-shows-menubar` and, because of
   the enum numbering (`desktop_icons = 3`, GNOME bug 787407), also
   `gtk-shell-shows-desktop`: `GtkPlacesSidebar` shows "Desktop". With
   `shows-menubar` both GTK 3 and GTK 4 remove the internal menubar of
   `GtkApplicationWindow`; GTK 4 no longer has its own app menu.

## Consequences

The compositor adds `wayland-scanner` (already present), not GTK. A client that bound `gtk_shell1` before `menubar.app_menus` changed keeps the previous value until it reconnects: the protocol does not resend `capabilities`.

## Rejected alternatives

- Using only dbusmenu: native GTK 3/4 does not export it without external modules.
