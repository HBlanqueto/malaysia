# yuki

Compositor Wayland construido sobre Smithay, desarrollado bloque a bloque.

> [!NOTE]
> Este es un proyecto vibecoded, por gusto personal. No tiene más que uso diario como hobby.

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

## Comandos y configuración del escritorio

### Shell

Cada componente es un proceso propio:

```sh
yuki-shell wallpaper   # fondo de pantalla
yuki-shell menubar     # barra superior: app activa + reloj
yuki-shell dock        # dock de apps
yuki-shell lock        # pantalla de bloqueo
```

### Configuración (ADR 0021)

La interfaz acordada es el comando `domain`, con la sintaxis de `defaults` de
macOS. Habla con el compositor por D-Bus (`org.yuki.Compositor1`); el binario
todavía no está implementado, pero la interfaz es esta:

```sh
domain read   <dominio> <clave>
domain write  <dominio> <clave> <valor> [-bool | -int | -float | -string]
domain delete <dominio> <clave>          # vuelve al default
domain rename <dominio> <antigua> <nueva>
domain find   <palabra>
domain domains

domain write appearance.color_scheme -string dark
domain write dock.autohide -bool true
domain read  effects.shadow.radius
```

Los dominios son `appearance`, `window`, `dock`, `menubar`, `lock`, `effects` y
`debug`.

### Archivos `.domain`

La configuración se guarda como TOML, un archivo por dominio, al estilo de los
`.plist` de macOS:

```text
~/.config/yuki/                 preferencias
├── appearance.domain
├── window.domain
├── dock.domain
├── menubar.domain
├── lock.domain
├── effects.domain
└── debug.domain

~/.local/share/yuki/            estado y datos
├── wallpaper.domain            catálogo + selección actual
└── window.domain               app activa (la escribe el compositor)
```

Cada archivo es opcional; si falta, se usan los valores por defecto. El
compositor lee sus preferencias al arrancar; la shell relee sus almacenes en
vivo (por ejemplo, el wallpaper cambia al editar `wallpaper.domain`).
`YUKI_CONFIG_DIR` y `YUKI_DATA_DIR` permiten usar otros directorios.

El wallpaper se elige en `~/.local/share/yuki/wallpaper.domain`:

```toml
dir = "/run/current-system/sw/share/wallpapers/solid-colors"
scale = "fill"

[desktops]
default = "file:///run/current-system/sw/share/wallpapers/solid-colors/Solid Aqua Blue.png"
```

`dir` es el catálogo de imágenes PNG. Si no hay `[desktops].default`, se usa
`Solid Aqua Blue.png` dentro de `dir` (o la primera imagen si no existe).

Toda la documentación vive en [`docs/`](docs/README.md):

- [`docs/yuki.md`](docs/yuki.md) — visión general y desarrollo
- [`docs/roadmap.md`](docs/roadmap.md) — el roadmap por bloques
- [`docs/bloques/`](docs/bloques/) — lo implementado, bloque por bloque
- [`docs/decisiones/`](docs/decisiones/) — las decisiones (ADRs)
- [`docs/investigacion/`](docs/investigacion/) — investigación sobre macOS y Wayland
