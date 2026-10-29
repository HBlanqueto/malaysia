# Auditoría Ponytail P1 — Núcleo del compositor

- Commit: `718a7e4`
- Alcance: bloques 0–2, 4–5 (ADR 0001–0004, 0010, 0012–0016).
- Archivos leídos completos:
  - `crates/malaysia-compositor/src/state.rs` (1362 líneas)
  - `crates/malaysia-compositor/src/session.rs` (3440 líneas)
  - `crates/malaysia-compositor/src/handlers.rs` (1234 líneas)
  - `crates/malaysia-compositor/src/window.rs` (1740 líneas)
  - `crates/malaysia-compositor/src/xdg.rs` (98 líneas)
  - `crates/malaysia-compositor/src/popup.rs` (471 líneas)
  - `crates/malaysia-compositor/src/layers.rs` (557 líneas)
  - `crates/malaysia-compositor/src/spawn.rs` (241 líneas)
  - `crates/malaysia-compositor/src/cli.rs` (313 líneas)
  - `crates/malaysia-compositor/src/main.rs` (72 líneas)
  - `crates/malaysia-compositor/src/lib.rs` (45 líneas)
  - `crates/malaysia-compositor/src/logging.rs` (101 líneas)
  - `crates/malaysia-scene/src/geometry.rs` (570 líneas)
  - `crates/malaysia-scene/src/lib.rs` (15 líneas)
  - `crates/malaysia-scene/src/scene.rs` (897 líneas)
- No leído fuera de alcance salvo los grepes y tramos necesarios para verificar cruces con `input.rs`, `idle.rs`, `foreign.rs`, `infinity.rs` y `udev.rs`.

## Hallazgos

### P1-01 · El redondeo lógico→físico no respeta ADR 0004 (floor/ceil)

- Tipo: bug / deriva
- Dónde:
  - `crates/malaysia-compositor/src/session.rs:284`
  - `crates/malaysia-compositor/src/session.rs:1222-1230`
  - `crates/malaysia-compositor/src/state.rs:972`
  - `crates/malaysia-compositor/src/window.rs:980-981`
  - `crates/malaysia-compositor/src/popup.rs:440-443`
- Evidencia:
  ```rust
  // session.rs:284
  fn physical_point(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
      point.to_f64().to_physical(scale).to_i32_round()
  }
  // session.rs:1225-1230
  Point::from(((rect.loc.x * scale).round() as i32, (rect.loc.y * scale).round() as i32))
  Size::from(((rect.size.w * scale).round() as i32, (rect.size.h * scale).round() as i32))
  // state.rs:972
  self.output_size.to_f64().to_logical(self.scale.fractional_scale()).to_i32_round()
  ```
- Por qué importa: ADR 0004 decidió `floor` para posiciones y `ceil` para tamaños para evitar costuras de 1 píxel entre superficies adyacentes bajo escala fraccionaria. El núcleo sigue usando `to_i32_round()` y `.round()` en todos los caminos al renderizador. `malaysia_scene::geometry` sí lo implementa bien, pero el compositor no lo usa para convertir a píxeles físicos, así que dos ventanas/popups/layers que compartan una arista lógica fraccionaria pueden dejar un hueco o solaparse mal.
- Arreglo mínimo: crear helpers `to_physical_point`/`to_physical_rect` que apliquen `floor` a la esquina y `ceil` al tamaño, y usarlos en `session.rs`, `state.rs::logical_output_size`, `window.rs::usable_rect` y `popup.rs::smithay_rect`. Reutilizar `malaysia_scene::geometry` donde el tipo de dato lo permita.
- Tamaño: M
- Líneas que se borrarían: ~20 (los helpers actuales se reemplazan, no se añaden abstracciones nuevas)
- Confianza: alta

### P1-02 · `LayerFocus` no limpia `previous_window` al ceder a otra capa exclusiva

- Tipo: bug
- Dónde: `crates/malaysia-compositor/src/layers.rs:347-361`
- Evidencia:
  ```rust
  if let Some(next) = self.exclusive.last().cloned() {
      self.focused = Some(next.clone());
      return LayerUnfocus::Layer(next);
  }
  LayerUnfocus::Window(self.previous_window.take())
  ```
- Por qué importa: si hay dos capas exclusivas, la primera guarda `previous_window` y al desmaparse cede el foco a la segunda **sin borrar** `previous_window`. Cuando la segunda desmaparse, devuelve `previous_window` que puede ser una ventana completamente distinta a la que tenía foco antes de la segunda capa. El test actual no cubre la cascada de dos exclusivas.
- Arreglo mínimo: en la rama `LayerUnfocus::Layer(next)`, hacer `self.previous_window = None` (la siguiente capa exclusiva debe recordar su propio foco previo o no restaurar nada).
- Tamaño: S
- Líneas que se borrarían: 0
- Confianza: alta

### P1-03 · `client_disconnected` no destruye las layer surfaces del cliente muerto

- Tipo: bug / riesgo
- Dónde: `crates/malaysia-compositor/src/state.rs:944-956`
- Evidencia:
  ```rust
  for surface in layers {
      self.layer_surface_destroyed(&surface);
  }
  ```
- Por qué importa: para ventanas y popups se llama a `forget_window`/`popup_destroyed`, que las quitan de `Windows`/`Popups` y de la escena. Para layers solo se llama a `layer_surface_destroyed` (gestión de foco en `input.rs`), pero no a `self.layers.destroy(&surface)`. Las entradas muertas permanecen en `Layers::mapped` hasta que `Layers::commit` las poda (l. 82-84). Mientras tanto, `exclusive_top()`/`exclusive_bottom()` siguen sumando sus zonas y el workspace es más pequeño de lo debido.
- Arreglo mínimo: añadir `self.layers.destroy(&surface);` dentro del bucle de layers en `client_disconnected`.
- Tamaño: S
- Líneas que se borrarían: 0
- Confianza: alta

### P1-04 · `Windows::set_maximized` y `set_fullscreen` no cambian la geometría y están muertos

- Tipo: riesgo / sobra
- Dónde:
  - `crates/malaysia-compositor/src/window.rs:952-961`
  - `crates/malaysia-compositor/src/window.rs:924-938`
- Evidencia:
  ```rust
  pub fn set_maximized(&mut self, id: WindowId, maximized: bool) -> bool {
      if self.windows[index].maximized == maximized { return false; }
      self.windows[index].maximized = maximized;
      self.send_configure(index);
      true
  }
  ```
- Por qué importa: estas funciones son públicas pero nadie las llama (`grep` no encuentra `windows.set_maximized(` ni `windows.set_fullscreen(`). Todos los caminos usan `set_maximized_with_outer` / `set_fullscreen_with_outer`. Si alguien las usa en el futuro, cambiarán el estado enviado al cliente sin mover la geometría, dejando la ventana en el mismo sitio pero con bandera maximizada/fullscreen. Es una trampa.
- Arreglo mínimo: borrar `set_maximized` y `set_fullscreen`; forzar a usar siempre las variantes `with_outer`. Si se quieren como helpers internos, que reciban el rectángulo y deleguen.
- Tamaño: S
- Líneas que se borrarían: ~35
- Confianza: alta

### P1-05 · `schedule_frame_release` libera buffers antes de tiempo si falla el fence fd

- Tipo: riesgo
- Dónde: `crates/malaysia-compositor/src/session.rs:1742-1764`
- Evidencia:
  ```rust
  let Some(fd) = release.sync.export() else { ... return; };
  let Ok(fd) = fd.try_clone() else {
      tracing::error!("could not duplicate the frame fence fd");
      return;
  };
  if let Err(error) = loop_handle.insert(source, move |_, _, _| { ... }) {
      tracing::error!(...);
  }
  ```
- Por qué importa: en ambos errores (`try_clone` o `insert_source`) la función retorna inmediatamente. `release` (que contiene los `Buffer` clonados) se destruye en el stack, lo que dispara `wl_buffer.release`/el punto de release de explicit-sync **antes** de que el GPU haya terminado. En condiciones normales no pasa, pero si el kernel se niega a duplicar el fd o el loop rechaza la fuente, un cliente podría reescribir un buffer que el compositor todavía lee.
- Arreglo mínimo: documentar el atajo o, mejor, en el camino de error mantener vivo `PendingFrameRelease` hasta el siguiente frame (por ejemplo, almacenándolo en un `Vec` del `Session` que se limpie cuando el fence ya no sea necesario).
- Tamaño: M
- Líneas que se borrarían: 0
- Confianza: media

### P1-06 · `xdg::parse_desktop_dir` no expande `$HOME` cuando no lleva barra

- Tipo: bug
- Dónde: `crates/malaysia-compositor/src/xdg.rs:57-61`
- Evidencia:
  ```rust
  let expanded = match value.strip_prefix("$HOME/") {
      Some(rest) => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(rest))
                                      .unwrap_or_else(|| PathBuf::from(value)),
      None => PathBuf::from(value),
  };
  ```
- Por qué importa: la especificación de `user-dirs.dirs` permite `XDG_DESKTOP_DIR="$HOME"`. Con ese valor el código crea `PathBuf::from("$HOME")` literal en lugar de expandirlo. Aunque el caso común es `$HOME/...`, el fallback es incorrecto.
- Arreglo mínimo: comprobar primero si `value == "$HOME"` y expanderlo; luego el prefijo `$HOME/`.
- Tamaño: S
- Líneas que se borrarían: 0
- Confianza: alta

### P1-07 · `session::draw` duplica el bucle de frame callbacks

- Tipo: sobra
- Dónde:
  - `crates/malaysia-compositor/src/session.rs:1605-1628`
  - `crates/malaysia-compositor/src/session.rs:1932-1956`
- Evidencia: el bloque de l. 1605-1628 recorre lock/windows/layers/popups exactamente igual que `send_frame_callbacks_for_state` (l. 1932-1956). `finish_empty_frame` ya usa el helper.
- Por qué importa: cualquier cambio en la política de frame callbacks hay que hacerlo en dos sitios; es el tipo de duplicación que ya se resolvió para `send_presentation_feedbacks_for_state`.
- Arreglo mínimo: reemplazar el bucle inline por `send_frame_callbacks_for_state(&session.state, now);`.
- Tamaño: S
- Líneas que se borrarían: ~25
- Confianza: alta

### P1-08 · `State::infinity_global` está muerto

- Tipo: sobra
- Dónde:
  - `crates/malaysia-compositor/src/state.rs:307-308`
  - `crates/malaysia-compositor/src/state.rs:588`
- Evidencia:
  ```rust
  #[allow(dead_code)]
  pub(crate) infinity_global: GlobalId,
  ...
  infinity_global: crate::infinity::InfinityGlobalData::create_global(display_handle),
  ```
- Por qué importa: el `GlobalId` no se lee en ningún sitio. El global sigue registrado por el efecto colateral de `create_global`; almacenar el id no aporta nada. El `#[allow(dead_code)]` es una señal.
- Arreglo mínimo: eliminar el campo y la asignación; conservar la llamada a `create_global`.
- Tamaño: S
- Líneas que se borrarían: ~3
- Confianza: alta

### P1-09 · `Windows::id_of_surface` duplica `Windows::id_of`

- Tipo: sobra
- Dónde:
  - `crates/malaysia-compositor/src/window.rs:646-651`
  - `crates/malaysia-compositor/src/window.rs:1161-1166`
  - `crates/malaysia-compositor/src/window.rs:1172`
- Evidencia: ambas funciones hacen `windows.iter().find(|data| data.toplevel.wl_surface() == surface).map(|data| data.id)`. `id_of` es pública; `id_of_surface` es privada y solo se usa en `dialog_parent`.
- Por qué importa: duplicación innecesaria; si cambia la forma de buscar, hay dos sitios.
- Arreglo mínimo: borrar `id_of_surface` y usar `self.id_of(&parent)` en `dialog_parent`.
- Tamaño: S
- Líneas que se borrarían: ~7
- Confianza: alta

### P1-10 · `session::border_element` devuelve `Option` pero nunca `None`

- Tipo: abstracción
- Dónde:
  - `crates/malaysia-compositor/src/session.rs:1235-1278`
  - `crates/malaysia-compositor/src/session.rs:1179-1195`
- Evidencia: `border_element` termina siempre en `Some(PixelShaderElement::new(...))`. Los llamados hacen `if let Some(element) = border_element(...) { ... }`.
- Por qué importa: el `Option` obliga a los llamados a manejar un caso imposible y añade ruido. El único motivo histórico era un posible fallo, pero ahora no hay ramas `None`.
- Arreglo mínimo: cambiar el tipo de retorno a `PixelShaderElement` y ajustar los dos llamados.
- Tamaño: S
- Líneas que se borrarían: ~4
- Confianza: alta

### P1-11 · Lógica de renderizado de ventanas en `session.rs` que pertenece a `chrome.rs`/`effects.rs`

- Tipo: responsabilidad / sobra
- Dónde: `crates/malaysia-compositor/src/session.rs:987-1208`
- Evidencia: `WindowFrameData`, `ssd_frame_rect`, `frame_radius`, `frame_has_border`, `append_window_frame`, `frame_order`, `border_element` viven en `session.rs`, a pesar de que `chrome.rs` y `effects.rs` ya existen para el marco, sombra, bordes y recorte redondeado.
- Por qué importa: `session.rs` tiene ~3,4 k líneas; este bloque es el más grande de lógica que no tiene que ver con el bucle de eventos ni con el backend winit. Dificulta mantener la separación backend/estado que justifica ADR 0012.
- Arreglo mínimo: mover `append_window_frame` y sus helpers privados a `chrome.rs` o `effects.rs`, dejando en `session.rs` solo la llamada desde `build_output_elements`.
- Tamaño: L
- Líneas que se borrarían: 0 (se mueven)
- Confianza: media

### P1-12 · Helpers de screenshot en `session.rs` que pertenecen a `capture.rs`

- Tipo: responsabilidad / sobra
- Dónde: `crates/malaysia-compositor/src/session.rs:1302-1424`
- Evidencia: `save_screenshot`, `capture_window_png`, `capture_output_png`, `screenshot_elements`, `log_screenshot` y `ScreenshotOutcome` están en `session.rs`, pero todo el código de captura/offscreen vive en `capture.rs`.
- Por qué importa: es compartido entre winit y DRM, así que no hay razón para que esté en el módulo del backend anidado.
- Arreglo mínimo: mover las funciones y el enum a `capture.rs`; `session.rs` solo llama a `capture::save_screenshot(...)`.
- Tamaño: M
- Líneas que se borrarían: 0 (se mueven)
- Confianza: alta

### P1-13 · Helpers de cursor en `session.rs` que pertenecen a `cursor.rs`

- Tipo: responsabilidad / sobra
- Dónde: `crates/malaysia-compositor/src/session.rs:1812-1908`
- Evidencia: `append_cursor`, `cursor_origin`, `cursor_surface_hotspot` y `reset_dead_cursor` están en `session.rs`. `cursor.rs` ya existe y carga los cursores temáticos.
- Por qué importa: igual que P1-11/P1-12, `session.rs` acumula responsabilidades que tienen su propio módulo.
- Arreglo mínimo: mover los tres helpers a `cursor.rs` y dejar `append_cursor(&mut elements, ...)` en `build_output_elements`.
- Tamaño: M
- Líneas que se borrarían: 0 (se mueven)
- Confianza: alta

### P1-14 · Handlers vacíos de Smithay sin explicación

- Tipo: atajo
- Dónde:
  - `crates/malaysia-compositor/src/handlers.rs:342-343`
  - `crates/malaysia-compositor/src/handlers.rs:647`
- Evidencia:
  ```rust
  impl ImageCaptureSourceHandler for State {
      fn source_destroyed(&mut self, _source: ImageCaptureSource) {}
  }
  impl OutputHandler for State {}
  ```
- Por qué importa: el auditado pidió revisar precisamente handlers que solo delegan o no hacen nada. Estos dos están vacíos por diseño (el estado no rastrea las fuentes por separado y `OutputManagerState` maneja los outputs), pero sin un comentario parecen olvidados.
- Arreglo mínimo: añadir `// Intentionally empty: ...` en cada uno.
- Tamaño: S
- Líneas que se borrarían: 0
- Confianza: alta

### P1-15 · `Scene::popup_subtree` es cuadrático

- Tipo: atajo / rendimiento
- Dónde: `crates/malaysia-scene/src/scene.rs:352-369`
- Evidencia:
  ```rust
  loop {
      let before = subtree.len();
      for entry in &self.popups {
          if let PopupParent::Popup(parent) = entry.parent
              && subtree.contains(&parent)
              && !subtree.contains(&entry.id)
          { subtree.push(entry.id); }
      }
      if subtree.len() == before { break; }
  }
  ```
- Por qué importa: en una cascada profunda de submenús hace `O(n²)` por los `.contains`. El número de popups es pequeño en la práctica, pero es una ineficiencia evitable.
- Arreglo mínimo: usar un `HashSet<PopupId>` para los ya visitados y recorrer la lista una vez, o construir el árbol desde `id` hacia abajo.
- Tamaño: S
- Líneas que se borrarían: ~5
- Confianza: media

### P1-16 · `Layers::commit` poda superficies muertas en cada commit

- Tipo: atajo
- Dónde: `crates/malaysia-compositor/src/layers.rs:82-84`
- Evidencia:
  ```rust
  self.mapped.retain(|e| e.surface.wl_surface().alive());
  self.unmapped.retain(|s| s.wl_surface().alive());
  ```
- Por qué importa: es defensivo y funciona, pero oculta el hecho de que la limpieza normal debería pasar por `WlrLayerShellHandler::layer_destroyed` o `State::client_disconnected` (ver P1-03). Si la poda fuera innecesaria, P1-03 sería más evidente.
- Arreglo mínimo: dejar la poda como `debug_assert`/prueba de sanity, pero asegurar la limpieza explícita en los destructores y en `client_disconnected`.
- Tamaño: S
- Líneas que se borrarían: 0
- Confianza: media

### P1-17 · `Windows::iter` es cuadrático

- Tipo: atajo / rendimiento
- Dónde: `crates/malaysia-compositor/src/window.rs:346-357`
- Evidencia:
  ```rust
  self.scene.iter().filter_map(|(id, outer)| {
      self.windows.iter().find(|data| data.id == id).map(|data| Window { ... })
  })
  ```
- Por qué importa: para cada ventana en la escena recorre el vector `windows` linealmente. Con muchas ventanas se vuelve `O(n²)`. Hoy el número de ventanas es pequeño, pero el patrón es fácil de corregir con un `HashMap<WindowId, usize>` o manteniendo `windows` en el mismo orden que la escena.
- Arreglo mínimo: añadir `Windows::index` ya existe; usar `self.index(id)` en lugar de `.find()`.
- Tamaño: S
- Líneas que se borrarían: 0
- Confianza: media

## Preguntas

1. **ADR 0004 vs. redondeo al más cercano en el compositor.** ADR 0004 dice "El redondeo al más cercano desaparece de la API" y fija `floor`/`ceil` en `malaysia_scene::geometry`. Sin embargo, `session.rs`, `state.rs`, `window.rs` y `popup.rs` usan `to_i32_round()` y `.round()` para las coordenadas que entran al renderizador. ¿Es intencional que `malaysia_scene::geometry` aplique `floor`/`ceil` solo a la escena interna, mientras que Smithay recibe redondeo al más cercano? Si es intencional, el ADR debería aclararlo; si no, P1-01 es un bug real.

2. **`Windows::set_maximized` / `set_fullscreen`.** Están definidos en `window.rs` pero nadie los llama; todos los caminos usan las variantes `with_outer`. ¿Se mantienen como API pública futura o se pueden eliminar (P1-04)?

3. **Handlers vacíos.** `OutputHandler` e `ImageCaptureSourceHandler::source_destroyed` están vacíos por diseño. ¿Conviene añadir un comentario `// intentionally empty` para que no se vuelvan a auditar (P1-14)?

## Revisado y correcto

- Limpieza de ventanas y popups al morir un cliente: `State::client_disconnected` recopila superficies antes de iterar y llama a `forget_window`/`popup_destroyed`, que quitan la ventana de la escena, el grab, `chrome_pointer`, `titlebar_click` y `maximized` (la corrección faltante es solo para layers, P1-03).
- Lógica de bloqueo de commits con explicit/implicit sync: `pending_blocker` y `insert_blocker_source` son correctos; el path `None` para `acquire_point` sin dmabuf se descarta adecuadamente.
- Negociación de decoraciones xdg/KDE: `decoration_for_xdg_mode` y `decoration_for_kde_mode` cubren todos los modos; los tests de `handlers.rs` validan cada combinación.
- Sanitización del entorno en `spawn.rs`: `sanitize_with` restaura las variables originales y elimina las del wrapper; los tests verifican que un hijo no hereda `LD_LIBRARY_PATH` del wrapper.
- `cli.rs`: el parser manual cubre `--log-level` inline/separado, `--drm-device`, validación de opciones DRM y niveles inválidos, todo con tests.
- Región de entrada de layers: `accepts_input` distingue correctamente entre `None` (toda la superficie), región vacía (nada) y región con rectángulos.
- `malaysia-scene::geometry`: las conversiones `Point::to_physical`/`Size::to_physical` usan `floor`/`ceil` según ADR 0004, y los tests cubren rectángulos adyacentes sin costura.
