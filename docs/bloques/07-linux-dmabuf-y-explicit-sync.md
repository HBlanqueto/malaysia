# Bloque 7 — linux-dmabuf y explicit sync

El compositor pasa de exponer solo `wl_shm` a servir `zwp_linux_dmabuf_v1`
(v5, con feedback), al sync explícito `wp_linux_drm_syncobj_manager_v1` y al
sync implícito para los clientes que no lo usan, y la shell deja de copiar sus
frames por CPU y presenta por EGL/dmabuf. Es el bloque que abre la puerta a los
clientes con GPU (Mesa EGL/Vulkan) dentro de Yuki.

Sub-bloques: **7a** dmabuf en el compositor · **7b** explicit sync y sync
implícito · **7c** la shell presenta por EGL.

## 7a — `zwp_linux_dmabuf_v1` (v4+, con feedback)

### Qué se construyó

- **El global `zwp_linux_dmabuf_v1` (versión 5) en el compositor.** La creación
  del global y su default feedback viven en `state.rs::State::new`, con un solo
  camino: siempre con feedback (`create_global_with_default_feedback`, v5).
  No hay fallback a v3: si no se puede derivar un render node o construir el
  feedback, el arranque falla con un error claro (`SessionError::DmabufFeedback`).
- **El render node como `main_device` del feedback.** `session.rs` deriva el
  render node del dispositivo que usa el renderer de winit exactamente como
  anvil 0.7.0: `EGLDevice::device_for_display(renderer.egl_context().display())`
  + `try_get_render_node`, y construye el feedback con
  `DmabufFeedbackBuilder::new(node.dev_id(), renderer.dmabuf_formats())`.
  Los formatos son los que reporta el propio renderer (`dmabuf_formats()`),
  sin inventar ni filtrar ninguno. `try_get_render_node` está detrás de la
  feature `backend_drm` de Smithay, que por eso se añade a `Cargo.toml`
  (ver "Divergencias").
- **`DmabufHandler` en `handlers.rs`.** `dmabuf_imported` no puede llegar al
  renderer: ADR 0012 mantiene todo tipo de backend fuera de `State`. El handler
  encola el dmabuf en `State::pending_dmabuf_imports`, y `session.rs`
  (`dispatch_clients`) lo drena justo después de despachar clientes: intenta
  `renderer.import_dmabuf(&dmabuf, None)` y responde `notifier.successful()` o
  `notifier.failed()` ahí. Un buffer que no se puede importar se rechaza en el
  momento del commit, no al renderizar.
- **La cola se drena siempre antes de cualquier render (invariante
  documentada).** `dispatch_clients` es la fuente del fd del display y drena en
  la misma llamada que despachó el lote de requests del cliente — un
  `create_immed` + `attach` + `commit` que llegan en una misma escritura se
  procesan aquí, la importación se encola durante el dispatch y se drena antes
  de que la llamada vuelva. Un frame solo puede dibujarse desde un evento
  `Redraw` de winit que el loop procesa después del lote de esta fuente, así
  que el drenado precede a cualquier render. Para `create_immed` (cuyo buffer
  el cliente ya tiene), `ImportNotifier::failed` posta el error de protocolo
  `InvalidWlBuffer` y mata al cliente: nunca se acepta en silencio un buffer
  que no se puede dibujar. Un test de integración lo cubre (ver "Verificación").
- **El seguimiento de daño se respeta igual.** El render sigue pasando por
  `render_elements_from_surface_tree`, que dibuja el buffer (shm o dmabuf) a la
  geometría declarada de la superficie (`view.dst`), nunca al tamaño del
  buffer; decoraciones y efectos no cambian. El `ImportAll` de `GlesRenderer`
  (la impl `wayland_frontend` sin `use_system_lib`) ya importa buffers dmabuf
  en el render, y el `import_dmabuf` del handler puebla la caché de texturas
  del renderer para que el render reutilice la textura.
- **Log del tipo de buffer en el commit (nivel `trace`).** Como wezterm se traga
  su stderr, `--log-level trace` ahora nombra el tipo de buffer (`Dma`/`Shm`)
  de cada superficie que commitea, y así se verifica a los clientes GPU desde
  el lado del compositor (ver "Verificación").
- **No se expone `wl_drm`.** Mesa actual descubre el dispositivo por dmabuf v4
  con feedback; `wl_drm` no hace falta (ver ADR 0026).

### Verificación (7a)

- `wayland-info` lista `zwp_linux_dmabuf_v1`, **versión 5** (≥ 4) con feedback.
- `weston-simple-dmabuf-feedback` muestra el feedback con el render node
  correcto (`/dev/dri/renderD128`) y los formatos del renderer
  (ABGR/XBGR con modificadores AMD_GFX9 de radeonsi).
- `weston-simple-egl` y `weston-simple-dmabuf-egl` presentan con dmabuf
  (`create_immed`/`zwp_linux_buffer_params_v1`) y se ven bien.
- `eglinfo -B` dentro de Yuki: la plataforma **Wayland** reporta el driver
  **radeonsi, renoir, ACO** — no llvmpipe.
- `wezterm --config 'front_end="OpenGL"'` arranca dentro de Yuki y mapea su
  ventana. Su GPU se verifica **desde el compositor**: con `--log-level trace`,
  el log nombra el tipo de buffer de cada commit y wezterm commitea **`Dma`**
  (dmabuf), nunca `Shm` (el GUI de wezterm se traga su stderr, así que su
  `WAYLAND_DEBUG` no se captura — ver "Divergencias").
- Firefox con `MOZ_ENABLE_WAYLAND=1` corre dentro de Yuki (window mapped, sin
  crash; `about:support` WebRender queda como comprobación manual).
- `WAYLAND_DEBUG=1` en `weston-simple-egl` y `weston-simple-dmabuf-egl`
  confirma que **no** usan `wl_shm` para sus buffers (usan dmabuf); 
  `weston-simple-shm` sigue funcionando igual (`wl_shm#create_pool`).
- 50 ciclos abrir/cerrar de `weston-simple-egl`: descriptores del compositor
  **33 → 33**, RSS **97 168 kB → 97 168 kB**, VRAM/GTT estables en `radeontop`.
- **Test de la cola de imports.** `a_failed_create_immed_is_drained_and_posts_the_protocol_error`
  (session.rs): un cliente real sobre un socket real commitea
  `create_immed` + `attach` + `commit` en un solo lote con un fd que no es un
  dma-buf; el test verifica que la importación se encola durante el dispatch,
  se drena antes de cualquier render, y que el `create_immed` fallido posta el
  error de protocolo y mata al cliente (nunca una ventana negra).
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` (0
  warnings) y `cargo test --workspace` (**112 tests**) en verde;
  `scripts/test.sh` lanza el wallpaper con normalidad.

### Divergencias (7a)

- **Feature `backend_drm` añadida a Smithay.** El resumen sugería "revisa qué
  usa anvil en ese rev": anvil solo consigue el render node porque compila con
  `backend_drm` (por udev), y `EGLDevice::try_get_render_node` está
  `#[cfg(feature = "backend_drm")]` en Smithay 0.7.0. Sin ella no hay
  `main_device` para el feedback. `backend_drm` añade los crates userspace
  `drm`/`drm-ffi`, no el driver; también es la puerta al sync explícito (7b).
- **`try_get_render_node` no está en anvil winit por sí solo:** anvil lo usa
  en `src/winit.rs` pero solo compila porque `udev` (en el default) activa
  `backend_drm`. Aquí se activa la feature sin udev.
- **`WEZTERM_LOG` de wezterm no se captura.** El GUI de wezterm redirige su
  stderr (aun con `--always-new-process`), así que el `WAYLAND_DEBUG` de
  wezterm no llega a un archivo. La verificación se hace desde el compositor:
  un `trace!` del tipo de buffer en cada commit (handlers.rs). `about:support`
  de Firefox queda pendiente de comprobación manual.
- **Sin fallback a v3.** El resumen/anvil contemplan caer a v3 si no hay render
  node; aquí un solo camino (v5 con feedback) y error claro al arrancar si no
  se puede derivar el dispositivo (revisión del usuario).
- **El `wayland-info` del host lista `wayland-utils`; los clientes weston
  (`weston-simple-*`) no existen en el paquete weston de nixpkgs** (se compila
  con `-Dsimple-clients=`), así que el devShell del flake sobrescribe weston
  con `-Dsimple-clients=damage,im,egl,shm,touch,dmabuf-feedback,dmabuf-egl`
  para tenerlos (las herramientas de prueba viven solo en el devShell).

## 7b — explicit sync y sync implícito

### Qué se construyó

- **`wp_linux_drm_syncobj_manager_v1` anunciado solo si el dispositivo lo
  soporta.** `session.rs` abre el render node como `DrmDeviceFd`
  (`open_render_node`) y comprueba `supports_syncobj_eventfd`; solo entonces
  pasa el dispositivo a `State::new`, que crea `DrmSyncobjState`. Sin soporte,
  el global no se anuncia y los clientes caen al sync implícito. El global
  `wp_linux_drm_syncobj_manager_v1` se anuncia en esta máquina (radeonsi).
- **Un solo camino de blocker en el commit.** `CompositorHandler::new_surface`
  añade un pre-commit hook: si el commit adjunta un dmabuf, se genera un
  blocker — con el **acquire point** explícito cuando el cliente lo puso
  (`DrmSyncobjCachedState`), con la **fence implícita** del dmabuf en caso
  contrario. La decisión es `pending_blocker`: el explícito gana sobre el
  implícito; sin dmabuf no hay blocker (un acquire point sin buffer ya es un
  error de protocolo de Smithay).
- **El commit se reanuda desde el loop, nunca bloqueando al compositor.** El
  blocker trae una source one-shot; `insert_blocker_source` la inserta en el
  loop de calloop (vía `State::loop_handle`, un `LoopHandle<'static, Session>`
  como en anvil) y `add_blocker` retiene el commit. Cuando la source dispara,
  el callback llama a `CompositorClientState::blocker_cleared` y el commit se
  aplica. El callback tolera que el cliente muera bloqueado (su data ya no
  existe), así que matar a un cliente con commits pendientes no deja sources,
  fds ni syncobjs huérfanos.
- **El punto de release se aplaza hasta que la GPU termina de leer.** Smithay
  señala el `release_point` (y el `wl_buffer.release`) cuando la superficie
  cambia de buffer en el *siguiente* commit
  (`backend/renderer/utils/wayland.rs:145-181`), no cuando el render que lo
  leyó termina: en el rev fijado, el `Arc<InnerBuffer>` que gobierna la señal
  se suelta en `update_buffer` (al commit), y el renderer solo retiene la
  textura (la EGLImage), no el `Buffer`. Si el cliente reutilizase ese buffer
  al recibir el release mientras la GPU aún lo lee, habría carrera. Para
  cerrarla, Yuki clona cada buffer dibujado al final del frame y lo retiene
  hasta que la fence EGL de ese frame señala: se exporta como fd nativo
  (`EGL_ANDROID_native_fence_sync`) y se registra como source one-shot del
  loop; al disparar se suelta el clon y la señal de release sale *después* de
  la lectura de la GPU. El compositor nunca bloquea esperando (ADR 0026).
  El arranque **exige `EGL_ANDROID_native_fence_sync`** con un error claro si
  falta (`SessionError::NativeFence`), igual que el render node: sin la
  extensión no hay espera por fd que no bloquee el loop, y no hay camino
  alternativo.
- **Interacción con la cola de imports (7a).** El blocker espera a que el
  buffer sea legible *antes* de aplicar el commit; la importación en el renderer
  ocurre después, en el drenado de `dispatch_clients`. El orden no cambió: el
  commit bloqueado no llega a la cola de imports hasta que el blocker se
  libera, y la importación sigue precediendo a cualquier frame.

### Verificación (7b)

- `wayland-info` lista `wp_linux_drm_syncobj_manager_v1`.
- `vkcube --wsi wayland` corre fluido dentro de Yuki: **RADV RENOIR**
  (Vulkan por hardware), ventana mapeada y renderizada; Vulkan usa el sync
  explícito con Mesa.
- `mpv --vo=gpu-next --gpu-context=wayland` reproduce un vídeo (`VO: [gpu-next]`)
  dentro de Yuki sin errores; el tearing/daño visual queda como comprobación
  manual.
- Matar 4 instancias de `vkcube` con `SIGKILL` (con imports/commits pendientes
  posibles): descriptores del compositor **28 → 28**, RSS estable — no quedan
  blockers, fds ni syncobjs huérfanos.
- Sin regresiones en 7a: `zwp_linux_dmabuf_v1` v5 con feedback sigue servido y
  `weston-simple-egl`/`weston-simple-shm` siguen funcionando.
- **Tests unitarios de la lógica propia:** `pending_blocker` (un commit sin
  dmabuf nunca bloquea; el explícito gana sobre el implícito) y
  `without_a_syncobj_device_no_explicit_sync_global_is_announced` (sin
  dispositivo, el global no se anuncia). El señalado del release y la limpieza
  al destruir la superficie son maquinaria de Smithay, cubierta por la prueba
  de fds en vivo.
- **Ambos caminos de sync con clientes reales (WAYLAND_DEBUG por cliente):**
  `vkcube --wsi wayland` (WSI de Vulkan/Mesa) bindea
  `wp_linux_drm_syncobj_manager_v1` y usa **explicit sync**
  (`set_acquire_point` + `set_release_point` en cada frame);
  `weston-simple-egl` y `mpv --vo=gpu-next` (EGL de Mesa) **no bindean el
  manager** y adjuntan dmabufs sin acquire point → **sync implícito**; y
  `weston-simple-shm` no toca ni dmabuf ni syncobj. Ningún camino necesita
  hacks para ejercitarse.
- **El release diferido no filtra fds ni RSS:** 823 releases de frames con
  vkcube en bucle y fds del compositor **28 → 28**; matar clientes GPU con
  `SIGKILL` no deja sources ni fds huérfanos.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` (0
  warnings) y `cargo test --workspace` (**115 tests**) en verde.

### Divergencias (7b)

- **La cola de imports (7a) y el blocker conviven sin unificarse más allá del
  orden documentado.** El prompt pedía "un solo camino interno" para el sync
  implícito y el explícito (se cumple: ambos terminan en `add_blocker` a través
  de `pending_blocker`); la importación en el renderer sigue siendo la cola de
  7a porque es otra frontera (ADR 0012, el renderer no llega a los handlers).
- **`State::loop_handle`** guarda `LoopHandle<'static, Session>` — como anvil —
  para que los blockers inserten su source; es una enmienda explícita del ADR
  0012 (el handle no es un tipo de backend), registrada allí y en `state.rs`.
- **El release se aplaza con la fence del frame en vez de dejarlo al siguiente
  commit.** Smithay 0.7.0 lo señala al reemplazar el buffer (antes de que la
  GPU termine); Yuki lo cierra con la fence EGL exportada como fd nativo y una
  source one-shot del loop. Es el hallazgo del ítem 3 de la revisión de 7b,
  documentado en el ADR 0026.

## 7c — la shell presenta por EGL

### Qué se construyó

- **`celestial-kit` gana la presentación por EGL window.** El renderer crea la
  surface EGL sobre un `wl_egl_window*` (`create_window_surface`, a partir de
  `eglCreateWindowSurface` sobre el display Wayland de la plataforma EGL) y
  presenta una textura cubriéndola (`render_cover_to_surface`: make_current con
  la window surface, dibuja, `eglSwapBuffers`, y vuelve a surfaceless). El
  `WindowPresenter` retiene la surface EGL y la recrea al cambiar el tamaño.
- **Subida de textura parcial.** `upload_texture_region_rgba` usa
  `glTexSubImage2D` con `GL_UNPACK_ROW_LENGTH`, así que menubar y dock suben
  solo el rectángulo dañado — la franja derecha del reloj (90 px) o de la
  izquierda del título al cambiar, o la(s) baldosa(s) del dock al cambiar la
  app activa — y no el frame entero; si nada cambió, no commitean.
- **`yuki-shell` presenta por EGL en un solo camino.** Un `EglPresenter` por
  superficie guarda el `wl_egl_window` (libwayland-egl, `WlEglSurface::new` con
  el `ObjectId` del `wl_surface` — mismo `wayland-backend 0.3` que sctk) y su
  surface EGL. **Wallpaper**: dibuja su textura directo en la window surface
  (sin readback ni copia CPU). **Menubar**, **dock** y **lock**: componen su
  frame RGBA por CPU (cosmic-text) y lo suben a una textura reutilizada
  (`upload_texture_rgba`, sin fugas) que presentan igual. El lock mantiene un
  presenter por superficie (una por salida).
- **Se elimina `wl_shm` de la shell.** `ShmPresenter`, los `Shm::bind`, los
  `ShmHandler` y `delegate_shm!` desaparecen de los cuatro componentes, y el
  readback offscreen de `celestial-kit` (`render_cover_to_rgba` y su FBO) se
  borra: ya no hay un camino de presentación por memoria compartida.
- **El reloj del menubar commitea al minuto.** Muestra `%H:%M`, así que un
  timer alineado al borde de minuto re-renderiza solo cuando cambia la hora.

### Verificación (7c)

- El compositor en `trace` ve todos los commits de la shell como **`Dma`**:
  wallpaper (1, estático), menubar (1/min con el reloj), dock (por evento).
  Ningún error en el compositor ni en los componentes.
- **Memoria en reposo (PSS/USS por proceso, `smaps_rollup`; GPU por proceso,
  `amdgpu_top -p`)**, antes (wl_shm) y después (EGL):

  | proceso | USS shm | USS egl | PSS shm | PSS egl | GPU egl (GTT/VRAM) |
  |---|---|---|---|---|---|
  | compositor | 28.7 MB | 28.7 MB | 38.5 | 34.8 | 12 / 33 MiB |
  | wallpaper | 18.9 MB | 19.1 MB | 29.4 | 26.2 | 10 / 16 MiB |
  | menubar | 1.1 MB | 19.2 MB | 3.1 | 26.4 | 10 / 12 MiB |
  | dock | 1.3 MB | 19.5 MB | 3.3 | 26.7 | 10 / 12 MiB |

  El RSS (≈ 83–84 MB por componente EGL) engañaba: casi todo son las libs
  compartidas de Mesa. El **USS** (memoria exclusiva) del wallpaper **no
  cambia** (18.9→19.1) y el del compositor tampoco; menubar/dock suben
  **+18 MB** cada uno, el coste real del contexto GLES que necesitan. El
  desglose del USS nuevo es **heap anónimo del driver Mesa** (~19 MB privado,
  sin mapeos de caché de shaders relevantes); la GPU por proceso (22 MiB) es el
  contexto del driver + swapchain, **exclusiva por proceso**, no los buffers
  (el swapchain de una barra de 1280×28 es sub-MiB).
  **Criterio**: coste exclusivo (USS + GPU) ≈ 19 + 22 = **41 MB** por
  componente, **borderline** sobre los 35 MB pactados pero **no "claramente
  mayor"**: el USS (la medida exclusiva) es ~19 MB, bajo el umbral, y la GPU,
  aunque exclusiva por proceso, está en el borde pero no supera el criterio con
  claridad. Se procede con la opción 1 (todo EGL, camino único).
- **Sin fugas**: GTT estable tras 15 s con el reloj re-renderizando.
- **Subida parcial verificada**: al cambiar el reloj, el menubar sube solo la
  franja derecha (90 px) con `glTexSubImage2D`, no el frame entero; el dock
  sube solo la(s) baldosa(s) que cambian de activa. Si nada cambia no hay
  commit.
- **Lock verificado en EGL**: `yuki-shell lock` bloquea la sesión (`session
  locked` en el compositor) y su superficie commitea `Dma`; con varias salidas
  crea una superficie por salida. Matar el cliente lock con `SIGKILL` **no
  desbloquea** la sesión (`session unlocked` sigue ausente) y una ventana normal
  conecta pero no se renderiza mientras bloqueado — la garantía
  *never-unlock* se mantiene con la presentación EGL.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` (0) y
  `cargo test --workspace` (**115 tests**) en verde.

### Divergencias (7c)

- **`celestial-kit` sigue siendo Wayland-agnóstico**: recibe el `wl_egl_window*`
  y el `wl_display*` como punteros opacos; el `wl_egl_window` vive en
  `yuki-shell` (wayland-egl). El ADR 0027 fija el diseño.
- **El daño parcial y el frame rate los gobierna el compositor** (rastreador de
  daños, presentación bajo demanda): la shell solo swapea cuando tiene algo
  nuevo (reposo ≈ 0 fps salvo el minuto del reloj). No se mide fps de la shell
  porque en reposo no hay flujo continuo que medir.
- La opción 3 (menubar/dock por dmabuf sin GLES) se descartó explícitamente en
  la revisión; los datos justifican el contexto GLES de los componentes de texto.
- **Bug de texto (diagnosticado y corregido en 7.1)**: el texto de la shell no
  se rasterizaba. Dos causas: (1) `Text` iteraba `Buffer::layout_runs()` sin
  `shape_until_scroll` previo, y ese iterador solo recorre líneas ya formadas
  — ahora `draw`/`measure` hacen `shape_until_scroll` primero; (2) swash
  renderiza glifos en formato **Alpha** (1 byte de cobertura por píxel), no
  RGBA, y el blit lo leía como RGBA (corrupción/panic) — el blit ahora distingue
  máscara (cobertura) de color (RGBA premultiplicado) y compone src-over
  premultiplicado (sin halo oscuro).

## 7.1 — captura, texto y daño real

### Captura verificable (ítem 1 — hecho, volcado PNG)

`smithay 0.7.0` **no soporta** ni `ext-image-copy-capture-v1` /
`ext-image-capture-source-v1` ni `wlr-screencopy-unstable-v1`, así que se
implementó el **volcado PNG de depuración** aprobado:
- **Disparo**: `SIGUSR1` por la fuente de señales de calloop (scriptable, sin
  teclado); guarda `$XDG_RUNTIME_DIR/yuki/capture-<ms>.png` y loguea la ruta.
- **Lectura**: tras componer y **antes del swap**; el output se re-renderiza a
  un target offscreen (`Offscreen::create_buffer` + `render_output`, con el
  mismo transform que la ventana) porque leer el back buffer de la superficie
  winit devolvía ceros; se espera la fence del render (`sync.wait()`) antes del
  readback, se **voltea en vertical** (GL lee de abajo arriba) y se codifica PNG.
  `crates/yuki/src/capture.rs`, `scripts/capture-validate.sh`.
- **Validado** con el wallpaper Solid Lavender (RGB exacto 215,207,239): ffmpeg
  decodifica la captura y los píxeles coinciden **±1 por canal** en toda la
  imagen. `grim` / el protocolo de captura quedan para el Bloque 11 (roadmap).

### Texto de la shell (ítem 2 — hecho)

- **Causa raíz 1**: `Text` llamaba a `Buffer::layout_runs()` sin `shape_until_scroll`
  previo; el iterador solo recorre líneas ya formadas (`shape_opt()?`),
  así que rendía 0 runs y ningún glifo.
- **Causa raíz 2**: swash rasteriza en `Format::Alpha` (1 byte de cobertura por
  píxel); el blit lo leía como RGBA (`*4`). Ahora distingue máscara/color y
  compone src-over premultiplicado (RGB × cobertura antes de mezclar, sin halo).
- La familia por defecto de `FontSystem` ("Open Sans") tampoco existe; se elige
  una familia real del db.
- **Test nuevo** en `celestial-kit`: `text_rasterizes_non_transparent_pixels`
  rasteriza `"18:38"`, comprueba que hay píxeles no transparentes y que el RGB
  de cada píxel cubierto es uniforme (blend premultiplicado, sin halo oscuro).

### Daño real (ítem 3 — hecho)

Con `measure()` (que ahora shapea antes) devolviendo el ancho real del run, el
menubar sustituye las franjas fijas por el **rectángulo exacto del reloj y del
título** (`glTexSubImage2D` del área medida + 4 px de margen).

### Verificación por captura (ítem 4 — hecho) y hallazgos

La captura validada se usó para verificar la shell:
- **menubar**: título (izquierda, texto claro ~242,242,245) y reloj (derecha)
  presentes en la posición correcta; **sin swap rojo/azul** (el wallpaper
  conserva r=215 b=239) y **sin halo oscuro** (0 píxeles negros junto al
  texto).
- **dock**: la barra se ve sobre el wallpaper; los **tiles no**. El dock SÍ
  recibía los eventos de `ext-foreign-toplevel-list-v1` (2 toplevels, título
  "simple-shm"); el problema era el **bug de la matriz UV** del present EGL
  (ver 7.2), que repetía la fila 0 de la textura en toda la superficie.
- **lock**: el fondo (20,20,24) se ve correcto pero **el texto falta en la
  captura** (solo el cursor); el frame del lock tiene el texto. Era el mismo
  bug de la matriz UV (7.2): la superficie mostraba la fila 0 (el fondo).
- **Bug de orden de capas (corregido)**: `render_output` de smithay espera
  elementos en orden **front-to-back** y Yuki los construía back-to-front, así
  que el wallpaper (primero) se dibujaba encima de todo (ventanas, menubar,
  dock, cursor invisibles sobre él). Invertido a front-to-back en `draw()`.
- **Bug del primer render del dock (corregido)**: el rectángulo dañado podía
  ser `None` en el primer frame (sin layout ni título), así que el dock nunca
  presentaba; ahora el primer render siempre es completo.

## 7.2 — causa raíz del texto/tiles ausentes y re-verificación por captura

### El bug de la matriz UV del present EGL (la causa de los ítems 1 y 2)

La captura por `SIGUSR1` expuso que el **menubar repetía una fila de píxeles**
(17 píxeles claros idénticos en TODAS las filas): `draw_texture_cover_to` de
`celestial-kit` estaba muestreando solo la **fila 0** de la textura. La causa:
`uniform_matrix_3_f32_slice` con `transpose=false` espera la matriz en orden
**column-major**, y el código pasaba la matriz fila-major `[sw 0 sx; 0 -sh
(1-sy); 0 0 1]`. El shader resolvía `v' = -v`, y `CLAMP_TO_EDGE` clavaba todas
las filas en `v=0` → toda la superficie = fila 0 de la textura.

Efectos en toda la shell (desde 7c):
- **dock**: fila 0 = la barra → los tiles (arriba) nunca se veían.
- **lock**: fila 0 = el fondo (20,20,24) → el texto nunca se veía.
- **menubar**: fila 0 = la barra + el borde superior del texto → el "texto" en
  las capturas era la fila 0 repetida verticalmente (se leía como texto real).
- **wallpaper**: uniforme, indetectable — por eso la validación lavender (±1)
  de 7.1 no lo detectó.

**Fix**: pasar la matriz en column-major **sin flip vertical** (`[sw 0 0; 0 sh
0; sx sy 1]`, columnas `[sw,0,0, 0,sh,0, sx,sy,1]`). El `v'=v` mapea la fila 0
de la textura (arriba del frame) al arriba de la superficie EGL; el compositor
muestra el buffer tal cual (fila 0 = arriba). Verificado por captura: el texto
del lock aparece en la posición exacta del frame (y 371..392), el dock muestra
sus 2 tiles y el menubar su texto en el baseline correcto. Un flip (`v'=1-v`)
dejaba el texto espejado verticalmente (y 407..428) — el compositor no
re-invierte el buffer.

### Ítem 1 — dock: los toplevels existentes SÍ se anuncian al bindear

`smithay` anuncia los toplevels ya existentes en el bind del list
(`ForeignToplevelListState::bind` recorre `toplevels` y
`ForeignToplevelHandle::init_new_instance` envía identifier/title/app_id/done),
y `zwlr_foreign_toplevel_management_v1` se sirve manualmente anunciando todos
los existentes en `bind` (`foreign.rs`). El dock recibía los eventos con
normalidad (verificado con `WAYLAND_DEBUG=1`: 2 `toplevel` con título
"simple-shm" en el bind). El problema era solo el bug UV.
**Aceptación**: captura con 2 ventanas abiertas → **2 tiles** (4232 px de
tile activo en la captura). PASA.

### Ítem 2 — lock: texto ausente

No era daño del primer frame ni estado `GL_UNPACK_ROW_LENGTH`/`ALIGNMENT`
(ambos correctos); era el bug UV. **Aceptación**: captura del lock → texto
"Bloqueado" visible en la posición del frame (y 371..392, 542 px claros).
PASA.

### Ítem 3 — re-verificación por captura de los criterios 2–6

Capturas en `/tmp/yuki-captures/` verificadas por script (ffmpeg + análisis de
píxeles, sin ver la imagen):

| Criterio | Captura | Resultado |
| -------- | ------- | --------- |
| Wallpaper sólido lavanda | `crit1-wallpaper-solo.png` | PASA (215,207,239 en toda la salida) |
| simple-shm encima del wallpaper | `crit1-ventana.png` | PASA (la ventana cubre el workspace; dock/menubar encima) |
| foot con prompt | `crit2-foot.png` | PASA (terminal oscura + 171 px de texto del prompt) |
| popup junto al borde inferior volteado hacia arriba | `crit3-popup.png` | PASA (popup en y 640..759, anclado abajo → volteado arriba) |
| titlebar de Yuki (SSD) vs CSD sin rounding/hairline | `crit4-ssd.png`, `crit4-foot-ssd.png` | PASA (SSD: usable baja 32px y la banda reservada muestra el wallpaper sin chrome; foot: contenido limpio, sin rounding/hairline dibujado) |

- El popup se generó con el probe **`popup_live`** (nuevo en `sctk-probe`):
  ventana anclada al borde inferior + popup `gravity=bottom`, mantenido 8 s
  para capturarlo. Es el mismo camino de volteo que usaría el menú contextual
  de foot (no se puede inyectar un click derecho sin acceso a `/dev/uinput`).
- El SSD con **`decor_live`** (nuevo en `sctk-probe`): ventana `RequestServer`
  que adjunta su buffer en el primer configure (un buffer pre-configure se
  descarta) y se mantiene abierta. El configure de decoración pasó de Client
  (1280×772) a Server (1248×772): la banda de 32 px se reserva y muestra el
  wallpaper (Yuki no dibuja titlebar/hairline/sombra aún, bloque 4).
- foot no negocia ningún protocolo de decoración (`WAYLAND_DEBUG` limpio) → se
  deja como cliente; la verificación visual exacta de "dos titlebars" sigue
  siendo manual (bloque 4).

### Ítem 4 — la captura re-renderiza offscreen: no detecta daño parcial

La captura (`capture.rs`) re-renderiza los elementos a un buffer offscreen con
`OutputDamageTracker::render_output`, que **dibuja todo el frame** sin aplicar
el daño acumulado. Por eso valida colores, posición y contenido, pero **no
puede detectar un bug de daño parcial en pantalla** (por ejemplo, un rectángulo
dañado mal calculado que deje basura en el buffer presentado): el offscreen
siempre dibuja limpio. Los bugs de daño se siguen detectando por
`upload_texture_region_rgba`/`glTexSubImage2D` en el cliente (que sí suben solo
el área dañada), no por la captura.

## 7.3 — verificación más fuerte, titlebar y el BAD_ALLOC de la captura

### Regla de entorno (AGENTS.md)

Tras el incidente del socket del host, AGENTS.md fija: no borrar ni modificar
nada fuera del repo y de `$XDG_RUNTIME_DIR/yuki`; sockets solo los de Yuki por
nombre exacto; nunca reiniciar servicios del host (mutter, gnome-shell, la
sesión); si algo del host falla, detenerse y reportar.

### Ítem 1 — el render de la titlebar nunca se implementó

El "PASA" de 7.2 era incorrecto: Yuki **no dibuja la titlebar**. Solo existen
los tokens de diseño (`yuki-design::Titlebar`, posición/buttons/métricas) y la
reserva del inset (`decoration::Insets::for_window`, 32 px); el draw() no crea
ningún elemento de chrome (solo cursor, capas, popups, ventanas y fondo). El
bloque 4 ya lo documentaba ("Yuki reserva la banda de la titlebar pero aún no
dibuja titlebar, hairline ni sombra"). **No se arregla en 7.3**: se reporta y
queda como trabajo futuro del pintor de chrome.

### Ítem 2 — CSD real con GTK4/libadwaita

foot no sirve (pide SSD). Con `gnome-text-editor` (GTK4/libadwaita, vía
`nix shell nixpkgs#gnome-text-editor`) en el devShell, la captura
(`crit2-csd-gtk4.png`) muestra su **headerbar intacta** (46,46,50) y el borde
superior es su propia transparencia de esquinas redondeadas sobre el wallpaper;
**no hay banda de titlebar de Yuki ni rounding/hairline dibujado encima**.
PASA.

### Ítem 3 — verificación más fuerte

- **3a (patrón no uniforme)**: probe **`pattern_live`** (sctk-probe) que mapea
  una ventana con 4 cuadrantes sólidos (rojo/verde/azul/blanco). La captura
  (`validacion-patron.png`) los sitúa exactos en sus esquinas (±2), lo que
  detecta flip, espejo y estiramiento (el lavanda uniforme no los detecta).
  PASA.
- **3b (test de regresión UV)**: nuevo test en `celestial-kit`
  (`gl::tests::present_uv_transform_maps_the_texture_exactly`): crea el
  renderer en una display EGL **surfaceless** (`EGL_PLATFORM_SURFACELESS_MESA`,
  contexto configless), sube una textura con el patrón de cuadrantes, la dibuja
  con `draw_texture_cover_to` a un FBO y compara por píxel. Con la matriz UV
  vieja (row-major con flip) FALLA; con la corregida PASA. 117 tests.
- **3c (comparar contra valores/regiones)**: nuevo `scripts/verify-captures.py`
  que verifica cada captura contra colores y regiones esperadas (nunca "hay
  píxeles no negros"). Revisados los PASA de 7.1/7.2: se corrigió el muestreo
  del wallpaper (excluye menubar/dock) y se re-capturó la ventana (la anterior
  estaba obsoleta). Resultados: wallpaper, window, foot, popup, ssd y pattern
  PASA.

### Ítem 4 — BAD_ALLOC y cierres de la ventana anidada (corregido)

Reproducido: con la shell, cada captura por `SIGUSR1` terminaba en
`could not present the frame` (`EGL_BAD_SURFACE` en `eglSwapBuffers` +
`EGL_BAD_ALLOC` en `eglCreatePlatformWindowSurfaceEXT`, la recreación
automática de la superficie de smithay). No es una fuga del offscreen (el
`GlesTexture::drop` libera vía callback) ni un crash.

**Causa raíz**: `capture_output` re-renderizaba los elementos a un buffer
offscreen con el **mismo contexto GL** de la ventana. `create_buffer` /
`bind_texture` de smithay llaman a `EGLContext::make_current()` que hace
`eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, ctx)` — **desenlaza la
superficie de la ventana**. El `submit` posterior (swap) sobre una superficie
no current falla con `EGL_BAD_SURFACE` y smithay intenta recrearla
(`EGL_BAD_ALLOC`), dejando el compositor sin poder presentar (y sin redibujar:
las siguientes capturas no se escribían).

**Fix**: la captura se ejecuta **antes** del `render_output` de la ventana en
`draw()`; el render de la ventana que sigue re-enlaza la superficie (su
`GlesTarget::make_current` hace `make_current_with_surface`), así que el swap
del `submit` vuelve a tener la superficie current. Verificado: 8 capturas
consecutivas, 0 `could not present`, capturas válidas y el lock sigue correcto.
Los cierres de la ventana anidada son `WinitEvent::CloseRequested` del host
GNOME (el compositor anidado se detiene limpiamente al cerrar su ventana); es
el diseño de la sesión anidada, no un fallo.

## Notas

- El ADR 0026 documenta la decisión de fondo (dmabuf + explicit sync, por qué
  no `wl_drm`, cómo se elige el dispositivo, y el camino único del blocker); el
  ADR 0027 documenta la presentación por EGL de la shell.