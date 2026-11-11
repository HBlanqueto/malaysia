# The dock (`malaysia-shell dock`)

> Overview map: [`interface.md`](interface.md).
> Target design: [`dock-mockup.html`](dock-mockup.html), a working mockup of the Dock and its Settings section.

Native component (ADR 0023): one icon per app, kept apps and open apps, as in
macOS. Its options live in `dock.domain`
([`options.md`](../configuration/options.md#dock--shell-dock-malaysia-shell-dock-macos-comappledock));
System Settings › Desktop & Dock writes them.

## How it works

```
ext-foreign-toplevel-list-v1 ──► windows ──► one tile per app (desktop id)
zwlr_foreign_toplevel_management_v1 ──► activate / close / minimize; active state
dock.domain + dock.state ──► items ──► layout ──► bk layer surface (Top)
```

1. **Items.** `dock/items.rs` groups the windows by app: the `app_id` is
   matched to a `.desktop` entry (`menubar::find_entry`), and the entry id
   is the key. The order is the kept apps (`persistent-apps` in
   `~/.local/share/malaysia/dock.state`), a separator, the open apps that are
   not kept and up to `DOCK_RECENTS` recent apps (`recent-apps`, with
   `show-recents`), a separator, Downloads, one tile per minimized window
   (unless `minimize-to-application`) and the Trash. `static-only` keeps only
   the open apps.
2. **Size.** `dock/geometry.rs` fits `tilesize` (16–128 px) to the screen:
   the Dock never crosses the edge, so with many items the largest size
   drops. Each icon takes a square slot of that size; a separator takes
   `DOCK_SEPARATOR` of it. The bar is the slots plus a padding of
   `DOCK_PADDING` + `DOCK_PADDING_SCALE` × size, `DOCK_EDGE_MARGIN` from the
   screen edge.
3. **Magnification.** While the pointer is on the bar, each icon grows to
   `largesize` along a cosine curve over `DOCK_MAGNIFY_RADIUS` icons. Icons
   grow away from the edge and the bar only widens.
4. **Surface.** The layer surface is anchored to the `orientation` edge. It
   is as deep as the bar plus room for magnified and bouncing icons and the
   name label. The exclusive zone is only the bar, so windows do not move
   when icons grow; with `autohide` it is 0. `set_input_region` takes the
   pointer on the bar only (on the magnified icons while the pointer is
   there), so clicks above it reach the windows. `set_blur_region` blurs
   behind the bar.
5. **Tiles.** The icon comes from the icon theme (`launcher::load_icon`),
   with the `LauncherEntry` badge and progress. A dot under the icon marks an
   open app (`show-process-indicators`). The name label shows over the icon
   under the pointer.
6. **Clicks.** A click on an open app activates its first visible window, or
   restores a minimized one. On a closed app it launches the `.desktop`
   entry with an activation token; with `launchanim` the icon bounces until a
   window of the app appears or `DOCK_LAUNCH_TIMEOUT_MS` passes. A minimized
   window tile restores that window. Downloads and Trash open in the file
   manager (`xdg-open`).
7. **Menus.** A secondary click on an app lists its windows, Options › Keep
   in Dock, and Hide and Quit (or Open). On the separator it turns hiding and
   magnification on or off, sets the position and the minimize effect, and
   opens Dock Settings. These write `dock.domain`.
8. **Dragging.** Dragging the separator changes `tilesize`. Dragging an app
   along the Dock moves it among the kept apps; dragging it away shows
   Remove and takes it out of `persistent-apps`. An open app stays until it
   quits.
9. **Hiding.** With `autohide`, the bar slides off the edge when the pointer
   leaves. A strip of `DOCK_REVEAL_STRIP` px at the edge takes the pointer;
   after `autohide-delay` seconds the bar slides back in. The slide lasts
   `DOCK_HIDE_MS` × `autohide-time-modifier`.

## Compositor side

`ext-foreign-toplevel-list-v1` uses Smithay's state; `zwlr_foreign_toplevel_management_v1` is served by hand (Smithay 0.7 does not ship it), announces the existing windows on bind and each request acts on its window.

## Divergences

- `zwlr_foreign_toplevel_management_v1` by hand (ADR 0023 allows for it).
- The minimize animation (`mineffect`) is pending: the Dock does not send
  `set_rectangle` yet, and the compositor ignores it.
- A minimized window tile shows the app icon, not a picture of the window.
- The drag image stays inside the Dock's surface.
- `activate` carries no press serial in this version of the XML, so it is not validated against the implicit grab.
