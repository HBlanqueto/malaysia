# Open technical debt

Open findings against `d0bba82` (2026-11-03); fixed findings and those of the toolkit moved to barrierkit are not listed. The [2026-11-03 audit](auditoria-claude-2026-11-03.en.md) collects the remaining pending items.

| id | where | what | batch |
|---|---|---|---|
| P1-05 | `crates/malaysia-compositor/src/session.rs:1826` | Unneeded `try_clone` of the fence; if it fails, the release retention is lost. | L1b |
| P1-11 | `crates/malaysia-compositor/src/session.rs:1095-1424` | Chrome helpers in `session.rs` even though `chrome.rs`/`effects.rs` exist. Deferred: move only. | L3c |
| P1-12 | `crates/malaysia-compositor/src/session.rs:1425-1560` | Capture and window in `session.rs` even though `capture.rs` exists. Deferred: move only. | L3d |
| P1-13 | `crates/malaysia-compositor/src/session.rs:1764` | `reset_dead_cursor` in `session.rs` even though `cursor.rs` exists. Deferred: move only. | L3e |
| P5-09 | `crates/malaysia-shell/src/menubar/mod.rs:265-278` | `prepare_kit_env` deletes `MALAYSIA_ORIG_*` from the child; the launcher cannot restore them. | L1i |
| P5-10 | `crates/malaysia-shell/src/menubar/focused.rs:230,639`, `dock.rs:49` and `dialogs.rs:290` | Ids built from a list index; opening/closing windows with the menu open can act on another one. | L1h |
| P5-12 | `crates/malaysia-shell/src/lock.rs:32,264,318-331` | `theme_revision` is stored but never read. | L2b |
| P5-17 | `crates/malaysia-shell/src/launcher.rs:93-110,146,156,369` | Clones entries and icons on every render. | L4e |
| P5-19 | `crates/malaysia-shell/src/menubar/dbusmenu.rs:317-327` | `ItemsPropertiesUpdated` does not invalidate `full`; Help sees stale data. | L1i |
| P5-20 | `crates/malaysia-shell/src/menubar/mod.rs:76-78` and `dock.rs:95-109` | Dead `ActiveWindow.title` and a dock with its own parser. | L2b |
| S-01 | Smithay `023b78b1`, `wayland/shell/xdg/mod.rs` (`PopupSurface::pre_commit_hook`) | `ERROR surface missing from known popups` when closing a GTK popover. GTK unmaps the `xdg_popup` (commit without a buffer: Smithay resets the role and `parent` becomes `None`), destroys it (it leaves `known_popups`) and commits the `wl_surface` again. The hook is still registered, sees `parent == None` and, as it does not find the popup, only logs the error. No protocol error is sent and nothing visible changes. The fix belongs in Smithay (do not run the hook after the role is destroyed) or in bumping its revision; a log filter does not hide it because it would hide real errors of the module. | upstream |
| S-02 | Smithay `023b78b1`, `src/backend/drm/compositor/mod.rs` | `DrmCompositor` does not set the planes' `pixel blend mode` property (grep `blend` in `src/backend/drm`: 0). On Intel i915, overlays with alpha blend wrongly. That is why `debug.overlay_planes` is off by default. nongio's fork fixes it in [`734ebe91`](https://github.com/nongio/smithay/commit/734ebe91) and [`33b10703`](https://github.com/nongio/smithay/commit/33b10703); it is only a reference: the fix belongs in upstream Smithay or in bumping its revision. | upstream |
| S-03 | `crates/malaysia-compositor/src/virtual_input.rs` | Switching back from a virtual keyboard's keymap restores the configured keymap but not the group or locked modifiers, unlike niri (`src/handlers/virtual_keyboard.rs`). The fix is per-device xkb state. | L6 |
| G-01 | `crates/xdg-desktop-portal-malaysia/src/gtk.rs:358-381` | `gtk_bridge` is read apart from barrierkit's `AppearanceConfig`; move it there. The base Adwaita backgrounds are pinned to libadwaita 1.6+. | L2b |
| L3 | `session.rs:2126`, `input.rs:2325`, `handlers.rs:1024`, `gtk.rs:432` | Four test harnesses; unify them and add the PX-06 test. | L3 |
