# El wallpaper (`malaysia-wallpaper`)

El fondo de escritorio de MalaysiaDreams es un cliente Wayland independiente
(`malaysia-wallpaper`, ADR 0049). Dibuja la imagen de escritorio en la capa
`Background`, como macOS, pero con su propio proceso y renderizador Vulkan/wgpu
para no arrastrar BarrierKit/Freya/Skia a un caso que solo necesita una
superficie opaca de pantalla completa.

## Cómo funciona

```
wallpaper.domain ──► decodificar PNG/JPEG ──► textura (wgpu)
   (~/.local/share/malaysia)                       │
                                              Vulkan
                                              (render "fill", cubrir)
                                                   │
                                                   ▼
                                      capa Background de MalaysiaDreams (compositor)
```

1. **`malaysia-wallpaper`** abre un contexto `wgpu` sobre el `wl_display` del
   compositor, pidiendo el backend Vulkan (`vulkan-portability`). La textura se
   dibuja con un shader que implementa la escala "fill" de macOS (cubrir,
   recortando el exceso).
2. Es un cliente `wlr-layer-shell`: superficie en la capa `Background`, anclada
   a todo el output, con `wp_fractional_scale_v1` + `wp_viewporter` para HiDPI.
3. El frame se presenta directamente a través de la superficie `wgpu` vinculada
   a un `WlSurface` raw-window-handle; el compositor recibe un buffer de
   `zwp_linux_dmabuf_v1` sin readback ni copias por CPU.
4. El componente **vigila `wallpaper.domain`** (cada 500 ms) y repinta al
   cambiar — como macOS, la selección actual vive en `~/Library/Application
   Support` (`~/.local/share/malaysia/`). Al cambiar de fondo calcula su color
   dominante y lo escribe en `wallpaper.state` (`dominant = [r, g, b]`) para
   el compositor y el portal; el compositor ya no decodifica imágenes.

## Crates y ficheros

- `crates/malaysia-wallpaper/src/main.rs` — punto de entrada y loop de eventos.
- `crates/malaysia-wallpaper/src/image.rs` — lectura del store, decodificación
  PNG/JPEG y cálculo del color dominante (movido desde `malaysia-shell`).
- `crates/malaysia-wallpaper/src/render.rs` — contexto wgpu, carga de texturas,
  generación de mipmaps y shader de escala.
- `crates/malaysia-wallpaper/src/wayland.rs` — cliente layer-shell, registro de
  outputs y presentación por SCTK + calloop.
- `crates/malaysia-shell/src/main.rs` — ya no expone el subcomando `wallpaper`.

## Supervisión (ADR 0023)

- **Producción**: unidad de usuario dedicada `malaysia-wallpaper.service` +
  `malaysia-session.target`, instalada por el flake en `$out/lib/systemd/user`.
  `Restart=on-failure`, `RestartSec=2`, límite de reintentos, logs por
  componente en el journal.
- **Desarrollo**: `./dev.sh demo` y `./dev.sh lab` lanzan `malaysia-wallpaper`
  automáticamente contra el compositor en marcha.

## Aceptación (verificada)

- El wallpaper se dibuja en la capa `Background` presentando por dmabuf a través
  de wgpu/Vulkan.
- Editar `wallpaper.domain` lo cambia **en vivo**.
- `kill -9` al proceso: el compositor y los demás componentes siguen; el
  supervisor lo relanza.
- El wallpaper ya no comparte proceso ni stack gráfico con `malaysia-shell`;
  `scripts/check-client-only-deps.sh` comprueba que `wgpu` no entre en el
  compositor ni Freya/Skia en el wallpaper.

## Relación

- [`interfaz.md`](interfaz.md) — el mapa completo de la UI.
- ADR 0023 — el modelo de procesos (un proceso por componente).
- ADR 0034 — el stack de render del toolkit (BarrierKit sobre Freya, Skia en
  los clientes del shell).
- ADR 0049 — migración del wallpaper a Vulkan/wgpu.
- `modelo-de-procesos.md` — la distribución de procesos resultante.
- `glossary.md` — barrier-kit / grade-graphics / Grade Animation / Infinity.
