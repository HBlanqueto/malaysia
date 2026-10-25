# 0037 — Marco de ventanas dibujado por el compositor

Estado: **aceptado** (2026-10-24).

## Contexto

El ADR 0019 fijó *quién* dibuja la barra de título y el ADR 0020 añadió que
los efectos de superficie (borde interior, sombra, blur) se aplican a todas las
ventanas, independientemente de quién las decora. Hasta ahora Yuki solo dibujaba
la titlebar SSD y dejaba las esquinas rectas, sin sombra ni borde exterior.

Neptunia (ADR 0032) define una ventana con radio 16, sombra, inner border y,
para ventanas SSD, una titlebar de 32 px. El reto técnico es dibujar todo ese
marco compositor-side sin depender de buffers extra por ventana y sin romper el
render de ventanas CSD (GTK, wezterm), que deben recibir esquinas, sombra y
borde pero no titlebar.

## Decisión

1. **Un solo elemento de marco por ventana.** `RoundedTextureElement` envuelve
   la textura del cliente (o de la titlebar) y la dibuja con el shader
   `ROUNDED_CLIP`. El marco consta de:
   - sombra (`shadow.frag`) debajo de la ventana,
   - borde exterior (`border.frag`) alrededor del rectángulo redondeado,
   - línea interior / hairline,
   - contenido recortado a las esquinas redondeadas.
2. **Las esquinas redondeadas y el borde se aplican a todas las ventanas**,
   incluidas las CSD. GTK y Qt siguen sin recibir la titlebar de Yuki (ADR 0019),
   pero sí reciben el radio 16, la sombra y el inner border del compositor. Esto
   respeta la regla "efectos para todos" del ADR 0020.
3. **La titlebar solo aparece en ventanas SSD.** La caché de chrome
   (`ChromeCache`) genera una `GlesTexture` con el título y los semáforos y la
   empaqueta como `RoundedTextureElement`; las ventanas CSD usan la textura del
   cliente directamente.
4. **Los diálogos usan el mismo marco y se centran sobre la ventana padre.** Se
   añade `DIALOG` como estilo en `yuki-design`; el compositor detecta el rol
   `xdg_toplevel.parent` y coloca el diálogo centrado sobre el rectángulo del
   padre cuando se mapea.
5. **No hay buffer intermedio por ventana.** El clip redondeado se hace en el
   fragment shader usando `v_coords * size`, con la esquina superior-izquierda en
   (0,0) y abajo-derecha en `size`. El shader emite RGBA premultiplicado.
6. **Los valores de diseño siguen viniendo de `yuki-design`.** Radio, sombra,
   espesores de borde y colores se leen de `yuki-design::WINDOW` y
   `yuki-design::DIALOG`; no hay literales en el compositor.

## Consecuencias

- Las ventanas SSD y CSD comparten la misma estética de marco (esquinas,
  sombra, borde), pero solo las SSD llevan titlebar del compositor.
- Se elimina el render CPU de la titlebar anterior y se unifica el camino
  GLES para chrome, contenido y cursores.
- Los diálogos emergen centrados sobre su padre, como en macOS, y no en la
  esquina del output.
- Queda pendiente incluir el marco en la captura de ventana (Bloque 11) y
  ajustar el daño/damage tracker para el área de sombra.

## Referencias

- [`0019`](0019-politica-de-decoracion.md) — quién decora.
- [`0020`](0020-politica-de-diseno-y-unificacion.md) — efectos para todas las
  ventanas.
- [`0032`](0032-neptunia-como-fuente-de-diseno.md) — valores de Neptunia.
- [`../arquitectura/ventanas.md`](../arquitectura/ventanas.md) — titlebar y
  semáforos.
- `crates/grade-graphics/src/{shadow.frag,rounded_clip.frag,border.frag}` —
  shaders del marco.
- `crates/yuki/src/effects.rs` — `RoundedTextureElement`.
- `crates/yuki/src/chrome.rs` — generación de titlebar y elementos de chrome.
- `crates/yuki/src/window.rs` — colocación de diálogos.
- `crates/yuki-design/neptunia/components.json` — componente `window` y estilo
  `dialog`.
