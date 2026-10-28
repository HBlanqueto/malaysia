# Mockups de contrato

Un mockup de esta carpeta no es una imagen de referencia: es la especificación
que la app tiene que cumplir, y un test lo comprueba. Cada pantalla tiene:

| Archivo | Quién lo hace | Qué es |
| --- | --- | --- |
| `<pantalla>.html` | diseño (Claude) | El mockup. Solo usa variables de diseño. |
| `<pantalla>.layout.json` | `scripts/mockup-extract.py` | Rectángulos y estilos de cada nodo marcado, por variante. Se commitea. |
| `<pantalla>.<variante>.png` | `scripts/mockup-extract.py` | Captura de referencia a escala 1, sin efectos del compositor. Se commitea. |

## Reglas del HTML

- Colores, radios, medidas y tipografía salen de
  `crates/vow-design/tengen/generated/tengen.css`. Las variables de
  componentes (`--settings-card-width`, `--window-sidebar-island-traffic-x`…)
  se nombran con la ruta de `components.json` sin `components.` y con guiones;
  los estilos de texto, `--type-<estilo>-size|line-height|weight|tracking`.
- `_provisional-tokens.css` cubre lo que `tengen.css` todavía no exporta.
  Desaparece cuando lo exporte `default.nix`.
- La ventana empieza en (0, 0) y mide lo que diga `components.json`. La app
  dibuja un rectángulo cuadrado: esquinas, sombra, borde exterior e inner
  border son del compositor (`data-layer="compositor"`, solo visible con
  `?compositor=1`).
- Atributos:
  - `data-id`: identificador estable y único; es lo que busca el test.
  - `data-kit`: componente de `barrier-kit` (`Sidebar`, `ToolGroup`…).
  - `data-app`: función de la app que lo dibuja (`theme_card`, `row_card`…).
  - `data-state`, `data-variant`: estado y variante que debe tener.
  - `data-diff="ignore"`: su contenido depende del sistema (fondo de pantalla,
    usuario); se compara su rectángulo, no sus píxeles.
- Las variantes (tema, búsqueda…) se declaran en `<script id="mockup-variants">`
  y se eligen con `?v=<nombre>`. El HTML no tiene más lógica que esa.

## Flujo

1. Diseño entrega o cambia el `.html`.
2. `nix run .#mockup-extract -- docs/mockups/<app>/<pantalla>.html` regenera
   `layout.json` y los PNG. `./dev.sh check` corre el mismo script con
   `--check` y falla si quedaron viejos.
3. El test de la app renderiza cada variante con `freya-testing` a la misma
   medida y compara nodo a nodo (±1 px) y por píxel.
