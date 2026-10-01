# Glosario de nombres

Los nombres del proyecto siguen el reparto de macOS (AppKit, Core Graphics, Core
Animation, SkyLight). Cada entrada tiene su nombre, su identificador en código, su
equivalente en macOS y su rol.

| Nombre | Identificador en código | Equivalente en macOS | Rol |
|---|---|---|---|
| **CelestialKit** | `celestial-kit` | AppKit | Toolkit de la **shell** y de las **apps propias** de Yuki (Settings, ...): ventanas, titlebars, semáforos, widgets, texto. Lo usan la shell (ADR 0023) y las apps propias; **no el compositor**. |
| **Grade Graphics** | `grade-graphics` | Core Graphics + Core Text | Crate de gráficos compartido: los **shaders GLSL** (squircle SDF, hairline, inner line, sombra 9-slice) y sus parámetros. Lo usan el **compositor** y **CelestialKit**; lo que se comparte es el **GLSL**, no el código GL de Rust. Los shaders que se habían previsto para `yuki-design` viven aquí; `yuki-design` conserva los tokens de diseño (ADR 0011). |
| **Grade Animation** | `celestial_kit::animation` (módulo) | Core Animation | Módulo de animación **dentro de CelestialKit**; aún no es crate. Solo sale a crate propio si el compositor también lo necesita (p. ej. compartir curvas de animación). |
| **Infinity** | `infinity` (reservado) | SkyLight | **Nombre reservado** para un futuro protocolo privado entre la shell y el compositor. **No crear nada hoy**: se usan los protocolos Wayland estándar (ADR 0023). |

## Reparto de responsabilidades

- `celestial-kit` — la UI de la shell y las apps propias (≈ AppKit).
- `grade-graphics` — los shaders y parámetros compartidos con el compositor
  (≈ Core Graphics + Core Text). Es la pieza que mantiene "una sola vez" el
  squircle SDF y el dibujo de la titlebar.
- `yuki-design` — los tokens de diseño (radios, sombras, blur, colores, márgenes —
  ADR 0011). No contiene shaders.
- `yuki-scene` — layout de ventanas del compositor (ADR 0015).
- `yuki` — el compositor.