# 0052 — Global menu for libadwaita

Status: accepted (2026-11-01). Implemented in the fork
[`HBlanqueto/libadwaita`](https://github.com/HBlanqueto/libadwaita) (branch
`claude/optimistic-bell-p2v9or`, `48d3ff5`), in the bar and in the NixOS
module; it still needs to be accepted with a real app.

## Context

14f-2 (ADR 0045) shows the GTK menubar through `org.gtk.Menus`, but
libadwaita apps have no menubar: `AdwApplicationWindow` forces
`show-menubar = FALSE` and its menu lives in the ☰ button of the header bar
(`GtkMenuButton` with `primary`), which GTK does not publish. The bar fell back to
the `.desktop` fallback (14c). GTK 4 no longer loads modules, so there is no way to
publish it without touching the library.

GTK sends the menubar path through `gtk_shell1.set_dbus_properties` only when
the window is realized (`gtkapplication-wayland.c`), and exports it in
`gtk_application_set_menubar` (`gtkapplication-dbus.c`).

## Decision

1. **In the fork** (`src/adw-global-menu.c`): `AdwApplicationWindow::realize`,
   before chaining up, publishes its own `GMenu` as the app menubar if
   `gtk-shell-shows-menubar` is `TRUE` (capabilities `3`, ADR 0045 §6), the app
   has no menubar of its own and `ADW_DISABLE_GLOBAL_MENU` is not set. The model is
   created once per app and afterwards only its content changes.
2. The model has a submenu with `g_get_application_name()` and the attribute
   `x-adw-app-menu = true`, linked to the `menu-model` of the `primary` button of the
   last active window (the first mapped one; otherwise, the first one found).
   It is recomputed on `notify::is-active`, `notify::focus-widget` (in idle) and
   the button's `notify::menu-model`. Without a button, the menubar stays empty and the bar
   uses the 14c fallback.
3. **In the bar** (`menubar/gtkmenus.rs`): the submenu with `x-adw-app-menu`
   is distributed by the GTK action of each entry, the way macOS orders the menus
   of an app (revised on 2026-11-01; before, it was folded whole at the end of the
   app menu, below "Quit"):
   - `app.about`, `app.preferences` and `app.quit` take "About X",
     "Settings…" and "Quit X" in the app menu. They go in a submenu with the
     app name and with the label of that entry; `remote::compose`
     (`fill_slots`) puts their action and state into the slot, which is no longer
     grey, instead of appending them at the end.
   - `app.help`, `app.shortcuts` and `win.show-help-overlay` go to Help, below
     the search.
   - `win.fullscreen` is not repeated: Window already has "Enter Full
     Screen".
   - The rest keeps its sections in its own title to the right of the
     app's: "File" if any action creates, opens or saves (`new-window`,
     `new-tab`, `open`, `save`…), "Options" otherwise.
   - An action with any other name falls into that own title: nothing is lost.
   Mockup: <https://claude.ai/artifact/D3DcNEJHPZK7XAnkkB9ZB7>.
4. **Nix package.** The flake exposes `packages.<system>.libadwaita-global-menu`,
   derived from `pkgs.libadwaita` with the fork patch applied on top of GNOME's
   `1.9.3` tag. The patch uses only GTK <= 4.22 API to match the
   nixpkgs libadwaita. Since ADR 0053 it also carries the focus mode
   patch (`libadwaita-focus-mode.patch`), applied after this one, and
   since ADR 0055 the live appearance patch
   (`libadwaita-live-appearance.patch`), the last one.
5. **No overlay, no app rebuilds.** The NixOS module uses
   `system.replaceDependencies.replacements` to replace the system
   libadwaita with the patched version. Existing libadwaita applications
   keep their cache and are not rebuilt.
6. **NixOS option.** `programs.malaysia.globalMenu.libadwaita.enable`
   (default `true`) controls whether the replacement is enabled. It can
   be disabled to avoid using the fork.
7. The ☰ button stays visible in the header bar; the app does not change.

## Consequences

- No nixpkgs overlay and no application rebuilds are needed.
  `system.replaceDependencies` swaps libadwaita in the system closure.
- Only apps that link the system libadwaita see it. Flatpak apps
  use the libadwaita of their runtime and stay on 14c.
- The patch applies to a specific libadwaita (1.9.3). When nixpkgs
  updates libadwaita, you must check that the patch still applies and that it
  still uses only GTK <= 4.22 API.
- The ☰ is a flat list: libadwaita gives an app menu, not File/Edit.
- `AdwWindow` is not hooked: it is not a `GtkApplicationWindow` and GTK does not export
  its `win.*` actions.
- The menu's `win.*` actions are activated on the `window_object_path` of the
  focused window, the same one the bar already uses.
- The entry point for users is the NixOS option, not a
  `.domain` domain.

## Rejected alternatives

- Deducing the menu from `org.gtk.Actions` without libadwaita: there are no labels, only
  action names.
- Hiding the ☰ when the bar shows it: it breaks apps that use it for more
  than the menu and is not needed for 14f.
