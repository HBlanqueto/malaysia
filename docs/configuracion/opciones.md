# Catálogo de opciones de Yuki

Catálogo canónico de las opciones del compositor (ADR 0021). **Los nombres y
valores son idénticos a los de macOS** siempre que exista un equivalente: las
secciones imitan System Settings (`appearance`, `window`, `dock`, `menubar`) y las
claves imitan `com.apple.dock` y demás (`autohide`, `orientation`, `tilesize`,
`mineffect`, `show-process-indicators`, ...).

**Como macOS, la configuración se guarda en un archivo por dominio** (extensión
**`.domain`**, el formato propio de Yuki, como `.plist` lo es de macOS) en dos
directorios — preferencias y estado:

```
~/.config/yuki/            ← ~/Library/Preferences
├── appearance.domain
├── display.domain
├── window.domain
├── dock.domain
├── menubar.domain
├── lock.domain
├── effects.domain
└── debug.domain

~/.local/share/yuki/       ← ~/Library/Application Support
├── wallpaper.domain       ← catálogo + selección actual ([desktops]), como
│                            com.apple.wallpaper
└── window.domain          ← estado: la app activa, publicada por el compositor
```

Cada archivo es opcional; los que no existen usan su default (como los dominios
`com.apple.dock`, `com.apple.menubar`... de macOS). El mismo catálogo alimenta el
comando `domain` y la futura app de ajustes (ambos por D-Bus, interfaz
`org.yuki.Compositor1`).

Estado de cada opción:
- **activa** — se lee en tiempo de ejecución y cambia el comportamiento.
- **datos** — está en `Config`, se documenta y valida, pero aún no se consume.
- **pendiente** — definida en el catálogo; se implementará con su efecto.

---

## `appearance` — apariencia (System Settings → Appearance)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `appearance.color_scheme` | `"auto"` \| `"dark"` \| `"light"` | `"auto"` | datos | portal `org.freedesktop.appearance`, shell |
| `appearance.accent_color` | `[r,g,b]` en `[0,1]` o `null` | `null` | datos | portal, shell, efectos |

macOS: "Appearance: **Light / Dark / Auto**" y "**Accent color**".

## `display` — escala de salida (System Settings → Displays)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `display.scale` | float `1.0..=4.0` | `1.0` | **activa** | `State::scale`, `wl_output`, `wp_fractional_scale_v1` |

No tiene equivalente exacto en macOS: macOS elige un **modo de pantalla** por
monitor, no una escala fraccional. Yuki sigue el modelo Wayland y publica la
parte entera por `wl_output` y la fraccionaria por `wp_fractional_scale_v1`.
`YUKI_SCALE` sigue existiendo como **override de desarrollo** y gana cuando está
definido; sirve para `dev.sh` sin escribir configuración. Con 8b esta opción
pasará a ser por salida.

## `window` — chrome y comportamiento de ventana (System Settings → Windows & Apps)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `window.decorations` | `"default"` \| `"force_server"` \| `"force_client"` | `"default"` | **activa** | handlers de decoración (ADR 0019/0020) |
| `window.titlebar` | bool | `true` | datos | renderer de decoración |
| `window.double_click_titlebar` | `"fill"` \| `"minimize"` \| `"zoom"` \| `"none"` | `"zoom"` | datos | input |
| `window.focus` | `"click"` \| `"sloppy"` | `"click"` | datos | input |
| `window.resize_border` | entero (px lógicos) | `6` | datos | `window::RESIZE_BORDER` (hoy constante) |

`"default"` respeta la negociación (GTK/Libadwaita conservan su headerbar; Qt y el
resto reciben el chrome de Yuki). `"force_server"` dibuja el chrome de Yuki en todas
las ventanas; `"force_client"` no dibuja ninguno.

`double_click_titlebar` usa las palabras de macOS: "**Fill / Minimize / Zoom / No
Action**".

## `wallpaper` — wallpapers del sistema (System Settings → Wallpaper)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `wallpaper.dir` | string (ruta) | `/run/current-system/sw/share/wallpapers/solid-colors` | datos | shell (`yuki-shell wallpaper`), app de ajustes |
| `wallpaper.scale` | `"fill"` \| `"stretch"` \| `"fit"` \| `"center"` | `"fill"` | datos | shell |
| `wallpaper.shuffle` | bool | `false` | datos | shell |

**Los wallpapers de Yuki viven en el propio proyecto** (`share/wallpapers/solid-colors/`,
el set macOS "Solid Colors", vendored en el repo) y el flake los instala como datos
del sistema en `/run/current-system/sw/share/wallpapers/`, igual que GNOME instala
los suyos en `/usr/share/backgrounds`. `wallpaper.dir` por defecto apunta ahí; un
desarrollo puede apuntarlo al repo o a otra carpeta. El componente
**`yuki-shell wallpaper`** dibuja el wallpaper en la capa `Background` con las
imágenes de esa carpeta (ADR 0023/0024; render GLES presentado por **EGL**/dmabuf,
ADR 0027).

**El wallpaper por defecto es `Solid Aqua Blue.png`** (`yuki_design::WALLPAPER`):
es el que se ve cuando el almacén no elige ninguno o todavía no existe, como la
imagen por defecto de una cuenta nueva de macOS. Si `wallpaper.dir` apunta a una
carpeta propia que no lo tiene, se usa la primera imagen de esa carpeta.

**Como macOS, la selección actual vive en un almacén aparte**:
`~/.local/share/yuki/wallpaper.domain` (el equivalente a `~/Library/Application
Support`, donde macOS guarda `com.apple.wallpaper`) — por desktop, con un `file://`
a la imagen:

```toml
# ~/.local/share/yuki/wallpaper.domain  (current selection, per desktop)
[desktops]
default = "file:///home/.../Solid Colors/Solid Mint.png"   # opcional; sin esta línea, Solid Aqua Blue
```

Editar ese archivo cambia el wallpaper **en vivo** (`yuki-shell wallpaper` vigila el
almacén y repinta). El catálogo (qué wallpapers existen) y la selección actual (qué
se muestra) están juntos en el dominio `wallpaper.domain`; el catálogo está en el
sistema (`share/wallpapers/`).

## `dock` — dock de la shell (`yuki-shell dock`; macOS `com.apple.dock`)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `dock.autohide` | bool | `false` | pendiente | shell (dock) |
| `dock.orientation` | `"bottom"` \| `"left"` \| `"right"` | `"bottom"` | pendiente | shell |
| `dock.magnification` | bool | `false` | pendiente | shell |
| `dock.tilesize` | entero (px) | `48` | pendiente | shell |
| `dock.largesize` | entero (px) | `64` | pendiente | shell |
| `dock.mineffect` | `"genie"` \| `"scale"` \| `"suck"` | `"genie"` | pendiente | shell |
| `dock.show-process-indicators` | bool | `true` | pendiente | shell |
| `dock.static-only` | bool | `false` | pendiente | shell |
| `dock.minimize-to-application` | bool | `false` | pendiente | shell |
| `dock.show-recents` | bool | `true` | pendiente | shell |

`mineffect` usa los nombres de macOS: "**Minimize windows using: Genie / Scale**".
Las claves con guion son **exactamente** las de `defaults write com.apple.dock ...`.

## `lock` — bloqueo de la sesión

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `lock.idle_timeout_seconds` | entero (segundos) | `300` | **activa** | `ext-idle-notify-v1` / auto-lock |

`0` desactiva el bloqueo automático. Al vencer el timeout sin input, el
compositor lanza `yuki-shell lock`. Un inhibidor `zwp_idle_inhibit_manager_v1`
(video en pantalla completa) detiene el reloj.

## `menubar` — barra de menús de la shell (macOS "Menu Bar")

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `menubar.autohide` | bool | `false` | pendiente | shell |

## `effects` — efectos de superficie del compositor (ADR 0020)

Se aplican a **todas** las ventanas, client- o server-decoradas.

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `effects.corner_radius` | float (px) | `0.0` | datos | render de superficies |
| `effects.inner_border.enabled` | bool | `false` | datos | render de decoración |
| `effects.inner_border.width` | float (px) | `1.0` | datos | idem |
| `effects.inner_border.color` | `[r,g,b]` | `[0.5,0.5,0.5]` | datos | idem |
| `effects.shadow.enabled` | bool | `false` | datos | render de sombras |
| `effects.shadow.radius` | float (px) | `24.0` | datos | idem |
| `effects.shadow.offset` | `[x,y]` (px) | `[0,6]` | datos | idem |
| `effects.shadow.opacity` | float `[0,1]` | `0.35` | datos | idem |
| `effects.blur.enabled` | bool | `false` | datos | render de blur |
| `effects.blur.radius` | float (px) | `20.0` | datos | idem |

Cuando exista un **tema transparente**, la app pinta con alpha y `effects.blur.*`
hace que el compositor difumine lo que hay detrás (vibrancy tipo macOS).

## `debug` — desarrollo

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `debug.damage_highlight` | bool | `false` | datos | render (resaltar daño por frame) |

---

## El comando `domain`

El comando de control del compositor, con la misma estructura que `defaults` de
macOS. Habla con el compositor por D-Bus (`org.yuki.Compositor1`); la app de
ajustes y la shell usan el mismo canal. El dominio de cada opción es su sección:
`appearance`, `window`, `dock`, `menubar`, `lock`, `effects`, `debug`.

```
domain read   <domain> <key>
domain write  <domain> <key> <valor> [-bool | -int | -float | -string]
domain delete <domain> <key>
domain rename <domain> <old> <new>
domain find   <palabra>
domain domains
```

Ejemplos (misma sintaxis que `defaults`):

```
domain write effects.blur.enabled -bool true        # o "read effects.blur.enabled"
domain write appearance.color_scheme -string dark
domain write dock.autohide -bool true
domain write window.decorations -string force_server
domain read  effects.shadow.radius
domain delete effects.blur.enabled                  # vuelve al default
domain find  blur
```

## Archivos de ejemplo (extensión `.domain`, un archivo por dominio)

Como macOS, un archivo por dominio en `~/.config/yuki/`:

```toml
# ~/.config/yuki/appearance.domain
color_scheme = "dark"           # auto | dark | light
accent_color = [0.208, 0.518, 0.894]
```

```toml
# ~/.config/yuki/display.domain
scale = 1.5                     # 1.0 .. 4.0
```

```toml
# ~/.config/yuki/window.domain
decorations = "default"         # default | force_server | force_client
titlebar = true
double_click_titlebar = "zoom"  # fill | minimize | zoom | none
focus = "click"                 # click | sloppy
resize_border = 6
```

```toml
# ~/.config/yuki/lock.domain
idle_timeout_seconds = 300      # 0 desactiva el auto-lock
```

```toml
# ~/.config/yuki/dock.domain
autohide = false
orientation = "bottom"          # bottom | left | right
magnification = false
tilesize = 48
largesize = 64
mineffect = "genie"             # genie | scale | suck
show-process-indicators = true
static-only = false
minimize-to-application = false
show-recents = true
```

```toml
# ~/.local/share/yuki/wallpaper.domain
dir = "/run/current-system/sw/share/wallpapers/solid-colors"
scale = "fill"                  # fill | stretch | fit | center (palabras de macOS)
shuffle = false

# Selección actual por desktop (como com.apple.wallpaper):
[desktops]
default = "file:///run/current-system/sw/share/wallpapers/solid-colors/Solid Aqua Blue.png"
```

```toml
# ~/.config/yuki/menubar.domain
autohide = false
```

```toml
# ~/.config/yuki/effects.domain
corner_radius = 12.0
[inner_border]
enabled = true
width = 1.0
color = [1.0, 1.0, 1.0]
[shadow]
enabled = true
radius = 24.0
offset = [0.0, 6.0]
opacity = 0.35
[blur]
enabled = false
radius = 20.0
```

```toml
# ~/.config/yuki/debug.domain
damage_highlight = false
```

> Nota: dentro de cada archivo de dominio las secciones se escriben **sin el
> prefijo del dominio** (`[inner_border]` en `effects.domain`, no
> `[effects.inner_border]`); el archivo *es* el dominio.

El catálogo vive en `crates/yuki/src/config.rs` como tipos `serde` tipados; cualquier
cambio en este documento debe reflejarse ahí y viceversa (ADR 0021).