# 0004 — Redondear rectángulos lógicos al espacio entero

Estado: **abierto** — decisión diferida hasta que exista la colocación de superficies

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

**Abierta.** El Bloque 0 mantiene el redondeo al más cercano porque nada coloca superficies
todavía y no hay evidencia sobre la que actuar. El requisito de renderizado conocido se
registra aquí para que el primer hito que coloque o dañe superficies
evalúe la opción 2 (floor para posición, ceil para tamaño) en lugar de redescubrir el
problema de las costuras.

La API deliberadamente no se construye todavía: esto es un requisito de renderizado, no
especulación, pero no tiene llamador, y la regla de no-API-especulativa sigue
aplicando.

## Consecuencias

- El redondeo al más cercano en `geometry.rs` es un marcador de posición, no una decisión final.
- Cuando se produzca el primer rectángulo de damage/scissor, este ADR debe
  revisarse y aceptarse o reemplazarse antes de que ese código se publique.
- La API de conversión probablemente necesitará expresar el redondeo de posición y de tamaño
  por separado; las firmas actuales de `to_physical`/`to_buffer` pueden cambiar.
- Este ADR no implica cambios de código hoy.

## Referencias

- El redondeo de rectángulos lógicos a regiones de daño enteras se discute en
  el código de renderizado de wlroots/Smithay y en las discusiones del protocolo
  fractional-scale; la convención floor/ceil es la resolución común.