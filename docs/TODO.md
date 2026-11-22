# Done and to do, 2026-11-22

**Goal:** record what the barrierkit and Malaysia sessions of 2026-11-22
finished, how it was verified, and what is left, so the next session starts
from one page.
**Reader:** whoever continues Malaysia or barrierkit.
**Scope:** blur, glass, corners, Settings and dependencies. Details live in the
barrierkit repository (`docs/research/` and `docs/decisions.md`); this page
only points to them.

## 1. State

| Item | Value |
|---|---|
| Malaysia `main` | `6d4521ea` |
| barrierkit pin | `ccf65557` in `Cargo.toml`, `Cargo.lock`, `flake.nix` and `flake.lock`; the flake hash was computed and checked with Nix |
| Verified | `164b25ca` with that pin and software EGL: fmt, clippy `-D warnings`, links, deps and tokens pass; 717 tests pass, 0 fail, 5 ignored |
| barrierkit `main` | verified at `0e1be0ff`: fmt, tokens, links, recipes, css, clippy and tests pass |
| Not run | `nix build` and `nix flake check` on x86_64, `check-client-dispatch.sh`, any run on a real session or a GPU |

## 2. Done

### Blur is composed by the compositor

Decision: barrierkit ADR 0091. The compositor composes every blur, as macOS's
render server does; an app declares a material and a style.

- Measured pipeline of macOS 27.2.1: Gaussian blur on a backdrop reduced by 8
  (sigma 27.6 px for AppKit materials, 59.2 px for SwiftUI), saturation, veil,
  cap or floor, in the display's own encoded space. The Dual Kawase is gone.
  Values: barrierkit `docs/research/macos-materials.md` section 10 and
  `macos-blur.md`; tokens `components.material`, `shell-glass` and
  `liquid_glass` in `ctdesign`.
- `df9efc81` the measured pipeline for window backdrops, dialogs, the dark Dock,
  menus, popovers and notifications, with the Reduce Transparency and
  Increase Contrast gates.
- `25167ce6` and `11e009e7` Infinity version 10: `set_backdrop` and
  `set_scroll_edge` for BarrierKit toplevels, popups, layer surfaces and
  overlapping chromes; window capture includes `within_window` material.
- `d9958794` and `164b25ca` the Liquid Glass lens, rim and depth blur for
  requested glass in every state and in popup frames.
- `f7da50e9` the Liquid Glass slider (`appearance.liquid_glass_tint`) drives
  menus and popovers and redraws live. The Dock and the menu bar do not follow
  it, as measured.
- barrierkit `dc9656b2` the bars and the sidebar request materials from the
  compositor and the kit's Skia blur of them is deleted.

### Corners

Measured on macOS 27.2.1 (barrierkit `docs/research/macos-corners.md`, ADR 0097):
every window kind 16 (silhouette 16.8, smoothing 0.745), alert 26, popover
about 20, menu 12.5, tooltip 5.2 circular, Dock 0.272 of its thickness,
banner 20.3, Notification Center panel 20.5, Control Center module 24.2,
Spotlight 24.5, Apps tile 31.6.

- `68611ff9` windows, sheets, menus and popovers.
- `7ddfe9b9` Dock, notifications, launcher and nested shapes with the
  concentric rule.
- barrierkit `ccf65557` the kit draws the continuous corner and the measured
  radius of every control (33 goldens changed).

### Settings and dependencies

- `dcee2803` Settings follow macOS 27: order, names and es-419 texts of
  Appearance and Accessibility Display. `appearance.glass_blur` is retired: a
  file that has it still loads and the next write drops it. Restoring it is one
  commit (barrierkit ADR 0096, Malaysia ADR 0092).
- `21cc077f` the Liquid Glass tint slider returns to Settings.
- `1a45e115` the macOS 27 traffic lights.
- `97f10790` ferricast comes from SergioRibera/ferricast, pinned at merge
  commit `f1c10b45` (pull request 7).
- `6d4521ea` the flake pins the same barrierkit as Cargo.

### Kit features not adopted by Malaysia code yet

Owned text (`Cow`), modifiers on every control, `Label`, `Button::label`,
views in `List` rows and `Table` cells, ambient `.disabled`, environment
values, focus by name, scenes and standard menus, `Spacer`, `Grid`, 151
symbols with weight and scale, `keyboard_shortcut`. They compile into
Malaysia but only its Settings and shell use what they already used.

## 3. To do, in order

1. Protocol version 11 in barrierkit: `tooltip = 3` in the popover shape and,
   per rectangle, material, radius, tint, interaction (none, hovered,
   pressed) and group with spacing, so `GlassContainer` and per-control
   materials can leave Skia. Shape awaits the owner.
2. Compositor serves version 11 and draws the tooltip corner (5.2 circular).
   Raise `VERSION` only when everything it advertises is served.
3. The kit moves `GlassContainer` and `ShapeStyle::Material` to the
   compositor and deletes its last Skia backdrop blur.
4. Measure what the Liquid Glass slider does to notification cards, banners,
   Control Center, Spotlight, Apps and the Spaces bar, and the light scheme of
   the Dock, menus and notifications: the user runs
   `~/src/probes/global-states.sh run lg-clear`, `run lg-tinted` or `all`
   (about 244 minutes for `all`). Until then those surfaces keep today's look.
5. Measure at 2x (state `hidpi-720` of the same runner).
6. Measure the blur pass on a real GPU (barrierkit
   `docs/research/blur-backdrop-design.md` section 10). Pass: median at most
   1.0 ms at 2x for a 96 px band, stop above 2.5 ms, no frame of 40 at 8.33 ms
   or more. Everything above ran on llvmpipe.
7. Settings parity: remaining panes against barrierkit
   `docs/research/macos-settings.md` (window 740 x 803 in English, sidebar 231;
   the test harness draws the sidebar over the content, fix it before
   comparing widths).
8. Older Malaysia fixes still open: the accent split (`resolved.accent` against
   `control_accent`: GTK bridge, Settings previews, folders ignore the `accent`
   key), visible text without `i18n::intern` in `malaysia-settings`, `.disabled`
   and `Spacer` instead of wrapper nodes, EGL tests that silently pass without
   EGL, design constants in the compositor outside `ctdesign`, per-frame
   allocations in the blur path, stale lines in docs.

## 4. Decisions for the owner

- barrierkit ADR 0086 to 0101 each assume choices marked pending: names `row`
  and `cell`, opt-in `Commands`, layout without a constraint solver, the
  Liquid Glass slider model, the corner smoothing.
- Shape of protocol version 11 and whether the chrome materials of the sidebar
  and the Dock enter it.
- Window backdrops of `ext-background-effect` clients use
  `under_window_background` with its veil; change only `look::windows` to use
  the blur alone.
- Retirement of `appearance.glass_blur`.

## 5. Rebuild

```sh
git pull --ff-only
./dev.sh build
./dev.sh demo --dark
```

Cost of the blur on the machine's GPU:

```sh
cargo test -p malaysia-compositor --lib blur::timing -- --ignored --nocapture --test-threads=1
```
