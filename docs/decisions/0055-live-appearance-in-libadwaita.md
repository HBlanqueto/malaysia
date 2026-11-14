# 0055 — Live accent and tint in libadwaita

Status: accepted (2026-11-01). Implemented in the fork
[`HBlanqueto/libadwaita`](https://github.com/HBlanqueto/libadwaita) (branch
`claude/practical-ritchie-cez5d7`, `2393318`), in the portal and in the
`libadwaita-global-menu` package; it still needs to be accepted in a session with real apps. It extends
ADR 0050 and closes debt item G-02.

## Context

With the bridge from ADR 0050, a libadwaita app sees the exact accent and the window
tint, but only when it opens: GTK reads `~/.config/gtk-4.0/gtk.css` once
(`gtksettings.c`, without watching the file). Light/dark does change live,
because libadwaita carries both schemes behind `@media (prefers-color-scheme)`
and the portal publishes `color-scheme`. The portal accent also arrives live,
but libadwaita rounds it to the closest of its nine presets
(`adw_accent_color_nearest_from_rgba`) and its `accent-color-rgba` returns the
preset. barrierkit apps change accent and tint live by reading
`appearance.domain`.

libadwaita sets `gtk-theme-name = "Adwaita-empty"`, so a GTK theme does not
work, and forcing a reload of `gtk.css` by toggling high contrast flickers
(rejected in ADR 0050).

## Decision

1. **Own namespace in the portal.** `xdg-desktop-portal-malaysia` publishes
   `org.malaysia.appearance` in `org.freedesktop.impl.portal.Settings`, with
   `SettingChanged` from the same watcher of `appearance.domain` and
   `wallpaper.state`:
   - `gtk4-css` (`s`): the bridge's GTK 4 rules (`gtk::live_css`), the
     same colors as `malaysia.css` but only the two `@media` blocks.
   - `accent-color` (`(ddd)`): the exact accent, the same as
     `org.freedesktop.appearance`.
   - `folder-color` (`(ddd)`): the color of the folders, sent after the
     portal writes `Malaysia-Tinted`; `(-1, -1, -1)` keeps them blue. The
     Dock redraws its folders on it.
   With `appearance.gtk_bridge = false` the stylesheet is empty and the accent out of
   range, `(-1, -1, -1)`: libadwaita stays stock.
2. **Fork patch** (`src/adw-live-appearance.c`, third patch on top of
   1.9.3, `nix/patches/libadwaita-live-appearance.patch`):
   - One object per process reads the namespace with `ReadAll` and follows
     `SettingChanged`. Without the namespace (any other desktop), or with
     `ADW_DISABLE_LIVE_APPEARANCE` or `ADW_DISABLE_PORTAL`, it does nothing.
   - `AdwStyleManager` loads `gtk4-css` into its own `GtkCssProvider` with
     priority `GTK_STYLE_PROVIDER_PRIORITY_USER + 1` and sets
     `prefers-color-scheme` and `prefers-contrast` on it as on the Adwaita stylesheet.
     GTK evaluates the `@media` blocks at load time, so without that the dark block would not
     apply. A change reloads only that provider, with transitions
     turned off like any libadwaita style change.
   - `accent-color-rgba` returns the exact accent and is notified when it changes.
     `accent-color` (the enum) is still the closest preset: it is public
     API and cannot give another value.
3. **Why above the user.** `gtk.css` still imports
   `malaysia.css` for Flatpak, GTK 3 and the unpatched libadwaita, and that
   copy goes stale after a change. With the same priority, the stale copy
   could win. With `USER + 1` the live one wins. In return, a user rule
   that redefines **those same** colors (`accent_bg_color`,
   `window_bg_color`…) loses while the bridge is active. To use
   their own, `appearance.gtk_bridge = false`. The rest of their rules still
   win.
4. **`accent_color` with contrast.** The bridge wrote the accent as is
   as `accent_color` (text and icons). Now it applies the libadwaita rule
   (`adw_rgba_to_standalone`, OKLab with L ≤ 0.5 in light and ≥ 0.85 in dark),
   both in `malaysia.css` and in `gtk4-css`. libadwaita 1.6+ already derives
   `--accent-color` this way, and the variable is not written.
5. **`dark_tint`.** The portal still publishes `color-scheme = 1` (dark).
   The dark block of `gtk4-css` carries the dark Adwaita backgrounds
   mixed towards the tint with the `dark_tint` palette, like the bridge.

## Consequences

- With the wallpaper as tint color (ADR 0065, step 12), the tinted
  backgrounds in `gtk4-css` read `var(--malaysia-behind)`, which the
  focus-mode patch sets per window.

- System libadwaita apps change accent, tint and light/dark
  live, together with barrierkit apps and inside the cross-fade of ADR 0044.
- Flatpak apps use the libadwaita of their runtime, without the patch: rounded
  accent live, light/dark live and, if they see `~/.config/gtk-4.0`,
  exact accent and tint on reopening.
- GTK 3 stays the same: scheme and colors on reopening.
- In another session (GNOME) the patch does not find the namespace and libadwaita
  behaves stock. The portal cleans `gtk.css` when it stops (ADR 0050 §6).
- Each app makes one more synchronous `ReadAll` at startup, like the
  `org.freedesktop.appearance` reads libadwaita already makes.

## Rejected alternatives

- **Watching `malaysia.css` from libadwaita:** two sources of truth, and the
  file format would become API between the portal and the fork.
- **Publishing the colors as an `a{s(dddd)}` dictionary:** it forces duplicating
  in C the CSS generation (variables, `@define-color`, blocks) that the
  bridge already does in Rust.
- **Infinity protocol (Wayland):** appearance already travels through the portal
  (ADR 0031/0050) and processes without a window need it too.
- **Provider with `USER` priority:** the order between providers with the same
  priority depends on which was added first. The stale copy of `gtk.css`
  could win.
