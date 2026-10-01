# El wallpaper (`yuki-shell wallpaper`)

El primer componente nativo del shell (ADR 0023/0024): un proceso Rust que dibuja
la imagen de escritorio en la capa `Background`, como macOS. Es parte de la shell
nativa (`yuki-shell`, ADR 0025).

## Cómo funciona

```
wallpaper.domain ──► decodificar PNG ──► textura (celestial-kit)
   (~/.local/share/yuki)                       │
                                              GLES offscreen
                                              (render "fill", cubrir)
                                               │  glReadPixels
                                               ▼
                                        buffer wl_shm
                                               │  attach + commit
                                               ▼
                                    capa Background de Yuki (compositor)
```

1. **`celestial-kit`** (el toolkit, ≈ AppKit) abre su propio contexto
   EGL/GLES sobre el `wl_display` del compositor y renderiza **offscreen** (FBO +
   `glReadPixels`), con la escala "fill" de macOS (cubrir, recortando el exceso).
2. **`yuki-shell wallpaper`** (el proceso del componente) es un cliente
   `wlr-layer-shell`: superficie en la capa `Background`, anclada a todo el
   output, con `wp_fractional_scale_v1` + `wp_viewporter` para HiDPI.
3. El frame renderizado se presenta por **`wl_shm`** (`Argb8888`).
4. El componente **vigila `wallpaper.domain`** (cada 500 ms) y repinta al
   cambiar — como macOS, la selección actual vive en `~/Library/Application
   Support` (`~/.local/share/yuki/`).

## Crates y ficheros

- `crates/celestial-kit/src/gl.rs` — el render GLES (EGL, FBO, quad, readback).
- `crates/yuki-shell/src/main.rs` — subcomandos: `yuki-shell wallpaper`.
- `crates/yuki-shell/src/wallpaper.rs` — el componente: cliente layer-shell,
  lectura del store, decodificación PNG, presentación.
- `crates/grade-graphics/` — los shaders GLSL compartidos (vacío aún).

## Divergencia: presentación por `wl_shm` (no dmabuf)

El plan de ADR 0024 es GLES; el frame se renderiza con GLES, pero se presenta
por **memoria compartida** (readback + `wl_shm`), no con una EGL window surface
sobre dmabuf. Motivo: el compositor aún no sirve `zwp_linux_dmabuf_v1`, sin el
cual Mesa no puede crear buffers de ventana EGL (se ve en los avisos
`MESA-EGL: failed to get driver name for fd -1`). Para un wallpaper estático
es correcto y suficiente; los componentes animados (menubar, dock) exigirán
dmabuf zero-copy en el compositor (trabajo futuro).

## Supervisión (ADR 0023)

- **Producción**: unidad de usuario `yuki-shell@.service` (template, `%i` =
  `wallpaper`) + `yuki-session.target`, instaladas por el flake en
  `$out/lib/systemd/user`. `Restart=on-failure`, `RestartSec=2`, límite de
  reintentos, logs por componente en el journal.
- **Desarrollo**: `scripts/test.sh [componente]` lanza el componente a mano
  contra el compositor en marcha.

## Aceptación (verificada)

- El wallpaper se dibuja en la capa `Background` (1280×800).
- Editar `wallpaper.domain` lo cambia **en vivo**.
- `kill -9` al proceso: el compositor y los demás componentes siguen; el
  supervisor lo relanza.
- El wallpaper era el primero en portarse a nativo: es lo que la shell anterior
  (Quickshell, ya eliminada — ADR 0025) dibujaba en esa capa.

## Relación

- ADR 0023 — el modelo de procesos (un proceso por componente).
- ADR 0024 — el stack de render del toolkit (GLES + cosmic-text + shaders).
- `modelo-de-procesos.md` — la distribución de procesos resultante.
- `glossary.md` — celestial-kit / grade-graphics / Grade Animation / Infinity.