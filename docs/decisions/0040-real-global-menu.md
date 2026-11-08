# 0040 — Real global menu: our own architecture

Status: accepted (2026-10-25); implemented in 14d-1..3, 14d-5 and 14d-6.

## Context

14a, 14b-1 and 14c already work; ADR 0036 set the protocols but left open the channel to the bar, the D-Bus client, the cache and the composition of titles.

## Decision

1. **Sources:** `org_kde_kwin_appmenu` → `com.canonical.AppMenu.Registrar` (bar, GTK by pid) → `gtk_shell1`/`org.gtk.Menus` (ADR 0045) → 14c fallback. No AT-SPI.
2. **Infinity v2 channel:** `app_menu(toplevel, service, path)`; empty = no menu. It is emitted when the pair changes, when the handle is created and on bind, not on focus change. The compositor does not speak dbusmenu or draw.
3. **Concurrency:** `zbus = "5"` in `malaysia-shell`; one thread with one connection and a FIFO queue. The renderer sends commands and receives `Arc<MenuSnapshot>`; a generation counter discards stale replies.
4. **Per-window cache** with revision: `GetLayout(0, 1, [])` on focus; `AboutToShow` + `GetLayout(id, 1, [])` when opening a submenu; `LayoutUpdated` invalidates the subtree; `ItemsPropertiesUpdated` updates in place; Help requests the full tree. Debounce 50 ms, timeout 500 ms.
5. **Mapping:** the `id` is kept; `label` drops the mnemonic; `enabled` defaults to `true`; `visible=false` omits; separator, submenu and toggle map to their variants; `shortcut` shows the first combination with `⌃⌥⇧❖`; `icon-name` only from the static set, and `icon-data` PNG ≤ 64 KiB is decoded and cached.
6. **Bar:** `❄ · {App} · app menus · Window · Help`; titles merge with our own menus, without duplicates or overflow. Activation sends `Event(id, "clicked", …)` to the snapshot that opened the menu and closes popups. `menubar.app_menus` disables it.
7. **Performance:** opening from cache < 16 ms; first opening < 100 ms; 0 D-Bus on the render thread.
8. **Phases:** 14d-1 compositor + Infinity v2; 14d-2 zbus client/cache; 14d-3 Registrar and acceptance; 14e our own apps.

## Consequences

ADR 0036 is superseded in the channel and in the Otto port.

## Rejected alternatives

- Publishing the pair in `innate.state`: two windows of the same app share an `app_id` and the file does not distinguish focus.
