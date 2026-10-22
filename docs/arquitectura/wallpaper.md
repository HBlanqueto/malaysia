# El wallpaper (`yuki-shell wallpaper`)

El primer componente nativo del shell (ADR 0023/0024): un proceso Rust que dibuja
la imagen de escritorio en la capa `Background`, como macOS. Es parte de la shell
nativa (`yuki-shell`, ADR 0025).

## Cómo funciona

```
wallpaper.domain ──► decodificar PNG/JPEG ──► textura (celestial-kit)
   (~/.local/share/yuki)                       │
                                              GLES
                                              (render "fill", cubrir)
                                               │  eglSwapBuffers
                                               ▼
                                        wl_egl_window (Mesa: dmabuf)
                                               │  attach + commit
                                               ▼
                                    capa Background de Yuki (compositor)
```

1. **`celestial-kit`** (el toolkit, ≈ AppKit) abre su propio contexto
   EGL/GLES sobre el `wl_display` del compositor y dibuja la textura con la
   escala "fill" de macOS (cubrir, recortando el exceso).
2. **`yuki-shell wallpaper`** (el proceso del componente) es un cliente
   `wlr-layer-shell`: superficie en la capa `Background`, anclada a todo el
   output, con `wp_fractional_scale_v1` + `wp_viewporter` para HiDPI.
3. El frame se presenta por **EGL** sobre un `wl_egl_window` (Bloque 7c,
   ADR 0027): el swap se convierte en un dmabuf que el compositor texturiza
   directo — sin readback, sin copias por CPU, sin `wl_shm`.
4. El componente **vigila `wallpaper.domain`** (cada 500 ms) y repinta al
   cambiar — como macOS, la selección actual vive en `~/Library/Application
   Support` (`~/.local/share/yuki/`). Al cambiar de fondo calcula su color
   dominante y lo escribe en `wallpaper.state` (`dominant = [r, g, b]`) para
   el compositor y el portal; el compositor ya no decodifica imágenes.

## Crates y ficheros

- `crates/celestial-kit/src/gl.rs` — el render GLES (EGL, quad, presentación a
  una EGL window surface).
- `crates/yuki-shell/src/main.rs` — subcomandos: `yuki-shell wallpaper`.
- `crates/yuki-shell/src/wallpaper.rs` — el componente: cliente layer-shell,
  lectura del store, decodificación PNG/JPEG, color dominante y presentación.
- `crates/yuki-shell/src/present.rs` — `EglPresenter` (el `wl_egl_window`).
- `crates/grade-graphics/` — los shaders GLSL compartidos (vacío aún).

## Supervisión (ADR 0023)

- **Producción**: unidad de usuario `yuki-shell@.service` (template, `%i` =
  `wallpaper`) + `yuki-session.target`, instaladas por el flake en
  `$out/lib/systemd/user`. `Restart=on-failure`, `RestartSec=2`, límite de
  reintentos, logs por componente en el journal.
- **Desarrollo**: `scripts/test.sh [componente]` lanza el componente a mano
  contra el compositor en marcha.

## Aceptación (verificada)

- El wallpaper se dibuja en la capa `Background` (1280×800) presentando por
  `Dma` (EGL), no `Shm`.
- Editar `wallpaper.domain` lo cambia **en vivo**.
- `kill -9` al proceso: el compositor y los demás componentes siguen; el
  supervisor lo relanza.
- Con EGL (Bloque 7c) el wallpaper **mejora** su huella: RSS −4 %, GTT −3 MiB,
  USS igual — su textura va directo por dmabuf y el compositor no re-uploada
  shm (ver bloque 07).
- El wallpaper era el primero en portarse a nativo: es lo que la shell anterior
  (Quickshell, ya eliminada — ADR 0025) dibujaba en esa capa.

## Relación

- [`interfaz.md`](interfaz.md) — el mapa completo de la UI.
- ADR 0023 — el modelo de procesos (un proceso por componente).
- ADR 0024 — el stack de render del toolkit (GLES + cosmic-text + shaders).
- ADR 0027 — la shell presenta por EGL (Bloque 7c).
- `modelo-de-procesos.md` — la distribución de procesos resultante.
- `glossary.md` — celestial-kit / grade-graphics / Grade Animation / Infinity.