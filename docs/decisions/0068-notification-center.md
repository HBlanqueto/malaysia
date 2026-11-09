# 0068 — Notification Center

Status: accepted (2026-11-08), all four phases.

## Goal and scope

This ADR fixes how Malaysia receives, shows, stores and silences
notifications, modelled on macOS 26 Tahoe. It covers the server, the banners,
the Notification Center panel, Do Not Disturb and Focus, the lock screen,
badges and progress, and Live Activities. Widgets and Control Center only
appear where they touch the panel. Readers: whoever builds the
`notifications` shell component, the portal, Settings, the menubar, the dock
and the lock.

Labels: [A] Apple official, [R] review, [E] reverse-engineering, [S]
specification or upstream code, [I] upstream issue. Nothing here comes from a
forum alone.

## Context: how macOS 26 does it

### Processes and storage

1. `usernoted` is the per-user daemon that owns notifications
   ([usernoted(8)](https://keith.github.io/xcode-man-pages/usernoted.8.html))
   [A]. The panel is a separate `NotificationCenter` process. Apps talk to the
   daemon through `UNUserNotificationCenter`
   ([docs](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter))
   [A].
2. Delivered notifications live in a SQLite database,
   `~/Library/Group Containers/group.com.apple.usernoted/db2/db` since macOS
   15, with tables `app`, `record` and per-app lists `delivered`,
   `displayed` and `snoozed`
   ([plaso parser](https://plaso.readthedocs.io/en/latest/_modules/plaso/parsers/sqlite_plugins/macos_notification_center.html))
   [E]. History survives restarts.

### What a notification is

1. **Content:** title, subtitle, body, attachments, sound, badge,
   `threadIdentifier`, `categoryIdentifier`, `interruptionLevel`,
   `relevanceScore`, `filterCriteria` and `targetContentIdentifier`
   ([UNNotificationContent](https://developer.apple.com/documentation/usernotifications/unnotificationcontent))
   [A].
2. **Interruption levels** (macOS 12)
   ([docs](https://developer.apple.com/documentation/usernotifications/unnotificationinterruptionlevel))
   [A]:

   | Level | Behaviour |
   |---|---|
   | `passive` | Goes to the list only; no sound, no banner. |
   | `active` | Default. Banner and sound; respects Focus. |
   | `timeSensitive` | Breaks through Focus and summaries if the user allows it. |
   | `critical` | Breaks through mute and Do Not Disturb; needs Apple's entitlement and the user's «Allow critical alerts». |

3. **Provisional delivery:** an app may post without asking. Its
   notifications appear only in the panel, with Keep and Turn Off buttons
   ([docs](https://developer.apple.com/documentation/usernotifications/asking-permission-to-use-notifications))
   [A].
4. **Communication notifications** show the sender's picture with a small app
   badge and the sender as title
   ([HIG](https://developer.apple.com/design/human-interface-guidelines/notifications),
   [WWDC21 10091](https://developer.apple.com/videos/play/wwdc2021/10091/))
   [A].
5. **Replace and remove:** a post with an existing identifier replaces the
   notification; the app can remove delivered ones
   ([identifier](https://developer.apple.com/documentation/usernotifications/unnotificationrequest/identifier))
   [A].
6. **Actions:** a category holds actions with the options `foreground`,
   `destructive` and `authenticationRequired`, plus text input actions. The
   HIG shows up to four buttons in the detail view [A]. A response carries
   the action id, the default action (clicked) or an explicit dismiss
   ([handling](https://developer.apple.com/documentation/usernotifications/handling-notifications-and-notification-related-actions))
   [A].
7. **Foreground app:** the system asks the app whether to show the banner
   (`willPresent`); by default the front app shows none [A].
8. **Badges** are set by the app (`setBadgeCount`, macOS 13). The HIG says a
   badge counts unread items and that zero clears the app's notifications
   [A].

### What the user sees

1. **Opening the panel:** click the date and time in the menu bar, or swipe
   left with two fingers from the right edge of the trackpad. Click the
   desktop, the clock again or swipe right to close. Option-click the clock
   toggles Do Not Disturb
   ([get notifications](https://support.apple.com/guide/mac-help/get-notifications-mchl2fb1258f/26/mac/26),
   [Focus](https://support.apple.com/guide/mac-help/mchl999b7c1a/26/mac/26))
   [A].
2. **Panel content:** notifications on top, widgets below, and «Edit Widgets»
   at the bottom
   ([widgets](https://support.apple.com/guide/mac-help/add-and-customize-widgets-mchl52be5da5/26/mac/26))
   [A].
3. **Stacks:** one app's notifications stack; a click expands the stack,
   «Show less» collapses it, and hovering the top card's Clear button turns it
   into «Clear All» [A].
4. **Card:** large app icon at the leading edge, title (or app name),
   body, and an arrow that shows details and actions. «More» offers mute,
   turn off and Notification Settings. Hidden previews show only the icon and
   «Notification» [A].
5. **Material:** Apple's Tahoe feature list gives Liquid Glass to the menu
   bar, Control Center, the Lock Screen, icons and widgets, but not to
   notifications
   ([features PDF](https://www.apple.com/os/pdf/All_New_Features_macOS_Tahoe_Sept_2025.pdf))
   [A]. Six Colors saw no glass in the panel during the beta
   ([first look](https://sixcolors.com/post/2025/07/first-look-macos-tahoe-public-beta/))
   [R].
6. **Volume and brightness** overlays now appear at the top right, under the
   menu bar, «notification-style»
   ([MacRumors](https://www.macrumors.com/2025/06/11/macos-tahoe-all-the-little-changes/))
   [R].

### Settings in Tahoe

Exact names from
[Change Notifications settings](https://support.apple.com/guide/mac-help/change-notifications-settings-mh40583/26/mac/26)
[A]:

1. **Global:** Show previews (Always / When Unlocked / Never); Show
   notifications when the display is sleeping, when the screen is locked, and
   when mirroring or sharing the display; Summarize notifications (Apple
   Intelligence).
2. **Per app:** Allow notifications; Alert Style, a location set (Desktop,
   Notification Center, Lock Screen) and a style (**Temporary** or
   **Persistent**); Allow critical alerts; Allow time sensitive alerts; Badge
   application icon; Play sound for notification; Show previews (Default /
   Always / When Unlocked / Never); Notification grouping (Automatic / By
   Application / Off).
3. **Change from Sequoia:** Banners and Alerts became Temporary and
   Persistent, and the «Show in Notification Center» and «lock screen»
   toggles became the location set
   ([15.0 page](https://support.apple.com/guide/mac-help/change-notifications-settings-mh40583/15.0/mac/15.0))
   [A]. Chrome confirms that `requireInteraction` no longer forces a
   persistent notification
   ([Chrome blog](https://developer.chrome.com/blog/notification-attribution-macos))
   [R].

### Focus

1. A Focus allows or silences chosen people and apps, has a Time Sensitive
   switch, call rules and Focus filters for Calendar, Mail, Messages and
   Safari
   ([set up a Focus](https://support.apple.com/guide/mac-help/set-up-a-focus-to-stay-on-task-mchl613dc43f/26/mac/26))
   [A].
2. An active Focus shows its icon in the menu bar; its duration can be «1
   hour» or «Until the end of this event» [A].
3. Apps read the Focus state (`INFocusStatusCenter`) and filter their own
   content (`SetFocusFilterIntent`, macOS 13) [A].

### Live Activities

1. In macOS 26 the Live Activities of a paired iPhone appear in the Mac menu
   bar, in the compact leading and trailing views, as in the Dynamic Island.
   A click shows the Lock Screen presentation below the menu bar; a
   double-click opens the app in iPhone Mirroring
   ([WWDC25 278](https://developer.apple.com/videos/play/wwdc2025/278/),
   [HT 120684](https://support.apple.com/en-us/120684)) [A].
2. Native Mac apps cannot start one in macOS 26: ActivityKit lists macOS
   only from 27.3 beta
   ([ActivityKit](https://developer.apple.com/documentation/activitykit))
   [A].
3. **Lifecycle** (iOS rules): active up to 8 hours, then up to 4 more on
   screen. `staleDate` marks it stale, `relevanceScore` orders several, and
   the dismissal policy is default, immediate or a date. An update may alert
   by expanding the activity instead of posting a banner
   ([displaying live data](https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities))
   [A].
4. Settings › Notifications has «Allow Live Activities from iPhone» [A].

### Neighbours

1. **Control Center** is a separate status menu. Now Playing lives there, not
   in Notification Center. Third-party controls arrive in macOS 26
   (`ControlWidget`)
   ([Control Center](https://support.apple.com/guide/mac-help/mchl50f94f8f/26/mac/26))
   [A].
2. **iPhone notifications** reach the Mac through iPhone Mirroring, with an
   iPhone badge and per-app switches [A] (HT 120684).

## Context: what Linux offers

1. **`org.freedesktop.Notifications` 1.3:** `Notify`, `CloseNotification`,
   `GetCapabilities`, `GetServerInformation`; signals `NotificationClosed`,
   `ActionInvoked` and `ActivationToken`; hints `urgency`, `category`,
   `desktop-entry`, `image-data`, `image-path`, `resident`, `transient`,
   `sound-*`
   ([spec](https://specifications.freedesktop.org/notification/latest/)) [S].
2. **`org.gtk.Notifications`:** `AddNotification(app_id, id, a{sv})` and
   `RemoveNotification`. GLib prefers it over the fdo interface whenever its
   name has an owner, and the app id is explicit
   ([ggtknotificationbackend.c](https://gitlab.gnome.org/GNOME/glib/-/raw/main/gio/ggtknotificationbackend.c))
   [S].
3. **Portal Notification v2** (xdg-desktop-portal 1.19.1): `markup-body`,
   `sound`, `display-hint` (`transient`, `tray`, `persistent`,
   `hide-on-lock-screen`, `hide-content-on-lock-screen`, `show-as-new`),
   `category` (`im.received`, `call.incoming`, `call.ongoing`,
   `alarm.ringing`, `os.battery.low`, …), button `purpose`
   (`im.reply-with-text`, `call.accept`, …) and `SupportedOptions`
   ([portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Notification.html))
   [S]. xdg-desktop-portal-gnome forwards it to `org.gtk.Notifications`
   ([notification.c](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/raw/main/src/notification.c))
   [S].
4. **Vendor extensions in real use:** KDE's `inline-reply` capability and
   `NotificationReplied` signal, which Chromium uses; KDE's `Inhibit` for Do
   Not Disturb
   ([interface XML](https://invent.kde.org/plasma/plasma-workspace/-/raw/master/libnotificationmanager/dbus/org.freedesktop.Notifications.xml))
   [S].
5. **Chromium** marks notifications that must not time out as critical unless
   the server calls itself «Plasma»
   ([notification_platform_bridge_linux.cc](https://chromium.googlesource.com/chromium/src/+/main/chrome/browser/notifications/notification_platform_bridge_linux.cc))
   [S]. Urgency 2 is therefore not a reliable «critical».
6. **Badges and progress:** `com.canonical.Unity.LauncherEntry` (`count`,
   `progress`, `urgent`), de facto and without a spec page, implemented by
   Plasma and Dash to Dock
   ([smartlauncherbackend.cpp](https://invent.kde.org/plasma/plasma-desktop/-/raw/master/applets/taskmanager/smartlauncherbackend.cpp))
   [S]. File transfers: KDE's `org.kde.JobViewServer`
   ([jobsmodel_p.cpp](https://invent.kde.org/plasma/plasma-workspace/-/raw/master/libnotificationmanager/jobsmodel_p.cpp))
   [S].
7. **Now playing:** MPRIS
   ([spec](https://specifications.freedesktop.org/mpris-spec/latest/)) [S].
8. **No standard** for Live Activities, progress in notifications, threads,
   Focus or persistent history. Progress is only a wish for portal v3
   ([discussion 1495](https://github.com/flatpak/xdg-desktop-portal/discussions/1495))
   [I]. GNOME keeps only `org.gtk.Notifications` across restarts and Plasma
   keeps nothing
   ([KDE 486483](https://bugs.kde.org/show_bug.cgi?id=486483)) [I].
9. **App identity:** `desktop-entry` is self-declared. GNOME resolves the
   sender's PID to a window's app
   ([notificationDaemon.js](https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/js/dbusServices/notifications/notificationDaemon.js))
   [S]; systemd names app scopes `app-<launcher>-<AppID>-<random>.scope`
   ([systemd.io](https://systemd.io/DESKTOP_ENVIRONMENTS/)) [S]. Malaysia's
   shell starts apps in unnamed scopes (`malaysia-shell/src/spawn.rs`).

## Decision

1. **One shell component, `malaysia-shell notifications`** (ADR 0023). It
   plays both `usernoted` and `NotificationCenter`: it owns the D-Bus names,
   keeps the store and draws banners and the panel on layer-shell. A crash
   loses nothing because of step 3.
2. **Three entry points, one model.**
   1. `org.freedesktop.Notifications` 1.3 with `actions`, `body`,
      `body-markup` (shown as plain text), `icon-static`, `inline-reply`
      (KDE's `NotificationReplied` signal), `persistence` and `sound`.
   2. `org.gtk.Notifications`, so GApplications post with a real app id.
   3. The portal crate implements `impl.portal.Notification` v2 and forwards
      to `org.gtk.Notifications`, as xdg-desktop-portal-gnome does. It turns
      `markup-body` into text, drops the icon and sound descriptors and marks
      the notification `x-malaysia-portal`; the shell answers clicks and
      replies with `org.malaysia.Notifications1.PortalAction`, and the portal
      sends them to the app as `ActionInvoked`. A button with purpose
      `im.reply-with-text` becomes the reply field, and the display hints
      `transient`, `hide-on-lock-screen` and `hide-content-on-lock-screen` are
      kept.
3. **Persistent store** in `~/.local/share/malaysia/notifications.state`,
   written atomically. Delivered notifications survive a restart of the
   component or the session, as on macOS.
4. **Identity:** the app id comes from `org.gtk.Notifications` or the portal.
   For fdo clients the server reads the sender's PID, then its cgroup scope;
   `desktop-entry` and `app_name` are fallbacks. The shell names app scopes
   `app-malaysia-<AppID>-<random>.scope`.
5. **Levels map to macOS:**

   | Linux | Malaysia level |
   |---|---|
   | urgency 0, GTK and portal `low` | passive |
   | urgency 1, GTK and portal `normal` | active |
   | GTK and portal `high` | time-sensitive |
   | urgency 2, GTK and portal `urgent` | critical, if the app has «Allow critical alerts»; otherwise active |

   Urgency 2 alone never breaks through Do Not Disturb (Context, item 5).
   A `transient` notification shows its banner and is not kept.
6. **The user decides presentation, not the app.** Per app: locations
   (Desktop, Notification Center, Lock Screen), style Temporary or
   Persistent, previews, sound, badge, grouping, time-sensitive and critical.
   `resident` and `display-hint: persistent` do not override the style. These
   live in `~/.config/malaysia/notifications.domain` and in Settings ›
   Notifications, with Tahoe's names.
7. **Grouping:** by app. «Automatic» groups by thread when the portal or GTK
   id carries one; there is no thread key on fdo.
8. **Banners** at the top right, under the menubar. A Temporary banner closes
   on its own; a Persistent one waits. The front app's own notifications show
   no banner and go to the panel, as `willPresent` does by default.
9. **Panel:** opened by clicking the clock and by a two-finger swipe from the
   right edge (the compositor reports the gesture); closed by a click outside,
   the clock or the swipe back. The compositor finds the panel by its layer
   namespace and anchors, and calls `OpenPanel` and `ClosePanel` on
   `org.malaysia.Notifications1` (`malaysia-compositor/src/notification_panel.rs`).
   libinput reports a two-finger swipe as finger scrolling with no touch
   positions, so the right edge of the screen stands in for the right edge of
   the trackpad: with the pointer there, a leftward swipe opens the panel; over
   the panel, a rightward one closes it. A press outside the panel and the
   menu bar closes it too. Stacks, «Show less», «Clear All», hover
   controls and the «More» menu as in Tahoe. Sizes, radii and material are
   tokens in `ctdesign`, calibrated against a real Tahoe screenshot; the
   panel does not use glass until one shows it. Cards, stack layers and lock
   cards take the window border colours of `ctappearance::palette`: in dark
   mode a white hairline inside and a dark line outside, as measured on a
   Tahoe dark call notification in the MacStories review; the expanded Live
   Activity has neither.
10. **Focus.** Four modes as on macOS: Do Not Disturb, Sleep, Personal and
    Work, each with its allowed apps and a «Time Sensitive Notifications»
    switch (`[focus.<mode>]` in `notifications.domain`). A mode lets through
    its allowed apps, time-sensitive alerts when both the mode and the app
    allow them, and allowed critical alerts; the rest go quietly to the panel.
    The active mode lives in `notifications.state` and in the `Focus` property
    of `org.malaysia.Notifications1`. A secondary click on the clock toggles Do
    Not Disturb, and the menubar shows the mode's symbol. Wayland sends modifiers only to
    the focused surface, so the menubar cannot see Option-click
    ([`wl_keyboard.modifiers`](https://wayland.app/protocols/wayland#wl_keyboard:event:modifiers)).
    The panel has the same switch at the top; its secondary-click menu picks
    any mode. Screen sharing and KDE's `Inhibit` stay for later. It turns on by itself while the
    screen is shared or mirrored, if the setting says so. KDE's `Inhibit`
    lets an app ask for it. Silenced notifications still go to the panel.
    Focus modes with allowed apps come later.
11. **Lock screen:** the lock component lists notifications whose app allows
    the Lock Screen location, if «Show notifications when screen is locked»
    is on. Previews other than «Always» and the hint
    `hide-content-on-lock-screen` show only icon, app name and
    «Notification». It reads `notifications.state` every 2 s.
12. **Reply:** a notification with a reply action shows «Reply» on hover; the
    field takes the keyboard (`Exclusive`) until it sends or Escape. The text
    goes back as `NotificationReplied` (fdo), as the target of the GTK action,
    or as the portal parameter.
13. **Badges and progress:** the dock owns `com.canonical.Unity` and reads
    `com.canonical.Unity.LauncherEntry` `Update` signals for `count` and
    `progress` (each with its `-visible` flag); an app's values go away when
    it leaves the bus. The badge is the app's number, not a count of
    notifications, and «Badge application icon» hides it
    (`malaysia-shell/src/launcher_entry.rs`).
14. **Activation:** a click mints an xdg-activation token, sends
    `ActivationToken` and then `ActionInvoked`, as `bk::notifications`
    already expects.
15. **Live Activities, Malaysia's own.** Linux has no standard, so the
    menubar gets a live area left of the clock: a compact pill per activity
    (percentage, pages or call time) that opens the expanded view below it,
    314 × 132 with radius 20 and 3 under the bar, with Pause or Resume and
    Cancel or Hang Up. An ended activity says «Done» for
    `LIVE_ACTIVITY_ENDED_MS` and goes away; one older than 8 hours ends, as
    in ActivityKit. The notifications component gathers them and publishes
    the `Activities` property of `org.malaysia.Notifications1`
    (`CancelActivity`, `PauseActivity`). Sources:
    1. **File transfers:** it owns `org.kde.JobViewServer` and
       `org.kde.kuiserver` like Plasma's notification server: `requestView`
       (V1 and V2) creates `/org/kde/notificationmanager/jobs/JobView_<n>`
       with `org.kde.JobViewV2` and `V3`, and Cancel or Pause send
       `cancelRequested` or `suspendRequested`. A job whose app leaves the
       bus ends.
    2. **Calls:** a GTK or portal notification with category `call.ongoing`
       becomes a call activity; its `call.hang-up` button is Hang Up, and
       `RemoveNotification` ends it.
    3. **Printing:** every 2 s an IPP `Get-Jobs` (`which-jobs=not-completed`,
       `my-jobs`) over CUPS's local socket lists the user's jobs with
       `job-media-sheets-completed` and `job-impressions`
       ([CUPS IPP](https://www.cups.org/doc/spec-ipp.html)); Cancel sends
       `Cancel-Job`. Pause is not offered: holding a job that is printing
       needs the printer, not the job.
16. **Built with barrierkit.** The component uses what bk already has
    (`Sidebar`, `Form`, `Switch`, `PopUpButton`, `SegmentedControl`,
    `ChoiceCards`, `ContextMenu`, `Button`, `ProgressView`, `Avatar`,
    `Badge`, `Material`, `bk::shell::layer` and `bk::shell::lock`). What bk
    lacks (notification card, stack, banner queue, sliding panel, swipe
    gesture, Live Activity views and widgets) is written first in
    `malaysia-shell`. Once phase 1 works, each piece moves to bk with its
    SwiftUI or AppKit name and its glossary row.
17. **Order of work:**
    1. Server (fdo + GTK), store, banners, panel, Do Not Disturb and
       Settings › Notifications.
    2. Portal v2, inline reply, lock screen and activation tokens.
    3. Dock badges and progress, Focus modes.
    4. Live Activities, then widgets in the panel: Calendar (the month with
       today marked) and Batteries (UPower, every 60 s), added and removed
       with «Edit Widgets» (`widgets` in `notifications.domain`). Widgets
       from apps need a contract Linux does not have.

## As built

1. `crates/malaysia-shell/src/notifications/`: `server.rs` serves the three
   D-Bus names, `store.rs` holds the model and the presentation rules,
   `view.rs` draws banners and the panel with bk. The surface grows with the
   banners and takes the full height for the panel through
   `bk::shell::set_layer_geometry`.
2. `org.malaysia.Notifications1` (`TogglePanel`, property `DoNotDisturb`)
   connects the menubar clock with the component.
3. `crates/malaysia-shell/src/app_scope.rs` names and reads app scopes.
4. System Settings › Notifications writes `notifications.domain`
   ([options](../configuration/options.md#notifications--notifications-system-settings--notifications-adr-0068)).
5. `xdg-desktop-portal-malaysia/src/notification.rs` is the portal backend;
   `nix/malaysia.nix` routes `org.freedesktop.impl.portal.Notification` to
   it.
6. `share/dbus-1/services/` activates the component through
   `malaysia-shell@notifications.service`. `dev.sh` does not start it,
   because in a nested session it would take the host's notification name.

## Consequences

- GLib apps and Flatpaks get exact app ids, so per-app settings work for
  them; plain fdo clients depend on the scope naming in step 4.
- Malaysia keeps history across restarts, unlike GNOME and Plasma.
- Chromium and others lose «critical» unless the user allows it, matching
  Tahoe, where the app no longer decides persistence.
- `xdg-desktop-portal-gtk` stops handling Notification once
  `xdg.portal.config.malaysia` in `nix/malaysia.nix` names our backend
  (ADR 0031 changes on that point).
- Live Activities are Malaysia-only until a portal standard exists.

## Not confirmed

- Card radius, sizes, spacing and the banner animation and timeout; Apple
  publishes none and no official image shows a banner. Measured on Apple's
  WWDC25 video, the expanded Live Activity is about 314 × 132 pt with a 20 pt
  radius, 3 pt under the menu bar; `NOTIFICATION_RADIUS` uses that 20 pt
  until a real Tahoe screenshot measures the banner.
- Whether the Tahoe panel uses Liquid Glass in the final release.
- Whether Focus-silenced notifications collect in the panel on macOS.
- The menu bar position of Live Activities relative to status items.
- The `usernoted` XPC protocol and how long the database keeps records.
