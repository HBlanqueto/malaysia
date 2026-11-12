# 0071 — Background blur and ext-background-effect-v1

Status: accepted (2026-11-11). Phase 1 is implemented (2026-11-11); the
rest is in «Plan».

## Goal and scope

This ADR fixes how the compositor blurs what is behind a surface. It covers
two blur modes, the `ext-background-effect-v1` protocol, which surfaces get
blur, the blur algorithm and how it is configured. It starts when a surface
commits a blur region and ends when the blurred backdrop reaches the frame.
How the shell and the kit draw their translucent materials is out of scope;
only the region they request is in. Readers: whoever works on the render path
of `malaysia-compositor`, on `grade-graphics` or on the kit's materials.

Labels: [S] specification or upstream code at the cited revision, [W] upstream
wiki or release notes, [I] inferred from code, not documented upstream.

## Context

### What Malaysia has today

1. The compositor draws no blur. ADR 0020 asks for blur on every window, CSD
   or SSD; this ADR narrows it to the regions clients request. ADR 0038 leaves it to the compositor once
   `appearance.reduce_transparency` is `false`. That switch defaults to
   `true`, so the desktop is opaque.
2. `effects.blur.enabled` and `effects.blur.radius` appear in
   [`options.md`](../configuration/options.md) as «pending»; `config.rs`
   does not read them.
3. BarrierKit blurs inside its own window with Skia (`bk/src/glass.rs`). That
   is macOS's `withinWindow` blending. Blurring what is behind the window,
   macOS's `behindWindow`, needs the compositor, because no client sees
   other clients' pixels.
4. The scene is built front to back in `scene_elements.rs`
   (`build_output_elements`). The wallpaper is the `Background` layer of
   `malaysia-wallpaper` (ADR 0049). It is static: it changes only when the
   selection or the output changes. A focus space draws `SPACE_GAP_COLOR`
   instead of the back layers (ADR 0051).

### The protocol

`ext-background-effect-v1` is the standard way for any client, whatever its
toolkit, to ask for blur behind a part of its surface
([spec](https://wayland.app/protocols/ext-background-effect-v1)) [S]:

1. `ext_background_effect_manager_v1.get_background_effect(surface)` creates
   one effect object per `wl_surface`; a second one is the
   `background_effect_exists` error.
2. `set_blur_region(region)` takes a surface-local `wl_region`, clipped to
   the surface. It is double-buffered and applies on the next
   `wl_surface.commit`. A null region removes the blur; `destroy` removes it
   on the next commit.
3. The manager sends `capabilities` on bind and «every time they change».
   The only flag is `blur = 1`. When a capability goes away the compositor
   stops the effect, even if the client had set it.
4. The client controls only the shape. «The blur algorithm is subject to
   compositor policies»: strength, and whether the blur shows other windows
   or only the wallpaper, are the compositor's choice.

It comes from KDE's `org_kde_kwin_blur` (in use since 2015). It reached
staging in wayland-protocols 1.45, and the `blur` value was fixed from 0 to
1 in 1.46 (commit `efbc060`) [S].

Who speaks it (November 2026):

- **Compositors:** KWin 6.7, Mutter 51 and niri v26.04. cosmic-comp has a
  draft ([pop-os/cosmic-comp#2179](https://github.com/pop-os/cosmic-comp/pull/2179)).
  wlroots, Hyprland and Sway do not [S].
- **Clients:** foot 1.26 (`blur=yes`), kitty 0.46.2 (`background_blur`),
  Quickshell 0.3 (`BackgroundEffect.blurRegion`), Dank Material Shell,
  Noctalia and Vicinae. Ghostty and winit plan to add it. GTK and QtWayland
  have no built-in support yet [W]. KDE apps use it through KWindowSystem
  once KWin offers it.

### Smithay at the pinned revision

`65aa86fa` (ADR 0028) already has everything needed [S]:

1. `smithay::wayland::background_effect`, added in
   [Smithay/smithay#1860](https://github.com/Smithay/smithay/pull/1860):
   `BackgroundEffectState::new::<State>`, the trait
   `ExtBackgroundEffectHandler` (`capabilities`, `set_blur_region` and
   `unset_blur_region`) and the cached state
   `BackgroundEffectSurfaceCachedState { blur_region }`. There is no delegate
   macro; it uses `delegate_dispatch2!`.
2. The handler runs when the request arrives, not on commit. The committed
   region is read from the cached state (`.current()`).
3. `capabilities` is sent only on bind (`dispatch.rs`). Smithay has no call
   to send it again.
4. `Element::is_framebuffer_effect` and `RenderElement::capture_framebuffer`
   ([Smithay/smithay#1910](https://github.com/Smithay/smithay/pull/1910)) let
   a render element read the pixels already drawn below it. The damage
   tracker calls `capture_framebuffer` again whenever damage below overlaps
   the element.
5. The `wayland-protocols` crate at 0.32.13 has the bindings under the
   `staging` feature. The workspace already enables it for clients.

### How niri does it

niri added blur in v26.04
([niri-wm/niri#3483](https://github.com/niri-wm/niri/pull/3483); notes in
[Window Effects](https://github.com/niri-wm/niri/wiki/Window-Effects)).
Read at `7f81a7f7` (2026-11-10):

1. **Two modes per surface.** *xray* blurs only the `Background` layer
   behind the window and ignores the windows in between. *Non-xray* blurs
   everything already drawn below. xray is the default «since it's cheaper»
   (`render_helpers/background_effect.rs`); non-xray is marked experimental
   [S][W].
2. **xray caches the blur.** Each output keeps an offscreen buffer
   (`render_helpers/xray.rs`, `effect_buffer.rs`). Every frame it renders
   the `Background` layer surfaces through its own `OutputDamageTracker`; it
   blurs again only when that tracker reports damage. Each window then draws
   the sub-rectangle of the cached texture under its screen position. Moving
   or animating a window costs one textured quad. «The wallpaper changes
   very rarely», so the blur is computed about once per wallpaper [S][W].
3. **Non-xray reads the framebuffer.** `framebuffer_effect.rs` uses
   `capture_framebuffer`: `glBlitFramebuffer` of the pixels under the
   element, then a full blur, every time anything below changes. It needs
   blit support; without it niri turns non-xray off (`e19df12c`). It vanishes
   while a window is drawn through an offscreen (open/close animations,
   dragging), and the cursor is never part of it [S][W].
4. **Pop-ups default to non-xray**, because «they're always on top of
   something» (`window/mapped.rs`) [S].
5. **The protocol turns blur on.** A non-empty region enables blur with the
   default mode; window and layer rules can force `blur` or `xray` off. The
   protocol region keeps the exact shape the client asks for; blur set by a
   rule follows the geometry and its corner radius [S].
6. **Algorithm.** Dual Kawase in GLES 2 (`render_helpers/blur.rs`,
   `shaders/blur_{down,up}.frag`): each pass halves the texture, then the
   same number of passes scales it back up. Defaults: `passes 3`,
   `offset 3`. A post-process shader adds saturation (Rec. 709 luminance)
   and white noise (`hash12`, to hide banding), and clips to a rounded
   rectangle. The blur is drawn at full alpha, so window opacity does not
   fade it [S].
7. Stacked non-xray windows compound: the upper one captures the lower one's
   blur [I].

## Decision

1. **Two modes, mixed by default.**
   1. Blur is drawn only when `appearance.reduce_transparency` is `false`
      and `appearance.contrast` is `false` (ADR 0038, 0066). There is no
      separate on/off key: `effects.blur.enabled` goes away.
   2. There are two modes:
      - `"wallpaper"`: blur only the wallpaper behind the surface, like
        niri's xray. Windows behind are not seen. This is cheap, because the
        wallpaper is blurred once and every surface reuses it.
      - `"behind"`: blur everything already drawn below the surface,
        windows included. This is true transparency, and it costs one
        capture and one blur per surface each time something below changes.
   3. The compositor picks the mode for each surface by its role; the client
      never picks it, because the protocol has no mode. `"behind"` goes only
      where a surface usually opens over a window:

      | Surface | How the compositor knows it | Mode |
      |---|---|---|
      | Pop-up of any client (menus, popovers) | `xdg_popup` | `"behind"` |
      | BarrierKit dialog, sheet or alert | `is_kit_client` (ADR 0035) and a parent or `xdg_dialog_v1` | `"behind"` |
      | BarrierKit window | `is_kit_client`, no parent | `"wallpaper"` |
      | Any other toplevel, dialogs included (GTK, Qt, foot…) | not a kit client | `"wallpaper"` |
      | Dock and notifications | layer namespace `malaysia-dock`, `malaysia-notifications` | `"behind"` |
      | Menu bar | layer namespace `malaysia-menubar` | `"wallpaper"` |
      | Lock screen | `ext_session_lock_surface_v1` | `"wallpaper"` |
      | Any other layer surface | layer-shell | `"wallpaper"` |

   4. `appearance.blur_behind` is `"mixed"` (default), which applies the
      table. `"wallpaper"` turns every row into `"wallpaper"`: the cheapest
      blur, for a weak GPU or for battery. `"windows"` turns every row into
      `"behind"`. With no blit support (step 7.2) the compositor behaves as
      `"wallpaper"`.
2. **The protocol is the standard way in.** The compositor exposes
   `ext_background_effect_manager_v1` through Smithay's module, version 1.
   Toplevels, layer surfaces, pop-ups and lock surfaces take the region.
   Subsurfaces, the cursor and drag icons do not, like niri. The protocol
   region keeps its exact shape, clipped to the window geometry.
3. **Capabilities tell the truth.**
   1. `capabilities` sends `blur` only while blur is drawn (step 1.1).
   2. When that changes, every bound manager gets the new value. Smithay
      sends it only on bind, so phase 1 adds that call upstream; until then
      a change applies to new binds only.
   3. Clients that already set a region keep it. The compositor stops drawing
      it, as the spec requires.
4. **Only what a client asks for.** A surface gets blur only in the region
   it sends through the protocol, and the mode decides whether that region
   shows true transparency (`"behind"`) or only the wallpaper. By default a
   window that sends nothing gets no blur, even if its buffer has alpha.
   1. **General blur, off by default.** `appearance.blur_all_windows` (default
      `false`) blurs every toplevel that sends no region: behind its
      geometry, clipped to its frame radius (ADR 0059), minus the opaque
      region it declares. Those windows are not kit clients, so they get
      `"wallpaper"` (step 1.3). It reaches
      GTK and Qt apps with a translucent theme before their toolkits speak
      the protocol.
   2. It is off by default because it costs GPU on every window: an app
      that looks opaque but declares no opaque region pays for a blur nobody
      sees, and an app translucent on purpose gets a blur it did not ask for.
5. **The shell and the kit use the protocol too.** With reduced
   transparency off, `malaysia-shell` (menubar, dock, menus, notifications)
   and BarrierKit windows draw their translucent material tokens and send
   the material's shape as the blur region. There is no blur request in
   Infinity: one path for every client.
6. **What `"wallpaper"` samples.**
   1. Per output, the `Background` layer surfaces are rendered into an
      offscreen texture through their own `OutputDamageTracker` and blurred
      only when it reports damage, or when the size, scale or blur tokens
      change.
   2. Each surface draws the part of that texture under its on-screen
      rectangle, so moves, the focus transition and space slides cost a
      textured quad.
   3. In a focus space, the backdrop is `SPACE_GAP_COLOR` and the blur
      samples that color.
   4. The `Bottom` layer is not part of it.
   5. Nothing is rendered or blurred while no visible surface needs it.
   6. While the session is locked the back layers are not drawn, but the
      lock screen samples the same cache.
7. **What `"behind"` does.**
   1. It uses Smithay's framebuffer-effect elements: `capture_framebuffer`
      blits the pixels below the element, blurs them, and draws them under
      the surface.
   2. Without blit support, the compositor logs a warning and falls back to
      `"wallpaper"`.
   3. Surfaces drawn through an offscreen (`popup_fade`, `theme_fade`,
      `display_filter`) draw their effect element outside the snapshot, with
      the same alpha, so the blur does not vanish during the fade.
   4. A screencast or capture that renders across frames keeps its damage
      tracker, because the capture cache lives in the tracker.
8. **One algorithm, in `grade-graphics`.** Dual Kawase in GLES 2 shaders
   (`blur_down.frag`, `blur_up.frag`), with the rounded clip
   (`window_shape.glsl`) applied when the blur is sampled. Saturation and
   noise come with the calibration of phase 4. The
   blur is drawn at full alpha below the surface. The window's own alpha
   does not fade it.
9. **Strength is a design value.** `ctdesign::BACKGROUND_BLUR_PASSES` and
   `BACKGROUND_BLUR_OFFSET` set it, not user options; `effects.blur.radius`
   goes away. The first values are 3 and 3, like niri's defaults, until
   they are measured against macOS 26 (phase 4). Light and dark share them.
10. **Configuration.** `blur_behind` and `blur_all_windows` are the only
    new keys. They live in `appearance.domain` (`ctappearance::AppearanceConfig`),
    like the other Accessibility › Display options (ADR 0066), and are read
    live. Each key and its System Settings control arrive with the phase that
    reads it: `blur_all_windows` with phase 2, `blur_behind` with phase 3.
    The control dims while «Reduce transparency» is on.

## Plan

1. **Protocol and `"wallpaper"`. Done (2026-11-11).** Smithay global and
   handler (`handlers.rs`); the committed region read from the cached state
   (`blur.rs`); the tokens in barrierkit; the shaders in `grade-graphics`;
   the wallpaper cache. Windows, alert layers and plain layer surfaces get
   it; every surface draws `"wallpaper"` until phase 3. Lock surfaces got it
   with ADR 0074: their rectangles are drawn as capsules, the shape of the
   password field. Pending: the upstream Smithay PR to resend
   `capabilities`, and pop-ups (phase 3). Test: `./dev.sh run` with foot `colors-dark.blur=yes` and
   `alpha=0.6`, or kitty `background_blur 1`, and
   `reduce_transparency = false` in `~/.config/malaysia/appearance.domain`.
2. **The shell, the kit and general blur.** The shell and the kit send
   their material regions (barrierkit change); `appearance.blur_all_windows`
   (step 4.1 of «Decision»).
3. **`"behind"` and pop-ups.** Framebuffer-effect elements, the blit
   fallback and the fades. The Dock got `"behind"` first (2026-11-12) without
   them: `blur.rs` keeps a second cache like the wallpaper's, rendered from
   the windows and the `Background` layer through its own
   `OutputDamageTracker`, so it blurs again only when something below changes.
   Pending: the other rows of the table.
4. **Calibration.** The blur values, saturation and noise, measured on a
   Mac with a solid and a striped wallpaper.

Each phase updates [`options.md`](../configuration/options.md) and
[`roadmap.md`](../roadmap.md) in its own commit.

## Consequences

- Any app that asks for blur, whatever its toolkit, gets it in Malaysia: KDE
  apps, foot, kitty and Quickshell-based shells today, GTK and Qt once they
  adopt the protocol.
- Most surfaces use `"wallpaper"`, which costs about one blur per wallpaper
  change per output. `"behind"` costs one blur per blurred surface for every
  frame in which something below it changes, so it stays on pop-ups, kit
  dialogs, the dock and notifications.
- In `"wallpaper"` mode a translucent window shows the wallpaper, not the
  windows between, which is what macOS shows for window backgrounds. A window
  that switches mode during an animation loses what was behind it at once
  [S].
- Stacked surfaces in `"behind"` compound their blur [I].
- GTK and Qt apps with a translucent theme are not blurred until their
  toolkit speaks the protocol, unless the user turns on general blur.
- Capabilities do not change for clients already bound until the Smithay
  change lands.

## Rejected alternatives

- **One mode for every surface, chosen by the user.** A menu or a dialog over
  a window needs `"behind"`; making every window pay for it to get that is
  the cost the mixed table avoids.
- **Only `"behind"`.** It is the true transparency, but it costs a capture
  and a blur per window on every change below, and it breaks during
  offscreen animations. niri keeps it experimental for those reasons.
- **Only `"wallpaper"`.** Menus over windows would show the wallpaper, and
  users who want real transparency would have no way to get it.
- **General blur on by default.** It would reach GTK and Qt today, but it
  costs GPU on every window, for the reasons in step 4.2 of «Decision».
- **KDE's `org_kde_kwin_blur`.** It is the predecessor: no capabilities, no
  errors and its own commit request. KWin is moving to the standard
  protocol, and niri implemented the KDE one but did not merge it.
- **Blur strength set by the client or the user.** The protocol leaves it to
  the compositor, and design values live in `ctdesign` (ADR 0011).
- **A blur request in Infinity.** Our clients would need a private path for
  something every toolkit can already ask for in the standard way. The
  role and `is_kit_client` already give the kit both modes.
- **Letting GTK and Qt dialogs use `"behind"`.** Third-party apps get the
  cheap mode; only pop-ups, which always open over something, are the
  exception.
