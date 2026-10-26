# Auditoría: propagación de apariencia y detalles de Ajustes

Fecha: 2026-10-26  
Rama: `main` limpia y al día con `origin/main`.  
Objetivo: unificar la vigilancia de `appearance.domain`/`wallpaper.state` en un
solo watcher inotify (<50 ms) y corregir cuatro detalles de Ajustes más la zona
de arrastre de la isla.

## Hallazgos

### H1. Tres bucles de sondeo leyendo el mismo fichero

- `crates/malaysia-compositor/src/session.rs:39` — `theme_sources.tick(500)`;
  refresca `appearance.domain` cada 500 ms.
- `crates/malaysia-compositor/src/udev.rs:244` — idem en el backend DRM.
- `crates/xdg-desktop-portal-malaysia/src/settings.rs:144-164` — hilo propio con
  `sleep(Duration::from_millis(500))` + comparación de `mtime`.
- `crates/malaysia-shell/src/menubar/bar.rs:97-99` —
  `schedule_appearance_poll(500)`.

Consecuencia: latencia máxima ~500 ms, CPU desperdiciada, y cada componente
reimplementa su propia heurística.

Acción: crear `vow-appearance::watch` con un único fd inotify; exponer un token
`AppearanceWatch` que se registra en calloop en compositor, shell y portal.
Eliminar todos los timers/buches de sondeo.

### H2. `barrier-kit` lee disco en el hot path de render

- `crates/barrier-kit/src/theme.rs:45-70` — `Theme::load()` se invoca desde el
  cuerpo de `Component`, leyendo `appearance.domain` y `wallpaper.state` en cada
  frame/vuelta de reconciliación.
- `crates/barrier-kit/src/titlebar.rs:33`, `sidebar.rs:20`, `traffic.rs:15`,
  etc. — usan `use_theme()` sin cachear ni suscribirse a cambios.

Consecuencia: I/O síncrona en el hilo de UI; el render escala mal y la
propagación sigue siendo reactiva-a-palo.

Acción: en `barrier-kit` mantener un `State<ResolvedTheme>` poblado por el
runner a partir del watcher; `use_theme()` lee ese estado. El componente solo se
re-renderiza cuando el watcher dispara el fd.

### H3. `vow-appearance` no tiene módulo de vigilancia

- `crates/vow-appearance/src/lib.rs` solo expone resolución estática
  (`resolve_appearance`, `traffic_geometry`).
- No hay abstracción compartida para "cuándo cambia la config".

Acción: añadir `vow-appearance/src/watch.rs` con:

```rust
pub struct AppearanceWatch { fd: OwnedFd, watch: InotifyWatch }
impl AppearanceWatch {
    pub fn new(data_dir: &Path, config_dir: &Path) -> io::Result<Self>;
    pub fn fd(&self) -> RawFd;
    pub fn drain(&self) -> Vec<AppearanceEvent>; // Appearance, Wallpaper, Accent
}
```

Usar `nix::sys::inotify` (ya está en workspace; añadir feature `inotify`) en
vez de introducir otra dependencia.
shortcut: si `nix::sys::inotify` no cubre `IN_CLOSE_WRITE`/`IN_MODIFY` de forma
cómoda, evaluar `rustix`, 2026-10-27.

### H4. El portal duplica lógica de parseo

- `crates/xdg-desktop-portal-malaysia/src/settings.rs:67-91` — parsea a mano
  `appearance.domain` en lugar de usar `AppearanceConfig::load`.
- `crates/xdg-desktop-portal-malaysia/src/settings.rs:144-164` — lógica de
  polling casera.

Acción: reemplazar el hilo de polling por una fuente calloop que use
`AppearanceWatch::drain()` y recargue con `AppearanceConfig::load`.

### H5. Ajustes (Parte D) hace trabajo en render

- `crates/malaysia-settings/src/state.rs:172-188` — `read_dominant()` abre
  `wallpaper.state` y decodifica JSON cada vez que se llama.
- `crates/malaysia-settings/src/groups.rs:119`, `213`, `251`, etc. — llama a
  `read_dominant()` directamente desde funciones de componente.
- `crates/malaysia-settings/src/cards.rs:27` — recalcula miniatura en cada
  render.

Consecuencia: la miniatura "Automático" parpadea o no refleja el color dominante
hasta un nuevo evento; el cálculo del tinte no se actualiza al cambiar el fondo.

Acción:

1. Guardar `dominant: State<Option<Rgba>>` en `malaysia-settings` y recalcularlo
   solo cuando el watcher reporte `AppearanceEvent::Wallpaper`.
2. Asegurar que `theme_card("Automático")` muestra el fondo actual y el color
   dominante calculado, no el anterior ni el por defecto.
3. Verificar que `window_color_card("Fondo")` refresca `resolved.accent` en el
   kit cuando el usuario elige "Fondo".

### H6. Arrastre de la isla no recalcula al cambiar resolución/escala

- `crates/barrier-kit/src/titlebar.rs:45-52` — calcula `drag_region` una vez al
  montar el componente.
- No escucha cambios de `scale_factor` ni de geometría de tráfico.

Acción: mover el cálculo de `drag_region` a un `use_effect` (o `use_memo`) con
dependencias `[window_size, scale_factor, lights_geometry]`; invalidar en cada
render si cambian. Recortar a `client_rect` para no capturar los semáforos.

### H7. Test de latencia no existe

No hay test automatizado que mida <50 ms entre `save_config` y que el compositor
portal/shell hayan refrescado.

Acción: añadir test de integración en `vow-appearance/tests` que:

1. Escriba `appearance.domain`.
2. Lance `AppearanceWatch`.
3. Mida el tiempo hasta `drain()` reporte el evento.

Aceptación: p95 <50 ms en tmpfs (proxy de disco rápido).

## Archivos que cambiarán

- `crates/vow-appearance/src/lib.rs` — añadir `pub mod watch`.
- `crates/vow-appearance/src/watch.rs` — nuevo.
- `crates/vow-appearance/Cargo.toml` — añadir `nix` con feature `inotify`.
- `crates/malaysia-compositor/src/session.rs` — quitar `theme_sources`, añadir
  `AppearanceWatch` en calloop.
- `crates/malaysia-compositor/src/udev.rs` — igual.
- `crates/malaysia-compositor/src/state.rs` — `refresh_theme` recibe evento del
  watcher.
- `crates/xdg-desktop-portal-malaysia/src/settings.rs` — quitar hilo de
  polling, usar `AppearanceWatch`.
- `crates/xdg-desktop-portal-malaysia/Cargo.toml` — probablemente necesite
  `vow-appearance` (si no lo tiene ya).
- `crates/barrier-kit/src/platform/mod.rs` — registrar watcher y mantener
  `ResolvedTheme`.
- `crates/barrier-kit/src/theme.rs` — leer del estado compartido, no del disco.
- `crates/barrier-kit/src/titlebar.rs` — `drag_region` reactivo.
- `crates/malaysia-shell/src/menubar/bar.rs` — quitar
  `schedule_appearance_poll`.
- `crates/malaysia-settings/src/state.rs` — cachear color dominante.
- `crates/malaysia-settings/src/groups.rs`, `cards.rs` — usar cache.

## Riesgos

- `nix::sys::inotify` puede requerir feature adicional; verificar en `nix
  develop` antes de tocar código de producción.
- El compositor y la shell usan calloop en threads distintos; el watcher debe
  ser `Send + Sync` o clonable por fd.
- Cambiar `use_theme()` afecta a todos los componentes del kit; hay que tocar
  los callers en `malaysia-shell` y `malaysia-settings`.
- `wallpaper.state` se escribe por `malaysia-shell wallpaper`; el watcher debe
  detectar `IN_CLOSE_WRITE`, no solo `IN_MODIFY`.
