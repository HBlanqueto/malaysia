# Deuda técnica abierta

Hallazgos abiertos contra `22070c6` (2026-10-31); los corregidos y los del toolkit movido a barrierkit no se listan.

| id | dónde | qué | lote |
|---|---|---|---|
| P1-04 | `crates/malaysia-compositor/src/window.rs:1091` | `Windows::set_maximized` sin llamadas; solo se usa `set_maximized_with_outer`. | L2a |
| P1-05 | `crates/malaysia-compositor/src/session.rs:1774` | `try_clone` del fence innecesario; si falla se pierde la retención del release. | L1b |
| P1-08 | `crates/malaysia-compositor/src/state.rs:311` | `infinity_global` solo se escribe; el global vive en el `Display`. | L2a |
| P1-09 | `crates/malaysia-compositor/src/window.rs:1297` | `id_of_surface` duplica `id_of`; solo lo usa `dialog_parent`. | L3b |
| P1-10 | `crates/malaysia-compositor/src/session.rs:1258` | `border_element` devuelve `Option` pero siempre `Some`. | L3a |
| P1-11 | `crates/malaysia-compositor/src/session.rs:1180-1300` | Helpers de chrome en `session.rs` pese a existir `chrome.rs`/`effects.rs`. Aplazado: solo mover. | L3c |
| P1-12 | `crates/malaysia-compositor/src/session.rs:1301,1376` | Captura y ventana en `session.rs` pese a existir `capture.rs`. Aplazado: solo mover. | L3d |
| P1-13 | `crates/malaysia-compositor/src/session.rs:1685` | `reset_dead_cursor` en `session.rs` pese a existir `cursor.rs`. Aplazado: solo mover. | L3e |
| P1-14 | `crates/malaysia-compositor/src/handlers.rs:345,658,666` | Handlers vacíos sin comentario de que es intencional. | L5a |
| PX-09 | `crates/malaysia-compositor/src/handlers.rs:597` | Un hilo por `receive`; `write_all` bloquea sin timeout y `spawn` puede panicar. | L1l |
| PX-13 | `crates/malaysia-compositor/src/input.rs:490` | Ctrl+Super no comprueba `locked`: el tiling mueve ventanas ocultas. | L1l |
| PX-14 | `crates/malaysia-compositor/src/idle.rs:84,191` | `note_activity` reinserta timers en cada evento (ratón a 1000 Hz). | L4d |
| P5-07 | `crates/malaysia-shell/src/desktop.rs:153,157` | Compara con `MalaysiaDreams` pero el escritorio es `Malaysia`; `OnlyShowIn`/`NotShowIn` se invierten. | L1i |
| P5-09 | `crates/malaysia-shell/src/menubar/mod.rs:279,304` | `prepare_kit_env` borra `MALAYSIA_ORIG_*` del hijo; el launcher no puede restaurarlos. | L1i |
| P5-10 | `crates/malaysia-shell/src/menubar/focused.rs:230,411` | Ids con índice de lista; abrir/cerrar ventanas con el menú abierto puede operar en otra. | L1h |
| P5-11 | `share/systemd/user/malaysia-shell@.service:18` y `malaysia-wallpaper.service:18` | `StartLimitIntervalSec` en `[Service]`; systemd lo ignora (real: 10 s). | L1j |
| P5-12 | `crates/malaysia-shell/src/lock.rs:36,272` | `theme_revision` se guarda pero no se lee. | L2b |
| P5-13 | `crates/malaysia-shell/src/launcher.rs:587` y `menubar/mod.rs:279` | Dos copias de `sanitize_client_env`. | L3f |
| P5-14 | `dock.rs:103`, `lock.rs:326`, `launcher.rs:427`, `menubar/mod.rs:115` | Cuatro copias de `rgba` en el shell. | L3f |
| P5-15 | `menubar/menu.rs:265,283,291` y `clock.rs:13,16` | Literales 13/10/16 donde hay tokens. | L5c |
| P5-17 | `crates/malaysia-shell/src/launcher.rs:97,153` | Clona entries e iconos en cada render. | L4e |
| P5-19 | `crates/malaysia-shell/src/menubar/dbusmenu.rs:859-875` | `ItemsPropertiesUpdated` no invalida `full`; la Ayuda ve datos viejos. | L1i |
| P5-20 | `crates/malaysia-shell/src/menubar/mod.rs:93,95` y `dock.rs:108` | `ActiveWindow.title` muerto y dock con parser propio. | L2b |
| L3 | `session.rs:2178`, `input.rs:3041`, `handlers.rs:1345` | Tres arneses de test; unificar y añadir el test de PX-06. | L3 |
