# Deuda técnica abierta

Hallazgos de las auditorías Ponytail que **siguen abiertos** contra `22070c6`
(2026-10-31). Los ya corregidos no se listan; los que dependían del toolkit
movido a barrierkit tampoco (se siguen allí). El lote remite al triaje de
P1/PX/P5.

| id | dónde | qué | lote |
|---|---|---|---|
| P1-04 | `crates/malaysia-compositor/src/window.rs:1091` | `Windows::set_maximized` no tiene llamadas; solo se usa `set_maximized_with_outer`. Borrarlo o documentar que no cambia geometría. | L2a |
| P1-05 | `crates/malaysia-compositor/src/session.rs:1774` | `try_clone` del fence es innecesario (`export()` ya devuelve un fd nuevo); si falla se pierde la retención del release. | L1b |
| P1-08 | `crates/malaysia-compositor/src/state.rs:311` | `infinity_global` solo se escribe; el `GlobalId` no mantiene vivo el global, que vive en el `Display`. | L2a |
| P1-09 | `crates/malaysia-compositor/src/window.rs:1297` | `id_of_surface` duplica `id_of`; solo lo usa `dialog_parent`. | L3b |
| P1-10 | `crates/malaysia-compositor/src/session.rs:1258` | `border_element` devuelve `Option` pero siempre termina en `Some`; los dos llamados tratan un `None` imposible. | L3a |
| P1-11 | `crates/malaysia-compositor/src/session.rs:1180-1300` | Helpers de chrome (`frame_order`, `border_element`) viven en `session.rs` aunque `chrome.rs`/`effects.rs` existen. Aplazado: solo mover, sin reducir líneas. | L3c |
| P1-12 | `crates/malaysia-compositor/src/session.rs:1301,1376` | `build_window_elements`/`screenshot_elements` están en `session.rs` y `capture.rs` ya tiene la captura PNG. Aplazado: solo mover. | L3d |
| P1-13 | `crates/malaysia-compositor/src/session.rs:1685` | `reset_dead_cursor` está en `session.rs` aunque `cursor.rs` existe. Aplazado: solo mover. | L3e |
| P1-14 | `crates/malaysia-compositor/src/handlers.rs:345,658,666` | Handlers vacíos (`source_destroyed`, `DndGrabHandler`, `OutputHandler`) sin comentario que explique que es intencional. | L5a |
| PX-09 | `crates/malaysia-compositor/src/handlers.rs:597` | Un hilo por `receive`; `write_all` bloquea sin timeout y `thread::spawn` puede entrar en pánico. Solo aplica a la selección del compositor. | L1l |
| PX-13 | `crates/malaysia-compositor/src/input.rs:490` | Ctrl+Super no comprueba `locked`: el tiling puede mover ventanas ocultas tras el lock. | L1l |
| PX-14 | `crates/malaysia-compositor/src/idle.rs:84,191` | `note_activity` reinserta cada timer (mutex + remove/insert) en cada evento de entrada, incluido ratón a 1000 Hz. | L4d |
| P5-07 | `crates/malaysia-shell/src/desktop.rs:153,157` | Compara con `MalaysiaDreams` pero `DesktopNames` y `XDG_CURRENT_DESKTOP` son `Malaysia`; `OnlyShowIn`/`NotShowIn` se evalúan al revés. | L1i |
| P5-09 | `crates/malaysia-shell/src/menubar/mod.rs:279,304` | `prepare_kit_env` elimina `MALAYSIA_ORIG_*` del hijo; un launcher abierto desde el menú ya no puede restaurar `LD_LIBRARY_PATH` al lanzar apps. | L1i |
| P5-10 | `crates/malaysia-shell/src/menubar/focused.rs:230,411` | Los ids capturan el índice de la lista y la acción lo reutiliza contra la lista actual; abrir/cerrar ventanas con el menú abierto puede operar en otra. | L1h |
| P5-11 | `share/systemd/user/malaysia-shell@.service:18` y `malaysia-wallpaper.service:18` | `StartLimitIntervalSec` está en `[Service]`; systemd lo ignora y el intervalo real es 10 s, no 30. | L1j |
| P5-12 | `crates/malaysia-shell/src/lock.rs:36,272` | `theme_revision` se guarda pero no lo lee nadie. | L2b |
| P5-13 | `crates/malaysia-shell/src/launcher.rs:587` y `menubar/mod.rs:279` | Dos copias idénticas de `sanitize_client_env`; una variable nueva hay que añadirla en ambas. | L3f |
| P5-14 | `crates/malaysia-shell/src/dock.rs:103`, `lock.rs:326`, `launcher.rs:427`, `menubar/mod.rs:115` | Cuatro copias exactas de `rgba` en el shell. | L3f |
| P5-15 | `crates/malaysia-shell/src/menubar/menu.rs:265,283,291` y `clock.rs:13,16` | Literales `13.0`/`10.0`/`16.0` donde existen `MENUBAR_FONT_SIZE` y tokens de padding/icono. | L5c |
| P5-17 | `crates/malaysia-shell/src/launcher.rs:97,153` | `self.entries.clone()` y clonado de iconos en cada render. | L4e |
| P5-19 | `crates/malaysia-shell/src/menubar/dbusmenu.rs:859-875` | `ItemsPropertiesUpdated` actualiza los subárboles pero no `full`; la Ayuda ve label/enabled viejos hasta `LayoutUpdated`. | L1i |
| P5-20 | `crates/malaysia-shell/src/menubar/mod.rs:93,95` y `dock.rs:108` | `ActiveWindow.title` no se lee y el dock mantiene su propio parser de `window.domain`. | L2b |
| L3 | `crates/malaysia-compositor/src/session.rs:2178`, `input.rs:3041`, `handlers.rs:1345` | Tres arneses de test locales (`Display`+`State` sin cliente); unificarlos y escribir con ellos el test de PX-06 (título de la ventana activa → `window.domain`). | L3 |
