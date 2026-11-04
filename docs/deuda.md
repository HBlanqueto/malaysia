# Deuda técnica abierta

Hallazgos abiertos contra `d0bba82` (2026-11-03); los corregidos y los del toolkit movido a barrierkit no se listan. La [auditoría del 2026-11-03](auditoria-claude-2026-11-03.md) recoge el resto de pendientes.

| id | dónde | qué | lote |
|---|---|---|---|
| P1-05 | `crates/malaysia-compositor/src/session.rs:1826` | `try_clone` del fence innecesario; si falla se pierde la retención del release. | L1b |
| P1-11 | `crates/malaysia-compositor/src/session.rs:1095-1424` | Helpers de chrome en `session.rs` pese a existir `chrome.rs`/`effects.rs`. Aplazado: solo mover. | L3c |
| P1-12 | `crates/malaysia-compositor/src/session.rs:1425-1560` | Captura y ventana en `session.rs` pese a existir `capture.rs`. Aplazado: solo mover. | L3d |
| P1-13 | `crates/malaysia-compositor/src/session.rs:1764` | `reset_dead_cursor` en `session.rs` pese a existir `cursor.rs`. Aplazado: solo mover. | L3e |
| P5-09 | `crates/malaysia-shell/src/menubar/mod.rs:265-278` | `prepare_kit_env` borra `MALAYSIA_ORIG_*` del hijo; el launcher no puede restaurarlos. | L1i |
| P5-10 | `crates/malaysia-shell/src/menubar/focused.rs:230,639`, `dock.rs:49` y `dialogs.rs:290` | Ids con índice de lista; abrir/cerrar ventanas con el menú abierto puede operar en otra. | L1h |
| P5-12 | `crates/malaysia-shell/src/lock.rs:32,264,318-331` | `theme_revision` se guarda pero no se lee. | L2b |
| P5-17 | `crates/malaysia-shell/src/launcher.rs:93-110,146,156,369` | Clona entries e iconos en cada render. | L4e |
| P5-19 | `crates/malaysia-shell/src/menubar/dbusmenu.rs:317-327` | `ItemsPropertiesUpdated` no invalida `full`; la Ayuda ve datos viejos. | L1i |
| P5-20 | `crates/malaysia-shell/src/menubar/mod.rs:76-78` y `dock.rs:95-109` | `ActiveWindow.title` muerto y dock con parser propio. | L2b |
| S-01 | Smithay `023b78b1`, `wayland/shell/xdg/mod.rs` (`PopupSurface::pre_commit_hook`) | `ERROR surface missing from known popups` al cerrar un popover de GTK. GTK desmapea el `xdg_popup` (commit sin buffer: Smithay reinicia el rol y `parent` queda en `None`), lo destruye (sale de `known_popups`) y vuelve a hacer commit del `wl_surface`. El hook sigue registrado, ve `parent == None` y, al no encontrar el popup, solo registra el error. No se envía error de protocolo ni cambia nada visible. Se arregla en Smithay (no ejecutar el hook tras destruir el rol) o subiendo su revisión; no se tapa con un filtro de logs porque ocultaría errores reales del módulo. | upstream |
| S-02 | Smithay `023b78b1`, `src/backend/drm/compositor/mod.rs` | `DrmCompositor` no fija la propiedad `pixel blend mode` de los planos (grep `blend` en `src/backend/drm`: 0). En Intel i915 los overlays con alfa se mezclan mal. Por eso `debug.overlay_planes` va apagado por defecto. El fork de nongio lo arregla en [`734ebe91`](https://github.com/nongio/smithay/commit/734ebe91) y [`33b10703`](https://github.com/nongio/smithay/commit/33b10703); solo es referencia: se arregla en Smithay upstream o subiendo su revisión. | upstream |
| S-03 | `crates/malaysia-compositor/src/virtual_input.rs` | Al volver del keymap de un teclado virtual se restaura el keymap configurado, pero no el grupo ni los modificadores bloqueados, a diferencia de niri (`src/handlers/virtual_keyboard.rs`). Se arregla con estado xkb por dispositivo. | L6 |
| G-01 | `crates/xdg-desktop-portal-malaysia/src/gtk.rs:358-381` | `gtk_bridge` se lee aparte del `AppearanceConfig` de barrierkit; moverlo allí. Los fondos base de Adwaita están fijados a libadwaita 1.6+. | L2b |
| L3 | `session.rs:2126`, `input.rs:2325`, `handlers.rs:1024`, `gtk.rs:432` | Cuatro arneses de test; unificar y añadir el test de PX-06. | L3 |
