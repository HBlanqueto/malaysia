# The dock (`malaysia-shell dock`)

> Overview map: [`interface.md`](interface.md).
> Target design: [`dock-mockup.html`](dock-mockup.html), a working mockup of the Dock and its Settings section.
> Minimize and restore: [`dock-genie-mockup.html`](dock-genie-mockup.html), the genie, the Dock's slot and the thumbnail, today against the target.

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
   is the key. The Dock reads the entries when it starts and again when a
   window has an `app_id` that is no entry's id. The order is the kept apps (`persistent-apps` in
   `~/.local/share/malaysia/dock.state`), a separator, the open apps that are
   not kept and up to `DOCK_RECENTS` recent apps (`recent-apps`, with
   `show-recents`), a separator, Downloads, one tile per minimized window
   (unless `minimize-to-application`) and the Trash. `dock/motion.rs` keeps the
   tiles: a new one opens its slot in `DOCK_SLOT_MS` and a window that leaves
   keeps its tile, empty, until its slot is shut. `static-only` keeps only
   the open apps.
2. **Size.** `dock/geometry.rs` fits `tilesize` (16–128 px) to the screen:
   the Dock never crosses the edge, so with many items the largest size
   drops. Each icon takes a square slot of that size; a separator takes
   `DOCK_SEPARATOR` of it, drawn with `resolved.dock.divider`. The bar is the slots plus a padding of
   `DOCK_PADDING` + `DOCK_PADDING_SCALE` × size, `DOCK_EDGE_MARGIN` from the
   screen edge.
3. **Magnification.** While the pointer is on the bar, each icon grows to
   `largesize` along a cosine curve over `DOCK_MAGNIFY_RADIUS` icons;
   `largesize` is at least `DOCK_MAGNIFY_MIN` times `tilesize`, so icons grow
   at the largest size too. Icons grow away from the edge and the bar only
   widens.
4. **Surface.** The layer surface is anchored to the `orientation` edge. It
   is as deep as the bar plus room for magnified and bouncing icons and the
   name label. The exclusive zone is only the bar, so windows do not move
   when icons grow; with `autohide` it is 0. `set_input_region` takes the
   pointer on the bar only (on the magnified icons while the pointer is
   there), so clicks above it reach the windows.
5. **Frame.** The bar is a card (ADR 0068) with a continuous corner of
   `DOCK_RADIUS` × its thickness. The compositor knows the Dock by its
   namespace (`DOCK_LAYER_NAMESPACE`) and gives that card only the blur of
   `set_blur_region`, clipped to the corner, and no shadow: it would show
   through the translucent fill. The Dock
   draws its glass and its frame itself, with `bk::style::glass_edge`, under its
   icons, so magnified icons rise over the bar's edge. The frame is two lines, as
   macOS draws it ([`dock-macos-measured.html`](dock-macos-measured.html) shows
   four captures next to their reproduction): a dark outline one pixel outside
   the glass, strongest on the sides, and a light rim one pixel inside it, with a
   glow that dies away over a few pixels, strongest on top and bottom. Around the
   corners the outline hands over to the rim (`ctdesign::dock_rim_dark_share`).
   The rim and the dividers follow the glass: `ctdesign::dock_rim` and
   `dock_divider` take the luminance the glass shows over the wallpaper and move
   it (the rim's by `lift + share ×` that luminance, a fixed step in light and a
   shrinking one in dark; the divider's by a fixed step), so they fade and
   come back along the bar as the wallpaper changes. `dock/rim.rs` reads the
   wallpaper just outside each `SEGMENT` pixels of the edge and behind each
   divider. A Dock on the left or right takes one colour from the wallpaper
   around the whole bar. The glass, outline and the two steps are
   `ctdesign::DockSurface` (`resolved.dock`): a grey with some opacity in light
   and in dark, solved from captures over black and over grey; with
   `appearance.reduce_transparency` the glass is a solid grey, as in macOS, and
   rim and divider come out of the same formulas. As in macOS the glass is a
   thin veil over the blur, so the wallpaper and the windows that pass behind
   the Dock show through (ADR 0071, `"behind"`). It takes the bar tint only
   with `bars` in `appearance.tint_parts`.
6. **Tiles.** The icon comes from the icon theme (`launcher::load_icon_in`),
   with the `LauncherEntry` badge and progress. A minimized window's tile is
   a picture of the window, taken once with `capture_window` when it
   minimizes, with the app icon at its corner. The picture is as tall as an
   icon's body (`DOCK_ICON_BODY`), as wide as its window's shape asks, with
   square corners and no border; its slot is as wide as the
   picture plus the margin of an icon. The picture shows when the genie is over,
   not before. Downloads is `DOCK_FOLDER_CANVAS` tiles wide and drawn on all of it so that
   the folder is as tall as an icon's body and keeps the icon margin from its
   neighbours, as the picture does. Every icon sits on macOS's
   grid: an app icon already sits on it, because a Malaysia theme draws it so and the kit puts
   any other on a plate (`Image::app_icon`, in the user's `appearance.icon_style` and color scheme); the
   places fill their canvas, so they are drawn at `DOCK_ICON_BODY` of the slot. A dot under the icon marks an
   open app (`show-process-indicators`). The name label shows over the icon
   under the pointer.
7. **Clicks.** A tile answers over its whole column, from the bar's edge up
   to the top of its icon (the icon itself takes no pointer events, nor the
   running dot). A click on an open app activates its first visible window, or
   restores a minimized one. With Option held it first hides the active app
   (minimizes its windows), and does nothing more on that app's own icon; the
   compositor sends `wl_keyboard.modifiers` to the client under a pointer
   button when it has no keyboard focus. On a closed app it launches the `.desktop`
   entry with an activation token; with `launchanim` the icon bounces until a
   window of the app appears or `DOCK_LAUNCH_TIMEOUT_MS` passes. An app whose
   `LauncherEntry` is `urgent` bounces until one of its windows is active. A minimized
   window tile restores that window. Downloads and Trash open in the file
   manager (`xdg-open`).
8. **Menus.** A secondary click opens the menu beside the icon, centred on
   it, toward the screen (`pop_up_menu`): above it on a bottom Dock. The Dock
   stays up and magnified until the menu closes. On an app it lists the
   windows (a diamond marks a minimized one), Options › Keep in Dock, and
   Hide and Quit (or Open). On Downloads and the Trash it has Open; the Trash adds Empty Trash, which
   asks first (`malaysia-shell empty-trash`) and erases the home trash
   (`$XDG_DATA_HOME/Trash`) and the user's trash on every mounted volume
   (`$topdir/.Trash/$uid` or `$topdir/.Trash-$uid`). On the
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
    tile, or the slot before the Trash where that tile will appear. The Dock
    sends the square the tile has at its centre, as drawn now: with the pointer
    on the Dock, magnified. The Dock is centred, so a slot's centre does not
    depend on how wide its tile ends up, and the compositor works the
    thumbnail's width out from the window's shape.
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
centre. The target is the thumbnail the Dock will show: the window's shape at
the height of the icon body (`DOCK_ICON_BODY`), centred on the slot the Dock
sends, and the genie is exactly that wide from the target's edge inward. The
rows of the window that cross the target's far edge are already part of the
thumbnail, which the compositor draws square: it fills from the Dock's edge while the window flows
in and empties while it flows out, so there is no jump when the Dock's tile
takes over. The effect is drawn over the Dock, so it leaves and reaches the
tile in sight. While it restores, the window is drawn only by the effect; the frame
where it ends already draws the window.

A popup whose root parent is a layer surface, such as a Dock menu, is drawn
above the `Top` and `Overlay` layers and takes the pointer before them; the
popups of windows stay below the shell. A card's blur is clipped to its
corner. A Dock on the left or right counts as shell chrome while it reserves
space, so focus spaces hide it like the bottom one.

## Divergences

- `zwlr_foreign_toplevel_management_v1` by hand (ADR 0023 allows for it).
- The drag image stays inside the Dock's surface.
- `activate` carries no press serial in this version of the XML, so it is not validated against the implicit grab.
