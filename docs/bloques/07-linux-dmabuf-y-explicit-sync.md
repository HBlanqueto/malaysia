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
- **Bug pre-existente descubierto: el texto de la shell no se rasteriza.** En
  este entorno, `cosmic-text 0.19` carga 283 fuentes (incluso con la familia
  corregida a una real, p. ej. Inter) pero `layout_runs()` no produce glifos,
  así que menubar, dock y lock dibujan el fondo sin el texto. No lo causa 7c
  (el mismo `Text` se usaba con `wl_shm`); queda como seguimiento antes de
  pulir la shell. Por eso el rectángulo dañado usa franjas fijas (90 px reloj /
  250 px título) en vez de medir el texto.

## Notas

- El ADR 0026 documenta la decisión de fondo (dmabuf + explicit sync, por qué
  no `wl_drm`, cómo se elige el dispositivo, y el camino único del blocker); el
  ADR 0027 documenta la presentación por EGL de la shell.