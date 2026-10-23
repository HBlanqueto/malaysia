# Glosario de nombres

Los nombres del proyecto siguen el reparto de macOS (AppKit, Core Graphics, Core
Animation, SkyLight). Cada entrada tiene su nombre, su identificador en código, su
equivalente en macOS y su rol.

| Nombre | Identificador en código | Equivalente en macOS | Rol |
|---|---|---|---|
| **CelestialKit** | `celestial-kit` | AppKit | Toolkit de la **shell** y de las **apps propias** de Yuki (Settings, ...): ventanas, titlebars, semáforos, widgets, texto. Se construye sobre los crates de núcleo de Freya `=0.4.3`, con runner Wayland propio y tokens de `yuki-design` (ADR 0034). Lo usan la shell (ADR 0023) y las apps propias; **no el compositor**. |
| **Grade Graphics** | `grade-graphics` | Core Graphics + Core Text | Crate de los **shaders GLSL** del compositor (squircle SDF, hairline, inner line, sombra 9-slice) y sus parámetros. El kit iguala el aspecto con los mismos tokens de `yuki-design`, sin usar estos shaders para dibujar su contenido (ADR 0033). |
| **Grade Animation** | `celestial-kit::animation` | Core Animation | Módulo de animación de CelestialKit sobre `freya-animation`, con un resorte propio (duración + rebote) escrito desde cero (ADR 0034). Lo usan la shell y las apps propias; el compositor no lo necesita. |
| **Infinity** | `infinity` | SkyLight | Protocolo privado app↔compositor, equivalente a `otto-surface-style`: esquinas, sombras, desenfoque y animaciones. El kit lo trata como **opcional** hasta que el compositor lo implemente (ADR 0034). |

## Reparto de responsabilidades

- `celestial-kit` — la UI de la shell y las apps propias (≈ AppKit), sobre los
  crates de núcleo de Freya y con Skia en los procesos cliente. Incluye el
  módulo Grade Animation (curvas, resortes, transacciones).
- `grade-graphics` — los shaders GLSL del compositor (≈ Core Graphics + Core
  Text).
- `infinity` — protocolo privado app↔compositor (≈ SkyLight), opcional.
- `yuki-design` — los tokens de diseño (radios, sombras, blur, colores, márgenes —
  ADR 0011). No contiene shaders.
- `yuki-appearance` — resuelve `appearance.domain` + `wallpaper.state` para el
  compositor, la shell y el portal.
- `yuki-scene` — layout de ventanas del compositor (ADR 0015).
- `yuki` — el compositor.