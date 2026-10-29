What this repo does: MalaysiaDreams is a NixOS desktop made of a Rust/Smithay Wayland compositor and its own shell (`malaysia-shell`: wallpaper, menubar, dock, lock, launcher), plus a settings app and an xdg-desktop-portal backend. The compositor draws server-side chrome and effects; the shell is one process per component (ADR 0023), drawn with BarrierKit/Freya (ADR 0034).

Load assumed: one local user on the ThinkPad L14 Gen 2a, one output, 5–10 Wayland clients, the session D-Bus, and the shell components started by systemd plus launcher/lock on demand. Not multi-user, not network-facing.

# Auditoría Ponytail P5 — `malaysia-shell` y units

## 1. Cabecera

- Commit: `718a7e4`.
- Alcance: `crates/malaysia-shell` completo (src + Cargo.toml), `share/systemd/user/`, y las rutas de `vow-appearance`, `barrier-kit`, compositor y Nix que el shell usa directamente.
- Leídos enteros (ruta y líneas):
  - `crates/malaysia-shell/Cargo.toml` (40)
  - `crates/malaysia-shell/src/main.rs` (69)
  - `crates/malaysia-shell/src/desktop.rs` (611)
  - `crates/malaysia-shell/src/dock.rs` (117)
  - `crates/malaysia-shell/src/launcher.rs` (843)
  - `crates/malaysia-shell/src/lock.rs` (511)
  - `crates/malaysia-shell/src/pam.rs` (480)
  - `crates/malaysia-shell/src/wallpaper.rs` (596)
  - `crates/malaysia-shell/src/menubar/mod.rs` (575)
  - `crates/malaysia-shell/src/menubar/appmenu.rs` (165)
  - `crates/malaysia-shell/src/menubar/bar.rs` (693)
  - `crates/malaysia-shell/src/menubar/clock.rs` (18)
  - `crates/malaysia-shell/src/menubar/dbusmenu.rs` (1469, en dos tramos)
  - `crates/malaysia-shell/src/menubar/dbusmenu_map.rs` (589)
  - `crates/malaysia-shell/src/menubar/focused.rs` (995)
  - `crates/malaysia-shell/src/menubar/help_topics.rs` (71)
  - `crates/malaysia-shell/src/menubar/icons.rs` (223)
  - `crates/malaysia-shell/src/menubar/menu.rs` (323)
  - `crates/malaysia-shell/src/menubar/remote.rs` (544)
  - `crates/malaysia-shell/src/menubar/system.rs` (131)
  - `share/systemd/user/malaysia-session.target` (6)
  - `share/systemd/user/malaysia-shell@.service` (21)
  - `nix/malaysia.nix` (86), en lo que instala y arranca el shell.
- Leído entero de archivos fuera del alcance que explican hallazgos: `crates/vow-appearance/src/watch.rs` (solo el watcher y sus tests), `crates/malaysia-compositor/src/active.rs`, y las zonas de `barrier-kit/src/platform/{mod,input}.rs`, `barrier-kit/src/password.rs`, `barrier-kit/src/theme.rs`, `barrier-kit/src/platform/mod.rs` y `menubar` de la barra que usa el shell.
- No leí entero: `barrier-kit` (P4), el compositor (fuera de P5), `vow-appearance/lib.rs` (solo las rutas) y el resto de docs. No ejecuté `./dev.sh check`, ni tests, ni una sesión real.

## 2. Hallazgos

### Must fix

### P5-01 · Las rutas de configuración reales no son las documentadas (= PX-02)
- Tipo: riesgo
- Dónde: `crates/vow-appearance/src/lib.rs:518`, `:536`; `crates/malaysia-compositor/src/active.rs:38`
- Evidencia:
  ```rust
  base.join("malaysia-compositor")   // config_dir
  base.join("malaysia-compositor")   // data_dir
  base.join("malaysia-compositor")   // active.rs, copia propia
  ```
- Por qué importa: README:139/148, `docs/configuracion/opciones.md:16/27` y ADR 0043 dicen `~/.config/malaysia/` y `~/.local/share/malaysia/`. Un usuario que escribe `wallpaper.domain` o `appearance.domain` donde dicen los docs no ve ningún efecto; solo `MALAYSIA_CONFIG_DIR`/`MALAYSIA_DATA_DIR` ocultan la diferencia en desarrollo. Afecta a todos los componentes del shell.
- Arreglo mínimo: cambiar los dos `join` a `"malaysia"`, hacer que `active.rs` use `vow_appearance::data_dir()` (no su copia), y mover los directorios a mano (ADR 0043 ya prohíbe migración automática). No añadir capa de compatibilidad.
- Tamaño: S
- Líneas que se borrarían: 0 (4 cambiadas; ~15 en `active.rs` si se reutiliza la función)
- Confianza: alta

### P5-02 · El lock no acepta mayúsculas, AltGr ni teclado con Caps/Num Lock (= PX-01 y PX-11)
- Tipo: bug
- Dónde: `crates/malaysia-shell/src/lock.rs:91`, `:204`; `crates/barrier-kit/src/platform/input.rs:106-109`
- Evidencia:
  ```rust
  Key::Character(_) if modifiers_empty => {        // lock.rs:91
  event.modifiers.is_empty(),                     // lock.rs:204
  Some(ch) => Key::Character(ch.to_ascii_lowercase().to_string()), // input.rs:106
  ```
- Por qué importa: `Shift+H` llega como `Character("H")` con SHIFT y se descarta; con Caps Lock o Num Lock activos `is_empty()` es falso y **no se escribe ninguna letra**; AltGr (Ctrl+Alt) hace imposible `@` en una distribución española. Además el kit ya ha pasado la tecla a minúsculas, así que ni quitando el filtro se recupera la mayúscula. Una contraseña normal deja al usuario fuera de su sesión. Los tests del lock (`lock.rs:359-372`) pasan `modifiers_empty = true` y construyen `Key::Character` a mano, así que no cubren esta ruta (= PX-11, parte lock).
- Arreglo mínimo: en `input::key` conservar el carácter real (la comparación de atajos ya normaliza); en `handle_key` aceptar `Character` si no hay CONTROL/ALT/META, dejando pasar SHIFT/CAPS/NUM (lo ideal es usar el texto UTF-8 que SCTK entrega, para AltGr). Añadir tests de `Shift+H`, Caps Lock y `@`/AltGr.
- Tamaño: M (dos crates; <30 líneas netas)
- Líneas que se borrarían: 1
- Confianza: alta

### P5-03 · `window.domain` no es TOML válido para títulos con comillas o saltos (= PX-05)
- Tipo: bug
- Dónde: `crates/malaysia-compositor/src/active.rs:59`; consumidores `dock.rs:114-116`, `menubar/mod.rs:107-110`
- Evidencia:
  ```rust
  let text = format!("[active]\ntitle = \"{}\"\napp_id = \"{}\"\n", title, app_id);
  ```
- Por qué importa: un título como `He said "hi"` o con `\n` invalida el TOML; el dock y la menubar no pueden parsearlo y pierden el resaltado/nombre de la app. Un salto de línea puede además inyectar claves. Es el estado que ambos leen.
- Arreglo mínimo: serializar un struct `#[derive(Serialize)]` con `toml::to_string` (ya está la dependencia) y añadir un test con `"` y `\n`.
- Tamaño: S
- Líneas que se borrarían: 2
- Confianza: alta

### P5-04 · Editar `wallpaper.domain` no cambia el fondo hasta reiniciar (= PX-04)
- Tipo: bug
- Dónde: `crates/malaysia-shell/src/wallpaper.rs:49-55`, `:72-80`; `docs/arquitectura/wallpaper.md:60`
- Evidencia:
  ```rust
  let image = load_snapshot(&store_path, &state_path);
  let app = AppComponent::new(WallpaperApp { image });
  // no hay watcher ni Timer: la imagen se carga una sola vez
  ```
- Por qué importa: la doc y la aceptación prometen “Editar `wallpaper.domain` lo cambia **en vivo**”. Cambiar imagen o `scale` no hace nada. El propio comentario del módulo dice que vigila el store; no lo hace.
- Arreglo mínimo: guardar la imagen en un `State<Option<ImageHandle>>`, un `Timers::after` de 500 ms que relea el store y redecodifique solo si cambió (mtime o contenido), y escribir `wallpaper.state` tras decodificar. Reutilizar el patrón de `schedule_active_poll`.
- Tamaño: M
- Líneas que se borrarían: 0 (unas 30 añadidas)
- Confianza: alta

### Should fix

### P5-05 · El dock resalta comparando un título viejo del store, no el estado vivo (= PX-06)
- Tipo: bug
- Dónde: `crates/malaysia-shell/src/dock.rs:29`, `:44`, `:103-117`; `crates/malaysia-compositor/src/active.rs`; `handlers.rs:724`, `:733`
- Evidencia:
  ```rust
  let active = load_active_title(&self.window_store).unwrap_or_default();
  let is_active = entry.title == active && !active.is_empty();
  // active.rs solo publica en focus; title_changed/app_id_changed no llaman a publish_active
  ```
- Por qué importa: `window.domain` es una foto que solo se actualiza al cambiar el foco. Si la app enfocada cambia de título (una pestaña del navegador, wezterm, un editor con documento sin guardar), el tile deja de estar resaltado; y dos ventanas con el mismo título se resaltan a la vez. La menubar también pierde un `app_id` que la app fija después de mapear.
- Arreglo mínimo: en el dock usar `entry.active` (el compositor ya envía `Activated` por `zwlr_foreign_toplevel_management_v1`) y borrar `window_store`/`load_active_title` (unas 20 líneas). La corrección de raíz para la menubar es publicar `window.domain` también en `title_changed`/`app_id_changed` (PX-06).
- Tamaño: S (shell) / M (compositor)
- Líneas que se borrarían: ~20 en el dock
- Confianza: alta

### P5-06 · Borrar `wallpaper.state` no despierta al watcher; los fallos dejan el tinte viejo (= PX-10)
- Tipo: bug
- Dónde: `crates/malaysia-shell/src/wallpaper.rs:92-103`, `:424-429`; `crates/vow-appearance/src/watch.rs:61`
- Evidencia:
  ```rust
  None => match std::fs::remove_file(path) { ... }   // wallpaper.rs:424
  AddWatchFlags::IN_CLOSE_WRITE | AddWatchFlags::IN_MOVED_TO // watch.rs:61
  ```
- Por qué importa: al pasar a un fondo gris (sin croma) se borra `wallpaper.state`, pero el borrado es `IN_DELETE` y el watcher no lo escucha; el compositor y el portal conservan el acento del fondo anterior hasta otro evento. Además, si no hay imagen o la decodificación falla, no se llama a `write_state` y también queda el dominante viejo.
- Arreglo mínimo: añadir `IN_DELETE` a `flags()` y clasificar `wallpaper.state`; llamar a `write_state(state_path, None)` en `Ok(_)` y en el error de decode; test que borra el archivo y espera `AppearanceEvent::Wallpaper`.
- Tamaño: S
- Líneas que se borrarían: 0 (3–5 añadidas)
- Confianza: alta

### P5-07 · `OnlyShowIn`/`NotShowIn` comparan con «MalaysiaDreams», pero la sesión es «Malaysia»
- Tipo: bug
- Dónde: `crates/malaysia-shell/src/desktop.rs:152-158`; `share/wayland-sessions/malaysia.desktop:6`; `crates/malaysia-compositor/src/udev.rs:535`
- Evidencia:
  ```rust
  if !show_in.is_empty() && !list_contains(show_in, "MalaysiaDreams") { return None; }
  if list_contains(not_show_in, "MalaysiaDreams") { return None; }
  // DesktopNames=Malaysia; XDG_CURRENT_DESKTOP=Malaysia
  ```
- Por qué importa: una app con `OnlyShowIn=Malaysia;` desaparece del launcher, y una con `NotShowIn=Malaysia;` aparece. Los tests (`desktop.rs:518`, `:526`) fijan el nombre equivocado, y `docs/bloques/13` aún dice `Yuki`.
- Arreglo mínimo: comparar con `"Malaysia"` y actualizar los tests; de paso corregir el nombre en el bloque 13.
- Tamaño: S
- Líneas que se borrarían: 0 (2 cambiadas)
- Confianza: alta

### P5-08 · El wallpaper declara píxeles premultiplicados que no lo son
- Tipo: bug
- Dónde: `crates/malaysia-shell/src/wallpaper.rs:85-90`
- Evidencia:
  ```rust
  ImageHandle::from_rgba(
      decoded.size.0, decoded.size.1,
      Bytes::from(decoded.rgba),
      AlphaType::Premul,
  )
  ```
- Por qué importa: PNG y JPEG se decodifican a RGBA con alfa recto (no premultiplicado). Skia interpreta los valores como premultiplicados y un PNG con transparencia se compone mal (colores fuera de rango/oscurecidos). Los fondos sólidos por defecto son opacos y lo esconden.
- Arreglo mínimo: `AlphaType::Unpremul` (o premultiplicar en `decode_image`); añadir un fixture PNG semitransparente si se quiere cubrir.
- Tamaño: S
- Líneas que se borrarían: 0 (1 cambiada)
- Confianza: alta

### P5-09 · El launcher lanzado desde la menubar pierde el `LD_LIBRARY_PATH` original de sus hijos
- Tipo: riesgo
- Dónde: `crates/malaysia-shell/src/menubar/mod.rs:303-315`; `crates/malaysia-shell/src/menubar/appmenu.rs:140`; `crates/malaysia-shell/src/launcher.rs:583-596`
- Evidencia:
  ```rust
  if let Some(value) = env::var_os(&saved) { command.env(name, value); }
  command.env_remove(saved);        // prepare_kit_env
  ...
  command.env_remove(name);         // sanitize_client_env del launcher
  ```
- Por qué importa: `spawn_component("launcher")` pasa por `prepare_kit_env`, que restaura `LD_LIBRARY_PATH` en el hijo pero borra `MALAYSIA_ORIG_*`. Cuando ese launcher lanza una app, `sanitize_client_env` ya no puede restaurar el valor original y simplemente elimina `LD_LIBRARY_PATH`. Abrir el launcher desde Archivo → Buscar y lanzar una app no es equivalente a abrirlo con Super+Space.
- Arreglo mínimo: no borrar `MALAYSIA_ORIG_*` en `prepare_kit_env` (el sanitizador del hijo ya los quita); o mantener un mapa explícito. Añadir un test de entorno con `prepare_kit_env` seguido de `sanitize_client_env`.
- Tamaño: S
- Líneas que se borrarían: 1
- Confianza: media (depende de que el wrapper ponga `MALAYSIA_ORIG_*`, que es el caso de producción)

### P5-10 · Las acciones de ventana usan índices viejos de la lista de toplevels (= PX-12)
- Tipo: bug
- Dónde: `crates/malaysia-shell/src/menubar/focused.rs:229-235`, `:411-420`; `crates/malaysia-shell/src/menubar/bar.rs:407`; `crates/malaysia-shell/src/dock.rs:51`
- Evidencia:
  ```rust
  id: format!("app-window-activate:{index}"),   // focused.rs:230
  "app-window-close" => { toplevels.close(index); } // focused.rs:417
  live_items: None,                              // bar.rs:407
  ```
- Por qué importa: el menú de la app captura el índice al abrirse (`live_items: None`) y la acción reutiliza ese índice contra la lista actual. Si una ventana se abre o se cierra mientras el menú está abierto, «Cerrar ventana»/«Minimizar»/activar puede actuar sobre otra ventana. En el dock el cierre de una ventana provoca un re-render, así que la carrera es más estrecha, pero el `on_press` también captura el índice.
- Arreglo mínimo: no codificar la posición; llevar `app_id`+título (o un id de ventana del kit) y resolverlo al ejecutar, sin hacer nada si ya no existe. En el dock, lo mismo. Es la corrección que pide PX-12.
- Tamaño: M
- Líneas que se borrarían: 0 (~20 cambiadas)
- Confianza: alta en los menús, media en el dock

### P5-11 · `StartLimitIntervalSec` está en `[Service]` y systemd la ignora
- Tipo: configuración
- Dónde: `share/systemd/user/malaysia-shell@.service:12-18`
- Evidencia:
  ```ini
  [Service]
  ...
  StartLimitBurst=10
  StartLimitIntervalSec=30
  ```
- Por qué importa: `systemd-analyze verify` (systemd 261) avisa: `Unknown key 'StartLimitIntervalSec' in section [Service], ignoring`. El intervalo real es el `DefaultStartLimitIntervalSec` (10 s), no 30 s; un componente que falla en bucle agota el límite antes de lo previsto y se queda caído hasta que alguien lo arranque. `StartLimitBurst` se acepta por compatibilidad, pero ambos pertenecen a `[Unit]`.
- Arreglo mínimo: mover las dos claves a `[Unit]`; verificar con `systemd-analyze verify` sobre una copia con `@out@` sustituido.
- Tamaño: S
- Líneas que se borrarían: 0 (2 movidas)
- Confianza: alta

### Nice to have

### P5-12 · `theme_revision` no lo lee nadie (= PX-15)
- Tipo: sobra
- Dónde: `crates/malaysia-shell/src/lock.rs:36`, `:272`, `:331-344`
- Evidencia:
  ```rust
  theme_revision: u64,
  theme_revision: theme_revision(),
  fn theme_revision() -> u64 { /* dos metadata().modified() */ }
  ```
- Por qué importa: se calcula al arrancar y se guarda en el modelo, pero ningún render lo consulta; el tema ya llega por `State<Resolved>` de BarrierKit. Cuesta dos `stat`, estado y ~20 líneas que parecen hacer algo.
- Arreglo mínimo: borrar el campo, la función y el `use std::time::SystemTime`.
- Tamaño: S
- Líneas que se borrarían: ~20
- Confianza: alta

### P5-13 · Dos `sanitize_client_env` idénticas (= PX-16)
- Tipo: sobra
- Dónde: `crates/malaysia-shell/src/launcher.rs:583-597`; `crates/malaysia-shell/src/menubar/mod.rs:278-292`
- Evidencia:
  ```rust
  fn sanitize_client_env(command: &mut Command) {
      for name in ["LD_LIBRARY_PATH", "__EGL_VENDOR_LIBRARY_DIRS", ...] {
          command.env_remove(name);
          let saved = format!("MALAYSIA_ORIG_{name}");
  ```
- Por qué importa: una variable nueva del wrapper hay que añadirla en dos sitios; olvidar uno filtra las rutas Mesa de Malaysia a las apps lanzadas desde la otra ruta.
- Arreglo mínimo: un helper en un módulo `spawn` del shell y llamarlo desde ambos. Mantener el `prepare_kit_env` como está (salvo P5-09).
- Tamaño: S
- Líneas que se borrarían: ~15
- Confianza: alta

### P5-14 · Cuatro copias de `rgba`
- Tipo: sobra
- Dónde: `crates/malaysia-shell/src/dock.rs:98-101`; `lock.rs:326-329`; `launcher.rs:423-426`; `menubar/mod.rs:114-117`
- Evidencia:
  ```rust
  fn rgba(c: vow_design::Rgba) -> Color {
      let [r, g, b, a] = c.to_rgba8();
      Color::from_argb(a, r, g, b)
  }
  ```
- Por qué importa: cuatro copias de la misma conversión; un cambio de orden de canales o de tipo hay que hacerlo cuatro veces.
- Arreglo mínimo: una función en un módulo compartido del shell (o en BarrierKit si es su lenguaje) e importarla.
- Tamaño: S
- Líneas que se borrarían: ~12
- Confianza: alta

### P5-15 · La menubar escribe 13.0 en vez del token `MENUBAR_FONT_SIZE`
- Tipo: deriva
- Dónde: `crates/malaysia-shell/src/menubar/menu.rs:286`; `crates/malaysia-shell/src/menubar/clock.rs:16`; token generado en `crates/vow-design/build.rs:1698`
- Evidencia:
  ```rust
  .font_size(13.0)          // menu.rs:286
  .font_size(13.0)          // clock.rs:16
  pub const MENUBAR_FONT_SIZE: f32 = 13.0; // generado
  ```
- Por qué importa: hoy coincide, pero ADR 0011/0032 exigen que los valores de diseño salgan de Neptunia. Cambiar `components.shell.menubar.font-size` no cambia el título ni el reloj. Lo mismo pasa con varios `10.0` de padding y el `16.0` del copo.
- Arreglo mínimo: importar y usar `MENUBAR_FONT_SIZE` en los dos sitios; para paddings/icono, añadir tokens o reutilizar los existentes antes de tocar más.
- Tamaño: S
- Líneas que se borrarían: 0 (2 cambiadas)
- Confianza: alta

### P5-16 · `raw_items_from_layout` se compila en producción y solo lo usan tests
- Tipo: sobra
- Dónde: `crates/malaysia-shell/src/menubar/dbusmenu.rs:1142-1150`
- Evidencia:
  ```rust
  #[allow(dead_code)]
  pub fn raw_items_from_layout(layout: &barrier_kit::app_menu::layout::LayoutItem) -> Vec<RawItem> {
      layout.2.iter().filter_map(|v| raw_item(v.clone())).collect()
  }
  ```
- Por qué importa: se mantiene con `allow(dead_code)` en el código de producción aunque solo lo llama `dbusmenu_map.rs:543/554` en tests (el linker puede descartarlo, pero el código y el `allow` siguen ahí). El `allow` oculta que es código de test.
- Arreglo mínimo: moverlo al módulo de tests o marcarlo `#[cfg(test)]`.
- Tamaño: S
- Líneas que se borrarían: ~19 del código de producción (el test conserva la copia)
- Confianza: alta

### P5-17 · El launcher clona todas las entradas y sus iconos en cada render
- Tipo: deriva/rendimiento
- Dónde: `crates/malaysia-shell/src/launcher.rs:97-114`, `:291-295`
- Evidencia:
  ```rust
  let entries = self.entries.clone();
  &entries.iter().map(|entry| entry.desktop.clone()).collect::<Vec<_>>(),
  ...
  SvgViewer::new((path.clone(), Bytes::from(bytes.clone())))
  ```
- Por qué importa: `self.entries` contiene `Vec<u8>` de SVG por app; `clone()` duplica todos los bytes en cada frame de la animación de apertura, y `desktop.clone()` duplica además nombres/keywords/Exec de todas las entradas. Con muchas apps son varios MB de asignaciones por frame sin cambiar nada.
- Arreglo mínimo: guardar `Rc<Vec<AppEntry>>` y puntuar sobre referencias/ids en vez de clonar `DesktopEntry`; el icono SVG puede ser `Arc<[u8]>`. No cambia la UI.
- Tamaño: M
- Líneas que se borrarían: ~10 (más cambios)
- Confianza: media (depende del número de apps; no medido)

### P5-18 · El wallpaper decodifica a resolución completa aunque solo se vea al tamaño de la salida
- Tipo: deriva/rendimiento
- Dónde: `crates/malaysia-shell/src/wallpaper.rs:79`, `:221-227`, `:262`
- Evidencia:
  ```rust
  let bytes = std::fs::read(path)?;
  let mut buf = vec![0; reader.output_buffer_size()];
  let rgba = decoder.decode()?;
  ```
- Por qué importa: en un panel de 1920×1200 una foto 4K ocupa ~33 MB de RGBA y una 8K ~132 MB, decodificada y subida entera para mostrarse escalada. No es una fuga (el proceso es aparte y muere con la sesión), pero el arranque y el RSS del componente crecen con la foto, no con la salida. No se decodifica dos veces: solo una por arranque; la recarga de PX-04 no debe perder esa propiedad.
- Arreglo mínimo: limitar dimensiones al decodificar (zune-jpeg tiene `set_max_width`/`set_max_height`; en PNG habría que decodificar y reescalar), o documentar el límite y medirlo. No hacerlo si se prefiere calidad máxima.
- Tamaño: S/M
- Líneas que se borrarían: 0
- Confianza: alta en el hecho, baja en que merezca la pena en este panel

### P5-19 · `ItemsPropertiesUpdated` no toca el árbol completo cacheado para la Ayuda
- Tipo: bug menor
- Dónde: `crates/malaysia-shell/src/menubar/dbusmenu.rs:327-337`
- Evidencia:
  ```rust
  for subtree in self.subtrees.values_mut() {
      changed |= apply_properties(&mut subtree.items, updated, removed);
  }
  ```
- Por qué importa: `MenuCache.full` (el árbol de `GetLayout(0,-1)` que usa el buscador de Ayuda) no se actualiza con las propiedades; si una app cambia `label`/`enabled` sin emitir `LayoutUpdated`, el buscador muestra el estado viejo hasta la próxima invalidación. Los menús visibles sí usan los subárboles y están bien.
- Arreglo mínimo: aplicar el mismo `apply_properties` a `self.full` y contar el cambio.
- Tamaño: S
- Líneas que se borrarían: 0 (3 añadidas)
- Confianza: alta

### P5-20 · `ActiveWindow.title` es un campo muerto y el dock tiene su propio parser del mismo archivo
- Tipo: sobra
- Dónde: `crates/malaysia-shell/src/menubar/mod.rs:91-98`; `crates/malaysia-shell/src/dock.rs:103-117`
- Evidencia:
  ```rust
  #[serde(default)]
  #[allow(dead_code)]
  pub title: String,
  ```
- Por qué importa: la menubar solo usa `app_id`; el `title` existe porque el archivo lo trae, no porque se lea. El dock duplica un `WindowStore`/`Active` solo para el título; P5-05 elimina esa copia y deja el campo de la menubar como único resto.
- Arreglo mínimo: quitar `title` de `ActiveWindow` (serde ignora las claves desconocidas) y, tras P5-05, no queda parser duplicado.
- Tamaño: S
- Líneas que se borrarían: ~8 (incluida la copia del dock si se aplica P5-05)
- Confianza: alta

## 3. Preguntas

- **Rutas**: ¿la canónica es `~/.config/malaysia` / `~/.local/share/malaysia` (ADR 0043, README y opciones) o `malaysia-compositor` (código)? Si es la primera, ¿se hace solo el `mv` manual que ya documenta la ADR o se lee la ruta vieja como respaldo durante una versión? P5-01 propone lo mínimo (sin respaldo).
- **PAM**: el lock cae a `login` si no existe `/etc/pam.d/malaysia-lock`. ¿Es deseable que una instalación sin la unidad Nix siga autenticando con la pila genérica, o debería fallar cerrado y decirlo en pantalla?
- **Dock**: el código y `docs/arquitectura/dock.md` dicen “una ventana, un tile”; la descripción de producto habla de “apps abiertas”. ¿Se queda por toplevel? Si se quiere por app, P5-05/P5-10 cambian de forma.
- **Wallpaper `scale`/`shuffle`**: `scale` se lee y solo se avisa de que no está implementado; `shuffle` ni se lee. ¿Se implementan antes de la recarga en vivo de P5-04 o se dejan fuera del alcance de la aceptación?
- **PAM y secretos**: en el camino correcto, la copia que devuelve `conversation` la libera Linux-PAM con `free` (no la borra); el `Zeroizing` cubre nuestras copias, no la de PAM. ¿Se acepta como límite de la biblioteca o se quiere envolver la conversación para borrar también esa copia?

## 4. Revisado y correcto

- **PAM/FFI**: `PAM_REFRESH_CRED` es `0x0010`, `PAM_CONV_ERR` 19 y `PAM_BUF_ERR` 5 en Linux-PAM 1.7.x; `PamMessage`/`PamResponse`/`PamConv` coinciden con `_pam_types.h`. Los bloques `unsafe` delicados (`conversation`, `discard_responses`) llevan `# Safety`; el resto son llamadas FFI directas. La contraseña usa `Zeroizing<String>` y `PasswordField` borra al denegar.
- **D-Bus del menú**: el hilo dedicado, la cola FIFO, el `Service` por generación, los timeouts (500 ms/2 s) y el drenaje por `Timers::wakeup` están bien separados; ningún `unwrap` sobre datos de la app. `RawItem` se decodifica con `downcast().ok()` y las propiedades con `try_from().ok()`, así que layouts/propiedades mal formados se ignoran sin panic.
- **Parser `.desktop`**: `Hidden`/`NoDisplay`/`TryExec`/`Terminal`, precedencia XDG, locale, comillas y códigos de campo están cubiertos por tests; no hay indexación por bytes fuera de límites.
- **Wallpaper**: los fixtures cubren JPEG, PNG 16-bit e indexado; la escritura de estado es atómica (`tmp` + rename) y el watcher sí ve `MOVED_TO`. La decodificación ocurre una sola vez por arranque (no por salida ni por tema).
- **Superficies**: `wallpaper` con `Anchors::ALL` ocupa toda la salida; `dock` usa `Anchors::BOTTOM` (incluye left/right, así que no es una superficie de ancho 0); menubar y launcher reservan las zonas/exclusividad correctas.
- **systemd**: cada componente es una unit independiente con `Restart=on-failure`; `PartOf` solo la detiene con el target, no la ata a las demás (ADR 0023). El único fallo es la sección de P5-11.
- **Iconos y menús**: los SVG y los golden tests del launcher cubren tema claro/oscuro; los ids de submenú remoto se parsean y se descartan si no son numéricos; `remote::merge_subtree`/`find_subtree` no asumen que el id exista.
- **Lanzamiento**: `setsid` + entorno saneado en launcher y menubar; el token de activación tiene el fallback de 250 ms para que Enter no se quede colgado; las apps de kit conservan su entorno a propósito.

## 5. Contraste con PX

- **PX-01 (lock y modificadores): CONFIRMADO.** `lock.rs:91` y `:204` siguen filtrando por `modifiers.is_empty()`, y además `barrier-kit/src/platform/input.rs:106/109` pasa el carácter a minúsculas. Es P5-02.
- **PX-04 (wallpaper sin recarga en vivo): CONFIRMADO.** `wallpaper.rs:53` carga una sola vez; no hay watcher ni timer, y `docs/arquitectura/wallpaper.md:60` promete lo contrario. Es P5-04.
- **PX-10 (borrado de `wallpaper.state`): CONFIRMADO.** `wallpaper.rs:424` borra y `vow-appearance/src/watch.rs:61` solo escucha `IN_CLOSE_WRITE`/`IN_MOVED_TO`. Es P5-06; añado el caso de decode fallido, que también deja el estado viejo.
- **PX-11 (tests del lock): MATIZADO.** La parte del lock es cierta: `lock.rs:359-372` siempre pasa `modifiers_empty = true` y construye `Key::Character` directamente, así que el bug de PX-01 pasa desapercibido. Las otras dos pruebas que pide PX-11 son del compositor y quedan fuera de P5. La prueba de `Shift+H` va incluida en el arreglo de P5-02.
- **PX-12 (posiciones viejas): CONFIRMADO.** `focused.rs:230` codifica el índice, `focused.rs:417-419` lo reutiliza y `bar.rs:407` mantiene el menú sin `live_items`. Es P5-10. En el dock la carrera es más estrecha (se re-renderiza al cambiar la lista), pero el `on_press` de `dock.rs:51` también captura el índice.
- **PX-15 (`theme_revision` muerto): CONFIRMADO.** `lock.rs:36`/`:272`/`:331-344`; nadie lee el campo. Es P5-12.
- **PX-16 (`sanitize_client_env` duplicado): CONFIRMADO.** `launcher.rs:583` y `menubar/mod.rs:278` son la misma función; P5-13.
- Además, aunque no estaban en la lista pedida: **PX-02** se confirma en P5-01, **PX-05** en P5-03 y **PX-06** en P5-05. La parte de docs de **PX-18** (wallpaper) se confirma en P5-04/P5-15.

## 6. No comprobado

- No ejecuté `./dev.sh check`, `cargo test`, clippy ni el compositor; no hay compilación ni ejecución real en esta pasada.
- No probé PAM real, el desbloqueo, la recuperación del lock ni una sesión DRM/VT; el análisis de PAM es lectura y comparación con las cabeceras 1.7.x.
- No probé el arranque systemd real; solo `systemd-analyze verify` sobre una copia con `@out@` sustituido. No medí el límite de reintentos en una sesión.
- No leí entero BarrierKit (P4) ni el compositor; solo las zonas que usa el shell. No medí la memoria del launcher ni del wallpaper; los tamaños son estimaciones.
- No probé multi-salida, escala fraccional, cambio de tema en caliente ni la recarga del wallpaper (no existe todavía).
- No revisé el build Nix/Skia ni el resto de docs.

Verdict: la arquitectura y la separación de procesos están bien; los defectos que un usuario diario nota están en la entrada del lock, las rutas de configuración, el estado publicado de la ventana activa y la recarga del wallpaper. Arreglar P5-01 a P5-04 primero.

Lean: −115 líneas, −0 dependencias posibles.

Not checked: ver «6. No comprobado».
