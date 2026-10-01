# 0021 — Superficie de configuración y control: valores, comandos e IPC

Estado: **aceptado.** Define cómo Yuki expone sus opciones al exterior y cómo, más
adelante, una app de ajustes y una shell podrán interactuar con el compositor.

## Contexto

Yuki necesita un diseño unificado que se pueda **configurar en caliente**: activar o
desactivar efectos (inner border, sombra, blur — ADR 0020), cambiar la apariencia
(dark/light, accent), la política de decoración, etc. El modelo de referencia es
macOS (preferencias del sistema desde *System Settings* **y** por comando `defaults`,
ambas escribiendo al mismo almacén) y Otto (archivo `config.toml` + app
`otto-settings` por D-Bus + comando `otto-msg` sobre `org.otto.Shell1`).

**Vocabulario.** Las opciones usan **los mismos nombres que macOS** donde existe un
equivalente (ADR 0021 bis): las secciones imitan System Settings (`appearance`,
`window`, `dock`, `menubar`) y las claves imitan `com.apple.dock` y demás
(`autohide`, `orientation`, `tilesize`, `mineffect`, `show-process-indicators`,
...), con los mismos valores (`light`/`dark`/`auto`, `genie`/`scale`,
`fill`/`minimize`/`zoom`). El comando de control se llama **`domain`** (el
nombre "domain" es una referencia a la Expansión de Dominio de *Jujutsu Kaisen*;
solo esa palabra) y repite la sintaxis de `defaults`: `read`/`write`/`delete`/
`rename`/`find`/`domains` con flags de tipo `-bool`/`-int`/`-float`/`-string`.

Este ADR fija la **superficie** (qué opciones existen y cómo se cambian) y la
**arquitectura** de los canales, de forma que una futura app de ajustes y la shell
hablen con el compositor por un único mecanismo, no por variables sueltas.

## Decisión

### 1. Un catálogo único de opciones

El catálogo canónico vive en `docs/configuracion/opciones.md`. Cada opción tiene:
nombre canónico (ruta de puntos, p.ej. `effects.blur.enabled`), tipo, valor por
defecto y qué componente la consume. Las opciones se agrupan por dominio:
`appearance`, `decoration`, `behavior`, `effects`, `shell`, `debug`.

### 2. Un único almacén en el compositor

El compositor mantiene una estructura `Config` (`crates/yuki/src/config.rs`) con
todas las opciones del catálogo, con **defaults** y **carga desde un `.domain` por
dominio** — como macOS y sus plists por dominio (`com.apple.dock`,
`com.apple.menubar`, ...). Los archivos llevan la extensión propia **`.domain`** y
se reparten como macOS: preferencias en `~/.config/yuki/`
(`~/Library/Preferences`), estado/`wallpaper` en `~/.local/share/yuki/`
(`~/Library/Application Support`): `appearance.domain`, `window.domain`,
`dock.domain`, `menubar.domain`, `shell.domain`, `effects.domain`, `debug.domain`
y `wallpaper.domain` (este último en el directorio de datos). Override con
`YUKI_CONFIG_DIR`/`YUKI_DATA_DIR`. Hoy la carga es solo de arranque
(`Config::load()`). Más adelante el almacén será **mutable y compartido** (ver
punto 4), de modo que el archivo, el comando y la app escriban al mismo sitio.

### 3. Canales de escritura

En orden de madurez:

1. **Archivos `.domain` por dominio** (hoy). `~/.config/yuki/<dominio>.domain` (y
   `~/.local/share/yuki/wallpaper.domain`); cada clave del catálogo es una clave
   de su archivo. Parcial: las claves ausentes usan el default, y un dominio sin
   archivo usa su default completo.
2. **Comando `domain`** (planificado). Un binario que envía comandos al
   compositor por D-Bus, con la sintaxis de `defaults` de macOS:
   - `domain read <domain> <key>`
   - `domain write <domain> <key> <valor> [-bool | -int | -float | -string]`
   - `domain delete <domain> <key>`
   - `domain rename <domain> <old> <new>`
   - `domain find <palabra>` · `domain domains`
   El "dominio" es la sección de la opción (`appearance`, `window`, `dock`,
   `menubar`, `effects`, `debug`). La app de ajustes y la shell usarán **el
   mismo** mecanismo D-Bus; el comando es solo otra piel sobre él.
3. **App de ajustes** (planificada). Un cliente que cambia opciones en caliente por
   D-Bus y las persiste al archivo TOML (como `otto-settings`).

### 4. El bus D-Bus (contrato)

El compositor expone una interfaz, propuesta `org.yuki.Compositor1` (nombre final a
confirmar):

- **Propiedades** por opción del catálogo (lectura) — `org.freedesktop.DBus.Properties`.
- **Método `SetOption(name, value)`** — escribe una opción por su nombre canónico.
- **Método `GetOption(name)`** — lee una opción.
- **Método `ReloadConfig()`** — recarga el archivo TOML.
- **Señal `OptionChanged(name, value)`** — emite cambios para que la shell y la app
  se mantengan al día.

La shell (`yuki-shell`, ADR 0025) también escucha `OptionChanged` y lee
`appearance.color_scheme`/`accent_color`/`dock.*`/`menubar.*` del mismo almacén,
garantizando que compositor, shell y app nunca se desincronicen.

### 5. Persistencia

El archivo TOML es la persistencia. La app de ajustes escribe al archivo; el
compositor recarga (`ReloadConfig`) o recibe `SetOption`. El comando puede ser
transitorio (no persistir) o persistir según la opción.

## Consecuencias

- Las opciones están **documentadas en un solo sitio** (`docs/configuracion/opciones.md`)
  y tipadas en `config.rs`; el archivo, el comando y la app comparten esos tipos.
- La política de diseño (ADR 0020) se hace **ajustable en caliente** sin tocar
  código: `window.decorations`, `effects.*`, `appearance.color_scheme`.
- Lo ya implementado: `Config` + carga TOML + la política de decoración leyendo
  `window.decorations` (los valores `force_server`/`force_client` ya funcionan).
- Lo pendiente (fuera de este ADR): el bus D-Bus, `domain`, la app de ajustes y
  la persistencia desde la app. El contrato de la interfaz queda fijado aquí para
  que esas piezas se construyan contra él desde el principio.
- `docs/configuracion/opciones.md` y `AGENTS.md` referencian este ADR.