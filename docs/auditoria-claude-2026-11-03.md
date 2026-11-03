# Auditoría Claude — 2026-11-03

Revisión completa del código de malaysia contra `AGENTS.md`/`CLAUDE.md`,
comparada con niri. Versión en inglés: [`auditoria-claude-2026-11-03.en.md`](auditoria-claude-2026-11-03.en.md).

## Objetivo y alcance

- **Objetivo:** diagnosticar el código muerto, el rendimiento, las fugas, los
  fallos y lo que falta, y proponer una reestructuración y una lista de
  ficheros que borrar.
- **Alcance:** todo el repo en `345ba40` (2026-11-03): 7 crates, `tools/`,
  `scripts/`, `share/`, `nix/`, `docs/`, `dev.sh` y `flake.nix`. Este documento
  no cambia código.
- **Lector:** quien trabaje en malaysia, persona o agente. Cada hallazgo se
  convierte en una fila de [`deuda.md`](deuda.md) o en un bloque de
  [`roadmap.md`](roadmap.md) cuando se acepte. Estado y deuda siguen viviendo allí.
- **Entradas:**
  - el código en `345ba40`;
  - Smithay en `023b78b1`;
  - barrierkit en `112632b`;
  - niri en `a8562a1` (2026-10-31);
  - cosmic-comp en HEAD.
- **Salidas:** las secciones 1 a 10 y el plan por lotes de la sección 11.

## Método

1. Lectura estática por rangos y con `grep`, repartida en seis frentes:
   - render y backend;
   - entrada y ventanas;
   - shell;
   - wallpaper, ajustes, portal, escena y shaders;
   - repo, docs y Nix;
   - investigación de niri.
2. Cada hallazgo de código muerto se buscó también en `tools/`. Como
   `lib.rs` exporta módulos con `pub mod`, rustc no avisa de ese código
   muerto (§4).
3. Los hallazgos críticos se comprobaron dos veces: contra el código y contra
   el código de Smithay del rev fijado.
4. **No se compiló.** `nix develop` falla en este contenedor: el proxy
   devuelve 403 al descargar las fuentes de Skia. Ningún resultado sale de
   clippy ni de los tests. El primer lote debe empezar con `./dev.sh check`.

Leyenda: **[V]** verificado contra el código · **[S]** sospecha razonada.
Severidad: **A** alta · **M** media · **B** baja.

## Resumen: las diez cosas que importan

| # | Hallazgo | Dónde | Sev. |
|---|---|---|---|
| 1 | Cualquier cliente Wayland desbloquea la pantalla: un segundo `ext_session_lock_v1.lock` sustituye al lock vivo y su `unlock_and_destroy` desbloquea | `malaysia-compositor/src/lock.rs:29-49`, `state.rs:285` | A [V] |
| 2 | Suspender no bloquea: nadie escucha `PrepareForSleep` | `malaysia-shell/src/menubar/system.rs:130` | A [V] |
| 3 | El daño se anula en cada frame: ids nuevos y `damage_since` siempre completo | `effects.rs:170-180`, `chrome.rs:225,270`, `session.rs:653,1384` | A [V] |
| 4 | No hay máquina de redibujado: render en cada vblank y en cada evento de entrada, y bucle de 60 Hz en reposo | `udev.rs:329-331,541-566,695-757`, `session.rs:1650-1680` | A [V] |
| 5 | Los tiempos de `wp_presentation` no son CLOCK_MONOTONIC y el refresco está fijo en 16,666 ms | `udev.rs:557`, `session.rs:1989`, `state.rs:275` | A [V] |
| 6 | `capture_sessions` solo crece: cada captura o screencast deja viva su sesión | `state.rs:90,433`, `handlers.rs:308` | A [V] |
| 7 | Las apps lanzadas desde la menubar heredan su cgroup y dejan zombis | `menubar/mod.rs:212,305-327`, `system.rs:96`, `nix/malaysia.nix:82` | A [V] |
| 8 | Unas 25 opciones documentadas en `opciones.md` se parsean y nadie las lee | `config.rs` (§4.1) | M [V] |
| 9 | `bandura.jpg` se incrusta en el binario sin licencia | `NOTICE:85-86`, `malaysia-wallpaper/src/image.rs:6-8` | A [V] |
| 10 | La regla bilingüe se incumple del todo: 0 `.en.md` de 73 documentos | `docs/`, `README.md` | M [V] |

## 1. Seguridad y fallos de corrección

### 1.1 Compositor

- **Bypass del lock** — `lock.rs:29-49` y `state.rs:285`.
  - Problema:
    - `lock()` llama siempre a `confirmation.lock()`, aunque ya haya un lock vivo.
    - En Smithay `023b78b1`, `SessionLocker::lock` sobrescribe `LockStatus::Locked(lock)` con el lock nuevo.
    - `UnlockAndDestroy` solo comprueba `is_locked_by(lock)`.
    - El filtro del global es `|_| true`.
  - Consecuencia: cualquier cliente hace `lock` y luego `unlock_and_destroy`, y la sesión queda desbloqueada.
  - Arreglo: como niri (`src/niri.rs`, `lock`), si `self.locked` y el lock anterior sigue vivo, soltar el `SessionLocker` sin confirmarlo; Smithay envía entonces `finished`. Unas 10 líneas.
  - A [V]
- **Clics bajo el lock** — `lock.rs:29-49` e `input.rs:613-625,1883,1912`.
  - Problema:
    - `lock()` no quita el foco del puntero ni termina `self.grab`.
    - Hasta el siguiente movimiento, los botones y el scroll llegan a la ventana oculta.
    - Un move o resize en curso sigue activo, porque la rama del grab va antes de la comprobación de `locked`.
  - Arreglo: en `lock()`, `grab = None` y reasignar el foco del puntero; en Button y Axis, salir si `locked` y el foco no es una lock surface.
  - A [V]
- **Acciones de ventanas con la sesión bloqueada (PX-13 ampliado)** — `input.rs:432-468`.
  - Problema: estas rutas tampoco miran `locked`:
    - `ArrangePair*`;
    - `ArrangeRestore`;
    - `exit_focus_mode` (`spaces.rs:309`);
    - foreign `SetMaximized`/`SetMinimized` (`foreign.rs:186-194`);
    - infinity `Arrange*` (`infinity.rs:155`).
  - Arreglo: un único punto de guarda en el despacho de atajos (§6.2).
  - M [V]
- **Globals abiertos a todos los clientes.**
  - Problema:
    - cualquier cliente, incluso en sandbox, puede usar wlr-foreign-toplevel-management (cerrar y minimizar cualquier ventana), image-copy-capture (de outputs y de ventanas), session-lock y layer-shell overlay;
    - no hay `security-context-v1`;
    - solo infinity filtra (`infinity.rs:132`).
  - Arreglo: añadir `security-context` y filtrar estos globals a clientes sin contexto, o a la shell.
  - M [V]
- **Confianza de infinity en rutas de desarrollo** — `infinity.rs:30-77`.
  - Problema:
    - se confía en `$CWD/target/{debug,release}/malaysia-shell`, en `$CWD/result/bin` y en `MALAYSIA_DEV=1`;
    - identificar por `/proc/pid/exe` tiene carrera de PID.
  - Arreglo: limitarlo a `cfg(debug_assertions)`.
  - B [V]
- **Popup grab rechazado sin `popup_done`** — `handlers.rs:799-855`.
  - Problema:
    - xdg-shell obliga a descartar el popup cuando se deniega el grab;
    - solo se acepta un serial de botón, así que los menús abiertos con teclado quedan sin grab.
  - M [V]
- **Toplevel sin unmap** — `window.rs:378-512`.
  - Problema:
    - `mapped` nunca vuelve a `false`;
    - un commit con buffer nulo deja la ventana en la escena, en foreign y con foco;
    - al volver a mapear no se repite el configure inicial.
  - M [V]
- **Pointer constraints y relative pointer** — `input.rs:1887,1995-2029`.
  - Problema:
    - una región de confine nula («toda la superficie», lo habitual en juegos) no confina;
    - el delta relativo se calcula sobre posiciones ya recortadas, así que es 0 en el borde y se pierde `delta_unaccel`.
  - Arreglo: usar `event.delta()` y `event.delta_unaccel()`.
  - M [V]
- **Idle-inhibit con ventanas invisibles** — `idle.rs:419-433`.
  - Problema:
    - una ventana minimizada o en otro espacio inhibe el auto-lock para siempre, y el protocolo dice «mientras sea visible»;
    - además, cada ciclo inhibit/uninhibit añade un `add_destruction_hook` más.
  - M [V]
- **`set_var` con hilos vivos** — `spawn.rs:49`.
  - Problema: el `// SAFETY:` dice «single-threaded during startup», pero udev ya creó EGL/GLES (`udev.rs:160-165`) y Mesa puede tener hilos.
  - Arreglo: exportar el entorno en `main()` antes del backend.
  - M [S]
- **Costura de 1 px en el titlebar** — `chrome.rs:319` frente a `session.rs:157`.
  - Problema: la textura del titlebar usa `to_i32_round` y la superficie `to_i32_floor`, contra el ADR 0004. Con escala fraccional puede quedar una costura de 1 px.
  - Las capturas también redondean al más cercano: `session.rs:1484` y `handlers.rs:367-368`.
  - M [V]
- **Release de buffers en DRM** — `udev.rs`.
  - Problema: udev no retiene los `Buffer` hasta la fence del frame. `collect_drawn_buffers` y `schedule_frame_release` solo existen en winit. Con `drm_syncobj`, el release podría señalarse antes de que la GPU termine.
  - M [S]
- **Menores** [V]:
  - `maximize_request` sobre una ventana ya maximizada no envía configure (`input.rs:996`).
  - foreign anuncia v3 pero ignora `SetFullscreen` (`foreign.rs:194`) y crea los handles con título vacío más `done` (`foreign.rs:242`).
  - `unlock` no restaura el foco de teclado.
  - `idle_timeout_seconds` no se recarga en `refresh_theme` (`state.rs:394`).
  - El icono de DnD no se dibuja (`handlers.rs:522`).

### 1.2 Shell

- **Suspender sin bloquear** — `menubar/system.rs:130`.
  - Problema: llama a logind `Suspend` y no hay inhibidor ni manejador de `PrepareForSleep` en ningún crate (grep 0).
  - Arreglo: tomar un inhibidor `delay` de logind y bloquear antes de dormir. Mejor en el compositor, que sobrevive a la shell.
  - A [V]
- **Cgroup y zombis.**
  - Problema:
    - la menubar corre como `malaysia-shell@menubar.service`;
    - `setsid` no saca a los hijos del cgroup, así que si la menubar cae, systemd mata las apps del usuario;
    - el `Child` se suelta sin `wait` (`menubar/mod.rs:212`, `system.rs:96`, `mod.rs:310-318`) y nadie atiende `SIGCHLD`.
  - Arreglo: un único `spawn.rs` en la shell, con scope `systemd-run --user --scope` o `StartTransientUnit` (como niri, `src/utils/spawning.rs`), y un hilo que haga `wait()`.
  - A [V]
- **Forzar salida por índice** — `dialogs.rs:290-293`, `focused.rs:230,639`, `dock.rs:49` (P5-10).
  - Problema: si la lista cambia con el diálogo abierto, SIGKILL va a otra app.
  - Causa: `ToplevelInfo` de barrier-kit no tiene id estable (`shell/toplevels.rs:132-190`).
  - A [V]
- **`unsafe` sin `// SAFETY:`.**
  - Problema: 44 bloques en la shell, contra la regla 6:
    - `pam.rs`, 28 bloques;
    - `launcher.rs:570`;
    - `menubar/mod.rs:121,172,204`;
    - `system.rs:64-77`.
  - Hay más: `udev.rs:164` y `malaysia-settings/src/state.rs:50`, que además no comprueba `pw_gecos` nulo.
  - M [V]
- **Restos de la contraseña en el heap** — barrierkit `password.rs:33`.
  - Problema: `Zeroizing<String>::push` realoca y libera el buffer viejo sin borrarlo.
  - Arreglo: reservar capacidad al crear el campo.
  - M [V]
- **Menores** [V]:
  - `exec_matches` (`menubar/mod.rs:66-71`) usa `contains`: tokens cortos como `sh` casan con apps ajenas.
  - Los códigos `%i %c %k` de Exec se descartan en vez de expandirse (`desktop.rs:313-319`).
  - `collect_desktop_files` sigue symlinks sin detectar ciclos (`desktop.rs:72-74`) [S].
  - El número de serie se registra a nivel `info` (`dialogs.rs:803`).
  - Si `current_user()` falla, el lock queda en «verificando» sin mensaje (`lock.rs:284-287`).

### 1.3 Wallpaper y portal

- **Orden de destrucción** — `malaysia-wallpaper/src/wayland.rs:340-345`.
  - Problema:
    - destruye el `wl_surface` antes que el `zwlr_layer_surface`, y el protocolo exige destruir antes el rol (SCTK 0.19.2, `wlr_layer/mod.rs:340-348`);
    - además, el `Drop` de SCTK lo destruye otra vez.
  - Arreglo: borrar `output.surface.destroy()`.
  - M [V]
- **Recarga contradictoria** — `image.rs:64-66` y test `image.rs:672`.
  - Problema: como la app hace `take()` de la imagen tras subirla, en producción una selección ilegible salta a `bandura.jpg`. El test afirma lo contrario.
  - Arreglo: decidir un comportamiento, borrar la rama o el test, y documentarlo.
  - M [V]
- **Imágenes de cámara** — `image.rs:255-270`.
  - Problema: los JPEG ignoran la orientación EXIF y el perfil ICC.
  - M [S]
- **Rama inalcanzable** — `xdg-desktop-portal-malaysia/src/settings.rs:72`.
  - La rama `ColorScheme::Auto` es inalcanzable. Por lo demás, el portal cumple la especificación de Settings.
  - B [V]

## 2. Render y rendimiento frente a niri

### 2.1 Cómo renderiza hoy malaysia

- **winit:**
  - `draw()` se ejecuta con `WinitEvent::Redraw`;
  - `render_output` recibe siempre age 0 (`session.rs:1609`), así que cualquier daño repinta la salida entera;
  - sin daño, un timer de 1/refresh vuelve a pedir frame (`session.rs:1650-1680`).
- **udev:**
  - `DrmCompositor` con `FrameFlags::empty()` y los planos `overlay`/`cursor` vaciados (`udev.rs:178-180,654`);
  - `render()` se ejecuta tras cada evento de libinput (`udev.rs:329-331`), tras cada dispatch con petición de redibujo y tras cada vblank sin condición (`udev.rs:546-566`);
  - sin estado de «frame en vuelo», `queue_frame` pisa el frame pendiente.
- **Elementos:**
  - `build_output_elements` reconstruye todo cada frame;
  - sombra y borde con `PixelShaderElement::new` (un `Id` nuevo cada vez);
  - titlebar con `Id::new()` aunque la textura esté cacheada;
  - el contenido recortado declara daño completo y ninguna región opaca.
- **Resultado:** con una ventana visible, cada frame tiene daño completo y udev recompone a la tasa de refresco aunque nada cambie.

### 2.2 Qué hace niri y qué adoptar

| # | Cambio | Ganancia | Esfuerzo | Referencia |
|---|---|---|---|---|
| 1 | Máquina `RedrawState` (Idle, Queued, WaitingForVBlank, WaitingForEstimatedVBlank): entrada y commits solo marcan `Queued`; un render por vblank; timer de vblank estimado si no hay daño | CPU/GPU en reposo ≈ 0; fin de los `queue_frame` pisados | Medio, ~120 líneas | niri `src/niri.rs` (`RedrawState`, `queue_redraw`), `src/backend/tty.rs` (`on_estimated_vblank_timer`), wiki `Development:-Redraw-Loop` |
| 2 | Elementos de shader persistentes por ventana (sombra, borde, titlebar, sólidos) con `Id` fijo y `CommitCounter` que solo sube al cambiar | El daño vuelve a ser parcial | Bajo-medio | `src/render_helpers/shader_element.rs`, `shadow.rs`, `border.rs` |
| 3 | `RoundedTextureElement` delega `damage_since` y `opaque_regions` (menos las esquinas) al elemento interior | Daño real y descarte por oclusión | Bajo | `src/render_helpers/clipped_surface.rs` |
| 4 | Presentation feedback correcta: `take_presentation_feedbacks` al renderizar y pasarla como user data de `queue_frame`; `meta.time` del vblank; `Refresh` del modo real | Corrige un bug de protocolo (A/V sync de mpv, Firefox y GTK4) | Bajo | `src/backend/tty.rs` (vblank y feedback) |
| 5 | Frame callbacks solo a superficies visibles (`surface_primary_scanout_output`), deduplicados por secuencia y con throttle de 995 ms para las ocultas | Las apps minimizadas o en otro espacio dejan de pintar a 60 fps | Bajo-medio | `src/niri.rs` (`send_frame_callbacks`, `FRAME_CALLBACK_THROTTLE`) |
| 6 | Planos de cursor y primario, `FrameFlags::DEFAULT`, `Kind::ScanoutCandidate` en fullscreen y feedback dmabuf por superficie con tramo de scanout | Vídeo y juegos sin composición; cursor sin repintar | Bajo (flags), medio (feedback) | `src/backend/tty.rs`, `src/niri.rs` (`send_dmabuf_feedbacks`) |
| 7 | Reloj congelado en `target_presentation_time`, un solo `now` por dispatch (`LazyClock`) y redibujar solo mientras queden animaciones | Animaciones sin jitter | Medio | `src/animation/clock.rs`, wiki `Development:-Animation-Timing` |
| 8 | Blur xray: difuminar una vez el fondo y compartirlo; dual-Kawase con texturas reutilizadas; `ext-background-effect` | Cumple la política 3 (blur para todas) a bajo coste | Alto, ~400 líneas; requiere 2 y 3 | niri `render_helpers/blur.rs`, `xray.rs`, wiki `Window-Effects`; cosmic `backend/render/wayland/blur_effect.rs` |
| 9 | `spawn` con doble fork y scope systemd `app-malaysia-*`, sin `NOTIFY_SOCKET` y con `RLIMIT_NOFILE` restaurado | Sin zombis; OOM aislado por app | Bajo-medio | `src/utils/spawning.rs` |
| 10 | Feature `profile-with-tracy` y opción de debug para dibujar el daño, más pasar a `trace!` el `info!` por frame (`udev.rs:552,667,682`) | Poder medir los puntos 1 a 8 | Bajo | niri `Cargo.toml` (features tracy), `render_helpers/debug.rs` |

Orden: 1→4 primero, porque corrigen el consumo en reposo y un bug de
protocolo; 8 depende de 2 y 3.

### 2.3 Otros puntos calientes

- **Compositor, entrada** [V]:
  - `note_activity` reinserta N+1 timers y reserva un `Vec` por evento (PX-14, `idle.rs:152-169`). Arreglo como niri: una bandera `notified_activity_this_iteration`.
  - `update_edge_cursor` repite las búsquedas que `pointer_motion` hace 10 líneas después (`input.rs:2121-2130` frente a `1936-1945`).
  - `Windows::iter` hace un `find` lineal por cada entrada de la escena: es O(n²) en cada movimiento (`window.rs:321-332`). `index`/`index_of` son O(n) (`window.rs:1181-1190`).
  - `log_focus_stack` escribe todas las ventanas a nivel INFO en cada clic (`input.rs:1051-1066`).
- **Compositor, render** [V]:
  - Asignaciones por frame:
    - `Vec` de ids (`session.rs:777,976,989`);
    - `String` del título por ventana (`session.rs:1101`, `chrome.rs:198`);
    - una fuente calloop `Generic` por frame (`session.rs:1791`);
    - el `vec!` de uniforms (`effects.rs:201`).
  - Cada cambio de hover de los semáforos rasteriza en CPU todo el titlebar e importa otra textura (`chrome.rs:194-221`).
  - Cada fade de popup crea una textura offscreen del tamaño de la salida (`popup_fade.rs:54-66`).
  - Cada frame de screencopy hace una textura y un tracker nuevos, `sync.wait()` bloqueante y daño completo con tiempo `Duration::ZERO` (`capture.rs:205-247`, `session.rs:1556-1601`, `udev.rs:609-657`).
  - `window.domain` se reescribe de forma síncrona y no atómica con cada cambio de título (`active.rs:22-37`). La shell no lee el título.
  - La sombra son dos quads a pantalla completa de la ventana más el margen, con `pow`/`exp` por fragmento (`grade-graphics/src/shadow.frag`). Arreglo: fusionar las capas o emitir solo el marco.
- **Shell** [V]:
  - La menubar consulta `window.domain` cada 250 ms (`bar.rs:824-837`) y `menubar.domain` cada 500 ms (`bar.rs:930-951`). `Toplevels` ya trae `active`.
  - El dock lee y parsea TOML en cada render (`dock.rs:22`).
  - El launcher escanea todos los `.desktop` y resuelve iconos de forma síncrona en cada Super+Espacio (`launcher.rs:429-435,470-524`), y clona entradas e iconos en cada render (P5-17).
  - `composition()` clona el árbol remoto entero por cada título (`bar.rs:516-574`).
  - `power::run(Suspend)` hace D-Bus bloqueante en el hilo de UI (`system.rs:130`).
- **Wallpaper** [V]:
  - Hasta 3 o 4 copias de la imagen en RAM durante la subida (`render.rs:231,267`; `image.rs:287`). Con un PNG de 16 bits de 24 MP, unos 700 MB de pico.
  - La textura va a resolución completa con mips, sin limitar al output (`render.rs:229-256`).
  - `desired_maximum_frame_latency: 2` para contenido estático (`wayland.rs:430-433`) [S].
  - El redibujado ya es correcto: solo en configure, cambio de escala o recarga.
- **Portal** [V]:
  - Cada cambio de foco o de título reescribe `window.domain`, y el watcher lo clasifica como `Defaults`. El portal relee `appearance.domain` dos veces, regenera 3 CSS y lee 3 ficheros GTK (`settings.rs:224`).
  - Arreglo en una línea: `if !events.iter().any(|e| e.restyles()) { continue }`.

## 3. Fugas

| Dónde | Qué | Arreglo | Sev. |
|---|---|---|---|
| `state.rs:90,433`, `handlers.rs:308` | `capture_sessions` solo hace `push`; falta `session_destroyed` (Smithay `image_copy_capture/mod.rs:840`). Tampoco se llama a `update_constraints` al redimensionar | `retain` en `session_destroyed` y `update_constraints` en resize | A [V] |
| `udev.rs:736-757` | `schedule_repaint` mira `repaint_timer` pero nunca lo asigna, así que los timers se acumulan | Guardar el token | M [V] |
| `handlers.rs:489-509` (PX-09) | Un hilo por `send_selection`; `write_all` bloqueante sin timeout | Escritura no bloqueante con `Generic` de calloop | M [V] |
| `input.rs:1508-1552` | `forget_window` no limpia `arranged` ni `minimized` | +2 líneas | B [V] |
| `window.rs:1017-1030` | `pending_decorations` de superficies que mueren sin rol | Limpiar en `CompositorHandler::destroyed` | B [S] |
| `idle.rs:419-433` | Hooks de destrucción acumulados | Registrar una vez | B [V] |
| shell `menubar/mod.rs:212,305-327`, `system.rs:96` | Zombis por cada app, `xdg-open` y componente lanzado | Un hilo que haga `wait()` (§1.2) | A [V] |
| shell `dbusmenu.rs:157-180,735-779` | Si un cliente muere sin `UnregisterWindow`, su entrada se queda en el Registrar | Limpiar al desaparecer el dueño | B [V] |
| shell `bar.rs:37-41` | `MenuMemory.titles` crece sin límite | LRU | B [V] |
| shell `menubar/mod.rs:293-297` | Logs en tmpfs en modo append sin rotar, y `info!` en cada `ItemsPropertiesUpdated` (`dbusmenu.rs:953`) | Truncar al abrir y bajar a `debug` | B [V] |
| wallpaper `wayland.rs:303,340-345,580-583` | `wp_fractional_scale_v1` y `wp_viewport` no se destruyen; tras `closed()` el output se queda sin fondo | `Drop` en `OutputSurface` y recrear la superficie | M [V] |
| wallpaper y portal | Si se borra el directorio vigilado, `IN_IGNORED` no se trata y la vigilancia se pierde | Re-armar el watch | B [S] |

Sin fugas [V]: `ChromeCache` (`input.rs:1533`), `popup_fades`, `focus_notices`,
gtk, appmenu, foreign, infinity, layers, popups y la textura del wallpaper.

## 4. Código muerto

### 4.1 Configuración que nadie lee

`config.rs` parsea estas opciones y `opciones.md` las documenta, pero nadie las
lee fuera de los tests [V]:

| Opción | Líneas | Notas |
|---|---|---|
| `effects.*` (`inner_border`, `shadow`, `blur`, `corner_radius`) | `config.rs:271-342` | El compositor toma los efectos de `vow-design`; además los defaults son literales de diseño |
| `debug.damage_highlight` | `config.rs:347` | Implementarla (tabla de §2.2, punto 10) o borrarla |
| `dock.*` (las 11) | `config.rs:219-251` | El dock no lee `dock.domain` |
| `menubar.autohide`; `menubar.app_menus` duplicado | `config.rs:256-258` | La shell tiene su propio `MenubarConfig` (`bar.rs:913`) con otro valor por defecto |
| `wallpaper.*` | `config.rs:83-89,174-187` | Lo lee `malaysia-wallpaper` con otro esquema (`image.rs:93-121`) |
| `window.titlebar`, `window.focus` (`Sloppy`) | `config.rs:194` | `Sloppy` no está implementado |
| `screenshot.{show_thumbnail, include_date, disable_shadow, timer, remember_selection}` | `config.rs:107-115` | La interfaz de capturas no existe |

Decisión que hay que tomar: borrar del compositor todo lo que no consume (regla
2) y dejar cada opción en el proceso que la usa. Se estiman unas −150 líneas. Si
se quieren conservar como contrato, que `opciones.md` las marque «sin
implementar» y el roadmap las tenga como trabajo.

### 4.2 Funciones, campos y variantes [V]

- **Compositor:**
  - Sin llamadas:
    - `window.rs:543 remove_by_id`;
    - `window.rs:941 Windows::set_maximized` (P1-04);
    - `xdg.rs:54 is_directory`.
  - Solo la usan los tests: `window.rs:160 usable_size`.
  - Duplicado de `id_of`: `window.rs:1106 id_of_surface` (P1-09).
  - Campos que solo se escriben:
    - `state.rs:145-147` `infinity_global` (con `#[allow(dead_code)]`, P1-08) y `_focus_notice_global`;
    - `state.rs:90` `capture_sessions`;
    - `gtk.rs:75` `GtkSurfaceState.modal`.
  - Siempre `Some`:
    - `gtk.rs:74 owner`;
    - `session.rs:89` y `udev.rs:71` `appearance_watch`;
    - `session.rs:1356 border_element` (P1-10).
  - Valores o variantes que no se usan:
    - el `bool` que devuelve `handlers.rs:212 insert_blocker_source`;
    - `decoration.rs:49 Role::Popup` (solo en tests);
    - el parámetro `titlebar` de `for_window`, que siempre recibe `&TITLEBAR`.
  - Código inalcanzable o pisado:
    - `infinity.rs:196-224 WEnum::Unknown(0..=9)`;
    - `handlers.rs:947-951 clear_requested_decoration`, pisado justo después;
    - el filtro de `idle.rs:162-168`.
  - Literales o uniformes que sobran:
    - `chrome.rs:352 Metrics::new(13.0, 17.0)`, que se sobrescribe;
    - `chrome.rs:262-263`, donde `rect == window_rect`, y el uniforme `window_rect` de `shadow.frag`;
    - el uniforme `"size"` de `effects.rs:349,373`, que Smithay ya define.
  - Ganchos de entorno sin usuario:
    - `MALAYSIA_TEST_LEAK_WINDOW` (`input.rs:1562`, `state.rs:156`);
    - `MALAYSIA_TEST_CURSOR_BOOST` (`state.rs:749`);
    - `MALAYSIA_THEME_FADE_HOLD_MS` (`theme_fade.rs:25`).
  - **Causa de fondo:** `lib.rs:6-31` exporta `capture`, `config`, `decoration`, `spawn`, `state`, `window` y `xdg` como `pub mod`, aunque `main.rs` solo usa `session` y `udev`. Al pasarlos a `mod`, rustc listará el resto de código muerto.
- **malaysia-scene:**
  - `geometry.rs`: `Physical`, `Buffer` y todas sus conversiones solo se usan en tests (unas −150 líneas). El invariante del ADR 0004 se prueba en código que nadie ejecuta.
  - `scene.rs:81-87`: `len` e `is_empty` no tienen llamadas.
  - `spaces.rs:137-143`: `show_next` y `show_previous` solo se usan en tests.
- **Shell:**
  - `dbusmenu.rs:1240 raw_items_from_layout` (con `allow(dead_code)`, solo la usan tests).
  - `menubar/mod.rs:77 ActiveWindow.title` (P5-20).
  - `lock.rs:32,264,318-331 theme_revision` (P5-12).
  - `bar.rs:356-378 last_gtk_menus`, que solo sirve para logs.
  - `dbusmenu.rs:25-37 RENDER_CALLS`, que siempre vale 0.
  - El parámetro `args` del closure de `appmenu.rs:587`.
- **Wallpaper:**
  - `image.rs:95 dir` (con `allow(dead_code)`).
  - `wayland.rs:79 fractional_scale`, que es `Option` pero siempre `Some`.
  - `render.rs:449-467 gpu_texture_bytes`.
  - `render.rs:1170 corner_inset_for_capture`.
  - El campo `_pad` (`render.rs:35`, `wallpaper.wgsl:12`).
  - El wrapper `LoadedImage`/`load_snapshot` (`image.rs:29-36`).
- **Dependencias:**
  - shell: `png` en `[dependencies]` solo se usa en tests y `freya-engine` solo en tests (pasarlas a dev-deps);
  - wallpaper: `png` duplicada en dev-deps y la feature `bytemuck` de `half` sin uso;
  - workspace: el perfil `dev-fast` no lo usa nadie, y `sctk` y `smithay-client-toolkit` declaran el mismo crate dos veces.

### 4.3 Tests que no protegen nada (regla 8) [V]

- `launcher.rs:626-856`: el golden dibuja una copia del layout (`Case`), no el `LauncherApp` real, y si falta el PNG lo escribe y pasa.
- `appmenu.rs:665-1292`: unas 600 líneas que comprueban etiquetas de menús estáticos.
- `grade-graphics/src/lib.rs:28-167`: comprueban subcadenas del GLSL y una copia del SDF reescrita en Rust.
- `logging.rs`: prueba una tabla trivial, y `LogLevel` reimplementa `LevelFilter::from_str`.
- `malaysia-wallpaper/src/render.rs:779-1129`: exige un adaptador Vulkan dentro de `cargo test`.
- Los tests del portal leen `appearance.domain` real del host.

## 5. Reducir código: duplicados y reglas

### 5.1 Duplicados (regla 1) [V]

| Qué | Copias | Ahorro |
|---|---|---|
| Bucle de captura y overlays de debug | `session.rs:1533-1601` y `udev.rs:577-657` | ~−100 |
| Montaje del event loop (ping, watcher, socket, señales, xwayland) | `session.rs:263-381` y `udev.rs:217-420` | ~−150 |
| Timer de repintado | `session.rs:1650-1675`, `udev.rs:696-719,736-757` | ~−40 |
| Frame callbacks inline | `session.rs:1688-1710` y `1959-1981` | ~−20 |
| Color de fondo `[0.06, 0.06, 0.09, 1.0]`, literal de diseño | 7 sitios: `session.rs:1511,1589,1611`, `udev.rs:646,666`, `theme_fade.rs:243`, `capture.rs:112` | token en `vow-design` |
| `set_focus` + `data_device` + `primary` | `input.rs:1097,1343,1378,1409` | helper `set_seat_focus` |
| `parse_ms` y `parse_resize_timeout` (y sus tests) | `input.rs:269`, `window.rs:38` | ~−30 |
| Cuatro funciones de easing | `theme_fade.rs:194`, `state.rs:778,782`, `spaces.rs:27` | ~−20 |
| `app_id`, predicado de visibilidad y `round()` | `window.rs:90/1077`; `window.rs:81,651,681`; `input.rs:2261`, `window.rs:1354`, `infinity.rs:538` | ~−30 |
| `sanitize_client_env` (P5-13) y `pre_exec(setsid)` ×4 | `launcher.rs:570,584`, `menubar/mod.rs:121,172,204,245` | ~−40 |
| Rutas XDG ×3 y búsqueda en PATH ×2 | `desktop.rs:24-41,238-256`, `launcher.rs:486-497`, `recents.rs:89-95`, `menubar/mod.rs:183-193` | ~−40 |
| `RawItem` y `RawNode` copiados del kit | `dbusmenu.rs:69-74,1238` frente a `barrier_kit::app_menu` | ~−25 |
| Lectores de `OwnedValue` ×3 | `dbusmenu_map.rs:72-82`, `gtkmenus.rs:394,419` | ~−15 |
| Transferencia sRGB ×6 | `wallpaper/image.rs:379-413`, `render.rs:471,722`, `vow-appearance/backdrop.rs`, `vow-design/lib.rs:61` | ~−40, un par en `vow-design` |
| Parseo de `.domain` sin código común | `wallpaper.domain` ×2, `appearance.domain` ×2 (G-01), `menubar.domain` ×2, `window.domain` ×2 | §6.4 |
| Lista de comandos de `dev.sh` ×3; árbol de dominios ×2 | `README.md`, `docs/README.md`, `AGENTS.md`, `opciones.md` | enlazar |
| Bloque `build_probes` ×5; espera del socket ×6 | `dev.sh` | ~−80 |

### 5.2 Literales de diseño fuera de `vow-design` [V]

- **Compositor:**
  - `window.rs:245-246`: `CLIENT_RESIZE_BORDER 6.0` y `CORNER 12.0`.
  - `infinity.rs:25`: `TILE_GAP 8`.
  - `input.rs:282-286`: los umbrales `SHAKE_*`, aunque ya existen los tokens `CURSOR_SHAKE_*`.
  - `state.rs:61`: 150 ms.
  - `spaces.rs:16,18`: 0.6 y 300 ms.
  - `chrome.rs:438`: 12.0.
  - `session.rs:1376`: el ancho del borde.
  - `cursor.rs:218`: tamaño 24.
  - `state.rs:292`: repetición de teclado 200/25, sin opción.
  - Los valores por defecto de `config.rs`.
- **Shell:**
  - `dialogs.rs`: unas 20 métricas.
  - `launcher.rs`: alfas, radios, gaps, 250 ms, rebote 0.15 y 16 ms.
  - `dock.rs:46` y `lock.rs:233`: `/4`.
  - `menu.rs:249-275` y `clock.rs:8-11` (P5-15); el token `MENUBAR_FONT_SIZE` existe.
- **Otros:**
  - `scene.rs:248`: `CORNER_INSET`.
  - `wallpaper/image.rs:416-417,506`.
  - `portal/gtk.rs:149-150`.
- `scripts/check-design-tokens.sh` solo revisa el compositor y la shell. Hay que ampliarlo a scene, wallpaper y portal.

### 5.3 Wrappers y abstracciones de un solo uso (regla 3) [V]

- `spawn.rs:32-41`:
  - `spawn_shell` envuelve `Command::spawn`;
  - `spawn()` repite lo que hace `command()`;
  - su parámetro `config` siempre es `Config::default()`.
- `appmenu.rs:26-55`: `AddressStore` es un `HashMap` 1:1.
- `capture.rs:87-135`: `screenshot_png` → `render_png` → `render_and_read`.
- `State.minimized` duplica `WindowData.hidden` y obliga a pasar `&HashSet` en unas 20 llamadas.
- Wallpaper: `LoadedImage` y `load_snapshot`.

### 5.4 Comentarios

Los comentarios cumplen la regla 6 casi en todo el código: en todos los crates
solo hay 5 líneas `//` y ningún `TODO`.

- Excepciones en `input.rs:2092,2207`: explican qué hace el código.
- Logs en español en `handlers.rs:906,923`, que además están duplicados.
- Lo que falta son los `// SAFETY:` de §1.2.

## 6. Reestructuración propuesta

Solo mover, salvo donde se indique. Cada paso es un commit y debe pasar
`./dev.sh check`.

### 6.1 `session.rs` (3348 líneas; unas 2090 sin tests)

1. `winit.rs`: el backend winit, unas 750 líneas.
2. `render.rs`: `build_output_elements`, layers, reveal, transiciones de espacio, `append_solid` y `physical_*` (unas 500). Los efectos laterales (`popup_fade::start_requested`, `dismiss_popups`, `reset_dead_cursor`) salen a un paso previo, porque hoy se ejecutan 3 o 4 veces por frame.
3. `effects.rs` (P1-11): marco de ventana, `frame_order` y `border_element` (`session.rs:1027-1392`).
4. `capture.rs` (P1-12): overlays, PNG y un único `process_pending_captures` compartido por los dos backends.
5. `cursor.rs` (P1-13): `reset_dead_cursor` y los elementos de cursor.
6. `frame_clock.rs` (nuevo): presentación, frame callbacks, release de buffers, un único timer y la máquina `RedrawState` de §2.2.
7. Montaje común del loop, genérico sobre el trait `IdleHost` que ya existe.
8. `test_support.rs`: un solo arnés en lugar de los cuatro (L3: `session.rs:2094`, `input.rs:2846`, `handlers.rs:1215`, `gtk.rs:546`).

### 6.2 `input.rs` (3180), `window.rs` (1872) y `handlers.rs` (1420)

- **`input.rs`:**
  - `input/script.rs`: la entrada sintética `MALAYSIA_TEST_*`, unas 620 líneas, detrás de una feature.
  - `shortcuts.rs`: atajos y despacho; aquí se arregla PX-13 una sola vez.
  - `input/pointer.rs`: motion, button, axis, constraints y cursor de borde.
  - `focus.rs`: el foco, con `set_seat_focus`.
  - `wm.rs`: maximizar, el botón verde, move y resize.
  - `shake.rs`: el detector de sacudida.
- **`window.rs`:**
  - `placement.rs`: la geometría pura. Mejor en `malaysia-scene`.
  - `configure.rs`, `hit.rs` y `surface.rs`.
  - `pending_decorations` pasa a `decoration.rs`.
- **`handlers.rs`:** un módulo por protocolo:
  - `blockers.rs`, `capture.rs` y `selection.rs` (que arregla PX-09);
  - `activation.rs` y `xdg_shell.rs`;
  - `decoration.rs`.
- **Frontera con `malaysia-scene`:** que `Scene` sea la dueña del orden y del mapa id → entrada, y que `Windows` guarde `HashMap<WindowId, WindowData>` más `ObjectId → WindowId`. Así desaparecen las búsquedas O(n²) de §2.3.
- **`chrome.rs`:** sacar el rasterizador CPU a `chrome/raster.rs` (`chrome.rs:567-830`) y comprobar si `vow-appearance::traffic` ya ofrece las primitivas de los semáforos (ADR 0047).

### 6.3 Shell: `menubar/` (unas 6100 líneas de código y 9100 con tests)

1. Un solo `src/spawn.rs` en la shell: entorno saneado, `pre_exec` con `SAFETY`, `wait()` y scope de systemd. Cierra P5-09, P5-13, los zombis y el cgroup. Los iconos van a `src/icons.rs` con caché.
2. Un modelo y un worker para las dos fuentes de menú: `enum Source { DbusMenu, Gtk }`, sin trait; `cache.rs` con P5-19 corregido; y `RawItem` del kit.
3. Partir `BarApp::render`, que hoy es una función de unas 620 líneas (`bar.rs:201-822`):
   - `memory.rs` y `titles.rs`;
   - los comandos D-Bus salen a un efecto;
   - los pollers se cambian por `Toplevels` e inotify.
4. Menús locales en `menubar/local/`, con un único `enum Action`. Hoy hay tres despachadores de strings.
5. A barrier-kit:
   - el id estable en `ToplevelInfo` (P5-10);
   - los iconos como `Symbol`;
   - el parser de aceleradores;
   - `PasswordField` sin realocar.
6. `dialogs.rs` se parte en `power_dialog.rs`, `force_quit.rs` y `about/mod.rs`.

Se esperan unas −800 líneas y ningún fichero por encima de unas 600.

### 6.4 Configuración y crates

- Un módulo común de dominios (`load_section` y los tipos de `menubar`, `window` y `dock`) para el compositor y la shell. Puede ser un crate pequeño `malaysia-domain`.
- `wallpaper.domain` va a `vow-appearance`, junto a `wallpaper.state`.
- `gtk_bridge` entra en `AppearanceConfig` (G-01).
- Borrar `WallpaperConfig` del compositor.
- `malaysia-scene`: reducirlo, no fusionarlo. Dejar solo los tipos lógicos, absorber la geometría pura del compositor y corregir la descripción.
- `grade-graphics`: tiene un único consumidor y unas 90 líneas de GLSL. Se puede plegar en `malaysia-compositor/src/shaders/` con una enmienda a los ADR 0024 y 0048. Si sigue aparte, que exporte también los nombres de los uniformes.

## 7. Ficheros que borrar o arreglar

| Fichero | Motivo | Acción |
|---|---|---|
| `scripts/check-tengen-generated.sh`, `tools/sync-tengen.sh` | Apuntan a `crates/vow-design/tengen`, que ya está en barrierkit | Borrar |
| `scripts/medir-redimensionado.sh` | Duplica `resize-measure.sh` y usa un ejemplo `window_csd` que no existe | Borrar |
| `scripts/resize-measure.sh` | Lee un log `frame=…delta=` que ya no se emite | Borrar o convertir en un comando de `dev.sh` |
| `scripts/capture-validate.sh` | Duplica `verify-captures.py` | Borrar |
| `scripts/test.sh` | Usa el binario `malaysia-compositor-shell` y el componente `wallpaper`, que ya no existen | Borrar |
| `scripts/demo.sh`, `scripts/lab.sh` | Wrappers de `git pull && ./dev.sh …` | Borrar |
| `scripts/malaysia-drm.sh` | Usa `bin/malaysia`, que no existe, y hace `systemctl --user link --force` contra la regla de entorno | Borrar o pasar a `dev.sh` |
| `tools/sctk-probe/src/bin/{decor_live,popup_live,popup_frame_probe,toplevel_frame_probe}.rs` | Nadie los usa (1098 líneas) | Borrar o conectarlos a `dev.sh` |
| `crates/malaysia-compositor/examples/appmenu_registrar_stub.rs` | Nadie lo usa | Borrar o documentarlo |
| `docs/assets/marca/malaysia-icon.png` (1,1 MB), `malaysia-icon.svg`, `malaysia-banner.svg` | Nadie los referencia; solo se usa `malaysia-banner.jpg` | Dejar el SVG como fuente y borrar o comprimir el PNG |
| `dev.sh:757-765,920-930` | Ramas `result/bin` inalcanzables tras `require_devshell` | Borrar |
| `flake.nix:67-68` | `outputHashes` de barrierkit `610f852`, que ya no es el rev fijado | Actualizar a `112632b` o borrar |
| `flake.nix:164` | El alias `packages."malaysia-shell"` no tiene uso | Borrar |
| `flake.nix:255` | `amdgpu_top` no tiene uso | Borrar |
| `flake.nix:154,281` | `VK_DRIVER_FILES` fijo a `radeon_icd`: sin Vulkan fuera de AMD | Usar todos los ICD de Mesa |
| `Cargo.toml:114-117` | El perfil `dev-fast` no tiene uso | Borrar |
| `deny.toml` | Nada lo ejecuta y le falta `[sources]` para los git deps | Conectarlo a `check` o borrarlo |
| `.gitignore:8-12` | Patrones `neptune-*` obsoletos; faltan `/result-*` y `.direnv/` | Arreglar |
| `share/systemd/user/malaysia-session.target` | Duplica la definición de `nix/malaysia.nix:83-89` | Dejar una sola fuente |

Se conservan, porque se usan [V]: `protocols/gtk-shell.xml`, `nix/patches/*`, el resto de
`share/` y los goldens de `crates/malaysia-shell/tests/golden/`.

## 8. Qué no está implementado

### 8.1 Protocolos (compositor)

**Presentes:**

- Base: wl_compositor, subcompositor, shm, linux-dmabuf v5 y drm-syncobj.
- Salidas y ventanas: xdg-output, xdg-shell, xdg-decoration, kde-server-decoration y layer-shell.
- Presentación y escala: viewporter, fractional-scale y presentation-time (con el bug de §2).
- Entrada: xdg-activation, cursor-shape, pointer-constraints, relative-pointer, text-input-v3, input-method-v2, virtual-keyboard y virtual-pointer.
- Captura: ext-image-capture-source y ext-image-copy-capture.
- Selección: data-device (DnD sin icono) y primary-selection.
- Sesión: ext-session-lock (con el bypass de §1), idle-inhibit e idle-notify.
- Gestión de ventanas: foreign-toplevel-list y wlr-foreign-toplevel-management.
- Propios o de terceros: gtk_shell1, kde-appmenu e infinity con focus-notice.
- X11: xwayland-satellite.

**Faltan, con módulo en Smithay `023b78b1`:**

- Entrada:
  - tablet-v2: el handler existe, pero no hay global y los eventos se descartan en `input.rs:635`;
  - pointer-gestures;
  - keyboard-shortcuts-inhibit;
  - pointer-warp.
- Seguridad: security-context.
- Ventanas y diálogos:
  - xdg-dialog-v1: encaja con el `modal` de gtk, que hoy se descarta;
  - xdg-toplevel-icon y xdg-toplevel-tag;
  - xdg-foreign;
  - xdg-system-bell.
- Buffers y temporización: single-pixel-buffer, alpha-modifier, content-type, fifo y commit-timing.
- Hardware: drm-lease.
- Portapapeles: ext-data-control y wlr-data-control. Sin ellos no hay gestores de portapapeles.
- Efectos: ext-background-effect.
- X11: xwayland-keyboard-grab.

**Faltan y no tienen módulo en Smithay** (niri los implementa): wlr-gamma-control
(luz nocturna), wlr-output-management (kanshi, wdisplays), wlr-screencopy (grim),
ext-workspace (útil para los espacios del ADR 0051) y tearing-control.

### 8.2 Backend y render

- DRM:
  - una sola salida (`find_panel`, `udev.rs:795-826`);
  - sin hotplug ni cambio de modo;
  - sin VRR ni gamma;
  - sin multi-GPU.
- Sin direct scanout, plano de cursor ni overlays.
- Sin blur ni vibrancy, aunque la política 3 de `AGENTS.md` lo exige.
- Screencast por PipeWire: no está en el compositor ni en el portal.
- Cursores Xcursor sin animación (`cursor.rs:223-260`) [S].

### 8.3 Shell y escritorio

- Faltan servicios del sistema:
  - el demonio `org.freedesktop.Notifications`;
  - la bandeja StatusNotifierItem;
  - el agente polkit;
  - keyring/Secret Service y AT-SPI.
- Faltan indicadores y controles:
  - indicadores de volumen, red, batería y brillo;
  - OSD y teclas multimedia (`XF86Audio*` y `MonBrightness*`: grep 0);
  - el centro de control.
- Lock: falta bloquear al suspender. La pantalla no tiene reloj, usuario ni avatar, fondo, aviso de Bloq Mayús ni cambio de usuario.
- Dock:
  - muestra iniciales en vez de iconos;
  - no tiene apps ancladas, lanzamiento ni menú contextual;
  - no anima el minimizado.
- Menubar:
  - faltan F10/Alt, el desbordamiento de títulos y el HUD;
  - hay 62 entradas siempre desactivadas que imitan macOS (48 en `appmenu.rs` y 14 en `focused.rs`).
- Launcher: solo busca apps.
- Interfaz de capturas `Shift+Super+5`: temporizador, miniatura y opciones.

### 8.4 Portal, ajustes y configuración

- Portal:
  - solo `Settings` (`share/xdg-desktop-portal/portals/malaysia.portal:3`);
  - faltan `Screenshot` y `ScreenCast` (12a);
  - con el respaldo GTK no funcionan `Inhibit`, `Background`, `GlobalShortcuts` ni `RemoteDesktop` [S].
- Ajustes:
  - solo existe el panel Apariencia, y no expone `contrast`, `reduced_motion`, `reduce_transparency` ni `sidebar_style`;
  - faltan los paneles Fondo, Pantallas, Ventanas, Dock, Menubar, Capturas, Bloqueo y Accesibilidad.
- Configuración:
  - el comando `domain` y el D-Bus `org.malaysia.Compositor1` (ADR 0021): no hay código;
  - `wallpaper.scale` distinto de `fill` y `shuffle`;
  - `remove_desktop` en `malaysia-scene/src/spaces.rs`.

### 8.5 Pendientes del roadmap

Por bloque:

- 5: reposicionado reactivo de popups.
- 6: drag & drop.
- 8b: hotplug, varias salidas y escala por monitor.
- 8c: planos y scanout.
- 9: escala por salida.
- 10: desbloqueo con credenciales reales.
- 11:
  - presentation en DRM y captura con salida rotada;
  - la app de `Shift+Super+5`;
  - aceptar xwayland-satellite.
- 12a/12b: portal, notificaciones, polkit, keyring y AT-SPI.
- 13: captura de ventana con marco.
- 14:
  - aceptar 14b-2, los mnemónicos y el scroll;
  - pantalla completa desde el menú Ventana;
  - aceptar en DRM 14a, 14b-1 y 14c;
  - aceptar 14f-3.
- 16: 16a, 16b (HEIC) y 16c (fondos dinámicos).
- 17: de 17a a 17h están hechos pero sin aceptar en sesión.
- NixOS: polkit, keyring/AT-SPI y una VM.

Pendientes que hoy viven fuera del roadmap y deben entrar en él:

- el CLI `domain` y el D-Bus;
- el blur;
- las opciones «pendiente» y «datos» de `opciones.md`;
- la supervisión y el overview del ADR 0023.

## 9. Documentación y repositorio

- **Regla bilingüe:**
  - Hay 0 `.en.md` de 73 documentos. La regla entró en `24e26e3`; después, `2eaf3f7`, `d3625c1` y `345ba40` añadieron documentos solo en español.
  - Ningún documento sigue la regla 2 (objetivo, lector, entradas y salidas, pasos).
  - A [V]
- **Licencias:**
  - `NOTICE:85-86` deja `bandura.jpg` como `<ORIGEN Y LICENCIA: te lo paso yo>`, aunque se incrusta en el binario y se instala.
  - Los 31 PNG de `share/wallpapers/solid-colors/` llevan los nombres de los «Solid Colors» de macOS y su origen no figura en `NOTICE` [S].
  - `NOTICE:9` sigue diciendo «Yuki».
  - A
- **ADR:**
  - El índice repite el número 0056: el de malaysia (xwayland-satellite) y el de barrierkit (Vulkan). Los 0056 y 0057 de barrierkit no tienen stub (`decisiones/README.md:63-64`).
  - Los títulos del índice usan nombres antiguos (yuki, Neptunia, CelestialKit).
  - Nueve stubs no tienen línea `Estado:`.
  - El ADR 0036 dice que 14f-2 está pendiente, pero ya se aceptó.
  - El ADR 0049:13 promete una comprobación de `wgpu` en la shell que `check-client-only-deps.sh` no hace.
  - M [V]
- **Docs que no cuadran con el código** [V]:
  - `docs/README.md:51` y `flake.nix:96` citan `nix/skia.nix` y `nix/skia-prebuild`, que no existen.
  - `README.md:40-41` y `AGENTS.md:15` cuentan `wallpaper` como componente de la shell.
  - `AGENTS.md:43` y `glossary.md:10` dicen que Infinity es «opcional hasta que el compositor lo implemente», pero ya está implementado.
  - `capturas.md:40` dice que los probes usan SHM; hoy también usan dmabuf.
  - `menubar.md:13` cita `barrier-kit::text`, que no existe, y `menubar.md:22` dice «Malaysia» donde el código usa «Escritorio».
  - `share/systemd/user/malaysia-shell@.service:2` dice que `wallpaper` es un componente.
- **Variables y atajos sin documentar** [V]:
  - Variables de entorno: `MALAYSIA_SCREENSHOT_DIR`, `MALAYSIA_ICON_THEME`, `MALAYSIA_TERMINAL`, `MALAYSIA_LOG_LEVEL`, `MALAYSIA_RESIZE_CONFIGURE_TIMEOUT_MS` y `MALAYSIA_THEME_FADE_HOLD_MS`.
  - Atajos: `Ctrl+Super+flechas`, `Ctrl+Shift+Super+flechas`, `Ctrl+Super+Return/C/R` y `Esc`/`Espacio` en modo captura.
- **Roadmap** [V]:
  - El título de «Ahora» dice 2026-10-31.
  - 14f-2 sigue en «Pendientes».
  - 14f-3 aparece dos veces, contradiciéndose (`:217-220` y `:342-347`).
  - El bloque 13 dice «Pendiente: modo foco real», aunque 17c está hecho.
  - `:34` habla de `vow-design`, que ya no está en este repo.
- **Rutas de runtime** [V]:
  - `dev.sh` usa `$XDG_RUNTIME_DIR/malaysia-dev-<hash>`.
  - Las capturas de debug van a `$XDG_RUNTIME_DIR/malaysia-compositor` (`capture.rs:434`).
  - `AGENTS.md` solo permite `$XDG_RUNTIME_DIR/malaysia`, así que hay que alinear la regla o las rutas.
- **CI:** no hay `.github/workflows`. Nada ejecuta `fmt`, `clippy`, los tests, `nix flake check` ni `cargo-deny`, y los `checks` del flake solo validan los tokens. M [V]
- **Nix:**
  - `flake.lock` tiene dos nodos crane: falta `barrierkit.inputs.crane.follows`.
  - `tools/sctk-probe` usa `edition = "2021"` y no hereda `rust-version`.
  - Se compilan tres versiones de sctk (0.19, 0.20 y 0.21).
- **Git:** de los 50 commits del clon, 49 siguen la convención. La excepción es `61e0446 chore: …`, que no tiene ámbito; su `sed` dejó sin sentido el texto de P5-07.

## 10. Estado de `deuda.md`

- **Ya resueltas; borrar sus filas** [V]:
  - P5-07: el texto quedó roto por el rename y el código ya compara bien;
  - P5-11: `StartLimit*` ya está en `[Unit]`;
  - P5-14: no queda ningún `fn rgba`.
- **Propuesta de cierre:** P1-14 no es deuda, porque la regla 6 prohíbe justo el comentario que pide.
- **Siguen abiertas, con la línea actual:**

| id | línea actual |
|---|---|
| P1-04 | `window.rs:941` |
| P1-05 | `session.rs:1801` |
| P1-08 | `state.rs:145-147`, ahora tapado con `#[allow(dead_code)]` |
| P1-09 | `window.rs:1106` |
| P1-10 | `session.rs:1356` |
| P1-11 | `session.rs:1027-1392` |
| P1-12 | `session.rs:1412-1522` |
| P1-13 | `session.rs:1739` |
| PX-09 | `handlers.rs:489-509` |
| PX-13 | `input.rs:432-468`, ampliado en §1.1 |
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
| L3 | cuatro arneses: `session.rs:2094`, `input.rs:2846`, `handlers.rs:1215`, `gtk.rs:546` |

- **Hace falta otro ajuste:** la cabecera cita `22070c6`, un commit que no está en el clon; hay que cambiarla por `345ba40`.

## 11. Plan por lotes

Cada lote es un commit `tipo(ámbito): …` que pasa `./dev.sh check`. Lo que
se acepte se registra en `deuda.md` o en `roadmap.md`.

1. **Seguridad**:
   - `fix(lock)`: rechazar un segundo lock; quitar el grab y el foco del puntero al bloquear; la guarda de `locked` en los atajos y en infinity/foreign.
   - `fix(shell)`: bloquear antes de suspender (inhibidor de logind).
   - `fix(shell)`: Forzar salida por id estable (primero barrierkit).
2. **Render**:
   - `perf(compositor)`: ids y `CommitCounter` estables; `damage_since` y `opaque_regions` delegados; `buffer_age` en winit.
   - `perf(compositor)`: máquina `RedrawState` en udev y winit; `schedule_repaint` con token; quitar el `info!` por frame.
   - `fix(compositor)`: presentation con CLOCK_MONOTONIC, el metadata del vblank y el refresco real; feedback por frame.
   - `fix(compositor)`: `session_destroyed` en captura y `update_constraints`.
3. **Procesos**:
   - `fix(shell)`: un único `spawn.rs`, con scope systemd y `wait()` (P5-09, P5-13, zombis y cgroup).
   - `fix(wallpaper)`: orden de destrucción y `Drop` de los objetos por output.
   - `perf(portal)`: ignorar los eventos `Defaults`.
4. **Código muerto** (§4):
   - `refactor(compositor)`: pasar a `mod` los módulos de `lib.rs` y borrar lo que salga;
   - decidir sobre la configuración sin consumidor;
   - borrar la geometría muerta de `malaysia-scene`;
   - borrar los tests que no protegen nada.
5. **Ficheros** (§7):
   - `chore`: borrar los scripts y probes huérfanos;
   - `build(nix)`: `VK_DRIVER_FILES`, `outputHashes`, alias, `amdgpu_top` y `crane.follows`.
6. **Duplicados y tokens** (§5): el color de fondo y los demás literales a `vow-design`; un solo bucle de captura y un solo montaje del loop; el easing; sRGB.
7. **Reestructuración** (§6): `session.rs`, luego `input.rs`, `handlers.rs` y `window.rs`, y por último `menubar/`. Solo mover; un commit por módulo.
8. **Docs**: traducir al inglés los 73 documentos; corregir los desfases de §9; actualizar `deuda.md` y `roadmap.md`; resolver las licencias.
9. **Funcionalidad**, por orden de impacto:
   - planos y scanout;
   - frame callbacks a superficies visibles;
   - Screenshot y ScreenCast (PipeWire);
   - data-control;
   - text-input/IME;
   - multisalida y hotplug;
   - blur xray;
   - notificaciones y polkit;
   - indicadores y OSD.
10. **CI**: un workflow con `nix develop -c ./dev.sh check` y `nix flake check`. Tracy opcional.

## Fuentes

- niri `a8562a1`: https://github.com/YaLTeR/niri. Ficheros:
  - `src/niri.rs`;
  - `src/backend/tty.rs`;
  - `src/render_helpers/{shader_element,clipped_surface,shadow,border,blur,xray,debug}.rs`;
  - `src/utils/{spawning,watcher,vblank_throttle}.rs`;
  - `src/animation/clock.rs`;
  - `src/layout/tests.rs`;
  - `docs/wiki/{Development:-Redraw-Loop,Development:-Animation-Timing,Window-Effects,Configuration:-Debug-Options}.md`.
- cosmic-comp: https://github.com/pop-os/cosmic-comp, ficheros `src/backend/render/wayland/blur_effect.rs` y `src/wayland/handlers/background_effect.rs`.
- Smithay `023b78b1`, ficheros:
  - `src/wayland/session_lock/{mod,lock}.rs`;
  - `src/backend/renderer/damage/mod.rs`;
  - `src/backend/renderer/element/solid.rs`;
  - `src/backend/renderer/gles/element.rs`;
  - `src/backend/drm/compositor/mod.rs`;
  - `src/wayland/image_copy_capture/mod.rs`.
- Protocolos: https://wayland.app/protocols/ext-session-lock-v1 y https://wayland.app/protocols/presentation-time.
- Portal Settings: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html

## 12. Seguimiento

Commits que aplican esta auditoría, en orden:

| commit | sección | qué |
|---|---|---|
| `718605f` | §1.1 | Un segundo lock no sustituye al vivo; el bloqueo corta grab y puntero; atajos, foreign e infinity respetan `locked`. |
| `61bb3ea` | §3 | `session_destroyed` libera las sesiones de captura. |
| `74e9df2` | §1.3, §2.3 | Orden de destrucción del wallpaper; el portal ignora los eventos `Defaults`. |
| `b4e2796` | §2.2 (4) | presentation-time con CLOCK_MONOTONIC y el refresco del modo. |
| `cfcf7c0` | §2.2 (1) | Un render por vblank y sin bucle en reposo. |
| `18d90a5` | §2.2 (2, 3) | Ids y commits estables para sombra, borde, titlebar y sólidos; `buffer_age` en winit. |
| `5a9f87c`, `4e27331`, `b181161` | §4 | Código muerto del compositor, de `malaysia-scene` y del wallpaper. |
| `1ef49da`, `b84eaff` | §7 | Scripts, probes y entradas del flake huérfanos. |
| `cb630a2`, `b2a90e0`, `03b2ca3`, `d0bba82` | §2.3, §3, §1.1 | PX-14, PX-09, `forget_window` y deltas de relative-pointer. |
| `5b56fb8` | §10 | `deuda.md` al día. |
| `41b0661` | §2.2 | Causa del lag tras `cfcf7c0`: todo commit encola el redibujado (abajo). |
| `78c6a2e` | §2.2 (6) | Scanout en el plano primario para pantalla completa, plano de cursor y feedback dmabuf por superficie. `SKIP_CURSOR_ONLY_UPDATES` no se activa: sin VRR congela el cursor (Smithay `023b78b1`, `drm/compositor/mod.rs:2379`; niri solo la usa con VRR). |
| `4b5c35c` | §2.2 (6) | Planos overlay tras `debug.overlay_planes`, apagados por defecto. Smithay `023b78b1` no fija el `pixel blend mode`; en i915 quedan apagados (S-02). |
| `31c4dd0` | §8.1 | `text-input-v3` e `input-method-v2`; popups del IME bajo el cursor de texto o encima si no caben. |
| siguiente | §8.1, §1.1 | `virtual-keyboard` y `virtual-pointer` (rediseño de Smithay `7454d68a`, `2bf8be86`, `9786228e`), solo para clientes de confianza (S-03). |

### Lag tras `cfcf7c0`

1. **Síntoma:** desde `cfcf7c0`, la sesión va con lag en DRM y en anidado.
2. **Causa:** `CompositorHandler::commit` pedía redibujo solo si `commit_requests_redraw` veía buffer, daño o frame callbacks. Se evaluaba después de `on_commit_buffer_handler`, que en Smithay `023b78b1` ya hizo `attrs.buffer.take()` y `attrs.damage.drain(..)` (`src/backend/renderer/utils/wayland.rs:155,200`). Un commit con buffer nuevo y sin frame callback no pedía frame. El render incondicional en cada vblank lo ocultaba; `cfcf7c0` lo destapó.
3. **Arreglo:** todo commit encola el redibujado, como niri (`src/handlers/compositor.rs`, `queue_redraw` en cada commit de una ventana mapeada). El damage tracker decide si hay algo que pintar; un frame sin daño envía frame callbacks y arma el vblank estimado.
4. **Comprobado frente a niri** (`src/niri.rs`: `RedrawState`, `queue_redraw`, `send_frame_callbacks`; `src/backend/tty.rs`: `on_estimated_vblank_timer`):
   - un frame sin daño envía frame callbacks (`finish_empty_frame`);
   - el cursor y los commits encolan el redibujado con un frame en vuelo (`Udev::queue_redraw`);
   - `RoundedTextureElement` daña todo el elemento cuando su commit cambia, así que no pierde daño;
   - `schedule_repaint` guarda su token y no cancela un repintado: solo lo aplaza al vblank estimado.
5. **Sin verificar aquí:** el contenedor no tiene `/dev/dri` ni display anfitrión, así que no se pudo bisecar con `./dev.sh demo`. Hay que confirmarlo en sesión.
