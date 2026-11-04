# Claude audit — 2026-11-03

A full review of malaysia's code against `AGENTS.md`/`CLAUDE.md`, compared with
niri. Spanish version: [`auditoria-claude-2026-11-03.md`](auditoria-claude-2026-11-03.md).

## Goal and scope

- **Goal:** diagnose dead code, performance, leaks, bugs and missing
  features, and propose a restructuring and a list of files to delete.
- **Scope:** the whole repo at `345ba40` (2026-11-03): 7 crates, `tools/`,
  `scripts/`, `share/`, `nix/`, `docs/`, `dev.sh` and `flake.nix`. This
  document changes no code.
- **Reader:** anyone working on malaysia, human or agent. Each finding
  becomes a row in [`deuda.md`](deuda.md) or a block in
  [`roadmap.md`](roadmap.md) once accepted. Status and debt still live there.
- **Inputs:**
  - the code at `345ba40`;
  - Smithay at `023b78b1`;
  - barrierkit at `112632b`;
  - niri at `a8562a1` (2026-10-31);
  - cosmic-comp at HEAD.
- **Outputs:** sections 1 to 10 and the batch plan in section 11.

## Method

1. Static reading by line range and with `grep`, split across six fronts:
   - render and backend;
   - input and windows;
   - shell;
   - wallpaper, settings, portal, scene and shaders;
   - repo, docs and Nix;
   - niri research.
2. Every dead-code finding was also searched for in `tools/`. Because
   `lib.rs` exports modules with `pub mod`, rustc does not warn about that
   dead code (§4).
3. Critical findings were checked twice: against the code and against
   Smithay's code at the pinned rev.
4. **Nothing was compiled.** `nix develop` fails in this container: the
   proxy returns 403 when downloading the Skia sources. No result comes from
   clippy or the tests. The first batch must start with `./dev.sh check`.

Legend: **[V]** verified against the code · **[S]** reasoned suspicion.
Severity: **H** high · **M** medium · **L** low.

## Summary: the ten things that matter

| # | Finding | Where | Sev. |
|---|---|---|---|
| 1 | Any Wayland client can unlock the screen: a second `ext_session_lock_v1.lock` replaces the live lock, and its `unlock_and_destroy` unlocks | `malaysia-compositor/src/lock.rs:29-49`, `state.rs:285` | H [V] |
| 2 | Suspend does not lock: nothing listens for `PrepareForSleep` | `malaysia-shell/src/menubar/system.rs:130` | H [V] |
| 3 | Damage is defeated every frame: new ids and a `damage_since` that is always full | `effects.rs:170-180`, `chrome.rs:225,270`, `session.rs:653,1384` | H [V] |
| 4 | No redraw state machine: render on every vblank and on every input event, and a 60 Hz loop while idle | `udev.rs:329-331,541-566,695-757`, `session.rs:1650-1680` | H [V] |
| 5 | `wp_presentation` times are not CLOCK_MONOTONIC and the refresh is fixed at 16.666 ms | `udev.rs:557`, `session.rs:1989`, `state.rs:275` | H [V] |
| 6 | `capture_sessions` only grows: every capture or screencast leaves its session alive | `state.rs:90,433`, `handlers.rs:308` | H [V] |
| 7 | Apps launched from the menubar inherit its cgroup and leave zombies | `menubar/mod.rs:212,305-327`, `system.rs:96`, `nix/malaysia.nix:82` | H [V] |
| 8 | About 25 options documented in `opciones.md` are parsed and never read | `config.rs` (§4.1) | M [V] |
| 9 | `bandura.jpg` is embedded in the binary with no license | `NOTICE:85-86`, `malaysia-wallpaper/src/image.rs:6-8` | H [V] |
| 10 | The bilingual rule is broken across the board: 0 `.en.md` out of 73 documents | `docs/`, `README.md` | M [V] |

## 1. Security and correctness bugs

### 1.1 Compositor

- **Lock bypass** — `lock.rs:29-49` and `state.rs:285`.
  - Problem:
    - `lock()` always calls `confirmation.lock()`, even when a lock is already alive.
    - In Smithay `023b78b1`, `SessionLocker::lock` overwrites `LockStatus::Locked(lock)` with the new lock.
    - `UnlockAndDestroy` only checks `is_locked_by(lock)`.
    - The global filter is `|_| true`.
  - Consequence: any client sends `lock` and then `unlock_and_destroy`, and the session is unlocked.
  - Fix: as niri does (`src/niri.rs`, `lock`), if `self.locked` and the previous lock is still alive, drop the `SessionLocker` without confirming it; Smithay then sends `finished`. About 10 lines.
  - H [V]
- **Clicks under the lock** — `lock.rs:29-49` and `input.rs:613-625,1883,1912`.
  - Problem:
    - `lock()` neither clears pointer focus nor ends `self.grab`.
    - Until the next motion, buttons and scroll reach the hidden window.
    - A move or resize in progress stays active, because the grab branch runs before the `locked` check.
  - Fix: in `lock()`, set `grab = None` and reassign pointer focus; in Button and Axis, return if `locked` and the focus is not a lock surface.
  - H [V]
- **Window actions while the session is locked (PX-13 extended)** — `input.rs:432-468`.
  - Problem: these paths do not check `locked` either:
    - `ArrangePair*`;
    - `ArrangeRestore`;
    - `exit_focus_mode` (`spaces.rs:309`);
    - foreign `SetMaximized`/`SetMinimized` (`foreign.rs:186-194`);
    - infinity `Arrange*` (`infinity.rs:155`).
  - Fix: a single guard point in the shortcut dispatch (§6.2).
  - M [V]
- **Globals open to every client.**
  - Problem:
    - any client, even a sandboxed one, can use wlr-foreign-toplevel-management (close and minimize any window), image-copy-capture (of outputs and of windows), session-lock and layer-shell overlay;
    - there is no `security-context-v1`;
    - only infinity filters (`infinity.rs:132`).
  - Fix: add `security-context` and restrict these globals to clients without a context, or to the shell.
  - M [V]
- **Infinity trusts development paths** — `infinity.rs:30-77`.
  - Problem:
    - it trusts `$CWD/target/{debug,release}/malaysia-shell`, `$CWD/result/bin` and `MALAYSIA_DEV=1`;
    - identifying by `/proc/pid/exe` has a PID race.
  - Fix: limit it to `cfg(debug_assertions)`.
  - L [V]
- **Denied popup grab without `popup_done`** — `handlers.rs:799-855`.
  - Problem:
    - xdg-shell requires dismissing the popup when the grab is denied;
    - only a button serial is accepted, so menus opened from the keyboard get no grab.
  - M [V]
- **Toplevel never unmaps** — `window.rs:378-512`.
  - Problem:
    - `mapped` never goes back to `false`;
    - a commit with a null buffer leaves the window in the scene, in foreign and with focus;
    - on remap, the initial configure is not repeated.
  - M [V]
- **Pointer constraints and relative pointer** — `input.rs:1887,1995-2029`.
  - Problem:
    - a null confine region ("the whole surface", the usual case in games) does not confine;
    - the relative delta is computed from positions that were already clamped, so it is 0 at the edge and `delta_unaccel` is lost.
  - Fix: use `event.delta()` and `event.delta_unaccel()`.
  - M [V]
- **Idle-inhibit from invisible windows** — `idle.rs:419-433`.
  - Problem:
    - a minimized window, or one on another space, inhibits auto-lock forever, while the protocol says "while visible";
    - also, every inhibit/uninhibit cycle adds one more `add_destruction_hook`.
  - M [V]
- **`set_var` with live threads** — `spawn.rs:49`.
  - Problem: the `// SAFETY:` says "single-threaded during startup", but udev has already created EGL/GLES (`udev.rs:160-165`) and Mesa may own threads.
  - Fix: export the environment in `main()` before the backend starts.
  - M [S]
- **1 px seam on the titlebar** — `chrome.rs:319` vs `session.rs:157`.
  - Problem: the titlebar texture uses `to_i32_round` and the surface `to_i32_floor`, against ADR 0004. With fractional scale a 1 px seam can appear.
  - Captures also round to nearest: `session.rs:1484` and `handlers.rs:367-368`.
  - M [V]
- **Buffer release on DRM** — `udev.rs`.
  - Problem: udev does not hold the `Buffer`s until the frame fence. `collect_drawn_buffers` and `schedule_frame_release` exist only in winit. With `drm_syncobj`, the release could be signalled before the GPU finishes.
  - M [S]
- **Minor** [V]:
  - `maximize_request` on an already maximized window sends no configure (`input.rs:996`).
  - foreign advertises v3 but ignores `SetFullscreen` (`foreign.rs:194`) and creates handles with an empty title plus `done` (`foreign.rs:242`).
  - `unlock` does not restore keyboard focus.
  - `idle_timeout_seconds` is not reloaded in `refresh_theme` (`state.rs:394`).
  - The DnD icon is not drawn (`handlers.rs:522`).

### 1.2 Shell

- **Suspend without locking** — `menubar/system.rs:130`.
  - Problem: it calls logind `Suspend`, and no crate has an inhibitor or a `PrepareForSleep` handler (grep 0).
  - Fix: take a logind `delay` inhibitor and lock before sleeping. This belongs in the compositor, which outlives the shell.
  - H [V]
- **Cgroup and zombies.**
  - Problem:
    - the menubar runs as `malaysia-shell@menubar.service`;
    - `setsid` does not move children out of the cgroup, so if the menubar dies, systemd kills the user's apps;
    - the `Child` is dropped without `wait` (`menubar/mod.rs:212`, `system.rs:96`, `mod.rs:310-318`) and nothing handles `SIGCHLD`.
  - Fix: a single `spawn.rs` in the shell, with a `systemd-run --user --scope` or `StartTransientUnit` scope (as niri does, `src/utils/spawning.rs`), and a thread that calls `wait()`.
  - H [V]
- **Force Quit by index** — `dialogs.rs:290-293`, `focused.rs:230,639`, `dock.rs:49` (P5-10).
  - Problem: if the list changes while the dialog is open, SIGKILL goes to another app.
  - Cause: barrier-kit's `ToplevelInfo` has no stable id (`shell/toplevels.rs:132-190`).
  - H [V]
- **`unsafe` without `// SAFETY:`.**
  - Problem: 44 blocks in the shell, against rule 6:
    - `pam.rs`, 28 blocks;
    - `launcher.rs:570`;
    - `menubar/mod.rs:121,172,204`;
    - `system.rs:64-77`.
  - There are more: `udev.rs:164` and `malaysia-settings/src/state.rs:50`, which also does not check for a null `pw_gecos`.
  - M [V]
- **Password remnants on the heap** — barrierkit `password.rs:33`.
  - Problem: `Zeroizing<String>::push` reallocates and frees the old buffer without wiping it.
  - Fix: reserve capacity when the field is created.
  - M [V]
- **Minor** [V]:
  - `exec_matches` (`menubar/mod.rs:66-71`) uses `contains`: short tokens such as `sh` match unrelated apps.
  - The Exec codes `%i %c %k` are dropped instead of expanded (`desktop.rs:313-319`).
  - `collect_desktop_files` follows symlinks with no cycle detection (`desktop.rs:72-74`) [S].
  - The serial number is logged at `info` (`dialogs.rs:803`).
  - If `current_user()` fails, the lock stays at "verifying" with no message (`lock.rs:284-287`).

### 1.3 Wallpaper and portal

- **Destruction order** — `malaysia-wallpaper/src/wayland.rs:340-345`.
  - Problem:
    - it destroys the `wl_surface` before the `zwlr_layer_surface`, and the protocol requires destroying the role first (SCTK 0.19.2, `wlr_layer/mod.rs:340-348`);
    - on top of that, SCTK's `Drop` destroys it again.
  - Fix: delete `output.surface.destroy()`.
  - M [V]
- **Contradictory reload** — `image.rs:64-66` and test `image.rs:672`.
  - Problem: because the app calls `take()` on the image after upload, in production an unreadable selection falls back to `bandura.jpg`. The test asserts the opposite.
  - Fix: pick one behaviour, delete the branch or the test, and document it.
  - M [V]
- **Camera images** — `image.rs:255-270`.
  - Problem: JPEGs ignore EXIF orientation and the ICC profile.
  - M [S]
- **Unreachable branch** — `xdg-desktop-portal-malaysia/src/settings.rs:72`.
  - The `ColorScheme::Auto` branch is unreachable. Otherwise the portal meets the Settings spec.
  - L [V]

## 2. Rendering and performance compared with niri

### 2.1 How malaysia renders today

- **winit:**
  - `draw()` runs on `WinitEvent::Redraw`;
  - `render_output` always receives age 0 (`session.rs:1609`), so any damage repaints the whole output;
  - without damage, a 1/refresh timer asks for another frame (`session.rs:1650-1680`).
- **udev:**
  - `DrmCompositor` with `FrameFlags::empty()` and the `overlay`/`cursor` planes cleared (`udev.rs:178-180,654`);
  - `render()` runs after every libinput event (`udev.rs:329-331`), after every dispatch with a redraw request, and after every vblank unconditionally (`udev.rs:546-566`);
  - with no "frame in flight" state, `queue_frame` overwrites the pending frame.
- **Elements:**
  - `build_output_elements` rebuilds everything every frame;
  - shadow and border with `PixelShaderElement::new` (a new `Id` each time);
  - titlebar with `Id::new()` even when the texture is cached;
  - the clipped content reports full damage and no opaque region.
- **Result:** with one visible window, every frame has full damage, and udev recomposites at the refresh rate even when nothing changes.

### 2.2 What niri does and what to adopt

| # | Change | Gain | Effort | Reference |
|---|---|---|---|---|
| 1 | `RedrawState` machine (Idle, Queued, WaitingForVBlank, WaitingForEstimatedVBlank): input and commits only mark `Queued`; one render per vblank; an estimated-vblank timer when there is no damage | Near-zero idle CPU/GPU; no more overwritten `queue_frame`s | Medium, ~120 lines | niri `src/niri.rs` (`RedrawState`, `queue_redraw`), `src/backend/tty.rs` (`on_estimated_vblank_timer`), wiki `Development:-Redraw-Loop` |
| 2 | Persistent per-window shader elements (shadow, border, titlebar, solids) with a fixed `Id` and a `CommitCounter` that only rises on change | Damage becomes partial again | Low-medium | `src/render_helpers/shader_element.rs`, `shadow.rs`, `border.rs` |
| 3 | `RoundedTextureElement` delegates `damage_since` and `opaque_regions` (minus the corners) to the inner element | Real damage and occlusion culling | Low | `src/render_helpers/clipped_surface.rs` |
| 4 | Correct presentation feedback: `take_presentation_feedbacks` at render time, passed as `queue_frame` user data; the vblank `meta.time`; `Refresh` from the real mode | Fixes a protocol bug (A/V sync in mpv, Firefox and GTK4) | Low | `src/backend/tty.rs` (vblank and feedback) |
| 5 | Frame callbacks only to visible surfaces (`surface_primary_scanout_output`), deduplicated by sequence, with a 995 ms throttle for hidden ones | Minimized apps, or apps on another space, stop painting at 60 fps | Low-medium | `src/niri.rs` (`send_frame_callbacks`, `FRAME_CALLBACK_THROTTLE`) |
| 6 | Cursor and primary planes, `FrameFlags::DEFAULT`, `Kind::ScanoutCandidate` for fullscreen, and per-surface dmabuf feedback with a scanout tranche | Video and games without compositing; cursor without repaints | Low (flags), medium (feedback) | `src/backend/tty.rs`, `src/niri.rs` (`send_dmabuf_feedbacks`) |
| 7 | Clock frozen at `target_presentation_time`, a single `now` per dispatch (`LazyClock`), and redraw only while animations remain | Animations without jitter | Medium | `src/animation/clock.rs`, wiki `Development:-Animation-Timing` |
| 8 | Xray blur: blur the background once and share it; dual Kawase with reused textures; `ext-background-effect` | Meets policy 3 (blur for every window) at low cost | High, ~400 lines; needs 2 and 3 | niri `render_helpers/blur.rs`, `xray.rs`, wiki `Window-Effects`; cosmic `backend/render/wayland/blur_effect.rs` |
| 9 | `spawn` with a double fork and an `app-malaysia-*` systemd scope, without `NOTIFY_SOCKET` and with `RLIMIT_NOFILE` restored | No zombies; OOM isolated per app | Low-medium | `src/utils/spawning.rs` |
| 10 | A `profile-with-tracy` feature and a debug option to draw damage, plus turning the per-frame `info!` into `trace!` (`udev.rs:552,667,682`) | Being able to measure items 1 to 8 | Low | niri `Cargo.toml` (tracy features), `render_helpers/debug.rs` |

Order: 1→4 first, because they fix idle consumption and a protocol bug; 8
depends on 2 and 3.

### 2.3 Other hot spots

- **Compositor, input** [V]:
  - `note_activity` re-inserts N+1 timers and allocates a `Vec` per event (PX-14, `idle.rs:152-169`). Fix as niri does: a `notified_activity_this_iteration` flag.
  - `update_edge_cursor` repeats the lookups that `pointer_motion` does 10 lines later (`input.rs:2121-2130` vs `1936-1945`).
  - `Windows::iter` does a linear `find` for every scene entry: O(n²) on every motion (`window.rs:321-332`). `index`/`index_of` are O(n) (`window.rs:1181-1190`).
  - `log_focus_stack` logs every window at INFO on every click (`input.rs:1051-1066`).
- **Compositor, rendering** [V]:
  - Per-frame allocations:
    - id `Vec`s (`session.rs:777,976,989`);
    - a title `String` per window (`session.rs:1101`, `chrome.rs:198`);
    - one calloop `Generic` source per frame (`session.rs:1791`);
    - the uniforms `vec!` (`effects.rs:201`).
  - Every traffic-light hover change rasterizes the whole titlebar on the CPU and imports another texture (`chrome.rs:194-221`).
  - Every popup fade creates an output-sized offscreen texture (`popup_fade.rs:54-66`).
  - Every screencopy frame creates a new texture and tracker, blocks on `sync.wait()`, and reports full damage with time `Duration::ZERO` (`capture.rs:205-247`, `session.rs:1556-1601`, `udev.rs:609-657`).
  - `window.domain` is rewritten synchronously and non-atomically on every title change (`active.rs:22-37`). The shell does not read the title.
  - The shadow is two quads covering the window plus the margin, with `pow`/`exp` per fragment (`grade-graphics/src/shadow.frag`). Fix: merge the layers or emit only the frame.
- **Shell** [V]:
  - The menubar polls `window.domain` every 250 ms (`bar.rs:824-837`) and `menubar.domain` every 500 ms (`bar.rs:930-951`). `Toplevels` already carries `active`.
  - The dock reads and parses TOML on every render (`dock.rs:22`).
  - The launcher scans every `.desktop` file and resolves icons synchronously on every Super+Space (`launcher.rs:429-435,470-524`), and clones entries and icons on every render (P5-17).
  - `composition()` clones the whole remote tree for each title (`bar.rs:516-574`).
  - `power::run(Suspend)` makes a blocking D-Bus call on the UI thread (`system.rs:130`).
- **Wallpaper** [V]:
  - Up to 3 or 4 copies of the image in RAM during upload (`render.rs:231,267`; `image.rs:287`). For a 24 MP 16-bit PNG, a peak of about 700 MB.
  - The texture is uploaded at full resolution with mips, not capped to the output (`render.rs:229-256`).
  - `desired_maximum_frame_latency: 2` for static content (`wayland.rs:430-433`) [S].
  - Redraws are already correct: only on configure, scale change or reload.
- **Portal** [V]:
  - Every focus or title change rewrites `window.domain`, and the watcher classifies it as `Defaults`. The portal rereads `appearance.domain` twice, regenerates 3 CSS files and reads 3 GTK files (`settings.rs:224`).
  - One-line fix: `if !events.iter().any(|e| e.restyles()) { continue }`.

## 3. Leaks

| Where | What | Fix | Sev. |
|---|---|---|---|
| `state.rs:90,433`, `handlers.rs:308` | `capture_sessions` only `push`es; `session_destroyed` is missing (Smithay `image_copy_capture/mod.rs:840`). `update_constraints` is not called on resize either | `retain` in `session_destroyed` and `update_constraints` on resize | H [V] |
| `udev.rs:736-757` | `schedule_repaint` checks `repaint_timer` but never sets it, so timers pile up | Store the token | M [V] |
| `handlers.rs:489-509` (PX-09) | One thread per `send_selection`; blocking `write_all` with no timeout | Non-blocking write with a calloop `Generic` | M [V] |
| `input.rs:1508-1552` | `forget_window` does not clear `arranged` or `minimized` | +2 lines | L [V] |
| `window.rs:1017-1030` | `pending_decorations` of surfaces that die without a role | Clear in `CompositorHandler::destroyed` | L [S] |
| `idle.rs:419-433` | Destruction hooks pile up | Register once | L [V] |
| shell `menubar/mod.rs:212,305-327`, `system.rs:96` | Zombies for every app, `xdg-open` and component launched | A thread that calls `wait()` (§1.2) | H [V] |
| shell `dbusmenu.rs:157-180,735-779` | If a client dies without `UnregisterWindow`, its Registrar entry stays | Clear it when the owner disappears | L [V] |
| shell `bar.rs:37-41` | `MenuMemory.titles` grows without bound | LRU | L [V] |
| shell `menubar/mod.rs:293-297` | Logs in tmpfs, opened in append mode and never rotated, plus an `info!` on every `ItemsPropertiesUpdated` (`dbusmenu.rs:953`) | Truncate on open and lower to `debug` | L [V] |
| wallpaper `wayland.rs:303,340-345,580-583` | `wp_fractional_scale_v1` and `wp_viewport` are never destroyed; after `closed()` the output has no background | `Drop` on `OutputSurface`, and recreate the surface | M [V] |
| wallpaper and portal | If the watched directory is deleted, `IN_IGNORED` is not handled and watching stops | Re-arm the watch | L [S] |

No leak [V]: `ChromeCache` (`input.rs:1533`), `popup_fades`, `focus_notices`,
gtk, appmenu, foreign, infinity, layers, popups and the wallpaper texture.

## 4. Dead code

### 4.1 Configuration nobody reads

`config.rs` parses these options and `opciones.md` documents them, but nothing
reads them outside the tests [V]:

| Option | Lines | Notes |
|---|---|---|
| `effects.*` (`inner_border`, `shadow`, `blur`, `corner_radius`) | `config.rs:271-342` | The compositor takes effects from `vow-design`; the defaults are also design literals |
| `debug.damage_highlight` | `config.rs:347` | Implement it (§2.2 table, item 10) or delete it |
| `dock.*` (all 11) | `config.rs:219-251` | The dock does not read `dock.domain` |
| `menubar.autohide`; `menubar.app_menus` duplicated | `config.rs:256-258` | The shell has its own `MenubarConfig` (`bar.rs:913`) with a different default |
| `wallpaper.*` | `config.rs:83-89,174-187` | `malaysia-wallpaper` reads it with another schema (`image.rs:93-121`) |
| `window.titlebar`, `window.focus` (`Sloppy`) | `config.rs:194` | `Sloppy` is not implemented |
| `screenshot.{show_thumbnail, include_date, disable_shadow, timer, remember_selection}` | `config.rs:107-115` | The capture UI does not exist |

Decision to make: delete from the compositor everything it does not consume
(rule 2) and keep each option in the process that uses it. Estimated at about
−150 lines. To keep them as a contract instead, `opciones.md` should mark them
"not implemented" and the roadmap should carry them as work.

### 4.2 Functions, fields and variants [V]

- **Compositor:**
  - No callers:
    - `window.rs:543 remove_by_id`;
    - `window.rs:941 Windows::set_maximized` (P1-04);
    - `xdg.rs:54 is_directory`.
  - Only the tests use it: `window.rs:160 usable_size`.
  - Duplicate of `id_of`: `window.rs:1106 id_of_surface` (P1-09).
  - Write-only fields:
    - `state.rs:145-147` `infinity_global` (with `#[allow(dead_code)]`, P1-08) and `_focus_notice_global`;
    - `state.rs:90` `capture_sessions`;
    - `gtk.rs:75` `GtkSurfaceState.modal`.
  - Always `Some`:
    - `gtk.rs:74 owner`;
    - `session.rs:89` and `udev.rs:71` `appearance_watch`;
    - `session.rs:1356 border_element` (P1-10).
  - Unused values or variants:
    - the `bool` returned by `handlers.rs:212 insert_blocker_source`;
    - `decoration.rs:49 Role::Popup` (tests only);
    - the `titlebar` parameter of `for_window`, which always receives `&TITLEBAR`.
  - Unreachable or overwritten code:
    - `infinity.rs:196-224 WEnum::Unknown(0..=9)`;
    - `handlers.rs:947-951 clear_requested_decoration`, overwritten right after;
    - the filter in `idle.rs:162-168`.
  - Superfluous literals or uniforms:
    - `chrome.rs:352 Metrics::new(13.0, 17.0)`, which gets overwritten;
    - `chrome.rs:262-263`, where `rect == window_rect`, and the `window_rect` uniform in `shadow.frag`;
    - the `"size"` uniform in `effects.rs:349,373`, which Smithay already defines.
  - Environment hooks with no user:
    - `MALAYSIA_TEST_LEAK_WINDOW` (`input.rs:1562`, `state.rs:156`);
    - `MALAYSIA_TEST_CURSOR_BOOST` (`state.rs:749`);
    - `MALAYSIA_THEME_FADE_HOLD_MS` (`theme_fade.rs:25`).
  - **Root cause:** `lib.rs:6-31` exports `capture`, `config`, `decoration`, `spawn`, `state`, `window` and `xdg` as `pub mod`, though `main.rs` only uses `session` and `udev`. Once they become `mod`, rustc will list the rest of the dead code.
- **malaysia-scene:**
  - `geometry.rs`: `Physical`, `Buffer` and all their conversions are used only in tests (about −150 lines). The ADR 0004 invariant is tested in code nobody runs.
  - `scene.rs:81-87`: `len` and `is_empty` have no callers.
  - `spaces.rs:137-143`: `show_next` and `show_previous` are used only in tests.
- **Shell:**
  - `dbusmenu.rs:1240 raw_items_from_layout` (with `allow(dead_code)`, used only by tests).
  - `menubar/mod.rs:77 ActiveWindow.title` (P5-20).
  - `lock.rs:32,264,318-331 theme_revision` (P5-12).
  - `bar.rs:356-378 last_gtk_menus`, which only feeds logs.
  - `dbusmenu.rs:25-37 RENDER_CALLS`, which is always 0.
  - The `args` parameter of the closure in `appmenu.rs:587`.
- **Wallpaper:**
  - `image.rs:95 dir` (with `allow(dead_code)`).
  - `wayland.rs:79 fractional_scale`, an `Option` that is always `Some`.
  - `render.rs:449-467 gpu_texture_bytes`.
  - `render.rs:1170 corner_inset_for_capture`.
  - The `_pad` field (`render.rs:35`, `wallpaper.wgsl:12`).
  - The `LoadedImage`/`load_snapshot` wrapper (`image.rs:29-36`).
- **Dependencies:**
  - shell: `png` in `[dependencies]` is used only in tests and `freya-engine` only in tests (move both to dev-deps);
  - wallpaper: `png` duplicated in dev-deps, and `half`'s `bytemuck` feature unused;
  - workspace: no one uses the `dev-fast` profile, and `sctk` and `smithay-client-toolkit` declare the same crate twice.

### 4.3 Tests that protect nothing (rule 8) [V]

- `launcher.rs:626-856`: the golden test draws a copy of the layout (`Case`), not the real `LauncherApp`, and if the PNG is missing it writes it and passes.
- `appmenu.rs:665-1292`: about 600 lines that check labels of static menus.
- `grade-graphics/src/lib.rs:28-167`: they check GLSL substrings and an SDF copy rewritten in Rust.
- `logging.rs`: tests a trivial table, and `LogLevel` reimplements `LevelFilter::from_str`.
- `malaysia-wallpaper/src/render.rs:779-1129`: needs a Vulkan adapter inside `cargo test`.
- The portal tests read the host's real `appearance.domain`.

## 5. Reducing code: duplicates and rules

### 5.1 Duplicates (rule 1) [V]

| What | Copies | Savings |
|---|---|---|
| Capture loop and debug overlays | `session.rs:1533-1601` and `udev.rs:577-657` | ~−100 |
| Event loop setup (ping, watcher, socket, signals, xwayland) | `session.rs:263-381` and `udev.rs:217-420` | ~−150 |
| Repaint timer | `session.rs:1650-1675`, `udev.rs:696-719,736-757` | ~−40 |
| Inline frame callbacks | `session.rs:1688-1710` and `1959-1981` | ~−20 |
| Background colour `[0.06, 0.06, 0.09, 1.0]`, a design literal | 7 places: `session.rs:1511,1589,1611`, `udev.rs:646,666`, `theme_fade.rs:243`, `capture.rs:112` | token in `vow-design` |
| `set_focus` + `data_device` + `primary` | `input.rs:1097,1343,1378,1409` | `set_seat_focus` helper |
| `parse_ms` and `parse_resize_timeout` (and their tests) | `input.rs:269`, `window.rs:38` | ~−30 |
| Four easing functions | `theme_fade.rs:194`, `state.rs:778,782`, `spaces.rs:27` | ~−20 |
| `app_id`, the visibility predicate and `round()` | `window.rs:90/1077`; `window.rs:81,651,681`; `input.rs:2261`, `window.rs:1354`, `infinity.rs:538` | ~−30 |
| `sanitize_client_env` (P5-13) and `pre_exec(setsid)` ×4 | `launcher.rs:570,584`, `menubar/mod.rs:121,172,204,245` | ~−40 |
| XDG paths ×3 and PATH lookup ×2 | `desktop.rs:24-41,238-256`, `launcher.rs:486-497`, `recents.rs:89-95`, `menubar/mod.rs:183-193` | ~−40 |
| `RawItem` and `RawNode` copied from the kit | `dbusmenu.rs:69-74,1238` vs `barrier_kit::app_menu` | ~−25 |
| `OwnedValue` readers ×3 | `dbusmenu_map.rs:72-82`, `gtkmenus.rs:394,419` | ~−15 |
| sRGB transfer ×6 | `wallpaper/image.rs:379-413`, `render.rs:471,722`, `vow-appearance/backdrop.rs`, `vow-design/lib.rs:61` | ~−40, one pair in `vow-design` |
| `.domain` parsing with no shared code | `wallpaper.domain` ×2, `appearance.domain` ×2 (G-01), `menubar.domain` ×2, `window.domain` ×2 | §6.4 |
| `dev.sh` command list ×3; domain tree ×2 | `README.md`, `docs/README.md`, `AGENTS.md`, `opciones.md` | link instead |
| `build_probes` block ×5; socket wait ×6 | `dev.sh` | ~−80 |

### 5.2 Design literals outside `vow-design` [V]

- **Compositor:**
  - `window.rs:245-246`: `CLIENT_RESIZE_BORDER 6.0` and `CORNER 12.0`.
  - `infinity.rs:25`: `TILE_GAP 8`.
  - `input.rs:282-286`: the `SHAKE_*` thresholds, though `CURSOR_SHAKE_*` tokens already exist.
  - `state.rs:61`: 150 ms.
  - `spaces.rs:16,18`: 0.6 and 300 ms.
  - `chrome.rs:438`: 12.0.
  - `session.rs:1376`: the border width.
  - `cursor.rs:218`: size 24.
  - `state.rs:292`: key repeat 200/25, with no option.
  - The defaults in `config.rs`.
- **Shell:**
  - `dialogs.rs`: about 20 metrics.
  - `launcher.rs`: alphas, radii, gaps, 250 ms, the 0.15 bounce and 16 ms.
  - `dock.rs:46` and `lock.rs:233`: `/4`.
  - `menu.rs:249-275` and `clock.rs:8-11` (P5-15); the `MENUBAR_FONT_SIZE` token exists.
- **Others:**
  - `scene.rs:248`: `CORNER_INSET`.
  - `wallpaper/image.rs:416-417,506`.
  - `portal/gtk.rs:149-150`.
- `scripts/check-design-tokens.sh` only checks the compositor and the shell. It should also cover scene, wallpaper and portal.

### 5.3 Single-use wrappers and abstractions (rule 3) [V]

- `spawn.rs:32-41`:
  - `spawn_shell` wraps `Command::spawn`;
  - `spawn()` repeats what `command()` does;
  - its `config` parameter is always `Config::default()`.
- `appmenu.rs:26-55`: `AddressStore` is a 1:1 `HashMap`.
- `capture.rs:87-135`: `screenshot_png` → `render_png` → `render_and_read`.
- `State.minimized` duplicates `WindowData.hidden` and forces a `&HashSet` through about 20 calls.
- Wallpaper: `LoadedImage` and `load_snapshot`.

### 5.4 Comments

Comments follow rule 6 almost everywhere: across all crates there are only 5
`//` lines and no `TODO`.

- Exceptions in `input.rs:2092,2207`: they explain what the code does.
- Spanish log messages in `handlers.rs:906,923`, which are also duplicated.
- What is missing is the `// SAFETY:` comments from §1.2.

## 6. Proposed restructuring

Moves only, unless stated otherwise. Each step is one commit and must pass
`./dev.sh check`.

### 6.1 `session.rs` (3348 lines; about 2090 without tests)

1. `winit.rs`: the winit backend, about 750 lines.
2. `render.rs`: `build_output_elements`, layers, reveal, space transitions, `append_solid` and `physical_*` (about 500). The side effects (`popup_fade::start_requested`, `dismiss_popups`, `reset_dead_cursor`) move to a prior step, because today they run 3 or 4 times per frame.
3. `effects.rs` (P1-11): window frame, `frame_order` and `border_element` (`session.rs:1027-1392`).
4. `capture.rs` (P1-12): overlays, PNG and a single `process_pending_captures` shared by both backends.
5. `cursor.rs` (P1-13): `reset_dead_cursor` and the cursor elements.
6. `frame_clock.rs` (new): presentation, frame callbacks, buffer release, a single timer and the `RedrawState` machine from §2.2.
7. Shared loop setup, generic over the existing `IdleHost` trait.
8. `test_support.rs`: one harness instead of four (L3: `session.rs:2094`, `input.rs:2846`, `handlers.rs:1215`, `gtk.rs:546`).

### 6.2 `input.rs` (3180), `window.rs` (1872) and `handlers.rs` (1420)

- **`input.rs`:**
  - `input/script.rs`: the synthetic `MALAYSIA_TEST_*` input, about 620 lines, behind a feature.
  - `shortcuts.rs`: shortcuts and dispatch; PX-13 gets fixed here, once.
  - `input/pointer.rs`: motion, button, axis, constraints and the edge cursor.
  - `focus.rs`: focus, with `set_seat_focus`.
  - `wm.rs`: maximize, the green button, move and resize.
  - `shake.rs`: the shake detector.
- **`window.rs`:**
  - `placement.rs`: the pure geometry. Better in `malaysia-scene`.
  - `configure.rs`, `hit.rs` and `surface.rs`.
  - `pending_decorations` moves to `decoration.rs`.
- **`handlers.rs`:** one module per protocol:
  - `blockers.rs`, `capture.rs` and `selection.rs` (which fixes PX-09);
  - `activation.rs` and `xdg_shell.rs`;
  - `decoration.rs`.
- **Boundary with `malaysia-scene`:** `Scene` should own the order and the id → entry map, and `Windows` should hold `HashMap<WindowId, WindowData>` plus `ObjectId → WindowId`. That removes the O(n²) lookups from §2.3.
- **`chrome.rs`:** move the CPU rasterizer to `chrome/raster.rs` (`chrome.rs:567-830`) and check whether `vow-appearance::traffic` already provides the traffic-light primitives (ADR 0047).

### 6.3 Shell: `menubar/` (about 6100 lines of code and 9100 with tests)

1. A single `src/spawn.rs` in the shell: sanitized environment, `pre_exec` with `SAFETY`, `wait()` and a systemd scope. This closes P5-09, P5-13, the zombies and the cgroup issue. Icons move to `src/icons.rs` with a cache.
2. One model and one worker for both menu sources: `enum Source { DbusMenu, Gtk }`, no trait; `cache.rs` with P5-19 fixed; and the kit's `RawItem`.
3. Split `BarApp::render`, today one function of about 620 lines (`bar.rs:201-822`):
   - `memory.rs` and `titles.rs`;
   - the D-Bus commands move into an effect;
   - the pollers are replaced by `Toplevels` and inotify.
4. Local menus in `menubar/local/`, with a single `enum Action`. Today there are three string dispatchers.
5. Into barrier-kit:
   - a stable id in `ToplevelInfo` (P5-10);
   - the icons as `Symbol`;
   - the accelerator parser;
   - `PasswordField` without reallocation.
6. `dialogs.rs` is split into `power_dialog.rs`, `force_quit.rs` and `about/mod.rs`.

Expected result: about −800 lines and no file over about 600.

### 6.4 Configuration and crates

- A shared domain module (`load_section` and the `menubar`, `window` and `dock` types) for the compositor and the shell. It can be a small `malaysia-domain` crate.
- `wallpaper.domain` moves to `vow-appearance`, next to `wallpaper.state`.
- `gtk_bridge` joins `AppearanceConfig` (G-01).
- Delete `WallpaperConfig` from the compositor.
- `malaysia-scene`: shrink it, don't merge it. Keep only the logical types, take in the compositor's pure geometry, and fix the description.
- `grade-graphics`: it has a single consumer and about 90 lines of GLSL. It can fold into `malaysia-compositor/src/shaders/` with an amendment to ADRs 0024 and 0048. If it stays separate, it should also export the uniform names.

## 7. Files to delete or fix

| File | Reason | Action |
|---|---|---|
| `scripts/check-tengen-generated.sh`, `tools/sync-tengen.sh` | They point at `crates/vow-design/tengen`, which now lives in barrierkit | Delete |
| `scripts/medir-redimensionado.sh` | Duplicates `resize-measure.sh` and uses a `window_csd` example that does not exist | Delete |
| `scripts/resize-measure.sh` | Reads a `frame=…delta=` log that is no longer emitted | Delete or turn into a `dev.sh` command |
| `scripts/capture-validate.sh` | Duplicates `verify-captures.py` | Delete |
| `scripts/test.sh` | Uses the `malaysia-compositor-shell` binary and the `wallpaper` component, which no longer exist | Delete |
| `scripts/demo.sh`, `scripts/lab.sh` | Wrappers around `git pull && ./dev.sh …` | Delete |
| `scripts/malaysia-drm.sh` | Uses `bin/malaysia`, which does not exist, and runs `systemctl --user link --force` against the environment rule | Delete or move into `dev.sh` |
| `tools/sctk-probe/src/bin/{decor_live,popup_live,popup_frame_probe,toplevel_frame_probe}.rs` | Nothing uses them (1098 lines) | Delete or wire them into `dev.sh` |
| `crates/malaysia-compositor/examples/appmenu_registrar_stub.rs` | Nothing uses it | Delete or document it |
| `docs/assets/marca/malaysia-icon.png` (1.1 MB), `malaysia-icon.svg`, `malaysia-banner.svg` | Nothing references them; only `malaysia-banner.jpg` is used | Keep the SVG as source and delete or compress the PNG |
| `dev.sh:757-765,920-930` | `result/bin` branches unreachable after `require_devshell` | Delete |
| `flake.nix:67-68` | `outputHashes` for barrierkit `610f852`, no longer the pinned rev | Update to `112632b` or delete |
| `flake.nix:164` | The `packages."malaysia-shell"` alias has no use | Delete |
| `flake.nix:255` | `amdgpu_top` has no use | Delete |
| `flake.nix:154,281` | `VK_DRIVER_FILES` fixed to `radeon_icd`: no Vulkan outside AMD | Use every Mesa ICD |
| `Cargo.toml:114-117` | The `dev-fast` profile has no use | Delete |
| `deny.toml` | Nothing runs it, and it lacks `[sources]` for the git deps | Wire into `check` or delete |
| `.gitignore:8-12` | Stale `neptune-*` patterns; `/result-*` and `.direnv/` missing | Fix |
| `share/systemd/user/malaysia-session.target` | Duplicates the definition in `nix/malaysia.nix:83-89` | Keep a single source |

Kept, because they are used [V]: `protocols/gtk-shell.xml`, `nix/patches/*`, the rest of
`share/` and the goldens in `crates/malaysia-shell/tests/golden/`.

## 8. What is not implemented

### 8.1 Protocols (compositor)

**Present:**

- Core: wl_compositor, subcompositor, shm, linux-dmabuf v5 and drm-syncobj.
- Outputs and windows: xdg-output, xdg-shell, xdg-decoration, kde-server-decoration and layer-shell.
- Presentation and scale: viewporter, fractional-scale and presentation-time (with the §2 bug).
- Input: xdg-activation, cursor-shape, pointer-constraints, relative-pointer, text-input-v3, input-method-v2, virtual-keyboard and virtual-pointer.
- Capture: ext-image-capture-source and ext-image-copy-capture.
- Selection: data-device (DnD without icon) and primary-selection.
- Session: ext-session-lock (with the §1 bypass), idle-inhibit and idle-notify.
- Window management: foreign-toplevel-list and wlr-foreign-toplevel-management.
- Own or third-party: gtk_shell1, kde-appmenu and infinity with focus-notice.
- X11: xwayland-satellite.

**Missing, with a module in Smithay `023b78b1`:**

- Input:
  - tablet-v2: the handler exists, but there is no global and the events are dropped in `input.rs:635`;
  - pointer-gestures;
  - keyboard-shortcuts-inhibit;
  - pointer-warp.
- Security: security-context.
- Windows and dialogs:
  - xdg-dialog-v1: fits the gtk `modal` flag that is dropped today;
  - xdg-toplevel-icon and xdg-toplevel-tag;
  - xdg-foreign;
  - xdg-system-bell.
- Buffers and timing: single-pixel-buffer, alpha-modifier, content-type, fifo and commit-timing.
- Hardware: drm-lease.
- Clipboard: ext-data-control and wlr-data-control. Without them there are no clipboard managers.
- Effects: ext-background-effect.
- X11: xwayland-keyboard-grab.

**Missing, with no module in Smithay** (niri implements them): wlr-gamma-control
(night light), wlr-output-management (kanshi, wdisplays), wlr-screencopy (grim),
ext-workspace (useful for the ADR 0051 spaces) and tearing-control.

### 8.2 Backend and rendering

- DRM:
  - a single output (`find_panel`, `udev.rs:795-826`);
  - no hotplug or mode change;
  - no VRR or gamma;
  - multi-GPU for one output; several outputs on several GPUs wait for 8b.
- No direct scanout, cursor plane or overlays.
- No blur or vibrancy, even though policy 3 in `AGENTS.md` requires it.
- PipeWire screencast: not in the compositor or the portal.
- Xcursor cursors without animation (`cursor.rs:223-260`) [S].

### 8.3 Shell and desktop

- Missing system services:
  - the `org.freedesktop.Notifications` daemon;
  - the StatusNotifierItem tray;
  - the polkit agent;
  - keyring/Secret Service and AT-SPI.
- Missing indicators and controls:
  - volume, network, battery and brightness indicators;
  - OSD and media keys (`XF86Audio*` and `MonBrightness*`: grep 0);
  - the control center.
- Lock: locking on suspend is missing. The screen has no clock, user name or avatar, background, Caps Lock warning or user switching.
- Dock:
  - shows initials instead of icons;
  - has no pinned apps, launching or context menu;
  - does not animate minimizing.
- Menubar:
  - F10/Alt, title overflow and the HUD are missing;
  - there are 62 permanently disabled entries that imitate macOS (48 in `appmenu.rs` and 14 in `focused.rs`).
- Launcher: searches apps only.
- `Shift+Super+5` capture UI: timer, thumbnail and options.

### 8.4 Portal, settings and configuration

- Portal:
  - only `Settings` (`share/xdg-desktop-portal/portals/malaysia.portal:3`);
  - `Screenshot` and `ScreenCast` are missing (12a);
  - with the GTK fallback, `Inhibit`, `Background`, `GlobalShortcuts` and `RemoteDesktop` do not work [S].
- Settings:
  - only the Appearance pane exists, and it does not expose `contrast`, `reduced_motion`, `reduce_transparency` or `sidebar_style`;
  - the Background, Displays, Windows, Dock, Menubar, Captures, Lock and Accessibility panes are missing.
- Configuration:
  - the `domain` command and the `org.malaysia.Compositor1` D-Bus API (ADR 0021): no code;
  - `wallpaper.scale` other than `fill`, and `shuffle`;
  - `remove_desktop` in `malaysia-scene/src/spaces.rs`.

### 8.5 Roadmap items still pending

By block:

- 5: reactive popup repositioning.
- 6: drag & drop.
- 8b: hotplug, multiple outputs and per-monitor scale.
- 8c: planes and scanout.
- 9: per-output scale.
- 10: unlocking with real credentials.
- 11:
  - presentation on DRM and capture with a rotated output;
  - the `Shift+Super+5` app;
  - accepting xwayland-satellite.
- 12a/12b: portal, notifications, polkit, keyring and AT-SPI.
- 13: window capture with its frame.
- 14:
  - accept 14b-2, mnemonics and scrolling;
  - fullscreen from the Window menu;
  - accept 14a, 14b-1 and 14c on DRM;
  - accept 14f-3.
- 16: 16a, 16b (HEIC) and 16c (dynamic wallpapers).
- 17: 17a through 17h are done but not accepted in a session.
- NixOS: polkit, keyring/AT-SPI and a VM.

Pending work that lives outside the roadmap today and should move into it:

- the `domain` CLI and the D-Bus API;
- blur;
- the "pendiente" and "datos" options in `opciones.md`;
- the supervision and overview from ADR 0023.

## 9. Documentation and repository

- **Bilingual rule:**
  - There are 0 `.en.md` files out of 73 documents. The rule landed in `24e26e3`; since then, `2eaf3f7`, `d3625c1` and `345ba40` added Spanish-only documents.
  - No document follows rule 2 (goal, reader, inputs and outputs, steps).
  - H [V]
- **Licensing:**
  - `NOTICE:85-86` leaves `bandura.jpg` as `<ORIGEN Y LICENCIA: te lo paso yo>`, even though it is embedded in the binary and installed.
  - The 31 PNGs in `share/wallpapers/solid-colors/` carry the names of macOS "Solid Colors" and their origin is not in `NOTICE` [S].
  - `NOTICE:9` still says "Yuki".
  - H
- **ADRs:**
  - The index repeats number 0056: malaysia's (xwayland-satellite) and barrierkit's (Vulkan). barrierkit's 0056 and 0057 have no stub (`decisiones/README.md:63-64`).
  - Index titles use old names (yuki, Neptunia, CelestialKit).
  - Nine stubs have no `Estado:` line.
  - ADR 0036 says 14f-2 is pending, but it has been accepted.
  - ADR 0049:13 promises a `wgpu`-in-shell check that `check-client-only-deps.sh` does not perform.
  - M [V]
- **Docs that don't match the code** [V]:
  - `docs/README.md:51` and `flake.nix:96` cite `nix/skia.nix` and `nix/skia-prebuild`, which do not exist.
  - `README.md:40-41` and `AGENTS.md:15` count `wallpaper` as a shell component.
  - `AGENTS.md:43` and `glossary.md:10` say Infinity is "optional until the compositor implements it", but it is already implemented.
  - `capturas.md:40` says the probes use SHM; today they also use dmabuf.
  - `menubar.md:13` cites `barrier-kit::text`, which does not exist, and `menubar.md:22` says "Malaysia" where the code uses "Escritorio".
  - `share/systemd/user/malaysia-shell@.service:2` says `wallpaper` is a component.
- **Undocumented variables and shortcuts** [V]:
  - Environment variables: `MALAYSIA_SCREENSHOT_DIR`, `MALAYSIA_ICON_THEME`, `MALAYSIA_TERMINAL`, `MALAYSIA_LOG_LEVEL`, `MALAYSIA_RESIZE_CONFIGURE_TIMEOUT_MS` and `MALAYSIA_THEME_FADE_HOLD_MS`.
  - Shortcuts: `Ctrl+Super+arrows`, `Ctrl+Shift+Super+arrows`, `Ctrl+Super+Return/C/R` and `Esc`/`Space` in capture mode.
- **Roadmap** [V]:
  - The "Ahora" heading says 2026-10-31.
  - 14f-2 is still under "Pendientes".
  - 14f-3 appears twice, contradicting itself (`:217-220` and `:342-347`).
  - Block 13 says "Pendiente: modo foco real", though 17c is done.
  - `:34` mentions `vow-design`, which is no longer in this repo.
- **Runtime paths** [V]:
  - `dev.sh` uses `$XDG_RUNTIME_DIR/malaysia-dev-<hash>`.
  - Debug captures go to `$XDG_RUNTIME_DIR/malaysia-compositor` (`capture.rs:434`).
  - `AGENTS.md` only allows `$XDG_RUNTIME_DIR/malaysia`, so either the rule or the paths must change.
- **CI:** there is no `.github/workflows`. Nothing runs `fmt`, `clippy`, the tests, `nix flake check` or `cargo-deny`, and the flake `checks` only validate tokens. M [V]
- **Nix:**
  - `flake.lock` has two crane nodes: `barrierkit.inputs.crane.follows` is missing.
  - `tools/sctk-probe` uses `edition = "2021"` and does not inherit `rust-version`.
  - Three versions of sctk get compiled (0.19, 0.20 and 0.21).
- **Git:** 49 of the 50 commits in the clone follow the convention. The exception is `61e0446 chore: …`, which has no scope; its `sed` made the P5-07 text meaningless.

## 10. Status of `deuda.md`

- **Already resolved; delete their rows** [V]:
  - P5-07: the rename broke the text, and the code already compares correctly;
  - P5-11: `StartLimit*` is already in `[Unit]`;
  - P5-14: no `fn rgba` is left.
- **Proposed closure:** P1-14 is not debt, because rule 6 forbids exactly the comment it asks for.
- **Still open, with the current line:**

| id | current line |
|---|---|
| P1-04 | `window.rs:941` |
| P1-05 | `session.rs:1801` |
| P1-08 | `state.rs:145-147`, now hidden behind `#[allow(dead_code)]` |
| P1-09 | `window.rs:1106` |
| P1-10 | `session.rs:1356` |
| P1-11 | `session.rs:1027-1392` |
| P1-12 | `session.rs:1412-1522` |
| P1-13 | `session.rs:1739` |
| PX-09 | `handlers.rs:489-509` |
| PX-13 | `input.rs:432-468`, extended in §1.1 |
| PX-14 | `idle.rs:152-169` |
| P5-09 | `menubar/mod.rs:265-278` |
| P5-10 | `focused.rs:230,639`, `dock.rs:49`, `dialogs.rs:290` |
| P5-12 | `lock.rs:32,264,318-331` |
| P5-13 | `launcher.rs:584`, `menubar/mod.rs:245` |
| P5-15 | `menu.rs:249-275`, `clock.rs:8-11` |
| P5-17 | `launcher.rs:93-110,146,156,369` |
| P5-19 | `dbusmenu.rs:317-327` |
| P5-20 | `menubar/mod.rs:76-78`, `dock.rs:95-109` |
| G-01 | `portal/gtk.rs:358-381` |
| L3 | four harnesses: `session.rs:2094`, `input.rs:2846`, `handlers.rs:1215`, `gtk.rs:546` |

- **One more fix is needed:** the header cites `22070c6`, a commit missing from the clone; it should say `345ba40`.

## 11. Batch plan

Each batch is a `type(scope): …` commit that passes `./dev.sh check`. Whatever
is accepted gets recorded in `deuda.md` or `roadmap.md`.

1. **Security**:
   - `fix(lock)`: reject a second lock; clear the grab and pointer focus on lock; add the `locked` guard to shortcuts and to infinity/foreign.
   - `fix(shell)`: lock before suspend (logind inhibitor).
   - `fix(shell)`: Force Quit by stable id (barrierkit first).
2. **Rendering**:
   - `perf(compositor)`: stable ids and `CommitCounter`s; delegated `damage_since` and `opaque_regions`; `buffer_age` in winit.
   - `perf(compositor)`: a `RedrawState` machine in udev and winit; `schedule_repaint` with a token; drop the per-frame `info!`.
   - `fix(compositor)`: presentation with CLOCK_MONOTONIC, the vblank metadata and the real refresh; per-frame feedback.
   - `fix(compositor)`: `session_destroyed` for captures, plus `update_constraints`.
3. **Processes**:
   - `fix(shell)`: a single `spawn.rs`, with a systemd scope and `wait()` (P5-09, P5-13, zombies and cgroup).
   - `fix(wallpaper)`: destruction order and `Drop` of the per-output objects.
   - `perf(portal)`: ignore `Defaults` events.
4. **Dead code** (§4):
   - `refactor(compositor)`: make the `lib.rs` modules `mod` and delete what surfaces;
   - decide on the configuration with no consumer;
   - delete the dead geometry in `malaysia-scene`;
   - delete the tests that protect nothing.
5. **Files** (§7):
   - `chore`: delete the orphan scripts and probes;
   - `build(nix)`: `VK_DRIVER_FILES`, `outputHashes`, the alias, `amdgpu_top` and `crane.follows`.
6. **Duplicates and tokens** (§5): move the background colour and the other literals to `vow-design`; one capture loop and one loop setup; easing; sRGB.
7. **Restructuring** (§6): `session.rs`, then `input.rs`, `handlers.rs` and `window.rs`, and finally `menubar/`. Moves only; one commit per module.
8. **Docs**: translate the 73 documents into English; fix the mismatches in §9; update `deuda.md` and `roadmap.md`; resolve the licensing.
9. **Features**, by impact:
   - planes and scanout;
   - frame callbacks to visible surfaces only;
   - Screenshot and ScreenCast (PipeWire);
   - data-control;
   - text-input/IME;
   - multi-output and hotplug;
   - xray blur;
   - notifications and polkit;
   - indicators and OSD.
10. **CI**: a workflow with `nix develop -c ./dev.sh check` and `nix flake check`. Tracy optional.

## Sources

- niri `a8562a1`: https://github.com/YaLTeR/niri. Files:
  - `src/niri.rs`;
  - `src/backend/tty.rs`;
  - `src/render_helpers/{shader_element,clipped_surface,shadow,border,blur,xray,debug}.rs`;
  - `src/utils/{spawning,watcher,vblank_throttle}.rs`;
  - `src/animation/clock.rs`;
  - `src/layout/tests.rs`;
  - `docs/wiki/{Development:-Redraw-Loop,Development:-Animation-Timing,Window-Effects,Configuration:-Debug-Options}.md`.
- cosmic-comp: https://github.com/pop-os/cosmic-comp, files `src/backend/render/wayland/blur_effect.rs` and `src/wayland/handlers/background_effect.rs`.
- Smithay `023b78b1`, files:
  - `src/wayland/session_lock/{mod,lock}.rs`;
  - `src/backend/renderer/damage/mod.rs`;
  - `src/backend/renderer/element/solid.rs`;
  - `src/backend/renderer/gles/element.rs`;
  - `src/backend/drm/compositor/mod.rs`;
  - `src/wayland/image_copy_capture/mod.rs`.
- Protocols: https://wayland.app/protocols/ext-session-lock-v1 and https://wayland.app/protocols/presentation-time.
- Settings portal: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html

## 12. Follow-up

Commits that apply this audit, in order:

| commit | section | what |
|---|---|---|
| `718605f` | §1.1 | A second lock no longer replaces the live one; locking ends the grab and pointer focus; shortcuts, foreign and infinity respect `locked`. |
| `61bb3ea` | §3 | `session_destroyed` frees capture sessions. |
| `74e9df2` | §1.3, §2.3 | Wallpaper destruction order; the portal ignores `Defaults` events. |
| `b4e2796` | §2.2 (4) | presentation-time with CLOCK_MONOTONIC and the mode refresh. |
| `cfcf7c0` | §2.2 (1) | One render per vblank and no idle loop. |
| `18d90a5` | §2.2 (2, 3) | Stable ids and commits for shadow, border, titlebar and solids; `buffer_age` in winit. |
| `5a9f87c`, `4e27331`, `b181161` | §4 | Dead code in the compositor, `malaysia-scene` and the wallpaper. |
| `1ef49da`, `b84eaff` | §7 | Orphan scripts, probes and flake entries. |
| `cb630a2`, `b2a90e0`, `03b2ca3`, `d0bba82` | §2.3, §3, §1.1 | PX-14, PX-09, `forget_window` and relative-pointer deltas. |
| `5b56fb8` | §10 | `deuda.md` brought up to date. |
| `41b0661` | §2.2 | Cause of the lag after `cfcf7c0`: every commit queues a redraw (below). |
| `78c6a2e` | §2.2 (6) | Primary-plane scanout for fullscreen, cursor plane and per-surface dmabuf feedback. `SKIP_CURSOR_ONLY_UPDATES` stays off: without VRR it freezes the cursor (Smithay `023b78b1`, `drm/compositor/mod.rs:2379`; niri only uses it with VRR). |
| `4b5c35c` | §2.2 (6) | Overlay planes behind `debug.overlay_planes`, off by default. Smithay `023b78b1` does not set the `pixel blend mode`; on i915 they stay off (S-02). |
| `31c4dd0` | §8.1 | `text-input-v3` and `input-method-v2`; IME popups below the text cursor, or above it when they do not fit. |
| `338da33` | §8.1, §1.1 | `virtual-keyboard` and `virtual-pointer` (Smithay redesign `7454d68a`, `2bf8be86`, `9786228e`), trusted clients only (S-03). |
| `3deb003` | §8.2 | Rendering generic over `MalaysiaRenderer` (GLES or `MultiRenderer`); the rounded clip wraps the surface element and keeps its partial damage. |
| `9e46f23` | §8.2 | DRM renders through `GpuManager`. |
| `fef7763` | §8.2 | Multi-GPU: composition on the render GPU and copy to the scanout GPU when the latter can render. |
| `fb45726` | §2.2 | Pointer motion requests a redraw on every move (below). |
| `06b144e` | §1 (2) | With auto-lock on, a logind `delay` inhibitor locks the session before suspend (`sleep.rs`). |
| `8206879` | §2.2 (3) | Opaque regions for the rounded frame; kit popup borders and shadows keep a stable id. |
| `4018070` | §4.1 (8) | Options without a reader are gone; the catalog keeps them as "pending". barrierkit pinned by full sha (`abeb5e6`). |
| `db8c86a` | §1.1 | While locked, keys skip the IME grab. Infinity no longer trusts `MALAYSIA_DEV`, cwd paths, `LD_PRELOAD` or `LD_AUDIT`. |
| `425d1bc` | §2.2, §2.3 | Primary scanout output per surface: hidden surfaces get a frame callback every 995 ms, presentation with ZeroCopy/HwClock, recovery after a DRM error, `renderD` feedback, fewer per-frame allocations, reused capture offscreen texture. |
| `48cdb3d` | §1 (7) | Shell apps with a double fork and `systemd-run --scope`; menubar and dock without polling (`use_watched`); launcher with `use_animation`. |
| barrierkit `50d2843`…`abeb5e6` | §2.3 | Skia cache released when idle, one SkSurface per image, bounded icon cache, one thread for artwork, dmabuf import drop order, `use_watched`, single traffic-light rule and new tokens. |

### Lag after `cfcf7c0`

1. **Symptom:** since `cfcf7c0`, the session lags on DRM and nested.
2. **Cause:** `CompositorHandler::commit` asked for a redraw only if `commit_requests_redraw` saw a buffer, damage or frame callbacks. It ran after `on_commit_buffer_handler`, which in Smithay `023b78b1` has already done `attrs.buffer.take()` and `attrs.damage.drain(..)` (`src/backend/renderer/utils/wayland.rs:155,200`). A commit with a new buffer and no frame callback asked for no frame. The unconditional render on every vblank hid this; `cfcf7c0` exposed it.
3. **Fix:** every commit queues a redraw, as niri does (`src/handlers/compositor.rs`, `queue_redraw` on every commit of a mapped window). The damage tracker decides whether anything needs painting; a frame without damage sends frame callbacks and arms the estimated vblank.
4. **Checked against niri** (`src/niri.rs`: `RedrawState`, `queue_redraw`, `send_frame_callbacks`; `src/backend/tty.rs`: `on_estimated_vblank_timer`):
   - a frame without damage sends frame callbacks (`finish_empty_frame`);
   - the cursor and commits queue a redraw while a frame is in flight (`Udev::queue_redraw`);
   - `RoundedTextureElement` damages the whole element when its commit changes, so it loses no damage;
   - `schedule_repaint` keeps its token and cancels no repaint: it only defers it to the estimated vblank.
5. **Confirmed on the development machine** (2026-11-03):
   - The DRM session of the previous boot (1920x1080@60, through GDM) ran a binary built exactly from `cfcf7c0`. This was checked by hashing `udev.rs`, `session.rs` and `handlers.rs` against the rev pinned in the dotfiles flake.
   - The journal shows no frame errors: the lag came from frames that were never requested, not from failed frames.
   - That session also lacked `18d90a5`, so it repainted the whole output with one window visible.
   - Nested did not show it because `./dev.sh run` built a local checkout older than `cfcf7c0`, which still rendered on every vblank.
   - Validated on `23c1121` (2026-11-03): commits already queued frames, but the lag remained with no client commits.
6. **Second cause, confirmed while validating `23c1121`:** `pointer_motion_raw` requested a redraw only for grabs and cursor-image changes. Other motion relied on the unconditional vblank render that `cfcf7c0` removed; without client commits, the cursor advanced only when another surface requested a frame. That is why it was smooth over the dock and slow over the wallpaper. Fix: `request_redraw` on every pointer motion.

Pending: the `bandura.jpg` licence (§1 (9)) is the author's call; barrierkit's `sidebar_section_chevron_changes_shape_when_toggled` test is flaky on `73590c5` too.
