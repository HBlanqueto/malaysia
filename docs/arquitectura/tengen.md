# Tengen en malaysia: datos, generación y comprobaciones

Tengen es el sistema de diseño del escritorio (ADR 0032). En malaysia ya no se
lee de `~/dotfiles`: vive **dentro del repo**, en
`crates/vow-design/tengen/`, y `build.rs` convierte sus JSON en las
constantes Rust de `vow-design`. Siguen siendo constantes de compilación
(ADR 0011); lo que cambia es de dónde salen. `vow-appearance` aplica esas
constantes a `appearance.domain` y `wallpaper.state` para el compositor, la
shell y el portal.

## Fuente de verdad

```
crates/vow-design/tengen/
├── tokens.json        colores por tema, radios, espacios, tamaños, duración,
│                      tipografía, sombras y acentos
├── components.json    medidas y estados de los componentes, shell y ssd-titlebar
├── default.nix        generador Slint/CSS/QML (solo builtins, sin nixpkgs)
├── generated/         tengen.slint, tengen.css, Tengen.qml
├── demo.slint         galería para slint-viewer/SlintPad
├── SOURCE.md          procedencia, regla de la copia y cómo regenerar
└── README.md          catálogo de valores y guía de uso
```

`SOURCE.md` registra el commit de dotfiles del que se copió la carpeta. **Esta
copia es la fuente para malaysia**: los cambios se hacen aquí y, si se quieren,
se devuelven a dotfiles a mano. Ni Cargo ni Nix leen `~/dotfiles` ni otro repo.

`tools/sync-tengen.sh <ruta>` trae cambios de una copia externa en un solo
sentido, muestra el diff y actualiza `SOURCE.md`. No se ejecuta en el build ni
en CI; es una herramienta manual.

## Generación de constantes

`crates/vow-design/build.rs`:

1. Lee `tengen/tokens.json` y `tengen/components.json` con `serde_json`
   (solo `build-dependencies`).
2. Escribe `$OUT_DIR/tengen.rs` con paletas, radios, tipografía, sombras,
   semáforos, factores de tinte, medidas de shell y `ssd-titlebar`.
3. Emite `cargo:rerun-if-changed` para los dos JSON.
4. Si falta un campo que `vow-design` necesita, falla indicando el campo y el
   archivo.

`crates/vow-design/src/lib.rs` conserva los tipos (`Rgba`, `Palette`,
`Titlebar`, `ShellPalette`, `LockPalette`, …) y la API pública, y termina con:

```rust
include!(concat!(env!("OUT_DIR"), "/tengen.rs"));
```

Chrome, la shell y `barrier-kit` siguen leyendo los mismos nombres. Los
literales que antes estaban copiados a mano ya no viven en `src/`.

## Valores que manda Tengen

- **Temas**: `light`, `dark` y `dark-tint`, con `LIGHT`, `DARK` y `DARK_TINT`.
- **Semáforos**: `traffic-close/min/zoom` son `#ff5f57`, `#febc2e` y `#28c840`
  en los tres temas; `traffic-inactive`, `traffic-rim` (aro de 0,5 px) y
  `traffic-glyph` están en `tokens.json`.
- **Titlebar**: `titlebar-bg`/`titlebar-bg-inactive` (`bg-window`/`bg-sunken`),
  `titlebar-title`/`titlebar-title-inactive` (`ink-strong`/`ink-muted`) y
  `titlebar-line` (`line`).
- **Alturas**: `size.titlebar-h = 44` para apps propias; `size.titlebar-ssd-h =
  32` para la barra que dibuja el compositor en apps de terceros con decoración
  del servidor. El componente `ssd-titlebar` describe el título centrado,
  reserva simétrica, recorte con `…`, una sola línea y esquinas `radius.lg`.
- **Tipografía**: `font.families.sans` empieza por Geist; `titlebar-title` es
  13/17/600; los pesos usados son 400, 500, 600, 650 y 700.
- **Sombras**: `shadow.layers.{light,dark,dark-tint}` guardan las dos capas
  ambient/contact de cada tema, con su color y opacidad; `shadow.unfocused`
  fija los factores de ventana inactiva (blur ×0.45, offset ×0.4, opacidad
  ×0.55). Los textos CSS siguen existiendo para quien los consuma.
- **Shell**: `components.shell` define menubar, dock y lock; `shell-bar` es el
  fondo translúcido (`#rrggbbaa` en JSON/Slint/CSS, `#aarrggbb` en QML).

## Shell y wallpaper

- `ShellPalette` (`SHELL_LIGHT`, `SHELL_DARK`, `SHELL_DARK_TINT`) se elige por
  `appearance.color_scheme`; `auto` resuelve a `light` con
  `ColorScheme::resolved()`, la misma fuente que publica el portal.
- `window_tint_scope = "desktop"` mezcla barra y dock con el dominante del
  wallpaper (20 % claro / 26 % oscuro), leyendo `wallpaper.state`.
- `malaysia-shell wallpaper` decodifica PNG/JPEG, calcula el dominante (rejilla
  ~256×256, histograma 4 bits ponderado por croma, grises descartados, HSL
  45–65 %) y escribe `~/.local/share/malaysia/wallpaper.state`
  (`dominant = [r, g, b]`). El compositor ya no decodifica imágenes: lee ese
  archivo y vigila su mtime junto con `appearance.domain` para reconstruir
  `ChromeTheme` en caliente.
- El portal publica ese mismo dominante como `accent-color` en modo wallpaper,
  y vigila `appearance.domain` y `wallpaper.state`.

## Comprobaciones

En `./dev.sh check`:

- `scripts/check-design-tokens.sh`: ningún color o medida de UI fuera de
  `vow-design` en `crates/malaysia-compositor`, `crates/malaysia-shell` y `barrier-kit`; se
  saltan los módulos de test y se permiten formatos de buffer.
- `scripts/check-tengen-generated.sh`: compara `generated/tengen.slint`,
  `tengen.css` y `Tengen.qml` con lo que produce `default.nix`.
- `demo.slint` se compila con `slint-compiler` si está en el devShell; si no,
  se avisa y se omite.

En el flake:

- `checks.<system>.tengen-generated`: las tres salidas generadas.
- `checks.<system>.design-tokens`: el grep anterior.
- `checks.<system>.slint-demo`: renderiza `demo.slint` con `slint-viewer` bajo
  Xvfb (compila el archivo entero).

## Cambiar un valor

1. Edita `crates/vow-design/tengen/tokens.json` o `components.json`.
2. Regenera las salidas si hace falta:

   ```sh
   cd crates/vow-design/tengen
   nix eval --raw -f . slint > generated/tengen.slint
   nix eval --raw -f . css   > generated/tengen.css
   nix eval --raw -f . qml   > generated/Tengen.qml
   ```

3. Ejecuta `./dev.sh check` (y, si toca, `nix build .#checks…`).
4. No edites `crates/vow-design/src/` para cambiar un valor.

## Relación

- [`interfaz.md`](interfaz.md) — el mapa visual completo.
- [`ventanas.md`](ventanas.md) — titlebar y semáforos.
- [`wallpaper.md`](wallpaper.md) — el componente del fondo y su estado.
- ADR 0011 — los valores son constantes Rust.
- ADR 0032 — Tengen como fuente de diseño.
- `crates/vow-design/tengen/README.md` — catálogo y uso por proyecto.
