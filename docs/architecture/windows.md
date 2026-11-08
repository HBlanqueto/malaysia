# Windows and titlebar (Tengen)

> Overview map: [`interface.md`](interface.md).

The SSD titlebar belongs to the compositor (ADR 0020) and uses Tengen: a top bar of 32 logical px, 14 px traffic lights (radius 4) with 9 px spacing and a window radius of 16.

## What is drawn

- Bar background `titlebar_bg` (focused/unfocused) and a 1 px hairline at the bottom.
- Close/minimize/maximize traffic lights on the left, with `traffic-close`/`traffic-min`/`traffic-zoom`; gray when the window has no focus.
- Centered title, rasterized with `cosmic-text` and colored with `title.focused`/`title.unfocused`.
- Frame for all windows: continuous corners with their bar's radius: 16 (titlebar or SSD), 20 (compact), 26 (unified, CSD without a hint and dialogs) or 15 (utility) (ADR 0059), also on shell alerts (`malaysia-dialog` layer), shadow (`shadow.frag`), outer border (`border.frag`) and inner hairline. Fullscreen removes the frame; maximized keeps it (ADR 0037).
- Dialogs use the same frame and are centered over the parent window.
- A child window (`set_parent`) always stays above its parent and its other ancestors, as [xdg-shell](https://wayland.app/protocols/xdg-shell#xdg_toplevel:request:set_parent) asks: raising the parent raises its children with it, in the order they had (`raise_order`).
- In dark mode, every window, dialog and popup gets the Tahoe inner border: `panel-inner-edge` (22 %) at the top and bottom, blending into `panel-inner` (9 %) on the sides over the corner radius.
- The traffic lights are square (radius 4) or round following `appearance.traffic_light_shape`.

Only SSD windows receive a titlebar (ADR 0014/0019). GTK/Libadwaita and the clients that request CSD keep their chrome; `window.decorations = "force_server"` can force the bar on third-party apps (at the risk of duplicating it). Telegram (Qt) may need its frame disabled in Settings › Advanced; with `window.qt_server_decorations = true` (default) the compositor requests server-side.

## Our own apps

The compositor recognizes a BarrierKit app because it binds `infinity_window_style_v1`, whatever its `app_id` (ADR 0035). The app requests its mode through `xdg-decoration` before the first commit: `Client` draws its own 44 px bar (apps with a toolbar/sidebar); `Server` leaves the 32 px bar to the compositor (simple utilities). `force_server`/`force_client` only affect third-party apps.

## Moving

While dragged, a window does not go behind the menubar: its top edge stops at the work area (`State::workspace`, which subtracts the top exclusive zones), as on macOS. For a CSD window the visible edge counts: the shadow the app draws inside its surface (`window_geometry`) is subtracted. The sides and the bottom edge may leave the screen.

## Edge resize

Normal SSD windows (not maximized or fullscreen) are resized from an outer band: `window.resize_border` (10 px) on the sides and an «L» of `window.resize_corner` (20 px) at the corners, with 2 px of tolerance inward. The hit test walks the stack: the top window wins and its content blocks the band of a lower one. The traffic lights are checked before the resize. CSD windows handle their own edges.

## Color

`appearance.window_color` tints the titlebar: `neutral` does not tint, `accent` uses the accent (6 %/8 %) and `wallpaper` the dominant color (7 %/9 %). `traffic_lights` forces `standard`, `accent` or `wallpaper`; `window_tint_scope = "desktop"` tints the menubar and the dock (20 %/26 %).

## Buttons and double click

Close closes, minimize leaves the tile in the dock, maximize saves the rectangle and sends `maximized`; repeating restores. The traffic lights show their glyph on hover and darken only the pressed button. A double click (400 ms, 4 px) runs `window.double_click_titlebar`.

## Panels

An app declares a window a panel with `infinity_window_style_v1.set_panel` (version 3), as barrierkit's utility windows do. The compositor reproduces AppKit's `NSPanel`:

1. **It is never the main window.** While the panel holds the keyboard, the topmost window of its app that is not a panel stays `activated` and is the one published as active in foreign-toplevel, so the menu bar keeps its menus (`shown_active`).
2. **`hides_on_deactivate`:** the panel hides while the active window belongs to another client or there is none, and comes back at the same place and stacking when a window of its app becomes active. It is its own hiding reason, separate from minimizing (`set_deactivated`). Focus on a shell layer does not count as changing app.
3. **`floating`:** the panel stays above every window that does not float (`raise_order`).
4. **`key_only_if_needed`:** neither mapping nor a press gives it the keyboard; it only rises. The app asks for it with an xdg-activation token made from that press, which is accepted because the keyboard stays on the same client.
5. When the window with the keyboard closes, focus goes to the topmost window that is not a panel (`focus_fallback`).

Sources: [`hidesOnDeactivate`](https://developer.apple.com/documentation/appkit/nswindow/hidesondeactivate), [`isFloatingPanel`](https://developer.apple.com/documentation/appkit/nspanel/isfloatingpanel), [`becomesKeyOnlyIfNeeded`](https://developer.apple.com/documentation/appkit/nspanel/becomeskeyonlyifneeded) and the `activated` state of [xdg-shell](https://wayland.app/protocols/xdg-shell#xdg_toplevel:enum:state:entry:activated), which does not imply holding the keyboard.
