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
   there), so clicks above it reach the windows.
5. **Frame.** The bar is a card (ADR 0068): `set_card_frames` gives it the
   window frame, outer border, inner line and shadow, around a continuous
   corner of `DOCK_RADIUS` × its thickness, and the client draws the bar with
   the same corner, inset by the hairline. `set_blur_region` blurs behind it,
   clipped to that corner. The compositor knows the Dock by its namespace
   (`DOCK_LAYER_NAMESPACE`) and draws its frame under the surface, so
   magnified icons rise over the bar's edge.
6. **Tiles.** The icon comes from the icon theme (`launcher::load_icon`),
   with the `LauncherEntry` badge and progress. A minimized window's tile is
   a picture of the window, taken once with `capture_window` when it
   minimizes, with the app icon at its corner. Every icon sits on macOS's
   grid: Malaysia's app icons already keep the 824-on-1024 body; any other
   icon, and the places, is drawn at `DOCK_ICON_BODY` of the slot. A dot under the icon marks an
   open app (`show-process-indicators`). The name label shows over the icon
   under the pointer.
7. **Clicks.** A click on an open app activates its first visible window, or
   restores a minimized one. On a closed app it launches the `.desktop`
   entry with an activation token; with `launchanim` the icon bounces until a
   window of the app appears or `DOCK_LAUNCH_TIMEOUT_MS` passes. A minimized
   window tile restores that window. Downloads and Trash open in the file
   manager (`xdg-open`).
8. **Menus.** A secondary click opens the menu beside the icon, centred on
   it, toward the screen (`pop_up_menu`): above it on a bottom Dock. The Dock
   stays up and magnified until the menu closes. On an app it lists the
   windows, Options › Keep in Dock, and Hide and Quit (or Open). On the
   separator it turns hiding and magnification on or off, sets the position
   and the minimize effect, and opens Dock Settings; these write
   `dock.domain`.
9. **Dragging.** Dragging the separator changes `tilesize`. While an app is
   dragged the other icons make room where it would land; dropping it there
   keeps it in the Dock (`persistent-apps`), open or not. Dragging it away
   shows Remove, closes the gap and takes it out; an open app stays until it
   quits.
10. **Minimize target.** The Dock sends each window's slot with
    `set_minimize_target` (`zwlr_foreign_toplevel_handle_v1.set_rectangle`):
    the app's icon with `minimize-to-application`, otherwise the window's own
    tile, or the slot before the Trash where that tile will appear.
11. **Hiding.** With `autohide`, the bar slides off the edge when the pointer
   leaves. A strip of `DOCK_REVEAL_STRIP` px at the edge takes the pointer;
   after `autohide-delay` seconds the bar slides back in. The slide lasts
   `DOCK_HIDE_MS` × `autohide-time-modifier`.

## Compositor side

`ext-foreign-toplevel-list-v1` uses Smithay's state; `zwlr_foreign_toplevel_management_v1` is served by hand (Smithay 0.7 does not ship it), announces the existing windows on bind and each request acts on its window.

Minimizing or restoring a window runs `mineffect` toward its target
(`minimize.rs`): the compositor draws the window into a texture and bends it
with `grade-graphics/src/minimize.frag` for `DOCK_MINIMIZE_MS`. The genie
follows Wayfire's squeezimize (MIT, see `NOTICE`): the content slides toward
the target while its sides narrow along a sigmoid. The window travels toward
the screen edge nearest the target; without one it goes to the bottom
centre. While it restores, the window is drawn only by the effect.

A popup whose root parent is a layer surface, such as a Dock menu, is drawn
above the `Top` and `Overlay` layers and takes the pointer before them; the
popups of windows stay below the shell. A card's blur is clipped to its
corner. A Dock on the left or right counts as shell chrome while it reserves
space, so focus spaces hide it like the bottom one.

## Divergences

- `zwlr_foreign_toplevel_management_v1` by hand (ADR 0023 allows for it).
- The drag image stays inside the Dock's surface.
- `activate` carries no press serial in this version of the XML, so it is not validated against the implicit grab.
