# Otto y los forks de HBlanqueto

Qué hay en los forks de Otto, qué se puede reutilizar en Yuki y qué no. La regla de
este documento es la del prompt: **manda el código**. Donde el código contradice la
premisa, se dice y se explica.

Verificado el 2026-10-19 contra estas revisiones:

| Repo | Rama / tag | Commit |
|---|---|---|
| `HBlanqueto/otto` | `neptunia` | `e6ea23f2` |
| `HBlanqueto/otto` | `main` (espejo) | `0ffa2ea5` |
| `HBlanqueto/smithay-otto` | `neptunia` | `96783e9b` |
| `HBlanqueto/smithay-otto` | `feat/vulkan-device` | `9341e4d1` |
| `HBlanqueto/layers` | `neptunia` | `6706e9bb` |
| `Smithay/smithay` | `v0.7.0` | `a166cf4c` |
| `Smithay/smithay` | `master` | `023b78b1` |
| `HBlanqueto/yukiwm` | `main` | `04c1bed` |

---

## 1. Qué es cada fork

### 1.1 `smithay-otto/neptunia`

No es un fork genérico de Smithay: es el Smithay de Otto con los parches que Otto
necesita para su backend udev, su render Vulkan y su XWayland. La rama `neptunia`
tiene **22 commits por delante de Smithay master** (`023b78b1`), pero su base no es
master actual: es `79bbed5e1` (2026-09-20), que está **35 commits por detrás** de
master. Es decir, el fork está anclado a un master de septiembre y encima lleva sus
parches. (`origin/master` del fork es todavía más viejo, `3044f07d9` de mayo; no es
la base de `neptunia`.)

La punta `96783e9b` es `9341e4d1` (punta de `feat/vulkan-device`) más dos commits de
build. `feat/vulkan-device` no contiene solo Vulkan: contiene también los parches de
DRM/planos, XWM y text-input. El nombre de la rama es engañoso.

Reparto por autor de los 22 commits:

- 18 de Riccardo Canalicchio.
- 2 de Victoria Brekenfeld (los dos primeros commits del render Vulkan).
- 2 de H. Blanqueto (documentación y `build.rs`).

Parches propios, agrupados:

#### Vulkan y multigpu

| Commit | Autor | Qué hace |
|---|---|---|
| `b7c49261` | Victoria Brekenfeld | Añade el módulo `renderer/vulkan`. |
| `53dae880` | Victoria Brekenfeld | Primer borrador del renderer Vulkan; mueve lógica de `backend/allocator/vulkan` a `backend/vulkan`. |
| `d834a51a` | Riccardo | Adapta el borrador al master del momento. |
| `62f2f28e` | Riccardo | Implementa `Texture` para `VulkanImage` en la capa de dispositivo. |
| `84593c2d` | Riccardo | El allocator compila sin la feature `renderer_vulkan`. |
| `160097d5` | Riccardo | Importa dmabufs con tiling por modificador y con los tipos de memoria del exportador. |
| `9341e4d1` | Riccardo | No declara exportable la memoria de un dmabuf importado; evita el «pantalla negra con cursor» de NVIDIA. |
| `30fe84d7` | Riccardo | `ImportEgl` para APIs sin import EGL (multigpu). |

#### DRM, planos y scanout

| Commit | Qué hace |
|---|---|
| `29cffb67` | Añade `UnderlyingStorage::Dmabuf` y `ExportBuffer::Dmabuf`: dmabufs renderizados por el compositor pueden ir directos a un plano KMS. |
| `734ebe91` | Pone el `pixel blend mode` en planos overlay para Intel i915 (Gen9+). |
| `90173eb6` | Sonda `test_overlay_planes` con `TEST_ONLY` y logs de visibilidad de planos. |
| `e9db4736` | `UnderlyingStorage::Dmabuf` posee el `Dmabuf`; restaura el veto de solape opaco. |
| `33b10703` | Resuelve el enum de `pixel blend mode` por plano en vez de hardcodear el valor (i915 lo ordena al revés). |
| `31a59c01` | Mueve las timelines de syncobj a `backend::drm::sync`. |

#### XWM

| Commit | Qué hace |
|---|---|
| `de4227f5` | Expone `X11Surface::set_input_focus` y `offer_focus`; añade `_NET_ACTIVE_WINDOW` y siembra `_NET_WM_STATE` antes del map. |
| `1fa58373` | Quita `update_net_wm_state`; upstream ya lee el estado pre-map. |

#### Text-input y teclado virtual

| Commit | Qué hace |
|---|---|
| `096a079c` | Retiene el `cursor_rectangle` de `zwp_text_input_v3` para el compositor, aunque no haya IME. |
| `f01bcf3c` | Envía `enter`, `leave` y `done` sin IME, como exige el protocolo. |
| `bd295660` | Usa `self.handle` en la implementación de `Dispatch2`. |
| `48369d05` | Entrega las teclas del teclado virtual por el `KeyboardTarget` con foco, no solo por `wl_surface`. |

#### Build

| Commit | Qué hace |
|---|---|
| `0a8efd1a` | `FORK.md` del fork. |
| `96783e9b` | `build.rs` declara sus entradas para no reejecutarse sin motivo. |

**Lo que importa a Yuki:** casi nada de esto. Los parches de Vulkan son para el
renderer Vulkan de Otto; los de DRM/planos añaden almacenamiento dmabuf, pixel blend
mode de i915 y sondas de overlay pensadas para Intel. Yuki renderiza con GLES sobre
una Vega (AMD). Lo único con valor general es el patrón de `text-input` sin IME y,
si algún día se usa XWayland, `set_input_focus`/`offer_focus`.

**Pin real:** `otto/main` fija `9341e4d1`. `otto/neptunia` fija `96783e9b` (la punta
del fork, con los dos commits de build). El prompt dice que Otto fija `9341e4d1`:
eso es cierto para `main`, no para la rama de trabajo del fork.

### 1.2 `layers/neptunia`

Es `laye-rs` 1.18.2 con tres commits encima:

- `bb63d06` — documentación del fork.
- `4c4b1f5` — la API en C y el `.so` solo se generan con la feature `capi`.
- `6706e9b` — `build.rs` solo se reejecuta cuando cambia lo que lee.

Detalle importante: la feature `capi` activa `dep:cbindgen` y compila el módulo
`src/api`, pero **no cambia `crate-type`**. `Cargo.toml` deja `crate-type = ["lib"]`;
el `.so` se consigue con `cargo rustc --lib --features capi --crate-type cdylib`,
que es lo que hace `examples/hello-c/build_library.sh`. El comentario de
`Cargo.toml` que dice que `capi` «añade el cdylib» es impreciso.

La versión sigue siendo `1.18.2`; no hay release nuevo en esta rama.

### 1.3 `otto/neptunia`

Es `main` más 24 commits propios. `main` es un espejo de `nongio/otto`, pero a fecha
de esta investigación está **14 commits por detrás** del upstream (`0ffa2ea5` contra
`c5d320ce`).

Los commits propios hacen cuatro cosas:

1. **Quitan apps del workspace.** Fuera: `otto-greeter`, `otto-agents`,
   `otto-canvas`, `otto-stash`, `otto-dictate`, `otto-preview`, `otto-msg`,
   `otto-input-overlay` y `otto-emoji`, con sus recursos, su arranque y sus atajos.
   De `otto-search` queda la librería; se quita su binario. El `Cargo.toml` lo
   confirma: ya no están en `[workspace].members`.
2. **Flake propio.** `flake.nix` declara `layers` y `smithay-otto` como inputs;
   `nix/skia.nix` compila Skia una vez en la derivación `skia-binaries` y el paquete
   la consume por `SKIA_BINARIES_URL=file://…`. También añade módulo NixOS y
   devShell.
3. **mold.** `flake.nix` pone `clang` como linker y
   `RUSTFLAGS = "-C link-arg=-fuse-ld=mold"`; `nix/package.nix` usa
   `stdenvAdapters.useMoldLinker`.
4. **Ajustes estilo macOS.** `ca3c3e81` rehace los paneles de `otto-settings`
   (apariencia, accesibilidad, energía, tiling, wallpaper) y recorta los paneles
   técnicos; los commits siguientes siguen limpiando la estructura.

El fork fija `smithay-otto` en `96783e9b` y `layers` en `6706e9bb`.

---

## 2. Smithay: 0.7.0 frente a master

`0.7.0` sigue siendo el último release (`a166cf4c`, 2025-06-24). En `master`,
`Cargo.toml` sigue diciendo `version = "0.7.0"` y el `CHANGELOG.md` tiene una
sección **Unreleased**; no hay release posterior. Master está **486 commits** por
delante de `v0.7.0`.

Módulos de `src/wayland/` que existen en master y no en 0.7.0:

| Módulo | Qué es | ¿Importa al roadmap? |
|---|---|---|
| `image_copy_capture` | `ext-image-copy-capture-v1` (captura de outputs/toplevels). | **Sí, Bloque 11.** Es el protocolo elegido en el roadmap. |
| `image_capture_source` | `ext-image-capture-source-v1` (fuentes de captura). | **Sí, Bloque 11.** Va con el anterior. |
| `background_effect` | `ext-background-effect-v1` (blur de fondo declarado por el cliente). | **Sí, ADR 0020.** Es el protocolo estándar para que una superficie pida blur; no sustituye al shader de blur del compositor. |
| `pointer_warp` | `pointer-warp-v1` (mover el puntero respecto a una superficie). | No está en el roadmap; útil solo si se quiere centrar el puntero. |
| `fixes` | `wl_fixes` (destruir objetos de registro). | No es una feature del roadmap; es infraestructura de protocolo. |
| `dispatch2` | `Dispatch2`/`GlobalDispatch2`, la API que sustituye a `Dispatch`. | **Sí, para migrar.** No es una feature visible; es el coste de seguir master. |
| `virtual_pointer` | `zwlr_virtual_pointer_v1`. | No está en el roadmap. Aparece además de los seis pedidos. |

Cambios de la sección **Unreleased** que rompen API y tocan xdg-toplevel, popup y
layer-surface:

- **`current_state()` ya no existe.** Se sustituye por `with_committed_state()`
  (y `with_cached_state()` para el estado cacheado). Afecta a `ToplevelSurface`,
  `PopupSurface`, `LayerSurface` y `LockSurface`.
- **Los estados cacheados cambian de forma.** `configured`, `current`,
  `current_serial` y `committed` desaparecen; `last_acked` pasa a ser
  `Option<ToplevelConfigure>`, `Option<PopupConfigure>` o
  `Option<LayerSurfaceConfigure>` según el rol. `LayerSurfaceCachedState` deja de
  implementar `Copy`.
- **`reset_initial_configure_sent()` desaparece** para toplevel y popup; en layer
  surface, `initial_configure_sent` ya no se pone a mano.
- **xdg-shell y layer-shell exigen el ack del configure antes de commitear un
  buffer.** Antes Smithay era más permisivo; ahora aplica el protocolo.
- **Los macros `delegate_*!` se sustituyen por `delegate_dispatch2!`.**

Impacto concreto en Yuki hoy:

- `crates/yuki/src/window.rs:150` lee `self.toplevel.current_state().states`.
- `crates/yuki/src/layers.rs:200` lee `LayerSurfaceCachedState` con `.current()` y
  lo devuelve por copia.
- El resto de la migración es mecánica, pero toca los tres roles a la vez.

**Conclusión.** Yuki no debe usar `smithay-otto`. Sus parches apuntan al renderer
Vulkan de Otto y a DRM/planos de Intel; Yuki es GLES sobre AMD. La salida de 0.7.0
es la **opción 2 del ADR 0010**: upstream Smithay anclado por `rev`, no el fork.
Eso da los módulos de captura y `background_effect` sin arrastrar los parches de
Otto, a cambio del coste ya previsto en el ADR: dependencia git en `Cargo.toml` y
`flake.nix`, y migrar la API de la sección Unreleased. No se redacta aquí el ADR
nuevo; queda como recomendación.

---

## 3. layers

### Licencia

`layers` **no tiene `LICENSE` ni `COPYING`**, y su `Cargo.toml` **no tiene campo
`license`**. No hay permiso escrito para copiar su código. Lo único que se toma es
la idea y las fórmulas; no se copia ni un archivo.

### Cuánto depende de Skia

En `src/` hay 43 archivos `.rs`; **27 mencionan Skia** y **16 no**. La dependencia
de `skia-safe`/`skia-bindings` 0.93 es incondicional en el `Cargo.toml`, así que
«no mencionar Skia» no significa «extraíble sin Skia»: los tipos del motor siguen
atados a Skia. Los 16 sin mención son:

- `engine/animation/` completo (`mod.rs`, `spring.rs`, `easing.rs`, `timing.rs`).
- `engine/command.rs`, `engine/task.rs`, `engine/storage.rs`,
  `engine/debug_server.rs`.
- `api/engine.rs`, `api/layer.rs`, `api/mod.rs`.
- `layers/error.rs`, `layers/layer/state.rs`, `layers/mod.rs`.
- `prelude.rs`, `view/build_layer_tree.rs`.

Los módulos que interesan a Yuki —**animación, comandos y tareas**— están en ese
grupo. El resto del motor (drawing, renderer, tipos, utils, nodos) menciona Skia y
no se porta.

### El spring

Lo aprovechable es el resorte con API de **duración + rebote**:

- `Spring::with_duration_and_bounce(duration, bounce)`.
- `Transition::spring(duration, bounce)`, que construye el `TimingFunction`
  correspondiente.

Semántica exacta (`src/engine/animation/spring.rs`):

- `duration` en segundos, `bounce` en `-1.0..=1.0`.
- `mass = 1`, `omega = 2π / duration`, `stiffness = omega²`.
- `bounce < 0`: sobreamortiguado, `damping = 2·mass·omega·(1 + |bounce|)`.
- `bounce == 0`: críticamente amortiguado, `damping = 2·mass·omega`.
- `bounce > 0`: subamortiguado, `damping = 2·mass·omega·(1 - bounce)`.
- Se considera terminado cuando posición y velocidad están a menos de `0.003`.

Eso es física, no API de Skia. Se puede reimplementar en
`celestial-kit::animation`, que hoy es solo un módulo vacío.

**Tokens.** El prompt habla de `design/v1.nix`. Ese archivo **no existe**: el ADR
0011 eliminó el generador y dejó los tokens como constantes Rust en
`yuki-design`. Por tanto, la duración y el rebote por defecto van como constantes
en `yuki-design` (por ejemplo, un `ANIMATION` con `spring_duration` y
`spring_bounce`), y `celestial-kit` los consume. No se crea `design/v1.nix`.

---

## 4. otto-kit frente a CelestialKit

`otto-kit` es **MIT** (hereda el `license = "MIT"` del workspace de Otto; el
`LICENSE` es «Copyright (c) 2024 Riccardo Canalicchio»). CelestialKit es el toolkit
de Yuki; hoy solo tiene `gl`, `text` y un `animation` vacío. No hay widgets,
superficies ni bucle de app.

Módulos **sin Skia ni el crate `layers`** que se pueden portar (líneas y
referencias contadas con el mismo criterio de la sección 5):

| Módulo | Archivos | Líneas | `skia` | `layers` | Qué se lleva |
|---|---|---:|---:|---:|---|
| `app_runner` (bucle) | `mod.rs` | 2386 | 0 | 0 | Conexión, registro de globals, `poll`, despacho de eventos, atajos de protocolo. No usa el event loop de sctk: usa `libc::poll` con estados de sctk. |
| `app_runner` (teclas) | `key_repeat.rs` | 189 | 0 | 0 | Repetición de teclas en cliente con `repeat_info` (delay + rate). |
| `app_runner` (escala) | `output_scale.rs` | 171 | 0 | 0 | Escala fraccional por output al arrancar, leída con `wl_output` + `xdg_output`; devuelve 120avos. |
| `app_runner` (contexto) | `context.rs` | 2381 | 12 | 1 | **No** se porta entero: mezcla contexto Wayland con Skia/layers. Del bucle solo sirve `mod.rs`. |
| `context_menu` (estado) | `context_menu.rs`, `state.rs` | 1648 / 619 | 0 | 0 | Items, selección, submenús, scroll, reemplazo de un menú vivo conservando la posición. |
| `menu_bar` (estado) | `mod.rs`, `state.rs` | 7 / 164 | 0 | 0 | Items con label/icono, activo, hover, foco. |
| `text_input` (edición) | `keymap.rs`, `state.rs` | 278 / 552 | 0 | 0 | Mapa de teclas (Ctrl/Alt/Cmd, palabras, línea, portapapeles) y estado de edición (caret, selección, password, ghost, scroll). |
| `text_input` (widget) | `text_input.rs` | 459 | 2 | 0 | API del campo; las 2 referencias son de dibujo, se adaptan a cosmic-text. |
| `titlebar` (controles) | `controls_state.rs` | 128 | 0 | 0 | Hover/press/release de los traffic lights; solo estado, sin dibujo. |
| `app_menu` | `app_menu.rs` | 816 | 0 | 0 | `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`; dirección del menú global por superficie. |
| `portal_settings` | `portal_settings.rs` | 277 | 0 | 0 | Lectura de `org.freedesktop.portal.Settings`. |
| `color_scheme` | `color_scheme.rs` | 119 | 0 | 0 | Suscripción a `org.freedesktop.appearance color-scheme`. |
| `desktop_appearance` | `desktop_appearance.rs` | 231 | 0 | 0 | Apariencia propia de Otto (botones, etc.) por el portal. |
| `desktop_entry` | `desktop_entry.rs` | 303 | 0 | 0 | Parseo y caché de `.desktop`. |
| `icon_theme` | `icon_theme.rs` | 214 | 0 | 0 | Tema de iconos, rutas y fallback. |
| `clipboard` | `clipboard.rs` | 372 | 0 | 0 | `wl_data_device` y MIME types; usa `AppContext` para la selección, hay que adaptarlo. |
| `accessibility` (árbol) | `mod.rs`, `adapter.rs`, `tree.rs` | 32 / 143 / 332 | 0 / 0 / 1 | 0 | Árbol declarado con `accesskit`; la única mención a Skia es un comentario. `widgets.rs` (953 líneas) usa `SkRect` y **no** se porta. |

**Qué no se porta.** Todo lo que dibuja: las vistas y renderers de `components/*`
(`context_menu/renderer.rs`, `menu_bar/renderer.rs`, `text_input/renderer.rs`,
`titlebar/titlebar.rs`, `titlebar/decoration.rs`, `window_controls.rs`, etc.),
`rendering/context.rs`, `rendering/layers_renderer.rs`, `frosted.rs`,
`frosting.rs`, `backdrop.rs`, `lottie.rs`, `icons.rs` y `wallpaper/*`.
`backdrop.rs` no menciona Skia, pero elige el mecanismo de blur de Otto y no encaja
en el modelo de Yuki; se descarta igual.

**Choque de `panic`.** `otto-kit` usa `catch_unwind` en 3 sitios: dos en
`icons.rs` (fallback de tema) y uno en `portal_settings.rs`. Yuki compila en
release con `panic = "abort"`, así que `catch_unwind` no atrapa nada: el proceso
muere. Al portar hay que quitar esos `catch_unwind` y devolver `Result` o un
fallback que no dependa de unwinding.

---

## 5. Compositor de Otto frente a los Bloques 8–11

Criterio de conteo: ocurrencias de `skia` (sin distinguir mayúsculas) y del crate
`layers` (`laye_rs`, `layers::`, `use layers`). No se cuentan las apariciones de
«layer» como superficie ni `layer_shell`. Es un indicador de acoplamiento, no una
prueba de portabilidad.

### Bloque 8 — Direct Session (DRM/KMS)

| Archivo | Líneas | `skia` | `layers` | Veredicto |
|---|---:|---:|---:|---|
| `src/udev/init.rs` | 918 | 1 | 1 | Referencia. La única dependencia es un `layers::skia::Rect` para daño. |
| `src/udev/device.rs` | 1086 | 1 | 0 | Referencia. La mención es el import de `SkiaDeviceRenderer`; la lógica de dispositivos es genérica. |
| `src/udev/input.rs` | 187 | 0 | 0 | Referencia directa: rutea libinput, VT, tapa. |
| `src/udev/input_config.rs` | 142 | 0 | 0 | Referencia directa: aplica `input.*` a libinput. |
| `src/udev/schedule.rs` | 293 | 0 | 0 | Referencia directa: cuándo se dibuja (VBlank, daño, animaciones, scanout). |
| `src/udev/render.rs` | 3181 | 12 | 8 | **No portable.** Renderer de Otto (Skia/Vulkan), planos y feedback. |
| `src/udev/planes.rs` | 424 | 2 | 5 | **No portable.** Los `layers::skia::Rect` y la asignación de planos son de Otto. |
| `src/udev/backdrop.rs` | 1301 | 65 | 67 | **No portable.** Blur/vibrancy sobre Skia. |

El multi-plane de Otto está probado **mayormente en Intel**; el README lo dice y no
hay evidencia de validación en AMD. El código tiene manejo de underrun de amdgpu,
pero eso no es una prueba de planos. Yuki usa Vega (AMD): no se copia.

### Bloque 9 — Escala fraccional

| Archivo | Líneas | `skia` | `layers` | Veredicto |
|---|---:|---:|---:|---|
| `src/state/fractional_scale_handler.rs` | 63 | 0 | 0 | Referencia directa. Elige output primario y fija `preferred_scale`. |
| `components/otto-kit/src/app_runner/output_scale.rs` | 171 | 0 | 0 | Referencia directa. Lee la escala al arrancar (120avos) antes de crear la superficie. |

Ambos son lógica de protocolo; se adaptan sin Skia.

### Bloque 10 — Lock con autenticación

| Archivo | Líneas | `skia` | `layers` | Veredicto |
|---|---:|---:|---:|---|
| `components/otto-auth-ui/src/pam.rs` | 571 | 0 | 0 | Portable como referencia: FFI propio a `libpam`, conversación en un hilo, respuestas por canal, `zeroize`. |
| `src/lock.rs` | 1455 | 0 | 2 | Referencia. Usa `layers::prelude::*` en todo el archivo, así que el dibujo del lock no se copia; sí el modelo de bloqueo. |

`lock.rs` bloquea por atajo, temporizador propio, tapa, botón de encendido,
`loginctl lock-session` y suspensión. El temporizador es de calloop y mide desde el
último input (`note_input_activity`); consulta `idle_inhibit` (protocolo
`idle-inhibit-unstable-v1`) para no contar como idle una superficie que lo pide.
**No usa `ext-idle-notify-v1`.** El roadmap de Yuki sí lo prevé; el temporizador
de Otto es solo referencia.

### Bloque 11 — Protocolos de uso diario

| Archivo | Líneas | `skia` | `layers` | Veredicto |
|---|---:|---:|---:|---|
| `src/state/xdg_activation_handler.rs` | 71 | 0 | 0 | Portable. Token con serial fresco o gracia de 3 s tras una pulsación. |
| `src/state/mod.rs` (presentation y cursor-shape) | 3061 | 7 | 7 | Solo el patrón de inicialización. El archivo entero está acoplado a Skia/layers; no se copia. |
| `src/state/screencopy.rs` | 532 | 12 | 4 | **No portable.** `wlr-screencopy` sobre Skia; cliente dmabuf y `read_pixels` para shm. |
| `src/screenshare/pipewire_stream.rs` | 1742 | 0 | 0 | **Portable.** Pool de buffers, negociación de formato y fences; sin Skia ni layers. |
| `components/xdg-desktop-portal-otto/src/portal/interface.rs` | 934 | 0 | 0 | **Portable** como capa D-Bus/PipeWire; habla con `org.otto.ScreenCast`, que Yuki tendría que implementar. |
| `src/state/kde_appmenu.rs` | 143 | 0 | 0 | Extra portable: `org_kde_kwin_appmenu` para el menú global. |

Otto captura con `wlr-screencopy`, atado a su renderer Skia. Yuki eligió
`ext-image-copy-capture-v1` + `ext-image-capture-source-v1` en el roadmap
(implementados en el Bloque 11). De Otto no se copia la captura; sí la parte PipeWire: el pool y la
negociación de `screenshare/pipewire_stream.rs` y la capa de portal
(`interface.rs`, `stream.rs`, `otto_client/screencast.rs`), adaptando el D-Bus
privado de Otto a uno de Yuki.

---

## 6. Licencias

- **Otto**: MIT, «Copyright (c) 2024 Riccardo Canalicchio». `otto-kit` hereda MIT.
- **Smithay**: MIT, «Copyright (c) 2017 Victor Berger and Victoria Brekenfeld».
- **layers**: sin licencia. No se copia código; solo se reimplementan las fórmulas
  del spring.

Si se copia código de Otto, el `NOTICE` debe añadir el aviso de Otto junto al de
anvil:

```
Copyright (c) 2024 Riccardo Canalicchio
```

Yuki ya tiene `LICENSE` y `NOTICE` en la raíz; el `NOTICE` cubre Smithay/anvil. Si
se copia código de Otto, su copyright se añade a ese `NOTICE` en el mismo commit,
no en uno posterior.

---

## Resumen operativo

1. **No se usa `smithay-otto`.** Sus parches son Vulkan e Intel; Yuki es GLES/AMD.
   Para Bloque 11, opción 2 del ADR 0010: upstream master anclado por `rev`.
2. **De `layers` solo se toma el spring** (duración + rebote), reimplementado en
   `celestial-kit::animation` y con los valores por defecto en `yuki-design`.
3. **De `otto-kit` se portan estados y protocolos, no dibujo**: bucle, key repeat,
   escala, estados de menús/inputs/titlebar, DBusMenu, portal, desktop entries,
   iconos, clipboard y árbol de accesibilidad. Fuera todo lo que pinta con Skia.
4. **De Otto se leen como referencia** los archivos de udev/input/schedule,
   fractional scale, PAM y lock, xdg-activation y KDE appmenu. Render, planos,
   backdrop y screencopy no se portan.
5. **Antes de copiar una línea de Otto**: añadir el aviso de Otto al `NOTICE` ya
   existente en el mismo commit. Antes de copiar de `layers`: no hacerlo; no hay
   licencia.
