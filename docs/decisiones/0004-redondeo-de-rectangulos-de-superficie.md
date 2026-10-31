# 0004 — Redondear rectángulos lógicos al espacio entero

Estado: aceptado — implementado en `yuki-scene` (2026-10-21).

## Contexto

Redondear al más cercano abre costuras de un píxel entre superficies adyacentes con posiciones fraccionarias; con escala 1.25 es la norma, no un caso extremo.

## Decisión

La posición se redondea con `floor` y el tamaño con `ceil`. Dos rectángulos adyacentes comparten el píxel del borde y ninguno cubre de menos. `scale_up` se dividió en `scale_floor` y `scale_ceil`; el redondeo al más cercano desaparece de la API.

## Consecuencias

Los tests cubren dos rectángulos que comparten una arista fraccionaria; cada llamada usa `to_physical`/`to_buffer` sin elegir el redondeo.

## Alternativas descartadas

- Redondeo al más cercano: deja costuras.
