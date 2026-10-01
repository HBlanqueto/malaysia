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
- **Log del tipo de buffer en el commit (nivel `debug`).** Como wezterm se traga
  su stderr, `--log-level debug` ahora nombra el tipo de buffer (`Dma`/`Shm`)
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
  ventana. Su GPU se verifica **desde el compositor**: con `--log-level debug`,
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
  un `debug!` del tipo de buffer en cada commit (handlers.rs). `about:support`
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

(pendiente)

## 7c — la shell presenta por EGL

(pendiente)

## Notas

- El ADR 0026 documenta la decisión de fondo (dmabuf + explicit sync, por qué
  no `wl_drm`, cómo se elige el dispositivo, y el camino único del blocker); el
  ADR 0027 documenta la presentación por EGL de la shell.