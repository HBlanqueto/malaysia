# 0061 — Shortcut menus for apps without a global menu

Status: accepted (2026-11-07). Not yet tested in a session with real apps.

## Context

Some apps publish no global menu on Wayland through any path of ADR 0040:

- Apps with a ribbon they draw themselves: WPS Office and ONLYOFFICE, also on
  X11 ([ONLYOFFICE forum](https://community.onlyoffice.com/t/can-you-tweak-titlebar-style/2969)).
- Apps that removed it: JetBrains in 2024 ([IJPL-156149](https://github.com/JetBrains/intellij-community/commit/336265215c1ae9bf9fd7f9c23ebfabc8fc810743)).
- Apps that publish it only on X11: Electron without the ADR 0040 patch and
  Firefox without its preferences.
- X11 apps in general: satellite does not expose the X11 ID and the
  Registrar cannot match windows (ADR 0056).
- Terminals without menus (Alacritty, foot, kitty, WezTerm, Ghostty).

For them the bar only shows the 14c fallback: app menu, Window and Help.
`orbit-global-menu` (GPL-3.0) solves the same problem on GNOME with
hand-written per-app shortcut menus.

## Decision

1. **Our own catalog.** One TOML per app in
   `crates/malaysia-shell/shortcuts/`, embedded at build time. Each mapping is
   written from the app's official documentation or code and cites its URLs
   in `sources`. Nothing is copied from orbit or any other GPL project.
2. **Format.**

   ```toml
   ids = ["onlyoffice-desktopeditors", "ONLYOFFICE"]
   sources = ["https://helpcenter.onlyoffice.com/..."]

   [[menus]]
   label = "edit"
   items = [["undo", "Ctrl+Z"], [], ["copy", "Ctrl+C"]]
   ```

   - `ids` is compared, ignoring case, with the `app_id` and with the
     `.desktop` id that `find_entry` finds.
   - `[]` is a separator.
   - The label is the Fluent key `shortcut-<label>` in `malaysia-shell.ftl`
     (es and en). A test requires both.
   - Keys are `Ctrl`, `Shift` and `Alt` plus one xkbcommon keysym on the base
     level: `Ctrl+Shift+grave`, not `Ctrl+asciitilde`.
3. **Bar.** When the app publishes no menu and there are no remembered
   titles, the catalog menus go between the app menu and Window. A real menu
   always wins.
4. **Infinity v6, `send_shortcut(handle, keys)`.** When an entry is chosen the
   bar asks the compositor to type the chord into the app's active window. The
   compositor:
   1. focuses the window and checks that it has keyboard focus;
   2. looks up each keysym on the base level of the active layout;
   3. sends presses and releases to the window without going through the
      compositor shortcuts.

   If a key or the focus is missing, it ignores the request and logs it.
5. **No Super shortcuts:** Super belongs to Malaysia (menus, windows and
   spaces).

## Consequences

- WPS, ONLYOFFICE, JetBrains, VS Code and the rest of the catalog show File,
  Edit and View with real shortcuts.
- An entry cannot know whether the action applies: it is never greyed out or
  checked.
- A catalog shortcut can go stale if the app changes it; the sources let us
  review it.
- Only the shell sees Infinity (barrierkit ADR 0039), so only it can type into
  another window.

## Alternatives discarded

- **Copying orbit's mappings:** they are GPL-3.0 and Malaysia is MIT.
- **`zwp_virtual_keyboard_v1` from the shell:** it needs a keymap upload and
  depends on focus returning to the app before typing.
- **Generic menus for every app** (`global-menu-for-gnome`): Ctrl+C does not
  copy in a terminal and Ctrl+L does not align in a spreadsheet.
