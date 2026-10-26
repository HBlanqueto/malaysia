# La interfaz de MalaysiaDreams, de un vistazo

Este documento reúne **todo lo visual y de interacción** del escritorio: el
sistema de diseño, la shell, las ventanas, las capas, las capturas, el cursor y
la configuración que los cambia. Los detalles de cada pieza viven en los
documentos enlazados; aquí está el mapa completo y el estado actual.

## Principios

1. **Neptunia es el diseño** (ADR 0032). Los colores, radios, espacios,
   tipografía, sombras y medidas de componentes salen de
   `crates/vow-design/neptunia/` (`tokens.json` + `components.json`);
   `build.rs` genera las constantes de `vow-design`. Nadie inventa un color o
   una medida en el código. Detalle en [`neptunia.md`](neptunia.md).
2. **Chrome separado, efectos para todos** (ADR 0020). GTK/Libadwaita conservan
   su headerbar; el resto de apps reciben el chrome del compositor. Los
   efectos de superficie (borde interior, sombra, blur) son del compositor y
   se aplicarán a todas las ventanas.
3. **La shell es nativa y separada** (ADR 0023/0025). Un proceso `malaysia-shell`
   por componente; el compositor compone, coloca, da input y aplica efectos.
4. **Skia solo en clientes** (ADR 0034). La shell y las apps propias dibujan
   toda su UI con `barrier-kit` (Skia/Freya); el compositor sigue con GLES,
   `grade-graphics` y `cosmic-text`, y nunca enlaza Skia ni `freya-*`.
5. **macOS como referencia de UX**, no de código. Otto se lee como ideas; si se
   copia código MIT suyo, se actualiza `NOTICE`.
6. **El diseño se aprueba en Neptunia y se implementa en MalaysiaDreams.** Los cambios
   visuales se prueban en `demo.slint`/`slint-viewer` (o en los lienzos de
   Neptunia) y se vuelcan a los JSON; después se portan a `vow-design` y al
   toolkit. No se inventan pantallas directamente en Rust.

## Reparto de dibujo

| Quién | Qué dibuja | Cómo |
|---|---|---|
| Shell (`malaysia-shell wallpaper/menubar/dock/lock/launcher`) | Toda su UI, en su propio proceso | BarrierKit (Freya + Skia) sobre su superficie |
| Apps propias con `Decorations::Client` (por defecto) | Su UI, **su titlebar de 44 px y sus semáforos** | BarrierKit + tokens de `vow-design` |
| Apps propias con `Decorations::Server` | Su UI; MalaysiaDreams dibuja la titlebar SSD de 32 px | BarrierKit para el contenido; compositor para el chrome |
| Apps ajenas que piden SSD | Su contenido | El compositor les añade la titlebar SSD (`chrome.rs`) |
| Compositor (MalaysiaDreams) | Composición, sombras, esquinas, desenfoque y animaciones de ventana | GLES + `grade-graphics`, como WindowServer |

La shell nunca usa la titlebar del compositor. Las apps propias eligen: Client
dibuja su barra de 44 px; Server acepta la de MalaysiaDreams de 32 px. El kit
pedirá los efectos del compositor por Infinity cuando esté disponible; hasta
entonces el compositor los aplica igualmente a todas las superficies.

### Cuándo usar cada modo

| Modo | Barra | Cuándo |
|---|---|---|
| `Decorations::Client` | BarrierKit, 44 px, con toolbar | Apps con toolbar o sidebar, como Ajustes |
| `Decorations::Server` | MalaysiaDreams, 32 px | Utilidades simples, sin toolbar |

## Capas y orden de composición

```
Overlay   launcher, capturas, popups de la shell
   ▲
Top       menubar (arriba) · dock (abajo) · notificaciones (futuro)
   ▲
Windows   superficie del cliente + titlebar SSD + popups xdg
   ▲
Bottom    superficies Bottom de layer-shell (futuro)
   ▲
Background wallpaper (malaysia-shell wallpaper)
```

Las zonas exclusivas de la menubar y el dock recortan el **workspace** donde se
colocan y maximizan las ventanas. El lock, cuando está activo, sustituye todo:
solo se dibujan sus superficies (y el cursor).

## Sistema de diseño: Neptunia

- **Temas**: `light` (por defecto), `dark` y `dark-tint` en los tokens.
  `vow-design` implementa `LIGHT`, `DARK` y `DARK_TINT`; el valor
  `appearance.color_scheme = "dark_tint"` selecciona el último.
- **Forma**: radio de ventana 16, semáforos de 12 con radio 4, titlebar de
  cliente **32 px** arriba. Los 44 px de `components.window` son la ventana de
  las apps propias de Neptunia, no el chrome de terceros.
- **Tipografía**: Geist; el título usa la escala `headline` (13/600) y el
  cuerpo de la shell es `body` (13 px, 400), 11–13 px según el componente.
- **Sombras**: los valores de Neptunia están en `SHADOW` (claro) y
  `SHADOW_DARK` (oscuro); el compositor las dibuja con shader detrás de cada
  ventana y recorta todas las superficies (SSD, CSD y subsuperficies) con el
  radio 16.
- **Modos de color de ventana** (`appearance.window_color`):
  - `neutral`: paleta base, semáforos estándar fijos.
  - `accent`: tinte del acento (6 % claro / 8 % oscuro), semáforos derivados.
  - `wallpaper`: tinte del color dominante del wallpaper (7 % / 9 %), semáforos
    derivados.
  - `traffic_lights` puede forzar `standard`, `accent` o `wallpaper`.
  - `window_tint_scope = "desktop"` tiñe menubar y dock con el dominante
    (20 % claro / 26 % oscuro).

## Shell: un proceso por componente

| Componente | Proceso | Capa | Estado |
|---|---|---|---|
| Wallpaper | `malaysia-shell wallpaper` | Background | hecho; EGL/dmabuf, vigila el store |
| Menubar | `malaysia-shell menubar` | Top, 28 px | hecho; app activa + reloj, paleta clara Neptunia |
| Dock | `malaysia-shell dock` | Top, abajo | hecho; lista de ventanas, tiles y activar |
| Lock | `malaysia-shell lock` | lock surfaces | hecho; PAM, auto-lock, recuperación |
| Launcher | `malaysia-shell launcher` | Overlay | hecho; búsqueda, ranking, iconos y activación |
| Notificaciones | futuro | Top/Overlay | Bloque 12b |

Los componentes leen preferencias de `~/.config/malaysia/` y estado de
`~/.local/share/malaysia/`; no escriben su propia configuración. La menubar y el
dock comparten la `ShellPalette` de Neptunia (`SHELL_LIGHT`, `SHELL_DARK`,
`SHELL_DARK_TINT`), elegida por `appearance.color_scheme`. Ver
[`malaysia-y-shell.md`](malaysia-y-shell.md), [`modelo-de-procesos.md`](modelo-de-procesos.md),
[`wallpaper.md`](wallpaper.md), [`menubar.md`](menubar.md), [`dock.md`](dock.md)
y [`lock.md`](lock.md).

## Ventanas y chrome

- **Apps propias (ADR 0035)**: su `app_id` empieza por
  `io.github.hblanqueto.malaysia.` y piden CSD explícito antes del primer commit.
  MalaysiaDreams nunca les añade su titlebar, ni con `window.decorations = "force_server"`.
  El prefijo solo decide si se dibuja una barra; no es seguridad. Infinity lo
  sustituirá cuando exista.
- **SSD**: titlebar de 32 px arriba, fondo de la paleta, hairline, tres
  semáforos y título centrado. Cerrar / minimizar / maximizar funcionan;
  arrastrar desde la barra mueve y los bordes redimensionan.
- **CSD**: GTK/Libadwaita, wezterm y apps que piden client-side conservan su
  chrome; MalaysiaDreams no dibuja barra sobre ellas. `window.decorations =
  "force_server"` permite forzar la barra de MalaysiaDreams (con el riesgo de duplicar
  chrome si la app ya dibuja la suya).
- **Foco y apilado**: al enfocar se eleva la ventana; el render va de arriba a
  abajo, así la barra de la ventana enfocada es la visible.
- **Maximizar**: guarda el rectángulo, ocupa el workspace, envía el estado
  `maximized`; repetir restaura.
- **Efectos**: borde interior, sombra y blur están en el catálogo y en
  `vow-design`; el pintor de esquinas/sombra aún no está conectado.
- Detalle completo en [`ventanas.md`](ventanas.md).

## Capturas de pantalla

`Shift+Super+3` (pantalla), `Shift+Super+4` (región; `Espacio` para ventana,
`Esc` cancela), `PrintScreen` (ventana enfocada). Overlay con atenuación y borde
blanco; `screenshot.domain` decide puntero, destino, formato, fecha, miniatura,
temporizador y semáforo. `target = "clipboard"` copia `image/png`; el resto de
opciones están reservadas. Detalle en [`capturas.md`](capturas.md).

## Launcher

`Super+Space` abre o cierra `malaysia-shell launcher` (capa Overlay exclusiva,
centrada). Búsqueda de apps `.desktop` con ranking por prefijo, palabra,
subcadena y palabras clave; `↑`/`↓` navegan, `Enter` o clic lanzan con token de
`xdg-activation`, `Esc` cierra. Detalle en
[`../bloques/13-chrome-y-launcher.md`](../bloques/13-chrome-y-launcher.md).

## Cursor, input y popups

- Cursor `wp_cursor_shape_v1` (35 formas), tema Xcursor, HiDPI y shake-to-locate
  (crece a 3× con un juego de alta resolución, no amplía el bitmap normal).
- Move/resize con bandas de borde de 8 px, popups xdg con grabs, cierre en
  cascada y `Esc`, pointer constraints y relative-pointer.
- El puntero no entra en la banda de la titlebar como superficie del cliente:
  el compositor la trata como chrome.
- El launcher es una capa `Overlay` exclusiva: al mapearse recibe el foco de
  teclado y lo conserva; al cerrarse lo devuelve a la ventana anterior. Las
  capas `OnDemand` lo toman al pulsarlas y lo pierden al hacer clic fuera.

## Configuración y portal

El catálogo canónico es [`../configuracion/opciones.md`](../configuracion/opciones.md).
Los dominios que afectan a la interfaz:

- `appearance.domain`: `color_scheme`, `accent_color`, `window_color`,
  `traffic_lights`, `window_tint_scope`, `contrast`, `reduced_motion`.
- `window.domain`: `decorations`, `titlebar`, `double_click_titlebar`, `focus`,
  `resize_border` (estado de la app activa en el data dir).
- `dock.domain` / `menubar.domain`: catálogo de la shell.
- `screenshot.domain`: capturas.
- `effects.domain`: borde interior, sombra, blur.

El portal `xdg-desktop-portal-malaysia` publica `org.freedesktop.appearance`
(`color-scheme`, `accent-color`, `contrast`, `reduced-motion`) desde
`appearance.domain`. El tema de la titlebar se reconstruye **en vivo** cuando
cambian `appearance.domain` o `wallpaper.state`, sin reiniciar (el portal
también notifica a las apps).

## Lo que falta (UI)

- **Esquinas redondeadas** de ventana (16), **sombra** y **glifos de hover** en
  los semáforos.
- **Doble clic** en la titlebar para zoom, y titlebar en la captura de ventana.
- **Iconos** reales en el dock (hoy títulos) y menús globales de la menubar.
- **Notificaciones** y el resto del Bloque 12b.

## Verificación

```sh
./dev.sh demo          # wallpaper, menubar, dock y ventanas
./dev.sh titlebar      # cierra/minimiza/maximiza por el centro de cada botón
./dev.sh check         # fmt, clippy y tests
MALAYSIA_SCALE=1.5 ./dev.sh demo
```

Las capturas de depuración (`SIGUSR1`) y las capturas de usuario
(`Shift+Super+3/4`) sirven para comprobar el resultado visual. Ver
[`capturas.md`](capturas.md) y [`ventanas.md`](ventanas.md).
