# Informe: acento y tinte wallpaper con contraste WCAG

## Resumen

Se implementó la derivación del acento y el tinte de ventana a partir del color
dominante del fondo de pantalla, garantizando un contraste WCAG mínimo contra
los fondos de interfaz. El trabajo toca `barrierkit` (helpers de color,
`vow-appearance` y tokens de diseño) y `malaysia` (extracción del color
dominante con vecindario 3×3×3 e integración visual).

## Commits

### barrierkit

- `main`: `10a067e` – `feat(apariencia): acento y tinte wallpaper con contraste WCAG`
- Rama `for-malaysia`: `2de29a6` – mismo commit rebasado sobre `899c40c` para
  mantener la API de `barrier-kit` estable en malaysia.

### malaysia

- `531f9a6` – `feat(wallpaper): vecindario 3×3×3 en dominant() y rev de vow-appearance`
- `9a5c717` – `fix(wallpaper): compila el binario y respeta límites de textura`

## Cambios en barrierkit

- **`crates/vow-design/src/lib.rs`**: nuevos helpers en `Rgba`:
  - `relative_luminance()` (sRGB → lineal ponderado).
  - `contrast_ratio()` (WCAG 2.1).
  - `to_hsl()` / `from_hsl()`.
- **`crates/barrier-kit/src/menu.rs` y `theme.rs`**: se eliminaron las copias
  locales de luminancia/contraste y se usan los helpers de `vow-design`.
- **`crates/vow-design/tengen/components.json`**: tokens nuevos para el modo
  wallpaper (`WINDOW_TINT_WALLPAPER`, `ACCENT_LIGHTNESS_MIN/MAX`,
  `SATURATION_BOOST_THRESHOLD/FACTOR`, `WALLPAPER_ON_ACCENT_MIN`).
- **`crates/vow-design/build.rs`**: generación de constantes a partir de los
  tokens anteriores.
- **`crates/vow-appearance/src/lib.rs`**: `resolve()` aplica el tinte wallpaper
  a ventanas, paneles e iluminaciones, y usa `wallpaper_accent()` para derivar
  el acento del color dominante ajustando `L` para cumplir el contraste contra
  `bg-window` y `on-accent`.
- **`crates/vow-appearance/src/lib.rs`**: tests de regresión y contraste.

## Cambios en malaysia

- **`crates/malaysia-wallpaper/src/image.rs`**:
  - `dominant()` ahora suma el vecindario 3×3×3 de cubetas antes de elegir la
    ganadora, evitando que un tono repartido entre fronteras pierda contra un
    color minoritario concentrado.
  - El color resultante es la media ponderada por croma del vecindario.
  - Se usan `Rgba::to_hsl`/`from_hsl` de `vow-design`; se eliminaron
    `rgb_to_hsl`/`hsl_to_rgb` locales.
  - Nuevo test de frontera `a_green_gradient_across_bucket_boundaries_beats_a_concentrated_red`.
  - Ajustados los valores esperados del test del default embebido.
- **`crates/malaysia-wallpaper/src/render.rs`**:
  - Corregido error de move de `limits` y macro `warn` no importado.
  - Se expone `max_texture_dimension_2d` y se escala la imagen antes de subirla
    a la GPU.
- **`crates/malaysia-wallpaper/src/image.rs`**:
  - Nuevo `DecodedImage::downscale_to_fit()` con promediado 2×2 en luz lineal.
- **`crates/malaysia-wallpaper/src/wayland.rs`**:
  - Escala el buffer de salida si excede el límite de textura del adapter.
- **`crates/malaysia-compositor/src/capture.rs`**:
  - Directorio de capturas cambiado de `malaysia` a `malaysia-compositor`.
- **`Cargo.toml`**: todos los crates de barrierkit apuntan al mismo rev
  `2de29a6` de la rama `for-malaysia`.
- **`crates/malaysia-compositor/src/chrome.rs`**,
  **`crates/malaysia-settings/src/cards.rs`**,
  **`crates/xdg-desktop-portal-malaysia/src/settings.rs`**: tests actualizados
  para reflejar que el acento wallpaper ya no es el color dominante crudo, sino
  el derivado por contraste.
- **`docs/investigacion/chromaleon.md`**: nota de investigación sobre la
  extensión ChromaLeon (GPL-3.0-or-later); solo ideas, sin código copiado.
- **`dev.sh`**: mejora de robustez en `demo`, `lab` y `wallpaper-check` para
  detectar si `malaysia-wallpaper` muere.

## Verificación

### barrierkit

```bash
cargo test -p vow-design
cargo test -p vow-appearance
cargo clippy --workspace
./scripts/check-client-only-deps.sh
```

Todo verde.

### malaysia

```bash
nix develop -c cargo test --workspace
nix develop -c cargo clippy --workspace -- -D warnings
nix develop -c ./scripts/check-client-only-deps.sh
nix develop -c ./dev.sh wallpaper-check
```

Todo verde.

### Prueba visual automatizada

Se escribió `/tmp/opencode/visual-wallpaper-check.sh`, que arranca el
compositor anidado con `--winit`, lanza `malaysia-wallpaper`, `menubar`, `dock`
y una ventana `wezterm`, cambia el fondo a un amarillo sólido (`#F5D020`) con
modo wallpaper, captura la pantalla con `SIGUSR1` y muestrea la barra de menú y
el dock.

Resultado de la última ejecución:

```
menubar sample (r,g,b): 143,144,138
dock sample (r,g,b): 218,220,200
wallpaper (r,g,b): 245,208,32
PASS: menubar (143, 144, 138), dock (218, 220, 200) are tinted by wallpaper (245, 208, 32)
visual-check: PASS
```

La cromía del cromo cambia de azul (acento por defecto) a tonos cálidos
amarillentos, confirmando que el tinte wallpaper se aplica en la sesión real.

## Limitaciones / riesgos

- Amarillos puros con `L = 0.65` no pueden cumplir simultáneamente el contraste
  de ventana ≥ 3 y el de texto blanco sobre acento ≥ 3 en esquemas oscuros. El
  algoritmo prioriza el contraste de ventana y acepta el déficit sobre acento.
- La prueba visual se automatizó solo con fondo amarillo; no se ejecutó con
  fondos púrpura/azul en modo claro. La lógica está cubierta por los tests
  unitarios de `vow-appearance`.
- Hay un test preexistente en `barrier-kit/tests/traffic.rs` que falla fuera de
  estos cambios (`moving_the_pointer_away_after_a_press_clears_hover_and_press`);
  no se tocó.
- `malaysia` depende de la rama `for-malaysia` de `barrierkit`, no de `main`,
  porque `main` tiene refactors de API de `barrier-kit` incompatibles. Cuando
  `malaysia` esté listo para subir de rev, habrá que migrar esos usos.
