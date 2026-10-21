# 0004 — Redondear rectángulos lógicos al espacio entero

Estado: **aceptado** — implementado en `yuki-scene` (2026-10-21)

## Contexto

`crates/yuki-scene/src/geometry.rs` actualmente convierte rectángulos lógicos
al espacio entero Physical/Buffer redondeando la esquina y las extensiones al
píxel más cercano. Eso es correcto para un único rectángulo aislado. Es
incorrecto para el caso que más importa en un compositor: dos rectángulos que
comparten una arista.

Si dos superficies adyacentes están en posiciones lógicas que ambas caen en `x.5`,
el redondeo al más cercano mueve una a la izquierda y otra a la derecha, abriendo una
costura de un píxel físico entre ellas. Con escalas de salida fraccionarias (por ejemplo 1.25) las
aristas lógicas desalineadas son la norma, no la excepción, así que las costuras aparecerían
regularmente y no como un caso extremo.

La solución convencional es redondear asimétricamente:

- **posición: floor**
- **tamaño: ceil**

de modo que un rectángulo siempre cubra los píxeles que superpone lógicamente y los
rectángulos adyacentes compartan un píxel de límite en lugar de dejar un hueco. Esto es lo que
hacen varios compositores y renderizadores al convertir rectángulos lógicos en una
región de daño (damage) o de recorte (scissor) en píxeles.

## Opciones consideradas

1. **Redondeo al más cercano para posición y tamaño** (implementación actual).
   Simple y simétrico por rectángulo, pero produce costuras de un píxel entre
   superficies adyacentes bajo posiciones lógicas fraccionarias o no enteras.
2. **Floor para posición, ceil para tamaño.** Sin costuras entre superficies adyacentes y nunca
   cubre de menos, a costa de dibujar ocasionalmente un píxel dentro del
   espacio de un vecino. Esta es una convención bien establecida.
3. **Redondeo suministrado por el llamador.** Cada conversión toma el modo de redondeo que
   quiera. Lo más flexible, pero empuja una decisión que tiene un único valor por defecto
   correcto hacia cada sitio de llamada, e invita a la inconsistencia.
4. **Mantener los valores lógicos fraccionarios hasta el final y dejar que el renderizador
   rasterice analíticamente.** Evita por completo la cuestión del redondeo, pero no es
   posible con APIs de scissor/viewport enteras y añade un costo que un
   compositor a escala de escritorio no necesita.

## Decisión

**Opción 2.** La conversión de lógico a entero redondea la **posición con
`floor`** y el **tamaño con `ceil`**. Dos rectángulos adyacentes comparten así el
píxel del borde en vez de abrir una costura de un píxel, y ningún rectángulo
cubre de menos. Es la convención que usan los compositores al construir regiones
de damage y scissor.

Implementado en `crates/yuki-scene/src/geometry.rs`:

- `Point::to_physical` / `Point::to_buffer`: `floor`.
- `Size::to_physical` / `Size::to_buffer`: `ceil`.
- `Rect` combina ambos, así que su esquina se floor y sus extensiones se ceilan.

## Consecuencias

- `scale_up` se dividió en `scale_floor` (posición) y `scale_ceil` (tamaño);
  ya no hay un único redondeo al más cercano.
- Los tests cubren el caso de dos rectángulos que comparten una arista
  fraccionaria y no dejan hueco.
- El redondeo al más cercano desaparece de la API; cada sitio de llamada sigue
  usando `to_physical`/`to_buffer` y no elige el modo.

## Referencias

- El redondeo de rectángulos lógicos a regiones de daño enteras se discute en
  el código de renderizado de wlroots/Smithay y en las discusiones del protocolo
  fractional-scale; la convención floor/ceil es la resolución común.