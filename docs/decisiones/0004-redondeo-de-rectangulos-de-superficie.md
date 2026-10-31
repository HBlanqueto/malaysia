# 0004 — Redondear rectángulos lógicos al espacio entero

Estado: **aceptado** — implementado en `malaysia-scene` (2026-10-21).

## Contexto

Redondear al más cercano abre costuras de un píxel entre superficies
adyacentes con posiciones fraccionarias. Con escala 1.25 eso es la norma, no
un caso extremo.

## Decisión

Al convertir de lógico a entero, la **posición** se redondea con `floor` y el
**tamaño** con `ceil`. Dos rectángulos adyacentes comparten el píxel del borde
y ninguno cubre de menos. `scale_up` se dividió en `scale_floor` y
`scale_ceil`; el redondeo al más cercano desaparece de la API.

## Consecuencias

- Los tests cubren dos rectángulos que comparten una arista fraccionaria.
- Cada llamada sigue usando `to_physical`/`to_buffer`; no elige el redondeo.

## Alternativas descartadas

- Redondeo al más cercano: deja costuras entre superficies.
- Redondeo elegido por el llamador: invita a inconsistencias; hay un único
  valor por defecto correcto.
- Valores fraccionarios hasta el rasterizado: incompatible con scissor/viewport
  enteros y con coste innecesario.
