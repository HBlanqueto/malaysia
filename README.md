# yuki

Compositor Wayland construido sobre Smithay, desarrollado bloque a bloque.

## Datos generales

| Dato | Estado |
|---|---|
| Lenguaje | Rust (edición 2024) |
| Framework | Smithay, fijado por revisión (`023b78b1`, ADR 0028) |
| Backend de sesión | Winit anidado para desarrollo; DRM/KMS directo para el panel real (Bloque 8a) |
| Aceleración gráfica | Sí: renderizado por GPU con EGL + OpenGL ES 3 (GLES) |
| DRM/KMS | Sí: sesión directa atómica con libseat/libinput; vkms verificado, panel real pendiente |
| Vulkan | No como renderer del compositor; los clientes sí pueden usar Vulkan (p. ej. vkcube) |
| OpenGL | OpenGL ES 3 vía EGL; no hay renderer OpenGL de escritorio ni fallback por software |
| Shell | `yuki-shell` nativa, un proceso por componente (ADR 0023) |

Toda la documentación vive en [`docs/`](docs/README.md):

- [`docs/yuki.md`](docs/yuki.md) — visión general y desarrollo
- [`docs/roadmap.md`](docs/roadmap.md) — el roadmap por bloques
- [`docs/bloques/`](docs/bloques/) — lo implementado, bloque por bloque
- [`docs/decisiones/`](docs/decisiones/) — las decisiones (ADRs)
- [`docs/investigacion/`](docs/investigacion/) — investigación sobre macOS y Wayland
