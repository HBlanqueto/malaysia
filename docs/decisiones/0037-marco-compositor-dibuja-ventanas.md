# 0037 — Marco de ventanas dibujado por el compositor

Estado: aceptado (2026-10-24).

## Contexto

El ADR 0019 fijó quién dibuja la titlebar y el 0020 que los efectos se aplican a todas las ventanas. Hasta este ADR solo existía titlebar SSD, con esquinas rectas y sin sombra ni borde. Neptunia define radio 16, sombra, inner border y titlebar de 32 px.

## Decisión

1. Un elemento de marco por ventana: `RoundedTextureElement` envuelve la textura del cliente o de la titlebar y la dibuja con `ROUNDED_CLIP`. Incluye sombra (`shadow.frag`), borde exterior (`border.frag`), hairline y contenido recortado.
2. Esquinas, sombra y borde se aplican a **todas** las ventanas, también CSD. GTK/Qt no reciben titlebar (ADR 0019). Maximizado conserva el marco; solo fullscreen lo elimina.
3. La titlebar solo aparece en SSD; `ChromeCache` genera su textura con título y semáforos.
4. Los diálogos usan el mismo marco y se centran sobre el padre.
5. Sin buffer intermedio: el clip se hace en el fragment shader con `v_coords * size` y RGBA premultiplicado.
6. Radio, sombra, bordes y colores salen de `yuki-design`.

## Consecuencias

SSD y CSD comparten estética de marco; solo SSD llevan titlebar. `frame_radius` y `frame_has_border` dependen del estado (fullscreen los anula). Se unifica el camino GLES de chrome, contenido y cursores. Queda incluir el marco en la captura de ventana y ajustar el damage tracker a la sombra.

## Alternativas descartadas

- Que cada cliente dibuje sombra/borde: duplica efectos y no cubre apps ajenas.
- Buffer intermedio por ventana: memoria y copias extra.
