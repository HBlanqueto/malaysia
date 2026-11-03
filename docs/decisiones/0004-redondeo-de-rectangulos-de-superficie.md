# 0004 — Redondear rectángulos lógicos al espacio entero

Estado: aceptado — implementado en `yuki-scene` (2026-10-21).

## Contexto

Redondear al más cercano abre costuras de un píxel entre superficies adyacentes con posiciones fraccionarias; con escala 1.25 es la norma, no un caso extremo.

## Decisión

La posición se redondea con `floor` y el tamaño con `ceil`. Dos rectángulos adyacentes comparten el píxel del borde y ninguno cubre de menos. `scale_up` se dividió en `scale_floor` y `scale_ceil`; el redondeo al más cercano desaparece de la API.

## Consecuencias

El compositor convierte con los tipos de Smithay (ADR 0001): `to_i32_floor` para la posición y `to_physical_precise_up` para el tamaño. `malaysia-scene` ya no tiene conversiones propias.

## Alternativas descartadas

- Redondeo al más cercano: deja costuras.
