718a7e4

What this repo does: MalaysiaDreams is a NixOS desktop made of a Rust/Smithay Wayland compositor plus its own shell (`wallpaper`, `menubar`, `dock`, `lock`, `launcher`), a small settings app and a portal backend. It draws server-side titlebars and effects for Qt/other apps, leaves GTK apps with their own chrome, and keeps user settings in per-domain TOML `.domain` files.

Load assumed: one local user, one or two outputs, 5–10 Wayland clients, the shell components and the session D-Bus. Nothing is multi-user or network-facing.

## Must fix

**PX-01. The lock cannot type uppercase or AltGr characters** (`crates/malaysia-shell/src/lock.rs:91`, `:204`)
- **What this is:** The lock inserts a character only when `event.modifiers.is_empty()`.
- **Problem:** `Shift+H` arrives as `Key::Character("H")` with SHIFT set, so it is dropped. Caps Lock and AltGr (needed for `@` on Spanish layouts) are dropped too. A password with any uppercase or `@` can never be typed.
- **Fix:** Pass `event.modifiers` into `handle_key` and ignore only CONTROL/META; allow SHIFT, CAPS_LOCK and ALT_GRAPH. Add a test for `Shift+H`.
- **If we skip it:** The normal user is locked out of their own session; the workaround is a VT kill.

**PX-02. Settings live in `malaysia-compositor/`, but every document says `malaysia/`** (`crates/vow-appearance/src/lib.rs:518`, `:536`; `crates/malaysia-compositor/src/active.rs:38`)
- **What this is:** `config_dir()` and `data_dir()` append `malaysia-compositor`; README:148, `docs/configuracion/opciones.md:27` and ADR 0043 all say `~/.config/malaysia` and `~/.local/share/malaysia`.
- **Problem:** A user who writes `~/.local/share/malaysia/wallpaper.domain` or `~/.config/malaysia/appearance.domain` sees no effect. Only `MALAYSIA_CONFIG_DIR`/`MALAYSIA_DATA_DIR` hide the mismatch in dev.
- **Fix:** Change the two joins to `base.join("malaysia")`, make `active.rs` call `vow_appearance::data_dir()` instead of keeping a copy, and tell users to `mv` the old dirs (ADR 0043 already says no auto-migration).
- **If we skip it:** All configuration is stored in an undocumented directory, and the docs point at files the code never reads.

**PX-03. Screen capture keeps running while the session is locked** (`crates/malaysia-compositor/src/handlers.rs:402`, `crates/malaysia-compositor/src/session.rs:1481`)
- **What this is:** `ImageCopyCaptureHandler::frame` queues every frame; window capture calls `build_window_elements` directly.
- **Problem:** After lock, an existing `ext-image-copy-capture` client still gets frames: output capture returns the lock screen, window capture returns the hidden window content. The compositor already clears clipboard/selection focus on lock, so this defeats that isolation.
- **Fix:** In `frame`, if `self.locked`, call `frame.fail(CaptureFailureReason::Unknown)` and return. Add one test that locks with a queued frame.
- **If we skip it:** Any app that opened a capture source before lock can read the desktop or other windows while the user believes the session is locked.

**PX-04. Editing `wallpaper.domain` never changes the wallpaper until restart** (`crates/malaysia-shell/src/wallpaper.rs:53`)
- **What this is:** `run()` loads the image once and builds `WallpaperApp` with it; there is no watcher or timer.
- **Problem:** `docs/arquitectura/wallpaper.md:60` promises “Editar wallpaper.domain lo cambia en vivo”; the code never re-reads the store. Changing the image or `scale` does nothing.
- **Fix:** Hold the image in a `State`, poll `wallpaper.domain` every 500 ms with `Timers` (the menubar already does this), and re-decode only when the file changes.
- **If we skip it:** A documented and “accepted” feature is false; users will think the wallpaper component or settings app is broken.

## Should fix

**PX-05. `window.domain` is not valid TOML for ordinary window titles** (`crates/malaysia-compositor/src/active.rs:59`)
- **What this is:** The store is built with `format!` and pastes the title/app_id between raw double quotes.
- **Problem:** A title like `He said "hi"` or one containing a newline produces invalid TOML. `menubar::load_active_window` and `dock::load_active_title` then fail, so the bar loses the app name and the dock loses its highlight. A newline can also inject fields.
- **Fix:** Serialize a small `#[derive(Serialize)]` struct with `toml::to_string` (already a dependency) instead of formatting by hand; test with `"` and `\n`.
- **If we skip it:** Browsers and editors with quotes or unsaved-document markers break the active-app display.

**PX-06. The active window stays stale after a title or app_id change** (`crates/malaysia-compositor/src/handlers.rs:733`, `crates/malaysia-compositor/src/input.rs:1604`)
- **What this is:** `window.domain` is written only on focus changes; `title_changed`/`app_id_changed` only update foreign-toplevel clients.
- **Problem:** A browser tab change or an app that sets `app_id` after mapping updates the live title but not the store. The dock compares the live title with the stored title (`dock.rs:44`), so its highlight disappears; the menubar misses a late app_id.
- **Fix:** Make `publish_active` `pub(crate)` and call it from both handlers when the changed window is the active one.
- **If we skip it:** The bar and dock show wrong or missing state for common apps.

**PX-07. Lock does not cancel or block a pending screenshot** (`crates/malaysia-compositor/src/state.rs:789`, `crates/malaysia-compositor/src/lock.rs:48`)
- **What this is:** `finish_screenshot` writes the request without checking `locked`, and `lock()` leaves `screenshot`/`screenshot_mode` untouched. The draw path saves any request even while locked.
- **Problem:** Start a region shot, let the idle timer lock the screen, then release: the compositor saves an image of the lock screen and the crosshair overlay survives into the unlocked session.
- **Fix:** In `finish_screenshot`, require `!self.locked` like `request_screenshot` already does; in `lock()`, clear `screenshot` and `screenshot_mode` (reuse `cancel_screenshot`).
- **If we skip it:** Lock-screen images land on disk and a stale selection overlay returns after unlock.

**PX-08. App-menu D-Bus paths are reused after a window closes** (`crates/barrier-kit/src/app_menu/server.rs:64-70`)
- **What this is:** The path is `/MenuBar/{menus.len()}`.
- **Problem:** With two windows (paths 0 and 1), closing 0 leaves length 1; the next window also gets path 1, object registration fails, and the menu is silently missing. The app must restart to recover.
- **Fix:** Scan `menus` for the first unused index, or use a monotonic counter; add a test that registers A, B, unregisters A, then registers C.
- **If we skip it:** Multi-window native apps lose menus after any window closes.

**PX-09. One thread per clipboard paste can exhaust the compositor** (`crates/malaysia-compositor/src/handlers.rs:575-588`)
- **What this is:** `send_selection` spawns a thread for every `receive` request; each thread blocks writing the PNG into the client’s pipe.
- **Problem:** A buggy or hostile client can call `receive` repeatedly without reading. Each call creates a blocked thread; thread creation eventually fails and panics the compositor. It only applies while the compositor owns the clipboard (screenshot to clipboard), but then any client can trigger it.
- **Fix:** Use one clipboard writer thread with a bounded queue and drop excess requests, or write through the event loop with non-blocking file descriptors.
- **If we skip it:** One app can take down the whole session while a screenshot is on the clipboard.

**PX-10. Deleting `wallpaper.state` never wakes the watcher** (`crates/vow-appearance/src/watch.rs:60-62`, `crates/malaysia-shell/src/wallpaper.rs:424`)
- **What this is:** When a wallpaper has no usable dominant colour, `write_state` removes `wallpaper.state`. The watcher subscribes only to `IN_CLOSE_WRITE` and `IN_MOVED_TO`.
- **Problem:** A delete is `IN_DELETE`, so the compositor and portal never learn that the old dominant colour is gone. Switch from an orange wallpaper to a gray one and the old accent stays until some other file event happens.
- **Fix:** Add `IN_DELETE` to the watcher flags and a test that removes the state file and expects a `Wallpaper` event.
- **If we skip it:** The desktop keeps tinting with a colour from an image that is no longer selected.

**PX-11. The riskiest lock rules have no test** (`crates/malaysia-shell/src/lock.rs:78-110`, `crates/malaysia-compositor/src/handlers.rs:402`, `crates/malaysia-compositor/src/state.rs:789`)
- **What this is:** Lock tests only type characters with no modifiers; capture-while-locked and screenshot-while-locked have no test at all.
- **Problem:** A test pressing `Shift+H` fails today, and a test locking with a queued capture or screenshot would expose findings 3 and 7. The suite stays green while these are broken.
- **Fix:** Add three small tests: SHIFT inserts `H`; `frame` fails while locked; `finish_screenshot` while locked sets no request.
- **If we skip it:** These exact regressions will come back unnoticed.

**PX-12. Window menu actions use stale list positions** (`crates/malaysia-shell/src/menubar/focused.rs:411-445`, `crates/malaysia-shell/src/dock.rs:45-52`)
- **What this is:** Menu and dock items encode the position in the toplevel list; the action re-reads the list later and uses that position.
- **Problem:** If a window opens or closes while a menu is open, a window-specific action (“Cerrar”, “Minimizar”, dock click) can hit a different window. This is easiest to hit from the dock or a focused-app menu.
- **Fix:** Carry a stable key (app_id plus title, or a compositor window id) and look it up when the action runs; if it is gone, do nothing.
- **If we skip it:** Rare wrong-window closes/minimizes; they look like a compositor bug.

## Nice to have

**PX-13. Infinity pair shortcuts still run while locked** (`crates/malaysia-compositor/src/input.rs:471-500`)
- **What this is:** `compositor_shortcut` returns arrange actions without checking `locked`; pair tiling uses visible windows, not the active window.
- **Problem:** On the lock screen, `Ctrl+Shift+Super+Left` tiles the hidden windows behind the lock. The lock client never sees the keys because the compositor consumes them.
- **Fix:** Return `CompositorShortcut::None` for the Ctrl+Super block when `locked`.
- **If we skip it:** Hidden windows move while locked; confusing after unlock, but no data leak.

**PX-14. Idle timers are recreated on every input event** (`crates/malaysia-compositor/src/idle.rs:191-209`)
- **What this is:** `note_activity` removes and re-inserts one calloop timer per idle notification and the auto-lock timer on every key or motion event.
- **Problem:** A 1000 Hz mouse causes thousands of timer heap operations per second, each taking the token map mutex, with no behaviour change.
- **Fix:** Keep the current deadline and re-arm only when it moved meaningfully, or let the timer fire and compare against a last-activity `Instant`.
- **If we skip it:** Wasted CPU while the pointer moves; small on this desktop, but free to avoid.

**PX-15. Dead code: `theme_revision` and `_traffic_input`** (`crates/malaysia-shell/src/lock.rs:36`, `:331-345`; `crates/barrier-kit/src/titlebar.rs:454`)
- **What this is:** The lock hashes two file mtimes into a model field nobody reads; the titlebar keeps an `#[allow(dead_code)]` helper nobody calls.
- **Problem:** They cost a filesystem stat, extra state and about 30 lines to read and maintain. Theme updates already go through the BarrierKit watcher.
- **Fix:** Delete the field, the `theme_revision()` function and `_traffic_input`.
- **If we skip it:** Dead code stays and invites “fixes” to code that does nothing.

**PX-16. Two identical `sanitize_client_env` copies in the shell** (`crates/malaysia-shell/src/launcher.rs:583`, `crates/malaysia-shell/src/menubar/mod.rs:278`)
- **What this is:** The same 15-line function and wrapper-variable list are copied in two modules.
- **Problem:** A future wrapper variable must be added in both places; forgetting one lets Malaysia’s Mesa paths leak into launched apps.
- **Fix:** Keep one helper in a small `malaysia-shell` spawn module and call it from both places.
- **If we skip it:** App launches from the dock/menu and from the launcher will drift apart.

**PX-17. Two direct `wayland-backend` dependencies are never named** (`crates/barrier-kit/Cargo.toml:32`, `crates/malaysia-compositor/Cargo.toml:29`)
- **What this is:** No source file uses `wayland_backend`; the crates use the `wayland-client`/`wayland-server` APIs and Smithay re-exports.
- **Problem:** Two direct dependencies to track, license and update for no code use.
- **Fix:** Remove both lines and rebuild; the library remains in the graph through `wayland-client`/`wayland-server`.
- **If we skip it:** Unnecessary supply-chain surface.

**PX-18. Docs drift from the code** (`README.md:105`, `docs/arquitectura/wallpaper.md:38-43`)
- **What this is:** README says `window.resize_border` defaults to 6; code and `docs/configuracion/opciones.md:117` say 10. The wallpaper page still lists `barrier-kit/src/gl.rs`, `malaysia-shell/src/present.rs` and an empty `grade-graphics`, all gone after ADR 0034.
- **Problem:** Readers copy the wrong default and look for deleted files. The next agent may recreate or hunt for code that no longer exists.
- **Fix:** Change 6 to 10 and update the wallpaper file list to the current `barrier-kit`/Freya paths.
- **If we skip it:** Docs lose trust and mislead future work.

Verdict: The architecture is sound and the tests are unusually good for a hobby compositor, but the lock screen, the config paths and the live-state wiring have four defects a daily user will hit; fix 1–4 first.

Lean: -60 lines, -2 dependencies possible.

Not checked: I did not run `./dev.sh check`, the compositor or the probes (the dev shell is required); I did not exercise DRM/VT/resume, EGL/Skia unsafe code, the Nix/Skia build or real PAM stacks. I read the capture, lock, config, input, state, shell, menu and store flows; I did not read every line of the 64k-line tree (some UI and udev details were only skimmed).
