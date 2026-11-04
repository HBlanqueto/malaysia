# Decisions (ADRs)

Decision index. The toolkit ADRs live in
[`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit)
since 0048; a stub here points there.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-tipos-de-coordenadas.en.md) | Smithay / `yuki-scene` coordinates | accepted |
| [0002](0002-objetivos-de-biblioteca-y-binario.en.md) | Library and binary | accepted |
| [0003](0003-politica-de-codigo-unsafe.en.md) | Bounded `unsafe` | accepted |
| [0004](0004-redondeo-de-rectangulos-de-superficie.en.md) | Floor/ceil rounding | accepted |
| [0005](0005-entorno-de-desarrollo.en.md) | Environment and linker | accepted |
| [0006](0006-toolchain-fuente-de-verdad.en.md) | Flake toolchain | accepted |
| [0007](0007-compilar-fuera-del-devshell.en.md) | Build only in devShell | accepted |
| [0008](0008-contrato-de-design-tokens.en.md) | Tokens from Nix | superseded by 0011 |
| [0009](0009-backend-winit-y-gles2.en.md) | Winit + GLES2 | accepted |
| [0010](0010-anclaje-de-smithay.en.md) | Smithay 0.7.0 | replaced by 0028 |
| [0011](0011-valores-de-diseno-como-constantes-rust.en.md) | Tokens as Rust constants ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0012](0012-backend-junto-al-state.en.md) | Backend next to the State | accepted |
| [0013](0013-dispatch-consume-el-fd-duplicado.en.md) | Dispatch with duplicated fd | accepted |
| [0014](0014-insets-de-decoracion.en.md) | Decoration insets | accepted |
| [0015](0015-desktop-space-y-yuki-scene.en.md) | Layout in `yuki-scene` | accepted |
| [0016](0016-movimiento-y-redimensionado-interactivo.en.md) | Own move/resize | accepted |
| [0017](0017-xdg-decoration-del-lado-del-servidor.en.md) | Server-side xdg-decoration | superseded by 0019 |
| [0018](0018-transformacion-de-renderizado-de-winit.en.md) | Flip only when rendering | accepted |
| [0019](0019-politica-de-decoracion.en.md) | Decoration policy | accepted |
| [0020](0020-politica-de-diseno-y-unificacion.en.md) | Separate chrome and effects | accepted |
| [0021](0021-superficie-de-configuracion-y-control.en.md) | Configuration and IPC | accepted |
| [0023](0023-los-componentes-del-shell-corren-como-procesos-separados.en.md) | Shell in separate processes | accepted |
| [0024](0024-stack-de-render-del-toolkit.en.md) | Kit render stack | accepted |
| [0025](0025-la-shell-es-nativa.en.md) | Native shell (`yuki-shell`) | accepted |
| [0026](0026-linux-dmabuf-y-explicit-sync.en.md) | dmabuf and explicit sync | accepted |
| [0027](0027-shell-presenta-por-egl.en.md) | Shell through EGL | accepted |
| [0028](0028-smithay-por-revision.en.md) | Smithay by revision | accepted |
| [0029](0029-backend-por-flag.en.md) | Backend by flag | accepted |
| [0030](0030-direct-session-8a.en.md) | Direct Session 8a | accepted |
| [0031](0031-portal-backend-de-yuki.en.md) | Own portal backend | accepted |
| [0032](0032-neptunia-como-fuente-de-diseno.en.md) | Neptunia as design ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0033](0033-skia-en-clientes-y-celestialkit-desde-otto-kit.en.md) | Skia/CelestialKit ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | replaced by 0034 |
| [0034](0034-celestialkit-sobre-freya.en.md) | CelestialKit on Freya ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0035](0035-identificacion-de-apps-propias-y-csd.en.md) | Own apps and CSD ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0036](0036-menu-global.en.md) | Global menu | implemented |
| [0037](0037-marco-compositor-dibuja-ventanas.en.md) | Window frame | accepted |
| [0038](0038-reducir-transparencia-por-defecto.en.md) | Reduced transparency | accepted |
| [0039](0039-infinity-v1-gestion-de-ventanas-para-la-shell.en.md) | Infinity v1 ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0040](0040-menu-global-real.en.md) | Real global menu | implemented |
| [0041](0041-barra-lateral-en-isla.en.md) | Island sidebar ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0042](0042-ajustes-del-sistema.en.md) | System settings (`yuki-settings`) | accepted |
| [0043](0043-rebranding-a-malaysia.en.md) | Rebranding to Malaysia | accepted |
| [0044](0044-fundido-de-apariencia.en.md) | Appearance cross-fade | accepted |
| [0045](0045-gtk-shell1-y-org-gtk-menus.en.md) | GTK menus | accepted |
| [0046](0046-tengen-lenguaje-visual.en.md) | Tengen ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0047](0047-barrierkit-en-capas.en.md) | Layered BarrierKit ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| [0048](0048-barrierkit-repositorio-propio.en.md) | BarrierKit in its own repo | accepted |
| [0049](0049-fondo-en-vulkan.en.md) | Wallpaper on Vulkan | accepted |
| [0050](0050-puente-gtk-libadwaita.en.md) | GTK/libadwaita bridge | accepted |
| [0051](0051-espacios-y-modo-foco.en.md) | Spaces and focus mode | accepted |
| [0052](0052-menu-global-libadwaita.en.md) | Global menu for libadwaita | accepted |
| [0053](0053-modo-foco-libadwaita.en.md) | Focus mode in libadwaita | accepted |
| [0054](0054-titlebar-superpuesta-en-foco.en.md) | Overlaid titlebar in focus | accepted |
| [0055](0055-apariencia-en-vivo-libadwaita.en.md) | Live accent and tint in libadwaita | accepted |
| [0056](0056-xwayland-satellite.en.md) | X11 apps with xwayland-satellite | accepted |
| 0056 | BarrierKit draws with Vulkan ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |
| 0057 | Full-width fullscreen glass and sidebar over a tinted background ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | accepted |

> ADR 0022 (Quickshell) was removed by 0025.
