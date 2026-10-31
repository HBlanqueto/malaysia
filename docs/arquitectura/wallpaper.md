# El wallpaper (`malaysia-wallpaper`)

> Mapa general: [`interfaz.md`](interfaz.md).

Cliente Wayland independiente (ADR 0049) que dibuja el fondo en la capa `Background` con `wgpu` sobre Vulkan, sin BarrierKit, Freya ni Skia. Es una superficie opaca sin widgets, texto ni accesibilidad.

## Cómo funciona

```
wallpaper.domain ──► decodificar PNG/JPEG ──► textura wgpu
 (~/.local/share/malaysia)                         │
                                              Vulkan (fill)
                                                   ▼
                                   layer Background del compositor
```

1. Abre un contexto `wgpu` sobre el `wl_display` con backend Vulkan y dibuja con escala «fill» (cubre y recorta).
2. Es cliente `wlr-layer-shell`: capa `Background`, anclada a todo el output, con `wp_fractional_scale_v1` y `wp_viewporter` para HiDPI.
3. Presenta por dmabuf (`zwp_linux_dmabuf_v1`) sin readback ni copias por CPU.
4. Vigila `wallpaper.domain` con inotify y repinta al cambiar. Calcula el color dominante (vecindario 3×3×3, histograma ponderado por croma, descarte de grises y normalización HSL) y lo escribe en `wallpaper.state` (`dominant = [r, g, b]`). El compositor ya no decodifica imágenes.

## Supervisión

`malaysia-wallpaper.service` + `malaysia-session.target`; `Restart=on-failure`, `RestartSec=2`. En desarrollo lo lanzan `./dev.sh demo` y `lab`. Editar `wallpaper.domain` cambia el fondo en vivo; `kill -9` no tumba al compositor; `scripts/check-client-only-deps.sh` comprueba que `wgpu` no entre en el compositor ni Freya/Skia en el wallpaper.