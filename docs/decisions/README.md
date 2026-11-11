# Decisions (ADRs)

Decision index. Since 0048 the toolkit ADRs live in barrierkit; a stub here
points to each one it inherited. ADRs created in barrierkit after the split
(its 0056, 0057 and 0060 onwards) are only in
[its index](https://github.com/HBlanqueto/barrierkit/blob/main/docs/decisions/README.md).

| ADR | Title | Status |
|---|---|---|
| [0001](0001-coordinate-types.md) | Smithay / `malaysia-scene` coordinates | accepted |
| [0002](0002-library-and-binary.md) | Library and binary | accepted |
| [0003](0003-unsafe-code-policy.md) | Bounded `unsafe` | accepted |
| [0004](0004-surface-rectangle-rounding.md) | Floor/ceil rounding | accepted |
| [0005](0005-development-environment.md) | Environment and linker | accepted |
| [0006](0006-toolchain-source-of-truth.md) | Flake toolchain | accepted |
| [0007](0007-no-builds-outside-the-dev-shell.md) | Build only in devShell | accepted |
| [0008](0008-design-token-contract.md) | Tokens from Nix | superseded by 0011 |
| [0009](0009-winit-backend-and-gles2.md) | Winit + GLES2 | accepted |
| [0010](0010-smithay-pin.md) | Smithay 0.7.0 | replaced by 0028 |
| [0011](0011-design-values-as-rust-constants.md) | Tokens as Rust constants | moved to barrierkit |
| [0012](0012-backend-next-to-the-state.md) | Backend next to the State | accepted |
| [0013](0013-dispatch-polls-a-duplicated-fd.md) | Dispatch with duplicated fd | accepted |
| [0014](0014-decoration-insets.md) | Decoration insets | accepted |
| [0015](0015-layout-in-the-scene-crate.md) | Layout in `malaysia-scene` | accepted |
| [0016](0016-interactive-move-and-resize.md) | Own move/resize | accepted |
| [0017](0017-server-side-xdg-decoration.md) | Server-side xdg-decoration | superseded by 0019 |
| [0018](0018-winit-render-transform.md) | Flip only when rendering | accepted |
| [0019](0019-decoration-policy.md) | Decoration policy | accepted |
| [0020](0020-design-and-unification-policy.md) | Separate chrome and effects | accepted |
| [0021](0021-configuration-and-control-surface.md) | Configuration and IPC | accepted |
| [0023](0023-shell-components-as-separate-processes.md) | Shell in separate processes | accepted |
| [0024](0024-toolkit-render-stack.md) | Kit render stack | accepted |
| [0025](0025-native-shell.md) | Native shell (`malaysia-shell`) | accepted |
| [0026](0026-linux-dmabuf-and-explicit-sync.md) | dmabuf and explicit sync | accepted |
| [0027](0027-shell-presents-through-egl.md) | Shell through EGL | superseded by barrierkit 0056 (Vulkan) |
| [0028](0028-smithay-by-revision.md) | Smithay by revision | accepted |
| [0029](0029-backend-by-flag.md) | Backend by flag | accepted |
| [0030](0030-direct-session-8a.md) | Direct Session 8a | accepted |
| [0031](0031-own-portal-backend.md) | Own portal backend | accepted |
| [0032](0032-neptunia-as-design-source.md) | Neptunia as design | moved to barrierkit |
| [0033](0033-skia-in-clients-and-celestialkit-from-otto-kit.md) | Skia/BarrierKit | replaced by 0034; moved to barrierkit |
| [0034](0034-celestialkit-on-freya.md) | BarrierKit on Freya | moved to barrierkit |
| [0035](0035-own-app-identification-and-csd.md) | Own apps and CSD | moved to barrierkit |
| [0036](0036-global-menu.md) | Global menu | implemented |
| [0037](0037-compositor-drawn-window-frame.md) | Window frame | accepted |
| [0038](0038-reduce-transparency-by-default.md) | Reduced transparency | point 1 superseded by 0075 |
| [0039](0039-infinity-v1-window-management-for-the-shell.md) | Infinity v1 | moved to barrierkit |
| [0040](0040-real-global-menu.md) | Real global menu | implemented |
| [0041](0041-island-sidebar.md) | Island sidebar | moved to barrierkit |
| [0042](0042-system-settings.md) | System settings (`malaysia-settings`) | accepted |
| [0043](0043-rebranding-to-malaysia.md) | Rebranding to Malaysia | accepted |
| [0044](0044-appearance-cross-fade.md) | Appearance cross-fade | accepted |
| [0045](0045-gtk-shell1-and-org-gtk-menus.md) | GTK menus | accepted |
| [0046](0046-tengen-visual-language.md) | Tengen | moved to barrierkit |
| [0047](0047-barrierkit-in-layers.md) | Layered BarrierKit | moved to barrierkit |
| [0048](0048-barrierkit-own-repository.md) | BarrierKit in its own repo | accepted |
| [0049](0049-wallpaper-on-vulkan.md) | Wallpaper on Vulkan | accepted |
| [0050](0050-gtk-and-libadwaita-bridge.md) | GTK/libadwaita bridge | accepted |
| [0051](0051-spaces-and-focus-mode.md) | Spaces and focus mode | accepted |
| [0052](0052-libadwaita-global-menu.md) | Global menu for libadwaita | accepted |
| [0053](0053-libadwaita-focus-mode.md) | Focus mode in libadwaita | accepted |
| [0054](0054-overlaid-titlebar-in-focus.md) | Overlaid titlebar in focus | accepted |
| [0055](0055-live-appearance-in-libadwaita.md) | Live accent and tint in libadwaita | accepted |
| [0056](0056-xwayland-satellite.md) | X11 apps with xwayland-satellite | accepted |
| [0058](0058-window-radius-with-a-toolbar.md) | Window radius with a toolbar | superseded by 0059 |
| [0059](0059-radius-per-bar-and-continuous-corner.md) | Radius per bar kind and continuous corner | accepted |
| [0061](0061-shortcut-menus.md) | Shortcut menus for apps without a global menu | accepted |
| [0065](0065-wallpaper-tinting-per-window.md) | Wallpaper tinting per window | accepted |
| [0066](0066-accessibility-display.md) | Accessibility › Display | accepted |
| [0067](0067-text-size-per-toolkit-and-qt-bridge.md) | Text size per toolkit and Qt bridge | accepted |
| [0068](0068-notification-center.md) | Notification Center | accepted |
| [0069](0069-network-bluetooth-and-energy-settings.md) | Network, Bluetooth and energy settings | accepted |
| [0070](0070-freedesktop-options.md) | Your own themes for GTK and Qt | accepted |
| [0071](0071-background-blur-and-ext-background-effect.md) | Background blur and ext-background-effect-v1 | accepted |
| [0072](0072-wallpaper-settings.md) | Wallpaper in System Settings | accepted |
| [0073](0073-wallpaper-desktops-collections-dynamic-animated.md) | Wallpaper per desktop and display, collections, dynamic and animated wallpapers | accepted |
| [0074](0074-lock-screen-and-lock-transition.md) | Lock screen over the wallpaper and the lock transition | accepted |
| [0075](0075-menu-bar-styles-and-height.md) | Menu bar styles, height and blur by default | accepted |
| [0076](0076-lock-screen-clock-fonts.md) | Lock screen clock fonts | accepted |

> ADR 0022 (Quickshell) was removed by 0025.
