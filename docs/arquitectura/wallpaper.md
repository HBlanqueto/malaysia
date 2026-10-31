# El wallpaper (`malaysia-wallpaper`)

> Mapa general: [`interfaz.md`](interfaz.md).

Cliente Wayland independiente (ADR 0049) que dibuja el fondo en la capa
`Background` con `wgpu` sobre Vulkan, sin BarrierKit, Freya ni Skia. Es una
superficie opaca sin widgets, texto ni accesibilidad.

## Cómo funciona

```
wallpaper.domain ──► decodificar PNG/JPEG ──► textura wgpu
 (~/.local/share/malaysia)                         │
                                              Vulkan (fill)
                                                   ▼
                                   layer Background del compositor
```

1. Abre un contexto `wgpu` sobre el `wl_display` con backend Vulkan
   (`vulkan-portability`) y dibuja con escala «fill» (cubre y recorta).
2. Es cliente `wlr-layer-shell`: capa `Background`, anclada a todo el output,
   con `wp_fractional_scale_v1` y `wp_viewporter` para HiDPI.
3. Presenta por dmabuf (`zwp_linux_dmabuf_v1`) sin readback ni copias por CPU.
4. Vigila `wallpaper.domain` con inotify y repinta al cambiar. Calcula el color
   dominante (vecindario 3×3×3, histograma ponderado por croma, descarte de
   grises y normalización HSL) y lo escribe en `wallpaper.state`
   (`dominant = [r, g, b]`). El compositor ya no decodifica imágenes.

## Crates y ficheros

- `crates/malaysia-wallpaper/src/main.rs` — entrada y loop de eventos.
- `image.rs` — store, decodificación PNG/JPEG, downscale y dominante.
- `render.rs` — contexto wgpu, texturas, mipmaps y shader de escala.
- `wayland.rs` — layer-shell, outputs y presentación (SCTK + calloop).
- `malaysia-shell` ya no expone el subcomando `wallpaper`.

## Supervisión

- Producción: `malaysia-wallpaper.service` + `malaysia-session.target`
  instalados por el flake; `Restart=on-failure`, `RestartSec=2`, límite de
  reintentos y logs en el journal.
- Desarrollo: `./dev.sh demo` y `./dev.sh lab` lo lanzan contra el compositor.

## Aceptación

- Dibuja en `Background` y presenta por dmabuf a través de wgpu/Vulkan.
- Editar `wallpaper.domain` cambia el fondo en vivo.
- `kill -9` al proceso no tumba al compositor; el supervisor lo relanza.
- No comparte proceso ni stack con `malaysia-shell`;
  `scripts/check-client-only-deps.sh` comprueba que `wgpu` no entre en el
  compositor ni Freya/Skia en el wallpaper.

## Relación

ADR 0023 (procesos), ADR 0027 (presentación EGL de la shell), ADR 0049
(Vulkan). [`modelo-de-procesos.md`](modelo-de-procesos.md) describe la
distribución.
