# 0046 — Tengen: renombrado del lenguaje visual

Estado: **aceptado** (2026-10-28).

## Contexto

El lenguaje visual del escritorio se llamaba **Neptunia**. A partir de esta
fecha pasa a llamarse **Tengen**. Los nombres de los subsistemas que lo rodean
no cambian:

- **Tengen** = el lenguaje visual en sí (equivalente a Aqua en macOS).
- **VowDesign** = los tokens compilados (`crates/vow-design`).
- **VowAppearance** = la apariencia activa resuelta (`crates/vow-appearance`).
- **BarrierKit** = el toolkit de la shell (`crates/barrier-kit`).

Solo el nombre del lenguaje visual y de su carpeta fuente cambian; colores,
radios, sombras, medidas y comportamientos son exactamente los mismos.

## Decisión

1. **Renombrado completo.** La tabla siguiente resume el mapeo:

   | Antes | Después |
   |---|---|
   | `crates/vow-design/neptunia/` | `crates/vow-design/tengen/` |
   | `generated/neptunia.slint` | `generated/tengen.slint` |
   | `generated/neptunia.css` | `generated/tengen.css` |
   | `generated/Neptunia.qml` | `generated/Tengen.qml` |
   | Tipo `NeptuniaMode` en Slint | `TengenMode` |
   | Tipo `NeptuniaAccent` en Slint | `TengenAccent` |
   | `$OUT_DIR/neptunia.rs` | `$OUT_DIR/tengen.rs` |
   | Temas de BarrierKit `neptunia-shell`, `neptunia-shell-light`, `neptunia-shell-dark`, `neptunia-lock` | `tengen-shell`, `tengen-shell-light`, `tengen-shell-dark`, `tengen-lock` |
   | `scripts/check-neptunia-generated.sh` | `scripts/check-tengen-generated.sh` |
   | `tools/sync-neptunia.sh` | `tools/sync-tengen.sh` |
   | Check `neptunia-generated` en `flake.nix` | `tengen-generated` |

2. **Sin alias de compatibilidad.** No se mantienen nombres viejos en el código
   (`type NeptuniaMode = TengenMode`), no se leen rutas antiguas y los scripts
   no generan salidas con el nombre anterior.

3. **Documentación histórica intacta.** Los ADRs anteriores a la 0046, los
   informes de bloques, la investigación y la auditoría conservan el nombre
   **Neptunia** porque son registro histórico. La ADR 0032 recibe solo una línea
   que remite a esta decisión; el resto de su texto no cambia.

## Consecuencias

- La fuente de diseño del escritorio se llama ahora **Tengen** en todo el
  código, los scripts, la documentación viva y los artefactos generados.
- No hay cambio visual: los tokens, colores y medidas son idénticos.
- Cualquier referencia externa a rutas o tipos antiguos debe actualizarse a
  mano.

## Referencias

- `crates/vow-design/tengen/` — carpeta fuente renombrada.
- `crates/vow-design/build.rs` — genera `$OUT_DIR/tengen.rs`.
- `docs/decisiones/0032-neptunia-como-fuente-de-diseno.md` — ADR original,
  ahora con una línea de remisión.
- `docs/arquitectura/tengen.md` — documentación viva del lenguaje visual.
