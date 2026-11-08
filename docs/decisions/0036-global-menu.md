# 0036 — Global menu

Status: accepted (2026-10-23); implemented in 14a–14e, 14f-1 and 14f-2 (accepted 2026-11-01). Partly superseded by ADRs 0040 (Infinity v2 channel) and 0045 (GTK menus).

## Context

The menubar shows the active app and the clock; macOS adds its menus.

## Decision

1. The menu uses `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`: the first carries the service and path per surface; the second is the item tree.
2. The compositor serves `org_kde_kwin_appmenu`, stores the (service, path) pair per surface and sends each window's pair to the trusted shell through Infinity `app_menu` (ADR 0039). **It never speaks dbusmenu or draws menus.**
3. The menubar is the dbusmenu client: `zbus` and BarrierKit.
4. Our own apps export through the same path; there is no private shortcut through Infinity.
5. Sources per app: dbusmenu (KDE), `org.gtk.Menus` through `gtk_shell1` (ADR 0045), a shortcut menu (ADR 0061) and a `.desktop` fallback (app menu, Window and Help; Quit uses `zwlr_foreign_toplevel_handle_v1.close`, never SIGKILL). With no window, the menubar shows its own «Malaysia» menu.

## Consequences

The kit runner needs `xdg_popup` with a layer-shell parent; the menubar depends on `zbus`; the code ported from Otto requires updating `NOTICE`.

## Rejected alternatives

- `com.canonical.AppMenu.Registrar`: it depends on X11 IDs.
