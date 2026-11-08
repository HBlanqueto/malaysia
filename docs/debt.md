# Open technical debt

Open findings, checked again on 2026-11-08 (Remake-Redo); fixed findings and those of the toolkit moved to barrierkit are not listed. Missing features live in [`roadmap.md`](roadmap.md#not-in-a-block-yet). The 2026-11-03 audit that found most of them is in git history (`git show 7fa3d19:docs/audit-2026-11-03.md`).

| id | where | what | batch |
|---|---|---|---|
| P1-05 | `crates/malaysia-compositor/src/frame_clock.rs:108` | Unneeded `try_clone` of the fence; if it fails, the release retention is lost. | L1b |
| P1-13 | `crates/malaysia-compositor/src/session.rs:630` | `reset_dead_cursor` in `session.rs` even though `cursor.rs` exists. Deferred: move only. | L3e |
| P5-09 | `crates/malaysia-shell/src/menubar/mod.rs:265-278` | `prepare_kit_env` deletes `MALAYSIA_ORIG_*` from the child; the launcher cannot restore them. | L1i |
| P5-10 | `crates/malaysia-shell/src/menubar/focused.rs:230,639`, `dock.rs:49` and `dialogs.rs:290` | Ids built from a list index; opening/closing windows with the menu open can act on another one. | L1h |
| P5-12 | `crates/malaysia-shell/src/lock.rs:32,264,318-331` | `theme_revision` is stored but never read. | L2b |
| P5-17 | `crates/malaysia-shell/src/launcher.rs:93-110,146,156,369` | Clones entries and icons on every render. | L4e |
| P5-19 | `crates/malaysia-shell/src/menubar/dbusmenu.rs:317-327` | `ItemsPropertiesUpdated` does not invalidate `full`; Help sees stale data. | L1i |
| P5-20 | `crates/malaysia-shell/src/menubar/mod.rs:76-78` and `dock.rs:95-109` | Dead `ActiveWindow.title` and a dock with its own parser. | L2b |
| S-01 | Smithay `65aa86fa`, `wayland/shell/xdg/mod.rs` (`PopupSurface::pre_commit_hook`) | `ERROR surface missing from known popups` when closing a GTK popover. GTK unmaps the `xdg_popup` (commit without a buffer: Smithay resets the role and `parent` becomes `None`), destroys it (it leaves `known_popups`) and commits the `wl_surface` again. The hook is still registered, sees `parent == None` and, as it does not find the popup, only logs the error. No protocol error is sent and nothing visible changes. The fix belongs in Smithay (do not run the hook after the role is destroyed) or in bumping its revision; a log filter does not hide it because it would hide real errors of the module. | upstream |
| S-02 | Smithay `65aa86fa`, `src/backend/drm/compositor/mod.rs` | `DrmCompositor` does not set the planes' `pixel blend mode` property (grep `blend` in `src/backend/drm`: 0). On Intel i915, overlays with alpha blend wrongly. That is why `debug.overlay_planes` is off by default. nongio's fork fixes it in [`734ebe91`](https://github.com/nongio/smithay/commit/734ebe91) and [`33b10703`](https://github.com/nongio/smithay/commit/33b10703); it is only a reference: the fix belongs in upstream Smithay or in bumping its revision. | upstream |
| S-03 | `crates/malaysia-compositor/src/virtual_input.rs` | Switching back from a virtual keyboard's keymap restores the configured keymap but not the group or locked modifiers, unlike niri (`src/handlers/virtual_keyboard.rs`). The fix is per-device xkb state. | L6 |
| G-01 | `crates/xdg-desktop-portal-malaysia/src/gtk.rs:358-381` | `gtk_bridge` is read apart from barrierkit's `AppearanceConfig`; move it there. The base Adwaita backgrounds are pinned to libadwaita 1.6+. | L2b |
| L3 | `session/tests.rs`, `input/tests.rs`, `handlers/capture_lock_tests.rs`, `gtk.rs` tests | Four test harnesses that each build a `State`; unify them. None maps a window with a buffer, which the focused-window identity test needed after `bd839e2` (removed in `a450a79`). | L3 |
| R-01 | `crates/malaysia-compositor/src/session/tests.rs` (`a_failed_create_immed_is_drained_and_posts_the_protocol_error`) | Hangs on llvmpipe (no `EGL_ANDROID_native_fence_sync`); `./dev.sh check` needs a GPU to finish. `check-client-dispatch.sh` stops for the same reason. | — |
| R-02 | `crates/malaysia-wallpaper/src/render.rs` (`cover_matches_reference_at_1280x800`, `minified_quadrants_keep_orientation`) | Fail on lavapipe with wgpu 24 and 30 alike; not checked on a hardware GPU. | — |
| R-03 | `crates/malaysia-compositor/src/input.rs` | 1598 lines after the split; pointer motion (`pointer_motion_raw`, about 180 lines) and the chrome pointer are the next candidates (audit §6.2). | — |
| R-04 | `NOTICE` | `bandura.jpg` has no origin or licence; the 31 `share/wallpapers/solid-colors/` PNGs carry macOS names and no origin. The author's call. | — |
