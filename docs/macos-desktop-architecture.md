# macOS desktop architecture compared with Malaysia and barrierkit

- **Goal:** describe how the macOS desktop is wired underneath (processes,
  IPC, preferences, app integration, window management), compare each piece
  with Malaysia and barrierkit, and list what can be copied, renamed or
  changed.
- **Scope:** Mac OS X 10.0 to macOS 26 Tahoe. macOS 27 "Golden Gate"
  shipped on 2026-09-14
  ([9to5Mac](https://9to5mac.com/2026/09/09/apple-confirms-macos-27-golden-gate-launch-date-september-14/));
  it is not covered beyond that note. Visual design is out of scope (see
  barrierkit `docs/research/styling-apple.md`).
- **Reader:** whoever decides the next blocks of `roadmap.md` or changes a
  public name in barrierkit.
- **Inputs:** Apple documentation and release notes, Apple open source,
  WWDC sessions, man pages, and Eclectic Light, yabai and similar sources
  where Apple is silent. Malaysia at `1d5d25c`, barrierkit at `7172d25` (section 8 applied on `798bc89`).
- **Outputs:** the comparison tables (sections 1–6) and the proposals
  (section 8). Nothing here is decided; decisions go to an ADR.

Source marks: **[A]** Apple; **[C]** community (Eclectic Light, yabai,
forums); **[I]** inference from the cited facts; **[U]** not confirmed.

## 1. Processes: who owns what

macOS splits the desktop into small processes that launchd starts and
restarts. Malaysia does the same with systemd user units.

| macOS process | Owns | Malaysia | Status |
|---|---|---|---|
| `launchd` (PID 1, since 10.4), domain `gui/<uid>`, agents with `LimitLoadToSessionType = Aqua` (10.5) [A] ([lifecycle](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/Lifecycle.html), [TN2083](https://developer.apple.com/library/archive/technotes/tn2083/_index.html)) | starting and restarting every desktop agent (`KeepAlive`) | `systemd --user`, `malaysia-session.target`, `malaysia-shell@.service`, `Restart=on-failure` (`share/systemd/user/`) | done |
| `loginwindow` | login, launching Dock/Finder/SystemUIServer, logout: `kAEQuitApplication` to each app, 45 s, then `SIGKILL` [A] (lifecycle) | display manager + `share/wayland-sessions/malaysia.desktop`; Log Out calls logind `TerminateSession` directly (`malaysia-shell/src/power.rs`) | partial: no graceful quit of apps before logout |
| `WindowServer` (SkyLight.framework) | compositing, window list, input routing, window ids; restarted on crash, otherwise logout [C] ([Eclectic Light](https://eclecticlight.co/2020/06/08/windowserver-display-compositor-and-input-event-router/)) | `malaysia-compositor` | done; nothing restarts the compositor (ADR 0023: supervision is a proposal) |
| `Dock.app` | Dock, Spaces, Mission Control, Exposé, hot corners; "the entire spaces system is implemented in Dock.app" and it holds the privileged WindowServer connection [C] ([yabai wiki](https://github.com/koekeishiya/yabai/wiki/Disabling-System-Integrity-Protection)) | Dock: `malaysia-shell dock`. Spaces and window policy: the compositor (`spaces.rs`), driven by the shell through Infinity | split differently (see section 5) |
| `WindowManager.app` (13) | Stage Manager; Sequoia tiling settings share its `com.apple.WindowManager` domain [C] ([Eclectic Light](https://eclecticlight.co/2023/01/25/curious-bugs-in-help-viewer-and-stage-manager/)) | `arrange*` requests in Infinity, compositor tiling | tiling done; no Stage Manager |
| `SystemUIServer` (10.1) and `ControlCenter.app` (11) | menu extras; system status items moved to Control Center in Big Sur [C] ([Eclectic Light](https://eclecticlight.co/2020/09/25/whats-right-at-the-end-of-the-menu-bar-and-how-its-changing/)) | `malaysia-shell menubar`: StatusNotifierWatcher and host (`bk/src/shell/status_items.rs`) | status items done; no Control Center, no volume/network/battery |
| `NotificationCenter.app`, `usernoted` [A] ([man](https://keith.github.io/xcode-man-pages/usernoted.8.html)) | notifications, widgets | none | missing (roadmap 12b) |
| `WallpaperAgent.app` (14) | wallpaper, split out of the Dock; Dock's `desktoppicture.db` retired [C] ([Apple forum, DTS](https://developer.apple.com/forums/thread/761394)) | `malaysia-wallpaper` (ADR 0049) | done, same split |
| `Finder` | desktop icons | none | missing, not planned |
| `mds`, `mdworker` (Spotlight) [A] ([man](https://keith.github.io/xcode-man-pages/mds.8.html)) | metadata index and search | `malaysia-shell launcher` (apps only, `.desktop` parsing) | partial |
| `cfprefsd` (10.8) [A] ([CF release notes](https://developer.apple.com/library/archive/releasenotes/DataManagement/RN-CoreFoundationOlderNotes/index.html)) | preference cache and asynchronous writes | none: each process reads its `.domain` file and watches it with inotify (`ctappearance/src/watch.rs`) | different by design (section 2) |
| `distnoted`, `notifyd` [A] ([DistributedNotificationCenter](https://developer.apple.com/documentation/foundation/distributednotificationcenter), [notify(3)](https://github.com/apple-oss-distributions/Libnotify/blob/main/notify.3)) | system-wide notifications | inotify on `.domain` files; portal `SettingChanged` | done |
| `lsd`, `coreservicesd` (Launch Services) [A] ([man](https://keith.github.io/xcode-man-pages/lsd.8.html)) | app registry, document types, URL schemes, default apps | XDG: `.desktop`, `shared-mime-info`, `mimeapps.list`; parsed in `malaysia-shell/src/desktop.rs` | registry read; no "Open With" or default-app UI |
| `pboard` | pasteboard | `wl_data_device`, primary selection, drag and drop in the compositor | done; no `ext-data-control` |
| `talagent` (10.7) [A] ([man](https://keith.github.io/xcode-man-pages/talagent.8.html)) | Resume, automatic termination | `<app_id>.state` in bk (`bk/src/app/defaults.rs`) | partial: per-app restore only |
| `backgroundtaskmanagementd` (13) [A] ([man](https://keith.github.io/xcode-man-pages/backgroundtaskmanagementd.8.html)) | login items and their approval list | none (no XDG autostart) | missing |
| `tccd` (10.8, current form 10.14) [C] ([Eclectic Light](https://eclecticlight.co/2025/11/08/explainer-permissions-privacy-and-tcc/)) | per-app consent: Accessibility, Screen Recording, Automation | `security-context-v1` filter only (`security.rs`) | missing per-app consent |
| `universalaccessd` | accessibility settings | `appearance.contrast`, `reduced_motion`; no AT-SPI bus setup | partial |

Takeaways:

1. The two architectures have the same shape: one compositor and many small
   restartable clients under a service manager. Malaysia's
   one-process-per-component rule (ADR 0023) is what macOS has done since
   10.4. In 14, Apple moved the wallpaper out of the Dock, the same split as
   ADR 0049.
2. The real gaps are services, not architecture: notifications, login
   items, per-app consent, a status/control panel, graceful logout.

## 2. Preferences: CFPreferences against `.domain`

| Aspect | macOS | Malaysia |
|---|---|---|
| Unit | domain = bundle id (`com.apple.dock`), plus `NSGlobalDomain` [A] ([CFPreferences](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFPreferences/Concepts/PreferenceDomains.html)) | one TOML `<name>.domain` per area; per-app `<app_id>.domain` |
| Files | `~/Library/Preferences/<domain>.plist`; `ByHost/` per machine; `/Library/Preferences` for all users | `~/.config/malaysia/` (preferences) and `~/.local/share/malaysia/` (state); `MALAYSIA_CONFIG_DIR`/`DATA_DIR` |
| Lookup order | Managed → Argument → App → Suite → Global → Registration [A] ([UserDefaults](https://developer.apple.com/documentation/foundation/userdefaults)) | the file, then the defaults in code |
| Who writes | `cfprefsd`; editing the plist directly "is unlikely to have the expected results" since 10.8 [A] | the owner writes the file atomically (`ctappearance::write_appearance`); editing by hand is supported |
| Change notice | none in general; KVO on `UserDefaults` across processes, plus ad-hoc distributed notifications such as `AppleInterfaceThemeChangedNotification` [A]/[C] | inotify on the directory; every consumer reloads (`AppearanceWatch`) |
| Applying | many system domains need `killall Dock` or `killall SystemUIServer` [C] | live, no restart |
| CLI | `defaults read/write/delete/rename/domains/find/-currentHost/-g` [C] ([ss64](https://ss64.com/mac/defaults.html)) | `domain` command: planned, no code (`options.md`, "The `domain` command") |
| Managed values | configuration profiles, `Forced` and `Set-Once`; apps grey out with `CFPreferencesAppValueIsForced` [A] ([payload](https://github.com/apple/device-management/blob/release/mdm/profiles/com.apple.ManagedClient.preferences.yaml), [API](https://developer.apple.com/documentation/corefoundation/cfpreferencesappvalueisforced(_:_:))) | none; NixOS options only set what the module installs |

Takeaways:

1. Malaysia's "the file is the API" model is simpler than cfprefsd and
   reloads live. macOS needed `killall Dock` for the same keys.
2. That weakens the case for `org.malaysia.Compositor1` (ADR 0021) as a
   preferences channel. macOS apps do not set Dock preferences through IPC;
   they write the domain. A `domain` command that writes the file atomically
   gives the same result. D-Bus is only needed for actions that are not
   preferences.
3. The active app lived in `~/.local/share/malaysia/window.domain`, the same
   name as the window preferences in `~/.config/malaysia/`. The watcher only
   reports configuration domains, so the menubar never saw it change; it now
   takes the active app from foreign-toplevel, as macOS apps ask
   `NSWorkspace`, and the file is gone. State in the data directory is named
   `.state` (`recents.state`, `wallpaper-selection.state`).
4. No code parses `dock.domain` or `effects.domain`; `dock.md` now says so.

## 3. Appearance

| macOS | Since | Malaysia / barrierkit |
|---|---|---|
| `AppleInterfaceStyle = Dark`, `AppleInterfaceStyleSwitchesAutomatically` (Auto) in `NSGlobalDomain` [C] | 10.14, Auto 10.15 | `appearance.color_scheme`: `auto` resolves to `light` (`ctappearance/src/config.rs`); no time-based switch |
| `AppleAccentColor` (integers −1…6) and app accent via `NSAccentColorName` when the user picks multicolor [A] ([HIG Color](https://developer.apple.com/design/human-interface-guidelines/color)) | 10.14, app accent 11 | `appearance.accent_color` as RGB or `null`; no "app picks its own accent" |
| `NSApp.effectiveAppearance` KVO; appearances `aqua`, `darkAqua`, `vibrant*`, `accessibilityHighContrast*` [A] ([names](https://developer.apple.com/documentation/appkit/nsappearance/name-swift.struct)) | 10.14 | `bk::theme::resolved()` plus the inotify watch; GTK/Qt through the portal |
| Semantic dynamic colours (`labelColor`, `controlAccentColor`, `windowBackgroundColor`) [A] | 10.14 | `ctdesign` tokens resolved per scheme |
| Desktop tinting: `windowBackgroundColor` takes colour from the wallpaper in Dark mode; user toggle since 11 [A] ([docs](https://developer.apple.com/documentation/appkit/nscolor/windowbackgroundcolor)) | 10.14 | `wallpaper_tinting`: the compositor sends each BarrierKit window the wallpaper behind it (ADR 0065); `color_scheme = dark_tint` is a separate, cooler dark palette (26 of 81 Tengen tokens differ), not a tint |
| Reduce Transparency, Increase Contrast, Reduce Motion via `NSWorkspace` and `accessibilityDisplayOptionsDidChangeNotification` [A] ([NSWorkspace](https://developer.apple.com/documentation/appkit/nsworkspace)) | 10.10 / 10.12 | `reduce_transparency` (default `true`, ADR 0038), `contrast`, `reduced_motion` |
| Liquid Glass `NSGlassEffectView`; Clear/Tinted choice; icon styles Default/Dark/Clear/Tinted [A] ([NSGlassEffectView](https://developer.apple.com/documentation/appkit/nsglasseffectview), [adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass)) | 26, Clear/Tinted 26.1 | `.glass` modifier and `ButtonStyle::glass` in bk; no icon styles |
| Cross-fade on theme change | — | `theme_fade.rs` (ADR 0044); macOS does not document one |

Takeaways:

1. The appearance path matches macOS closely, and the portal gives
   non-native apps what macOS gives only AppKit apps.
2. Tinting is a separate switch in both: macOS's "Allow wallpaper tinting
   in windows" and Malaysia's `window_color`. `dark_tint` has no macOS
   counterpart; it is a second dark palette.
3. macOS Auto follows the time of day. In Malaysia `auto` is the same as
   `light`, which is "auto" in name only.

## 4. App ↔ desktop integration (AppKit/SwiftUI against bk)

| macOS API | System side | bk / Malaysia | Status |
|---|---|---|---|
| `NSApplication.mainMenu`; HIG order Apple › App › File › Edit › Format › View › Window › Help [A] ([HIG menu bar](https://developer.apple.com/design/human-interface-guidelines/the-menu-bar)) | WindowServer composites the app's menu-bar windows [U] | `MainMenu` → `com.canonical.dbusmenu` + `org_kde_kwin_appmenu` → menubar (`bk/src/main_menu/server.rs`) | done |
| Items the system inserts: Services, Help search, Start Dictation and Emoji & Symbols in Edit, Writing Tools (15.2), Window › Move & Resize (15) [A] ([helpMenu](https://developer.apple.com/documentation/appkit/nsapplication/helpmenu), [WWDC24 10124](https://developer.apple.com/videos/play/wwdc2024/10124/)) | AppKit and the system | menubar adds Help search (`help_topics.rs`) and a Services submenu with one disabled "none" item (`focused.rs:134`), like macOS's "No Services Apply" | partial |
| `NSStatusItem` (`isVisible`, `autosaveName`, ⌘-drag reorder); `MenuBarExtra` (13); per-app "Allow in the Menu Bar" (26) [A] ([NSStatusItem](https://developer.apple.com/documentation/appkit/nsstatusitem), [MenuBarExtra](https://developer.apple.com/documentation/swiftui/menubarextra)) | app process draws its item; ControlCenter/SystemUIServer host system items [C] | `bk::status_item::StatusItem` over SNI | done; user hide/reorder missing |
| `ControlWidget` in Control Center or the menu bar (26) [A] ([docs](https://developer.apple.com/documentation/swiftui/controlwidget)) | Control Center | none | missing |
| Activation policy `regular`/`accessory`/`prohibited`, `LSUIElement` [A] ([docs](https://developer.apple.com/documentation/appkit/nsapplication/activationpolicy-swift.enum)) | Dock and menu bar show or hide the app | none; the dock shows every window | missing |
| Cooperative activation: `activate()`, `yieldActivation(to:)` (14) [A] ([docs](https://developer.apple.com/documentation/appkit/nsapplication/yieldactivation(to:))) | WindowServer | `xdg-activation-v1` with serial checks (`handlers.rs`); the launcher passes `XDG_ACTIVATION_TOKEN` | done; Wayland has had this model from the start |
| Reopen event when the Dock icon is clicked with no windows [A] ([docs](https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationshouldhandlereopen(_:hasvisiblewindows:))) | Dock → app | single instance with `org.freedesktop.Application.Activate` (`bk/src/app/application.rs`) | kit side done; the dock does not call it |
| Quit: `applicationShouldTerminate` at app level; sudden/automatic termination [A] | loginwindow, Dock | Super+Q sends `xdg_toplevel.close` to each window of the `app_id` (`app_shortcuts.rs`) | different: no app-level quit |
| Dock tile badge `NSDockTile.badgeLabel`, dock menu `applicationDockMenu` [A] ([NSDockTile](https://developer.apple.com/documentation/appkit/nsdocktile), [dock menu](https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationdockmenu(_:))) | Dock draws them | none | missing |
| `collectionBehavior` (`canJoinAllSpaces`, `managed`, `transient`, `stationary`, `primary`/`auxiliary`), window levels, `NSPanel` [A] ([docs](https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct)) | Dock and WindowManager | `infinity_window_style_v1.set_panel` (`hides_on_deactivate`, `floating`, `key_only_if_needed`) and `UtilityWindow` | partial: no Spaces behaviour per window |
| `NSWindow.ToolbarStyle` `unified`, `unifiedCompact`, `expanded` (11) [A] | AppKit | Infinity `set_bar`: `titlebar` (no toolbar), `unified_compact`, `unified`, `utility` | done |
| Window tabbing (10.12), full screen as a Space (10.7) [A] | AppKit, Dock | focus mode = full-screen space (ADR 0051); no tabs | partial |
| `NSWindowRestoration` [A] | `talagent` | `<app_id>.state` | done |
| `NSServices` + `pbs` [A] ([properties](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/SysServices/Articles/properties.html)) | pasteboard server | none; Linux has no shared standard | empty submenu only |
| Launch Services: `CFBundleDocumentTypes`, `UTType`, default apps (12) [A] | `lsd` | XDG MIME | read only |
| `NSDocumentController` Open Recent [A] | — | `recently-used.xbel` + `recents.domain` in the ❄ menu | done |
| `UNUserNotificationCenter` (10.14), badges (13), Focus (12) [A] | `usernoted` | bk is a client (`bk/src/notifications.rs`); no server | missing server |
| Core Spotlight, App Intents, `.mdimporter` [A] | `mds` | launcher searches apps only | partial |
| `AXUIElement`, `AXIsProcessTrusted` [A] | TCC | accesskit over AT-SPI (`bk/src/platform/atspi.rs`); session bus not set up | partial |
| ScreenCaptureKit (12.3), `SCContentSharingPicker` (14); 15 prompts apps that skip the picker [A] ([15 release notes](https://developer.apple.com/documentation/macos-release-notes/macos-15-release-notes)) | WindowServer + TCC | `ext-image-copy-capture-v1` for unsandboxed clients; no portal ScreenCast | partial |

## 5. Window management

1. **Same split of effects.** macOS apps ask for a material
   (`NSVisualEffectView`, `.behindWindow` by default) and the server
   computes the backdrop: an app cannot see other apps' pixels [A]/[I]
   ([docs](https://developer.apple.com/documentation/appkit/nsvisualeffectview)).
   Shadows are also recomputed server-side (`invalidateShadow`). ADR 0020
   (compositor-side effects for every window) is the same model.
   `infinity_window_style_v1` is the place for a future "material" request.
2. **Different owner of policy.** macOS keeps the compositor and the
   window manager apart: WindowServer composites, the Dock decides Spaces
   through private SkyLight calls, and third parties get only Accessibility
   (`kAXPositionAttribute`) [C]. In Wayland the compositor must own policy,
   so Malaysia puts Spaces in the compositor and gives the shell a private,
   trusted channel (Infinity, checked against `/proc/pid/exe`). Infinity's
   window manager plays the role of the Dock's SkyLight connection; the
   glossary already maps Infinity to SkyLight.
3. **Spaces.** macOS has had Spaces per display since 10.9 ("Displays have
   separate Spaces", logout to apply), full-screen apps as their own Space
   since 10.7, `mru-spaces`, and "Assign To desktop" in the Dock menu [A]
   ([Apple Support](https://support.apple.com/guide/mac-help/work-in-multiple-spaces-mh14112/mac)).
   Malaysia has up to 99 desktops and focus mode as a full-screen space; no
   per-display Spaces, no assignment, no Mission Control.
4. **Shortcuts differ on purpose.** macOS switches Spaces with Ctrl+←/→
   because word jumps use Option. On Linux, Ctrl+←/→ is the word jump in
   most apps, so Malaysia uses Ctrl+Alt. Super takes ⌘'s role (Super+Q,
   Super+H). Keep this.
5. **Tiling.** Sequoia added edge-drag tiling and Window › Move & Resize
   with no app API [A] (WWDC24 10124). Malaysia already has `arrange`,
   `arrange_pair` and `arrange_quarters` (Ctrl+Super+arrows), but the
   shortcuts are missing from the table in `options.md`.

## 6. History that matters here

| Version | Change | Lesson for Malaysia |
|---|---|---|
| 10.4 | launchd replaces init, SystemStarter, cron, inetd | one service manager; done with systemd |
| 10.5 | Spaces, Core Animation, session types | — |
| 10.7 | Resume, automatic termination, sandbox, Mission Control, Launchpad | `<app_id>.state` covers Resume |
| 10.8 | cfprefsd caches preferences; direct edits stop working; TCC appears | keep files as the API, written atomically |
| 10.9 | Spaces and menu bar per display | per-display Spaces is the expected model |
| 10.14 | Dark Mode with an API; Automation consent for Apple Events | appearance through one source; consent belongs to the system |
| 10.15 | Screen Recording consent; window titles hidden without it | capture needs consent, not only sandbox detection |
| 11 | Control Center takes system status items | system indicators live in one shell panel |
| 13 | System Settings in SwiftUI; `SMAppService` login items with a visible list | autostart must be visible and switchable |
| 14 | cooperative activation; WallpaperAgent leaves the Dock | already matched by xdg-activation and ADR 0049 |
| 15 | tiling; capture prompts unless the system picker is used | portal ScreenCast with a chooser avoids prompts |
| 26 | Launchpad folded into Spotlight "Apps"; per-app menu-bar permission; Controls; Liquid Glass Clear/Tinted (26.1) | one launcher is enough; status items need user control |

Sources: Apple newsroom pages for
[10.5](https://www.apple.com/newsroom/2007/10/16Apple-to-Ship-Mac-OS-X-Leopard-on-October-26/),
[10.7](https://www.apple.com/newsroom/2011/06/06Mac-OS-X-Lion-With-250-New-Features-Available-in-July-From-Mac-App-Store/),
[10.9](https://www.apple.com/newsroom/2013/06/10Apple-Releases-Developer-Preview-of-OS-X-Mavericks-With-More-Than-200-New-Features/),
[11](https://www.apple.com/newsroom/2020/06/apple-introduces-macos-big-sur-with-a-beautiful-new-design/),
[13](https://www.apple.com/newsroom/2022/06/macos-ventura-brings-powerful-productivity-tools-new-continuity-features-to-mac/),
[15](https://www.apple.com/newsroom/2024/06/macos-sequoia-takes-productivity-and-intelligence-on-mac-to-new-heights/),
[26](https://www.apple.com/newsroom/2025/06/macos-tahoe-26-makes-the-mac-more-capable-productive-and-intelligent-than-ever/);
[SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice);
[Launchpad in 26](https://www.macworld.com/article/2846899/this-terminal-trick-that-brings-back-launchpad-on-macos-26-no-longer-works.html) [C].

## 7. Names: what already matches

- barrierkit follows its naming rule (SwiftUI, then AppKit without `NS`):
  `Application`, `Scene`, `UtilityWindow`, `MainMenu`, `StatusItem`,
  `SettingsWindow`, `AboutView`, `ContentUnavailableView`, `.help`,
  `.context_menu` (`docs/barrierkit/20-glossary.md`).
- Infinity's panel flags follow `NSPanel` (`hidesOnDeactivate`,
  `isFloatingPanel`, `becomesKeyOnlyIfNeeded`).
- `dock.domain` keys are `com.apple.dock`'s (`tilesize`, `mineffect`,
  `show-recents`); `screenshot` follows `com.apple.screencapture`.
- User-facing names such as Spotlight, Mission Control, Launchpad and
  Control Center are Apple trademarks; the glossary rule applies to API
  names, not to product features shown to the user.

## 8. Proposals

Each item says where it goes. Sections 8.1 and 8.2 are applied; the rest needs the author's decision.

### 8.1 Documentation fixes (applied)

1. `docs/architecture/dock.md` and `options.md`: say that `dock.domain` and
   `effects.domain` are not read yet.
2. `options.md`: add the `arrange` shortcuts (Ctrl+Super+arrows, Shift for
   pairs) to "Fixed compositor shortcuts".
3. `roadmap.md`: remove the stale "StatusNotifierItem tray" item from
   "Shell" and "drag & drop was not touched" from block 6; reconcile
   "virtual input only for the shell" with `security::unrestricted` in
   `state.rs` (any unsandboxed client can bind it).

### 8.2 Renames (applied)

| Before | After | Reason | Repo |
|---|---|---|---|
| `~/.local/share/malaysia/window.domain` (active app) | removed | clashed with the preferences file `window.domain` and had no live reader; the menubar uses foreign-toplevel. macOS: `NSWorkspace.frontmostApplication` | malaysia |
| `recents.domain`, data `wallpaper.domain` | `recents.state`, `wallpaper-selection.state` | state, not preferences | malaysia, barrierkit |
| Infinity `set_bar` entry `compact`, `bk` `Bar::Compact`, `WindowCorner::compact` | `unified_compact`, `Bar::UnifiedCompact`, `WindowCorner::unified_compact` | `NSWindow.ToolbarStyle.unifiedCompact`, per barrierkit's naming rule. Entry names are not on the wire, so the protocol version stays | barrierkit, malaysia |

Kept: `titlebar` (a window without a toolbar has no `ToolbarStyle`) and
`dark_tint` (a palette of its own, see section 3).

### 8.3 Copy as is (small, mapped to existing standards)

1. **Dock: one tile per app, not per window.** Group by `app_id`, pin apps
   from `dock.domain` `persistent-apps` (macOS key), and read the
   `.desktop` icon. Clicking an app without windows calls
   `org.freedesktop.Application.Activate`: macOS's reopen event. Repo:
   malaysia (`dock.rs`).
2. **Dock badges and menus.** `com.canonical.Unity.LauncherEntry` (badge,
   progress) as `NSDockTile`; `.desktop` `Actions` as `applicationDockMenu`.
   A bk API such as `DockTile::badge_label` would sit on the first. Repos:
   malaysia (dock), barrierkit (API).
3. **Window menu items the system adds.** Move & Resize (calls Infinity
   `arrange*`), Enter Full Screen and the window list, inserted by the
   menubar like AppKit does. Repo: malaysia (menubar).
4. **Graceful Log Out, Restart and Shut Down.** Close every toplevel and
   wait (loginwindow waits 45 s) before calling logind. Repo: malaysia
   (`power.rs`, plus a compositor request).
5. **`auto` that follows the time of day.** Or remove `auto` until it
   does. Repo: barrierkit (`ctappearance`).
6. **XDG autostart.** Pull in `xdg-desktop-autostart.target` from
   `malaysia-session.target`, and list the entries in Settings (Ventura's
   Login Items). Repo: malaysia (`nix/malaysia.nix`, settings).

### 8.4 Larger features, macOS model first

1. **Notifications server** (`org.freedesktop.Notifications`) in the shell,
   on the Top layer. macOS: `usernoted` + Notification Center.
2. **Control panel in the menubar** with volume, network, battery and
   brightness. macOS: ControlCenter.app. Expose system toggles as data so
   apps could add controls later (macOS 26 `ControlWidget`).
3. **Status item visibility and order**, stored in `menubar.domain`.
   macOS: `NSStatusItem.isVisible`, `autosaveName`, 26's "Allow in the
   Menu Bar".
4. **Capture consent.** Implement the portal `ScreenCast` and `Screenshot`
   with a chooser, so apps go through the picker as macOS 15 asks; keep
   direct `ext-image-copy-capture` for the trusted shell.
5. **`domain` command** with `defaults` verbs, writing files atomically;
   keep `org.malaysia.Compositor1` only for actions (section 2).
6. **Spaces per display**, an "assign app to desktop" option, and later an
   overview. macOS: 10.9, the Dock's Options menu, Mission Control.
7. **Activation policy in bk** (`regular`, `accessory`) when the first
   status-item-only app appears: the dock and menubar skip `accessory`
   apps. Not before (rule 2).

### 8.5 Changes to how the desktop is used

1. Quit becomes app-level for apps that export
   `org.freedesktop.Application` (bk apps): Super+Q calls the app's `quit`
   action, which can ask to save, as `applicationShouldTerminate` does.
   Other apps keep `xdg_toplevel.close` per window.
2. The dock shows apps, and windows only inside the app's menu.
3. The launcher stays the single entry point; macOS 26 removed Launchpad in
   favour of Spotlight's app list, which is what the launcher already is.

## 9. Not confirmed

- Which process draws the app menu bar, Notification Center and desktop
  widgets; the exact ControlCenter/SystemUIServer split.
- That Mission Control runs inside the Dock (yabai's evidence is strong but
  not from Apple).
- The defaults keys for Liquid Glass Clear/Tinted, icon style and wallpaper
  tinting.
- Whether cross-process KVO on `UserDefaults.standard` fires reliably.
