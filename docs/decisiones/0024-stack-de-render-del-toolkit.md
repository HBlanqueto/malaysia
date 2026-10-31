# 0024 — Stack de render del toolkit del shell

Estado: **aceptado (opción A)**, modificado para los clientes por los ADR 0033
y 0034: la shell y las apps propias usan BarrierKit sobre Freya con Skia; el
compositor sigue con GLES + `cosmic-text` + `grade-graphics`.

## Contexto

Cada componente del shell es un cliente Wayland que pinta su superficie
layer-shell (ADR 0023). El criterio era compartir el GLSL del squircle y la
titlebar entre compositor y shell para escribirlos una vez.

## Decisión

El toolkit (hoy `barrier-kit`, antes CelestialKit) es el stack de la shell y las
apps propias, **no del compositor**. Los shaders GLSL compartidos (squircle,
hairline, inner line, sombra 9-slice) viven en `grade-graphics`; lo compartido es
el GLSL y sus uniformes, no el código GL de Rust.

Parámetros del toolkit: iconos SVG con `resvg` rasterizados una vez, texto con
`cosmic-text` y atlas de glifos, y blur dual-Kawase solo en superficies pequeñas
y acotadas.

El ADR 0034 cambió el stack de los clientes a Freya + Skia; el compositor nunca
enlaza Skia ni `freya-*`.

## Consecuencias

- Hay un solo lenguaje visual compartido por GLSL; el compositor mantiene GLES.
- La shell paga Skia/Freya; el compositor no.
- `grade-graphics` sigue siendo la fuente de los shaders de efectos.

## Alternativas descartadas

- Skia en ambos lados: obligaba a portar el compositor a Skia o a duplicar el
  dibujo.
- Vello/wgpu: stack distinto y blur/filtros aún en alpha.
