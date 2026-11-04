# The menubar (`malaysia-shell menubar`)

> Overview map: [`interfaz.en.md`](interfaz.en.md).

Native component (ADR 0023/0034): a 28 px top bar with the active app, the clock and the menus.

## How it works

```
window.domain ──► active app (title/app_id)
        ▼
RGBA frame (barrier-kit) → layer-shell Top (28 px) → EGL/dmabuf
```

1. `Top` layer, anchored top/left/right, `exclusive_zone = 28`; windows do not go underneath.
2. The text is composed on the CPU with `barrier-kit::text` on `ShellPalette` and presented via EGL; the texture upload is partial (the clock or title strip), not the whole frame (ADR 0027).
3. The clock shows `%H:%M` and commits at the minute boundary.
4. The active app is read from `window.domain` (`[active] title`/`app_id`), which the compositor publishes on focus and clears when the session starts; `use_watched` reads it again when inotify reports a change, without polling. The menubar uses the **app_id**, never the window title.
5. Shell dialogs («Acerca de este PC» and an app's «Acerca de») do not change the active app: the bar keeps the last app that had focus, as on macOS.

## The app name

1. A `.desktop` whose id or `StartupWMClass` matches → its localized `Name` (`foot` → «Foot», `org.wezfurlong.wezterm` → «WezTerm»).
2. Without a `.desktop`, the last segment with an initial capital (`org.example.notes` → «Notes»).
3. Without an app or window, «Malaysia».

It is bold (`MENUBAR_APP_NAME_WEIGHT`, 700), color `ink-strong`, 13 px, with the `vow-design` sans (Geist); the clock uses the regular weight.

## Exclusive zone

The compositor computes the workspace as the output minus the exclusive zones of `Top`/`Overlay` layers anchored to the edges (`Layers::exclusive_top`/`exclusive_bottom`). New windows are placed there: origin `(0, top)` and size `(w, h − top − bottom)`.

## Global menu

The bar embeds `MenuView`: without a window it shows ❄ · Escritorio · Archivo · Edición · Visualización · Ir · Ventana · Ayuda; with a window, ❄ · {App} · real menus · Ventana · Ayuda. The real menu uses `org_kde_kwin_appmenu` + dbusmenu (ADR 0036/0040); if there is none, the `.desktop` fallback (14c). GTK uses `gtk_shell1`/`org.gtk.Menus` (14f-1 registers; 14f-2 draws). Pending: 14b-2, 14f-2, F10/Alt keyboard and overflow.

«Acerca de {App}» takes the app's «Acerca de…»/«About…» entry from whichever menu holds it (almost always Help) and removes it there (`remote::take_about`). If the app has none, it opens `malaysia-shell about-app <app_id>`: icon, name and `GenericName` from the `.desktop`.
