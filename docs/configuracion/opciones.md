# Catálogo de opciones de MalaysiaDreams

Catálogo canónico de las opciones del compositor (ADR 0021). **Los nombres y
valores son idénticos a los de macOS** siempre que exista un equivalente: las
secciones imitan System Settings (`appearance`, `window`, `dock`, `menubar`) y las
claves imitan `com.apple.dock` y demás (`autohide`, `orientation`, `tilesize`,
`mineffect`, `show-process-indicators`, ...).

Mapa visual completo: [`../arquitectura/interfaz.md`](../arquitectura/interfaz.md).

**Como macOS, la configuración se guarda en un archivo por dominio** (extensión
**`.domain`**, el formato propio de MalaysiaDreams, como `.plist` lo es de macOS) en dos
directorios — preferencias y estado:

```
~/.config/malaysia/            ← ~/Library/Preferences
├── appearance.domain
├── display.domain
├── window.domain
├── screenshot.domain
├── dock.domain
├── menubar.domain
├── lock.domain
├── effects.domain
└── debug.domain

~/.local/share/malaysia/       ← ~/Library/Application Support
├── wallpaper.domain       ← catálogo + selección actual ([desktops]), como
│                            com.apple.wallpaper
├── wallpaper.state        ← color dominante del fondo (lo escribe la shell)
└── window.domain          ← estado: la app activa, publicada por el compositor
```

Cada archivo es opcional; los que no existen usan su default (como los dominios
`com.apple.dock`, `com.apple.menubar`... de macOS). El mismo catálogo alimenta el
comando `domain` y la futura app de ajustes (ambos por D-Bus, interfaz
`org.malaysia.Compositor1`).

Estado de cada opción:
- **activa** — se lee en tiempo de ejecución y cambia el comportamiento.
- **datos** — está en `Config`, se documenta y valida, pero aún no se consume.
- **pendiente** — definida en el catálogo; se implementará con su efecto.

---

## Atajos fijos del compositor

No son opciones de dominio; están fijos y documentados aquí:

| Atajo | Acción |
|---|---|
| `Super+Space` | abre o cierra `malaysia-shell launcher`; bloqueado con la sesión bloqueada |
| `Ctrl+Alt+Backspace` | salida de emergencia (bloqueada con la sesión bloqueada) |
| `Ctrl+Alt+L` | bloquea o relanza el lock |
| `Ctrl+Alt+F1..F12` | cambia de VT |
| `Shift+Super+3` / `Shift+Super+4` | captura de pantalla / región |
| `PrintScreen` | captura la ventana enfocada |

---

## `appearance` — apariencia (System Settings → Appearance)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `appearance.color_scheme` | `"auto"` \| `"dark"` \| `"dark_tint"` \| `"light"` | `"auto"` | **activa** | portal `org.freedesktop.appearance`, shell, **Ajustes del sistema** |
| `appearance.accent_color` | `[r,g,b]` en `[0,1]` o `null` | `null` | **activa** | portal, shell, efectos, **Ajustes del sistema** |
| `appearance.contrast` | bool | `false` | **activa** | portal (`org.freedesktop.appearance` `contrast`) |
| `appearance.reduced_motion` | bool | `false` | **activa** | portal (`reduced-motion`) |
| `appearance.reduce_transparency` | bool | `true` | **activa** | shell y kit: fondos sólidos; `false` habilitará el blur del compositor (ADR 0038) |
| `appearance.window_color` | `"neutral"` \| `"accent"` \| `"wallpaper"` | `"neutral"` | **activa** | titlebar/chrome del compositor, **Ajustes del sistema** |
| `appearance.window_tint_scope` | `"windows"` \| `"desktop"` | `"windows"` | **activa** | `desktop` tiñe también menubar/dock (20 % / 26 %), **Ajustes del sistema** |
| `appearance.traffic_lights` | `"auto"` \| `"standard"` \| `"accent"` \| `"wallpaper"` | `"auto"` | **activa** | semáforos de la titlebar, **Ajustes del sistema** |
| `appearance.sidebar_style` | `"island"` \| `"flat"` | `"island"` | **activa** | estilo de la barra lateral del kit |

macOS: "Appearance: **Light / Dark / Auto**" y "**Accent color**"; "Increase
contrast" y "Reduce motion" viven en Accessibility y se publican en el mismo
namespace del portal.

La app **Ajustes del sistema** lee y escribe `appearance.domain` a través de
`vow-appearance::write_appearance`. La escritura es atómica (archivo temporal +
`rename`) y conserva claves desconocidas y comentarios del usuario, de modo que
editar el archivo a mano y usar la app son compatibles.

`sidebar_style` selecciona la barra lateral del kit: `"island"` (flotante, con
vibrancy, borde y sombra) o `"flat"` (lista plana integrada en el fondo de la
ventana). Con `reduce_transparency = true` la isla sigue opaca, pero conserva
su radio, contorno y sombra.

`window_color` es la idea de Tengen "Ajustes › Apariencia › Color de las
ventanas": `neutral` no tiñe, `accent` mezcla el acento con el fondo de la
titlebar (6 % en claro, 8 % en oscuro) y `wallpaper` usa el color dominante del
wallpaper (7 % / 9 %). Los semáforos en `auto` siguen el modo (estándar en neutral,
derivados del acento/wallpaper en los otros).

## `display` — escala de salida (System Settings → Displays)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `display.scale` | float `1.0..=4.0` | `1.0` | **activa** | `State::scale`, `wl_output`, `wp_fractional_scale_v1` |

No tiene equivalente exacto en macOS: macOS elige un **modo de pantalla** por
monitor, no una escala fraccional. MalaysiaDreams sigue el modelo Wayland y publica la
parte entera por `wl_output` y la fraccionaria por `wp_fractional_scale_v1`.
`MALAYSIA_SCALE` sigue existiendo como **override de desarrollo** y gana cuando está
definido; sirve para `dev.sh` sin escribir configuración. Con 8b esta opción
pasará a ser por salida.

## `window` — chrome y comportamiento de ventana (System Settings → Windows & Apps)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `window.decorations` | `"default"` \| `"force_server"` \| `"force_client"` | `"default"` | **activa** | handlers de decoración (ADR 0019/0020) |
| `window.titlebar` | bool | `true` | datos | renderer de decoración |
| `window.double_click_titlebar` | `"fill"` \| `"minimize"` \| `"zoom"` \| `"none"` | `"zoom"` | **activa** | input |
| `window.green_button` | `"focus"` \| `"zoom"` \| `"fullscreen"` \| `"none"` | `"focus"` | **activa** | titlebar SSD y CSD |
| `window.focus` | `"click"` \| `"sloppy"` | `"click"` | datos | input |
| `window.resize_border` | entero (px lógicos) | `10` | **activa** | ancho de la banda exterior de resize en lados |
| `window.resize_corner` | entero (px lógicos) | `20` | **activa** | longitud de la «L» exterior de resize en esquinas |
| `window.qt_server_decorations` | bool | `true` | **activa** | `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` en clientes y sesión |

`window.resize_border` es una banda **exterior** al lado; la «L» de
`window.resize_corner` es exterior y puede entrar 2 px. El hit-test solo se
ofrece en ventanas SSD normales (no maximizadas ni fullscreen).

`"default"` respeta la negociación (GTK/Libadwaita conservan su headerbar; Qt y el
resto reciben el chrome de MalaysiaDreams). `"force_server"` dibuja el chrome de MalaysiaDreams en todas
las ventanas; `"force_client"` no dibuja ninguno.

`double_click_titlebar` usa las palabras de macOS: "**Fill / Minimize / Zoom / No
Action**".

`green_button` decide qué hace el semáforo verde. Como en macOS, el valor por
defecto es **`focus`** (modo foco; de momento solo registra
`focus mode requested` y no cambia la geometría). `zoom` maximiza conservando
el marco; `fullscreen` y el modo foco se conectarán más adelante; `none` no hace
nada. El doble clic de la barra sigue gobernado por `double_click_titlebar`.

`qt_server_decorations` exporta `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` para los
clientes que lanza `malaysia_compositor::spawn` y en el entorno de la sesión, de modo que Qt
use el protocolo `xdg-decoration` y reciba el marco de MalaysiaDreams. Ponerlo a `false`
permite que Qt dibuje su propia barra (como hace Telegram; véase
[`../arquitectura/ventanas.md`](../arquitectura/ventanas.md)).

## `screenshot` — capturas (app Captura; macOS `com.apple.screencapture`)

Flujo, atajos y overlay: [`../arquitectura/capturas.md`](../arquitectura/capturas.md).

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `screenshot.show_pointer` | bool | `false` | **activa** | captura de pantalla/región (las de ventana nunca lo llevan) |
| `screenshot.show_thumbnail` | bool | `true` | pendiente | miniatura flotante |
| `screenshot.target` | `"desktop"` \| `"clipboard"` \| `"preview"` \| `"mail"` \| `"messages"` \| `"other"` | `"desktop"` | **activa** (`desktop`/`clipboard`); el resto cae al Escritorio | destino: fichero o `image/png` en el portapapeles |
| `screenshot.location` | string (ruta) o ausente | — | datos | carpeta propia para `target = "other"` |
| `screenshot.type` | `"png"` \| `"jpg"` \| `"heif"` \| `"tiff"` \| `"pdf"` | `"png"` | datos | formato (hoy siempre PNG) |
| `screenshot.include_date` | bool | `true` | datos | fecha y hora en el nombre |
| `screenshot.disable_shadow` | bool | `false` | datos | sombra de ventana en la captura |
| `screenshot.timer` | entero (segundos) | `0` | datos | temporizador (0 = ninguno, 5, 10) |
| `screenshot.remember_selection` | bool | `false` | datos | recordar la última región/ventana |

Los defaults son los de macOS: puntero **oculto**, guardar en el **Escritorio** en
**PNG** con fecha, miniatura flotante **sí** y sombra de ventana **sí**. Los nombres
siguen la app Captura ("Show Mouse Pointer", "Show Floating Thumbnail", "Save to",
"Timer", "Remember Last Selection") y `com.apple.screencapture` (`type`,
`location`, `include-date`, `disable-shadow`).

Estado: `show_pointer` y `target = "clipboard"` ya se aplican. El resto se lee
y valida, pero el flujo actual sigue guardando en el Escritorio, en PNG y con
fecha; `preview`/`mail`/`messages`/`other`, miniatura, temporizador, formatos y
sombra se conectarán en las siguientes pasadas. Las capturas de ventana nunca
incluyen el puntero. El portapapeles es `image/png` y lo sirve el compositor;
ver [`../investigacion/portapapeles-capturas.md`](../investigacion/portapapeles-capturas.md).

## `wallpaper` — wallpapers del sistema (System Settings → Wallpaper)

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `wallpaper.dir` | string (ruta) | `/run/current-system/sw/share/wallpapers/solid-colors` | datos | `malaysia-wallpaper`, app de ajustes |
| `wallpaper.scale` | `"fill"` \| `"stretch"` \| `"fit"` \| `"center"` | `"fill"` | datos | `malaysia-wallpaper` |
| `wallpaper.shuffle` | bool | `false` | datos | `malaysia-wallpaper` |

**Los wallpapers de MalaysiaDreams viven en el propio proyecto** (`share/wallpapers/solid-colors/`,
el set macOS "Solid Colors", vendored en el repo) y el flake los instala como datos
del sistema en `/run/current-system/sw/share/wallpapers/`, igual que GNOME instala
los suyos en `/usr/share/backgrounds`. `wallpaper.dir` por defecto apunta ahí; un
desarrollo puede apuntarlo al repo o a otra carpeta. El componente
**`malaysia-wallpaper`** dibuja el wallpaper en la capa `Background` con las
imágenes de esa carpeta (ADR 0049; render **Vulkan**/wgpu presentado por dmabuf).

**El wallpaper por defecto es `Solid Aqua Blue.png`** (`vow_design::WALLPAPER`):
es el que se ve cuando el almacén no elige ninguno o todavía no existe, como la
imagen por defecto de una cuenta nueva de macOS. Si `wallpaper.dir` apunta a una
carpeta propia que no lo tiene, se usa la primera imagen de esa carpeta.

**Como macOS, la selección actual vive en un almacén aparte**:
`~/.local/share/malaysia/wallpaper.domain` (el equivalente a `~/Library/Application
Support`, donde macOS guarda `com.apple.wallpaper`) — por desktop, con un `file://`
a la imagen:

```toml
# ~/.local/share/malaysia/wallpaper.domain  (current selection, per desktop)
[desktops]
default = "file:///home/.../Solid Colors/Solid Mint.png"   # opcional; sin esta línea, Solid Aqua Blue
```

Editar ese archivo cambia el wallpaper **en vivo** (`malaysia-wallpaper` vigila el
almacén y repinta). El catálogo (qué wallpapers existen) y la selección actual (qué
se muestra) están juntos en el dominio `wallpaper.domain`; el catálogo está en el
sistema (`share/wallpapers/`). Al cambiar de fondo, el cliente escribe el color
dominante en `wallpaper.state` (`dominant = [r, g, b]`) para el tinte de las
ventanas y el `accent-color` del portal.

## `dock` — dock de la shell (`malaysia-shell dock`; macOS `com.apple.dock`)

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
compositor lanza `malaysia-shell lock`. Un inhibidor `zwp_idle_inhibit_manager_v1`
(video en pantalla completa) detiene el reloj.

## `menubar` — barra de menús de la shell (macOS "Menu Bar")

| Opción | Tipo | Default | Estado | Consumida por |
|---|---|---|---|---|
| `menubar.autohide` | bool | `false` | pendiente | shell |
| `menubar.app_menus` | bool | `true` | **activa** | shell |

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
macOS. Habla con el compositor por D-Bus (`org.malaysia.Compositor1`); la app de
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

Como macOS, un archivo por dominio en `~/.config/malaysia/`:

```toml
# ~/.config/malaysia/appearance.domain
color_scheme = "dark"           # auto | dark | dark_tint | light
accent_color = [0.208, 0.518, 0.894]
contrast = false                # Increase contrast
reduced_motion = false          # Reduce motion
reduce_transparency = true      # Reduce transparency (fondos sólidos; false = blur futuro)
window_color = "neutral"        # neutral | accent | wallpaper
window_tint_scope = "windows"   # windows | desktop (tiñe menubar/dock)
traffic_lights = "auto"         # auto | standard | accent | wallpaper
```

```toml
# ~/.config/malaysia/display.domain
scale = 1.5                     # 1.0 .. 4.0
```

```toml
# ~/.config/malaysia/window.domain
decorations = "default"         # default | force_server | force_client
titlebar = true
double_click_titlebar = "zoom"  # fill | minimize | zoom | none
green_button = "focus"          # focus | zoom | fullscreen | none
focus = "click"                 # click | sloppy
resize_border = 10
resize_corner = 20
qt_server_decorations = true    # false para la barra nativa de Qt
```

```toml
# ~/.config/malaysia/screenshot.domain
show_pointer = false            # macOS lo oculta por defecto
show_thumbnail = true
target = "desktop"              # desktop | clipboard | preview | mail | messages | other
# location = "/home/yo/Capturas"  # para target = "other"
type = "png"                    # png | jpg | heif | tiff | pdf
include_date = true
disable_shadow = false
timer = 0                       # 0, 5 o 10
remember_selection = false
```

```toml
# ~/.config/malaysia/lock.domain
idle_timeout_seconds = 300      # 0 desactiva el auto-lock
```

```toml
# ~/.config/malaysia/dock.domain
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
# ~/.local/share/malaysia/wallpaper.domain
dir = "/run/current-system/sw/share/wallpapers/solid-colors"
scale = "fill"                  # fill | stretch | fit | center (palabras de macOS)
shuffle = false

# Selección actual por desktop (como com.apple.wallpaper):
[desktops]
default = "file:///run/current-system/sw/share/wallpapers/solid-colors/Solid Aqua Blue.png"
```

```toml
# ~/.config/malaysia/menubar.domain
autohide = false
app_menus = true                 # menú global de la app enfocada (dbusmenu)
```

```toml
# ~/.config/malaysia/effects.domain
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
# ~/.config/malaysia/debug.domain
damage_highlight = false
```

> Nota: dentro de cada archivo de dominio las secciones se escriben **sin el
> prefijo del dominio** (`[inner_border]` en `effects.domain`, no
> `[effects.inner_border]`); el archivo *es* el dominio.

El catálogo vive en `crates/malaysia-compositor/src/config.rs` como tipos `serde` tipados; cualquier
cambio en este documento debe reflejarse ahí y viceversa (ADR 0021).