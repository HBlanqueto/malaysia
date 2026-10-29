# Triaje Ponytail — P1, PX y P5

- HEAD: `79dd2d1` (`git rev-parse --short HEAD`). Los informes se escribieron sobre `718a7e4`.
- Informes triados: `docs/auditorias/2026-10-27-ponytail-p1-nucleo.md` (P1), `docs/auditorias/2026-10-27-ponytail-px-barrido.md` (PX) y `docs/auditorias/2026-10-27-ponytail-p5-shell.md` (P5).
- Desde `718a7e4` entraron `df1b9f7` (redondeo, cierra P1-01), `bc29c7a`, `a68a0f3`, `79dd2d1` (lock y entrada del kit, cierran PX-01/P5-02) y `a4fb750` (captura/screenshot con sesión bloqueada, cierra PX-03/PX-07/PX-11).
- `git diff 718a7e4 HEAD` toca el redondeo (compositor), el lock/entrada del kit y la captura/screenshot con sesión bloqueada; el resto de hallazgos PX/P5 siguen vigentes. En `lock.rs` el cambio de PX-01 no toca `theme_revision`.
- Sin cambios de código ni commits. Este fichero es el único añadido (ignorado por `.git/info/exclude`, como los demás `*-ponytail-*`).

## Hallazgos

| id | tipo | dónde | veredicto | motivo (una línea) | lote | estado |
|---|---|---|---|---|---|---|
| P1-01 | bug / deriva | `crates/malaysia-compositor/src/session.rs:283-284,1222-1231` | CONFIRMADO | ADR 0004 pide floor en posición y ceil en tamaño; `session.rs` usa `round` en lógico→físico. Las conversiones de `state.rs`/`window.rs`/`popup.rs` no son lógico→físico y quedan fuera. | L1a | hecho df1b9f7 |
| P1-02 | bug | `crates/malaysia-compositor/src/layers.rs:347-361` | FALSO | Restaurar `previous_window` (la ventana previa al primer exclusivo) al caer el último es lo documentado; `blocks_window_focus` impide que aparezca otra ventana. Borrarlo perdería el foco. | — | |
| P1-03 | bug / riesgo | `crates/malaysia-compositor/src/state.rs:944-956` | FALSO | Smithay 023b78b ejecuta `layer_destroyed` al limpiar el cliente y `handlers.rs:677-681` ya llama a `layers.destroy`. La entrada muerta no sobrevive. | — | |
| P1-04 | riesgo / sobra | `crates/malaysia-compositor/src/window.rs:951-961` | CONFIRMADO | `Windows::set_maximized` (952) no tiene llamadas (los `set_maximized` de `InputState`/`BarrierKit` son otros métodos); `set_fullscreen` simple no existe (924 es `_with_outer`, viva en `infinity.rs:338`). | L2a | |
| P1-05 | riesgo | `crates/malaysia-compositor/src/session.rs:1742-1764` | CONFIRMADO | El error pierde el clon que retiene el fence; el `Arc` de la superficie aún evita el release inmediato, pero el siguiente commit puede liberar antes que la GPU. `try_clone` es innecesario (`export()` ya devuelve un fd nuevo): se puede borrar. | L1b | |
| P1-06 | bug | `crates/malaysia-compositor/src/xdg.rs:57-63` | FALSO | `man user-dirs.dirs` exige `"$HOME/Path"` o `"/Path"`; `"$HOME"` pelado no es válido. Hoy queda literal y `create_dir_all` crearía `./$HOME`. | — | |
| P1-07 | sobra | `crates/malaysia-compositor/src/session.rs:1605-1628,1932-1956` | CONFIRMADO | El bucle inline es idéntico a `send_frame_callbacks_for_state`; `finish_empty_frame` ya usa el helper. | L3a | |
| P1-08 | sobra | `crates/malaysia-compositor/src/state.rs:307-308,588` | CONFIRMADO | Solo aparece en la definición y la asignación; `GlobalId` no mantiene vivo el global, que vive en el `Display`. | L2a | |
| P1-09 | sobra | `crates/malaysia-compositor/src/window.rs:646-651,1161-1166` | CONFIRMADO | `id_of_surface` es copia exacta de `id_of` y solo se usa en `dialog_parent` (1172). | L3b | |
| P1-10 | abstracción | `crates/malaysia-compositor/src/session.rs:1236-1289,1179-1195` | CONFIRMADO | `border_element` siempre termina en `Some`; los dos llamados manejan un `None` imposible. | L3a | |
| P1-11 | responsabilidad / sobra | `crates/malaysia-compositor/src/session.rs:987-1289` | CONFIRMADO | El bloque está en `session.rs` y `chrome.rs`/`effects.rs` ya existen; el rango citado no incluye `frame_order` (1215) ni `border_element` (1236). | L3c | |
| P1-12 | responsabilidad / sobra | `crates/malaysia-compositor/src/session.rs:1302-1426` | CONFIRMADO | Los helpers y `ScreenshotOutcome` están en `session.rs`; `capture.rs` ya tiene `screenshot_png`/`save_png`. | L3d | |
| P1-13 | responsabilidad / sobra | `crates/malaysia-compositor/src/session.rs:1814-1908,1661-1670` | CONFIRMADO | Los helpers están en `session.rs` y `cursor.rs` ya existe. `reset_dead_cursor` está en 1661, fuera del rango citado. | L3e | |
| P1-14 | atajo | `crates/malaysia-compositor/src/handlers.rs:342-343,647` | CONFIRMADO | Los dos handlers están vacíos y sin comentario; también hay `DndGrabHandler` vacío (639), no citado. | L5a | |
| P1-15 | atajo / rendimiento | `crates/malaysia-scene/src/scene.rs:352-369` | CONFIRMADO | `O(n²)` por `contains`; se ejecuta por evento de entrada al cerrar menús (clic/Esc, `input.rs:865/904/969`) y al destruir popups, no por frame. | L4a | |
| P1-16 | atajo | `crates/malaysia-compositor/src/layers.rs:82-84` | CONFIRMADO | La poda existe en cada commit; `layer_destroyed` ya limpia, pero cubre el caso de `wl_surface` muerta con rol vivo. Es defensa, no bug. | L4b | |
| P1-17 | atajo / rendimiento | `crates/malaysia-compositor/src/window.rs:346-357` | CONFIRMADO | `O(n²)` por `find`; se ejecuta por frame en `collect_drawn_buffers` (1686) y presentation (2007). `index()` también es lineal: el arreglo citado no basta. | L4c | |

### PX y P5

| id | tipo | dónde | veredicto | motivo (una línea) | lote | estado |
|---|---|---|---|---|---|---|
| P5-02 (= PX-01) | bug | `crates/malaysia-shell/src/lock.rs:91,204`; `crates/barrier-kit/src/platform/input.rs` | CONFIRMADO | `inserts_text` acepta SHIFT/CAPS/AltGr y el kit ya entrega el carácter real; tests de Shift y Ctrl añadidos. | — | hecho `bc29c7a`, `a68a0f3`, `79dd2d1` |
| PX-02 (= P5-01) | bug / configuración | `crates/vow-appearance/src/lib.rs:518,536`; `crates/malaysia-compositor/src/active.rs:38` | CONFIRMADO | Los joins usan `malaysia-compositor`; ADR 0043:33, README:139/148 y opciones:16/27 dicen `malaysia`. Hay 3 copias de config/data y 2 copias runtime del literal. | L1d (compositor) / L1e (vow-appearance) | hecho `fed1508`, `3e8e293` |
| PX-03 | bug / seguridad | `crates/malaysia-compositor/src/handlers.rs:402-412`; `session.rs:1467-1512`; `state.rs:914` | CONFIRMADO | `frame` no mira `locked`; la captura de salida devuelve la pantalla de bloqueo y la de ventana devuelve `build_window_elements`, es decir, el contenido de la ventana oculta. | L1c | hecho `a4fb750` |
| P5-04 (= PX-04) | bug | `crates/malaysia-shell/src/wallpaper.rs:49-55`; `docs/arquitectura/wallpaper.md:60` | CONFIRMADO | La imagen se carga una sola vez; no hay timer ni watcher y la doc promete recarga en vivo. | L1f | hecho `8d44e9f` |
| PX-05 (= P5-03) | bug | `crates/malaysia-compositor/src/active.rs:59` | CONFIRMADO | `format!` pega el título entre comillas; una comilla o un salto invalidan el TOML (o inyectan claves). `toml` ya es dependencia (`Cargo.toml:21`, también en 718a7e4). | L1d | hecho `3e8e293` |
| P5-05 (= PX-06) | bug | `crates/malaysia-shell/src/dock.rs:44`; `crates/malaysia-compositor/src/foreign.rs:295`; `handlers.rs:730-741` | CONFIRMADO | `window.domain` solo se publica al cambiar el foco; `title_changed`/`app_id_changed` actualizan el toplevel pero no el store, y el dock compara el título viejo. `ToplevelInfo.active` ya existe. | L1d (compositor) / L1h (shell) | hecho `3e8e293` (compositor; L1h pendiente) |
| PX-07 | bug / seguridad | `crates/malaysia-compositor/src/state.rs:789`; `lock.rs:54`; `session.rs:1452`; `input.rs:659-666` | CONFIRMADO | `finish_screenshot` no mira `locked` y `lock()` no limpia `screenshot`/`screenshot_mode`; un release tras el lock guarda la pantalla de bloqueo o la ventana oculta y deja el overlay. | L1c | hecho `a4fb750` |
| PX-08 | bug | `crates/barrier-kit/src/app_menu/server.rs:64-70,155` | CONFIRMADO | El path es `/MenuBar/{menus.len()}`; cerrar A deja len=1, la siguiente ventana reusa el path de B y zbus falla el registro. | L1k | |
| PX-09 | riesgo / seguridad | `crates/malaysia-compositor/src/handlers.rs:564-588` | CONFIRMADO | Un hilo por `receive`; `write_all` bloquea sin timeout en el pipe y `thread::spawn` entra en panic si falla. Solo aplica con la selección del compositor (screenshot). | L1l | |
| P5-06 (= PX-10) | bug | `crates/vow-appearance/src/watch.rs:61`; `crates/malaysia-shell/src/wallpaper.rs:424` | CONFIRMADO | El watcher solo escucha IN_CLOSE_WRITE/IN_MOVED_TO; borrar `wallpaper.state` (IN_DELETE) no avisa. Además, decode fallido o sin imagen no escribe `None`. | L1f (shell) / L1g (vow-appearance) | hecho `433aad7`, `8d44e9f` |
| PX-11 | riesgo / tests | `crates/malaysia-compositor/src/handlers.rs:402`; `state.rs:789`; tests de `lock.rs` | CONFIRMADO | La parte del lock ya está hecha; faltan los tests de `frame` con lock y de `finish_screenshot` con lock. | L1c | hecho `a4fb750` |
| P5-10 (= PX-12) | bug | `crates/malaysia-shell/src/menubar/focused.rs:230,417`; `bar.rs:407`; `dock.rs:51` | CONFIRMADO | Los ids capturan el índice de la lista y la acción lo reutiliza contra la lista actual; abrir o cerrar una ventana mientras el menú está abierto puede cerrar otra. | L1h | |
| PX-13 | bug | `crates/malaysia-compositor/src/input.rs:495-532`; `infinity.rs:278` | CONFIRMADO | Ctrl+Super no comprueba `locked`; los pair usan `iter_visible` (incluye ocultas), así que el tiling mueve ventanas tras el lock. | L1l | |
| PX-14 | rendimiento | `crates/malaysia-compositor/src/idle.rs:84-103,191-210`; `input.rs:618` | CONFIRMADO | `note_activity` re-inserta cada timer (mutex + remove/insert) en cada evento; el call site es `process_input_event`, ratón a 1000 Hz. | L4d | |
| P5-12 (= PX-15) | sobra | `crates/malaysia-shell/src/lock.rs:36,272,331`; `crates/barrier-kit/src/titlebar.rs:455` | CONFIRMADO | `theme_revision` no lo lee nadie y `_traffic_input` no se llama. | L2b (shell) / L2c (kit) | |
| P5-13 (= PX-16) | sobra | `crates/malaysia-shell/src/launcher.rs:583`; `crates/malaysia-shell/src/menubar/mod.rs:278` | CONFIRMADO | Dos copias idénticas de `sanitize_client_env`; una variable nueva hay que añadirla en ambas. | L3f | |
| PX-17 | sobra | `crates/barrier-kit/src/platform/infinity.rs:18,22`; `crates/malaysia-compositor/src/infinity.rs:35,39` | FALSO | El código generado por `wayland-scanner` referencia `wayland_backend`; quitar las dependencias rompe el build. | — | — |
| PX-18 | deriva / docs | `README.md:105`; `crates/malaysia-compositor/src/config.rs:345`; `docs/arquitectura/wallpaper.md:39,44,60` | CONFIRMADO | README dice 6 y el código 10; `wallpaper.md` cita `gl.rs`/`present.rs` borrados y en :60 dice «cada 500 ms» cuando la recarga ya es inotify. `grade-graphics` (línea 45) sí existe y tiene shaders: no está vacío. | L5b | |
| P5-07 | bug | `crates/malaysia-shell/src/desktop.rs:153-158`; `crates/malaysia-compositor/src/udev.rs:535`; `dev.sh:1054` | CONFIRMADO | Compara con `MalaysiaDreams` pero `DesktopNames` y `XDG_CURRENT_DESKTOP` son `Malaysia`; `OnlyShowIn=Malaysia;` desaparece y `NotShowIn=Malaysia;` aparece. | L1i | |
| P5-08 | bug | `crates/malaysia-shell/src/wallpaper.rs:85-90` | CONFIRMADO | El PNG se decodifica con alfa recto y se declara `AlphaType::Premul`; un PNG semitransparente se compone mal. El JPEG es opaco y no se ve afectado. | L1f | hecho `8d44e9f` |
| P5-09 | riesgo | `crates/malaysia-shell/src/menubar/mod.rs:140,303`; `launcher.rs:563`; `appmenu.rs:140` | CONFIRMADO | `spawn_component` borra `MALAYSIA_ORIG_*`; el launcher abierto desde Archivo → Buscar ya no puede restaurar `LD_LIBRARY_PATH` al lanzar apps. El wrapper de `flake.nix:123-126` sí define esas variables. | L1i | |
| P5-11 | configuración | `share/systemd/user/malaysia-shell@.service:17-18` | CONFIRMADO | systemd 261: `StartLimitIntervalSec` en `[Service]` se ignora; el intervalo real es el default de 10 s, no 30. | L1j | |
| P5-14 | sobra | `crates/malaysia-shell/src/dock.rs:98`; `lock.rs:326`; `launcher.rs:423`; `menubar/mod.rs:114` | CONFIRMADO | Cuatro copias exactas de `rgba` en el shell (y cinco más en `barrier-kit`, fuera del alcance de P5). | L3f | |
| P5-15 | deriva | `crates/malaysia-shell/src/menubar/menu.rs:278,286`; `clock.rs:16`; `crates/vow-design/build.rs:1698` | CONFIRMADO | Literales `13.0` donde existe `MENUBAR_FONT_SIZE` (`bar.rs:162` ya lo usa); paddings `10.0` e icono `16.0` sin token. | L5c | |
| P5-16 | sobra | `crates/malaysia-shell/src/menubar/dbusmenu.rs:1143-1150` | CONFIRMADO | `raw_items_from_layout` solo lo usan tests y lleva `#[allow(dead_code)]` en producción; `raw_item` sí lo usa el camino real. | L2b | |
| P5-17 | rendimiento | `crates/malaysia-shell/src/launcher.rs:97-114,295` | CONFIRMADO | `self.entries.clone()` copia todos los SVG y `desktop.clone()` todos los strings en cada render; los iconos se vuelven a clonar al pintar. | L4e | |
| P5-18 | rendimiento | `crates/malaysia-shell/src/wallpaper.rs:205-268` | CONFIRMADO | Decodifica a resolución completa (4K ~33 MB, 8K ~132 MB de RGBA) para escalar a la salida; una vez por arranque. | L4f (cond. Q7) | |
| P5-19 | bug menor | `crates/malaysia-shell/src/menubar/dbusmenu.rs:327-337,859` | CONFIRMADO | `ItemsPropertiesUpdated` actualiza los subárboles pero no `full`; la Ayuda ve label/enabled viejos hasta un `LayoutUpdated`. | L1i | |
| P5-20 | sobra | `crates/malaysia-shell/src/menubar/mod.rs:92-98`; `crates/malaysia-shell/src/dock.rs:103-117` | CONFIRMADO | `ActiveWindow.title` no se lee y el dock tiene su propio parser del mismo archivo; tras P5-05/P5-10 sobra el campo y la copia. | L2b | |

## Sublotes

| id | ids | crate | tamaño | modelo | test sí/no |
|---|---|---|---|---|---|
| L1a | P1-01 | malaysia-compositor | S (~20) | Kimi | sí |
| L1b | P1-05 | malaysia-compositor | M (~30) | Kimi | sí |
| L2a | P1-04, P1-08 | malaysia-compositor | S (~15) | Flash | no |
| L3a | P1-07, P1-10 | malaysia-compositor | S (~30) | Flash | no |
| L3b | P1-09 | malaysia-compositor | S (~7) | Flash | no |
| L3c | P1-11 | malaysia-compositor | aplazado (solo mover) | — | no |
| L3d | P1-12 | malaysia-compositor | aplazado (solo mover) | — | no |
| L3e | P1-13 | malaysia-compositor | aplazado (solo mover) | — | no |
| L4a | P1-15 | malaysia-scene | S (~5) | Flash | sí |
| L4b | P1-16 | malaysia-compositor | S (~0, comentario) | Flash | no |
| L4c | P1-17 | malaysia-compositor | S (~15, mapa/iteración) | Flash | no |
| L5a | P1-14 | malaysia-compositor | S (~2 comentarios) | Flash | no |

### Sublotes PX y P5 (en orden de ejecución)

| id | ids | crate | tamaño | modelo | test sí/no |
|---|---|---|---|---|---|
| L1c | PX-03, PX-07, PX-11 | malaysia-compositor | M (~50) | Kimi | sí |
| L1d | PX-02 (parte compositor), PX-05, PX-06 (parte compositor) | malaysia-compositor | S (~40) | Flash | sí |
| L1e | PX-02 (rutas) | vow-appearance | S (~8) | Flash | sí |
| L1f | P5-04 (= PX-04), P5-06 (parte shell), P5-08 | malaysia-shell | M (~70) | Flash | sí |
| L1g | P5-06 (parte watcher) | vow-appearance | S (~10) | Flash | sí |
| L1h | P5-05 (parte shell), P5-10 (= PX-12) | malaysia-shell | M (~60) | Flash | sí |
| L1i | P5-07, P5-09, P5-19 | malaysia-shell | S (~30) | Flash | sí |
| L1j | P5-11 | share/systemd (sin crate) | S (~2 movidas) | Flash | sí |
| L1k | PX-08 | barrier-kit | S (~15) | Flash | sí |
| L1l | PX-09, PX-13 | malaysia-compositor | M (~50) | Flash | sí |
| L2b | P5-12 (parte shell), P5-16, P5-20 | malaysia-shell | S (~35) | Flash | no |
| L2c | PX-15 (parte kit) | barrier-kit | S (~8) | Flash | no |
| L3f | P5-13 (= PX-16), P5-14 | malaysia-shell | S (~30) | Flash | sí |
| L3g | nuevo: `menubar/bar.rs:468` `window.domain` cada 250 ms y `:572` `menubar.domain` cada 500 ms → watcher de vow-appearance con wakeup | malaysia-shell | M | Flash | sí |
| L4d | PX-14 | malaysia-compositor | S (~15) | Flash | sí |
| L4e | P5-17 | malaysia-shell | M (~30) | Flash | no |
| L4f | P5-18 (cond. Q7) | malaysia-shell | S/M (~20) | Flash | no |
| L5b | PX-18 | docs (README, `docs/arquitectura`) | S (~10) | Flash | no |
| L5c | P5-15 | malaysia-shell | S (~4) | Flash | no |

## Verificación de las comprobaciones abiertas

### Redondeo (ADR 0004)

- ADR 0001: «En ese módulo, las coordenadas lógicas son `f64` y las físicas/de buffer son `i32`... La conversión a enteros ocurre solo al entrar en un espacio entero.»
- ADR 0004: «**Opción 2.** La conversión de lógico a entero redondea la **posición con `floor`** y el **tamaño con `ceil`**.» y «El redondeo al más cercano desaparece de la API».
- Conclusión: `session.rs::physical_point`/`physical_rect` convierten lógico→físico con `round`; es una desviación de 0004. `state.rs::logical_output_size` es físico→lógico, y `window.rs::usable_rect`/`popup.rs::smithay_rect` redondean lógico de escena a lógico entero: no son el caso que fija la ADR. P1-01 está cerrado en `df1b9f7`.

### `client_disconnected` y layers

- Smithay 0.7.0, rev `023b78b1c5a05ca512767cacc07efca55fae8be4`: al morir un cliente, `Client::kill` llama a `ClientData::disconnected` (enciende el ping); la limpieza del backend encola los destructores de todos los objetos y ejecuta `ObjectData::destroyed` (`wayland-backend-0.3.17/src/rs/server_impl/handle.rs:45-61`, `client.rs:749-756`).
- El destructor de `zwlr_layer_surface_v1` llama a `WlrLayerShellHandler::layer_destroyed` (`smithay-.../023b78b/src/wayland/shell/wlr_layer/handlers.rs:334-351`), y `handlers.rs:677-681` llama a `Layers::destroy`.
- Conclusión: la capa muerta se elimina en la misma limpieza del backend, antes de que calloop drene el ping de `client_disconnected`. La zona exclusiva no sobrevive al siguiente frame ni a un evento futuro; P1-03 es falso. La poda de `Layers::commit` cubre otro caso (un `wl_surface` destruido sin destruir el rol), no la desconexión.

### `schedule_frame_release`

- `SyncPoint::export()` ya devuelve un `OwnedFd` nuevo (llama a `eglDupNativeFenceFDANDROID` en `backend/egl/fence.rs:123-138`); el `try_clone` posterior duplica otra vez y el original se descarta. Es un paso innecesario: se puede usar el fd exportado directamente y eliminar ese camino de error.
- Si `insert_source` falla (loop cerrado o fallo de epoll), `PendingFrameRelease` se suelta. `Buffer` es `Arc<InnerBuffer>` compartido con `RendererSurfaceState`; soltar el clon no dispara el release de inmediato. Se pierde la retención del fence: cuando la superficie cambie de buffer en el siguiente commit (ADR 0026, punto 6), el release puede salir antes de que la GPU termine. No es un free inmediato, es una garantía perdida.
- `try_clone` podía fallar por `EMFILE`/`ENFILE`, pero sobra; `insert_source` puede fallar por el loop/epoll. Raro, pero el fallback sigue siendo necesario.

### `XDG_DESKTOP_DIR="$HOME"`

- `man user-dirs.dirs`: «VALUE must be of the form "$HOME/Path" or "/Path".» El valor pelado `$HOME` no está admitido.
- Hoy `parse_desktop_dir` lo deja como `PathBuf::from("$HOME")`; `screenshot_path` hace `create_dir_all` y crearía un directorio relativo `./$HOME` si no existe. Es una entrada inválida, no un caso que la especificación obligue a soportar; P1-06 es falso. Si se quiere defensa, expandir `$HOME` exacto o caer a `$HOME/Desktop`.

### PX-02 — directorios de configuración

- ADR 0043, tabla (línea 33): «`~/.config/yuki`, `~/.local/share/yuki` | `~/.config/malaysia`, `~/.local/share/malaysia`». Líneas 41-49: «No hay migración automática... `mv ~/.config/yuki ~/.config/malaysia`». README:139/148 y `docs/configuracion/opciones.md:16/27` repiten `malaysia`.
- Código: `crates/vow-appearance/src/lib.rs:518` y `:536` hacen `base.join("malaysia-compositor")`; `crates/malaysia-compositor/src/active.rs:38` tiene una tercera copia. Los comentarios de las mismas funciones (`:506` y `:524`) ya dicen `$XDG_CONFIG_HOME/malaysia` y `$XDG_DATA_HOME/malaysia`.
- Consumidores que pasan por `vow-appearance`: shell `wallpaper.rs:50-51` (`data_dir`), `menubar/bar.rs:631-632` (`data_dir`/`config_dir`), `dock.rs:81` (`data_dir`), `lock.rs:334-335` (`appearance_path`/`wallpaper_state_path`, dentro de `theme_revision`); compositor `chrome.rs:52` (`load_dominant`) y `active.rs:38` (copia propia); kit `titlebar.rs:380` (`config_dir`), `platform/mod.rs:1965,1999` y `theme.rs:69` (`load_appearance`/`load_dominant`); portal `xdg-desktop-portal-malaysia/src/settings.rs:12,174`; settings `malaysia-settings/src/main.rs:79`, `state.rs:17` (`appearance_path`), `cards.rs:140`, `sidebar.rs:134`, `groups.rs:145`; watcher `vow-appearance/src/watch.rs:39`. Todos salvo `active.rs` pasan por las funciones de `vow-appearance`; ninguno fija la ruta a mano.
- `share/systemd` y `nix/malaysia.nix` no fijan rutas de config; `dev.sh:761` y `:904` copian `~/.config/malaysia` a un temporal, y `dev.sh:778`/`:923` exportan `MALAYSIA_CONFIG_DIR` hacia ese temporal. dev.sh ya está alineado con los docs.
- Copias extra del literal: `crates/malaysia-compositor/src/capture.rs:511` (`$XDG_RUNTIME_DIR/malaysia-compositor`, capturas) y `crates/malaysia-shell/src/menubar/mod.rs:321` (logs de componentes). No son config/data y el ADR no las menciona, pero son dos copias más; total 5 (2 config/data, 1 active, 2 runtime).
- Conclusión: confirmado. El arreglo seguro es L1d (active.rs usa `vow_appearance::data_dir()`) y después L1e (cambiar los dos joins); así el cambio de ruta es atómico y el `mv` manual sigue siendo el único paso de migración.

### PX-03 y PX-07 — captura y screenshot con la sesión bloqueada

- Camino que sí rechaza: `request_screenshot` (`state.rs:675-681`) y `start_region_screenshot`/`start_window_screenshot` (`state.rs:697-720`) comprueban `locked`.
- Camino que no rechaza: `ImageCopyCaptureHandler::frame` (`handlers.rs:402-412`) no mira `locked`, y `new_session`/`capture_constraints` tampoco. El draw de ambos backends procesa `pending_captures` sin comprobar: winit `session.rs:1467-1512`, DRM `udev.rs:646-698`.
- Qué ve el cliente: para `Output`, `build_output_elements` (`state.rs:914`) con `locked` devuelve solo las lock surfaces, así que la captura es la pantalla de bloqueo. Para `Window`, el draw llama a `build_window_elements` (`session.rs:1477`) directamente y la captura es el contenido de la ventana oculta.
- PX-07: `finish_screenshot` (`state.rs:789`) no comprueba `locked`; `lock()` (`lock.rs:50-92`) no limpia `screenshot`/`screenshot_mode`; el draw (`session.rs:1452`, `udev.rs:632`) toma y guarda el request aunque esté bloqueado. El botón del puntero se procesa antes del filtro de lock (`input.rs:659-666`), así que un release tras el lock llama a `finish_screenshot`. El overlay no se dibuja con `locked` porque `build_output_elements` retorna antes, pero el estado sigue y reaparece al desbloquear.
- Conclusión: ambos confirmados. Mínimo: `frame` falla con `locked`; `finish_screenshot` exige `!locked`; `lock()` llama a `cancel_screenshot()`; tests de los tres casos (PX-11).

### PX-05 — `window.domain` y TOML

- `crates/malaysia-compositor/src/active.rs:59`: `format!("[active]\ntitle = \"{}\"\napp_id = \"{}\"\n", title, app_id)`.
- `toml` ya es dependencia directa del compositor en `crates/malaysia-compositor/Cargo.toml:21` y ya lo era en `718a7e4` (`git diff 718a7e4 HEAD -- crates/malaysia-compositor/Cargo.toml` está vacío). Serializar un struct con `toml::to_string` no añade dependencia.
- Disparo: un título `He said "hi"` invalida el TOML; un `\n` permite inyectar claves. Los consumidores `menubar/mod.rs:106-110` y `dock.rs:103-117` fallan el parseo y pierden nombre/resaltado.

### PX-09 — hilo por pegado

- `handlers.rs:564-588`: `send_selection` hace `std::thread::spawn` por cada `receive`. `write_all` sobre el fd del pipe es bloqueante y no tiene timeout. Si el cliente no lee, el hilo queda bloqueado; si el sistema no puede crear el hilo, `std::thread::spawn` hace panic en el hilo que lo llama (el del compositor), no devuelve un `Result` que registrar.
- Solo aplica mientras la selección es del compositor: `set_clipboard_png` (`state.rs:849-858`) instala `Arc<[u8]>` como `SelectionUserData`; es el camino de screenshot al portapapeles. Disparo: un cliente llama a `receive` repetidamente sin leer.

### PX-14 — temporizadores de idle

- `IdleTimers::insert` (`idle.rs:84-103`) hace `remove(id)`, `insert_source` y toma el mutex; `note_activity` (`idle.rs:191-210`) lo repite para cada notificación y para `arm_auto`.
- El call site es `process_input_event` (`input.rs:618`), que llama a `note_idle_activity()` antes del `match`: cada evento de entrada (incluido movimiento de ratón) pasa por ahí. Es camino caliente, así que entra en L4d.

### P5-11 — `StartLimit*` en systemd

- `systemd-analyze verify` (systemd 261.3) sobre una copia con `@out@` sustituido: `/…/malaysia-shell@wallpaper.service:18: Unknown key 'StartLimitIntervalSec' in section [Service], ignoring.` `StartLimitBurst` no avisa.
- Efectivo: burst 10 con el intervalo por defecto (10 s), no 30 s. Un componente que falla en bucle agota el límite antes y queda caído.

### P5-07 — nombre del escritorio

- `desktop.rs:153/157` compara con `MalaysiaDreams`; `share/wayland-sessions/malaysia.desktop:6` declara `DesktopNames=Malaysia`; `udev.rs:535` y `dev.sh:1054` exportan `XDG_CURRENT_DESKTOP=Malaysia`.
- Disparo: una app con `OnlyShowIn=Malaysia;` no aparece en el launcher y una con `NotShowIn=Malaysia;` sí aparece. Los tests de `desktop.rs:518/526` fijan el nombre equivocado.

## Preguntas

### Q1 — Alcance de ADR 0004 en las conversiones del compositor (P1-01)

- **A. Aplicar floor/ceil en el compositor.** Cambiar `physical_point`/`physical_rect` (y las conversiones lógico→físico de captura) a floor/ceil. Implica tocar geometría de chrome, sombras y capturas; requiere test de dos rectángulos adyacentes. Alinea el render con la ADR 0004.
- **B. Limitar la ADR a `malaysia-scene`.** Mantener `round` en el compositor y enmendar 0004 para decir que solo aplica a la escena. Implica aceptar posibles costuras de 1 px en bordes/sombra bajo escala fraccionaria y documentar la excepción.

### Q2 — `Windows::set_maximized` / `set_fullscreen` (P1-04)

- **A. Borrar `set_maximized`.** No existe un `set_fullscreen` simple; todas las llamadas usan `*_with_outer`. Menos API trampa y ningún consumidor roto.
- **B. Conservar `set_maximized` como API futura.** Documentar que solo cambia el estado y no la geometría, y que `set_fullscreen` no existe; el riesgo de uso incorrecto sigue.

### Q3 — Handlers vacíos (P1-14)

- **A. Comentar `// Intentionally empty: ...`** en `ImageCaptureSourceHandler::source_destroyed` y `OutputHandler`. Cuesta dos líneas y evita re-auditarlos.
- **B. Dejarlos como están.** No cambia el comportamiento; siguen pareciendo olvidos en auditorías futuras.

### Q4 — Rutas de configuración (PX-02 / P5-01)

- **A. `~/.config/malaysia` y `~/.local/share/malaysia` como canónicas.** Cambiar los dos joins de `vow-appearance`, hacer que `active.rs` use `data_dir()` y mover los directorios a mano (ADR 0043; sin compatibilidad). Es lo que dicen README, opciones y la ADR.
- **B. `malaysia-compositor` como canónicas.** Corregir ADR 0043, README y opciones, y no migrar. Evita tocar el código, pero contradice el rebranding y la documentación existente.

### Q5 — Captura con la sesión bloqueada (PX-03 / PX-07)

- **A. Fallar los frames y limpiar screenshot/mode al bloquear.** `frame` responde `CaptureFailureReason::Unknown`; `lock()` cancela la selección. Mínimo y cierra la fuga.
- **B. Pausar las sesiones y reanudarlas al desbloquear.** Conserva clientes de captura, pero añade estado y no está claro que el protocolo lo permita sin soltar el frame.

### Q6 — Pegado de imagen (PX-09)

- **A. Un hilo escritor con cola acotada y descarte de excesos.** Mantiene el event loop libre y pone un techo al número de hilos.
- **B. Escritura no bloqueante desde el event loop.** Sin hilo, pero exige gestionar `WouldBlock`, buffer por fd y el cierre ordenado.

### Q7 — Wallpaper a resolución completa (P5-18)

- **A. Limitar dimensiones al decodificar.** Menos RSS y arranque; puede perder calidad si la salida es mayor que el límite.
- **B. Mantener la resolución completa.** Prioriza calidad; solo documentar y medir el coste. La recarga de P5-04 debe seguir decodificando una vez por cambio.

### Q8 — Clave estable de ventana (P5-10 / PX-12)

- **A. `app_id` + título y resolver al ejecutar.** Sin tocar protocolos; falla si hay dos ventanas con el mismo título.
- **B. Id de ventana estable del compositor/kit.** Más correcto; toca la interfaz entre shell y compositor.

### Q9 — Resaltado del dock (P5-05 / PX-06)

- **A. Dock por `entry.active` + compositor publica en title/app_id.** Elimina el parser duplicado y corrige la menubar.
- **B. Solo corregir el compositor y dejar el dock leyendo `window.domain`.** Menos cambios en el dock, pero mantiene el acoplamiento y el parseo duplicado.

### Q10 — Entorno del launcher (P5-09)

- **A. No borrar `MALAYSIA_ORIG_*` en `prepare_kit_env`.** El sanitizador del hijo los quita; una línea.
- **B. Mantener un mapa explícito de variables originales.** Más control, más código y estado.

### Q11 — systemd `StartLimit*` (P5-11)

- **A. Mover las dos claves a `[Unit]`.** El intervalo de 30 s pasa a ser real; es lo que documenta systemd.
- **B. Mover solo `StartLimitIntervalSec` y dejar `StartLimitBurst` en `[Service]`.** Menos cambio, pero deja una clave fuera de su sección.

## Decisiones (las rellena el usuario)

- Q1 (P1-01): A. floor para posiciones y ceil para tamaños según ADR 0004 en session.rs, state.rs, window.rs, popup.rs y captura, con test de rectángulos adyacentes.
- Q2 (P1-04): A. Borrar Windows::set_maximized.
- Q3 (P1-14): A. Una línea de comentario por handler vacío explicando por qué está vacío, en el idioma de los comentarios del archivo.
- L3c, L3d, L3e: solo se hace la parte que borra código duplicado. Mover código entre módulos sin reducir líneas no se hace ahora; anótalo para docs/por-hacer.md. Vuelve a escribir esos sublotes en la tabla con lo que queda (o márcalos "aplazado").
- L4c (P1-17): DESCARTADO por la regla de camino caliente, salvo que se demuestre que va por frame o por evento de entrada.

### PX y P5

- Q4: A. ~/.config/malaysia y ~/.local/share/malaysia; sin migración automática (ADR 0043); el informe del lote dice el comando mv.
- Q5: A. Con locked, frame.fail() y limpiar la selección/screenshot pendiente.
- Q6: A. Límite de 4 escrituras de portapapeles a la vez; las que sobran se descartan con un log.
- Q7: A, prioridad baja: decodificar al tamaño de la salida más grande.
- Q8: B si la shell ya recibe un identificador estable del protocolo (ext-foreign-toplevel-list identifier u otro); si no, A (app_id + título).
- Q9: A.
- Q10: A.
- Q11: A.
- L1d y L1e se hacen en la misma sesión y se publican juntos.

## Pendiente P3

Pendiente para P3: duplicación winit (session.rs) / DRM (udev.rs), no auditada en P1.
