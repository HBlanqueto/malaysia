# 0024 — Stack de render del toolkit del shell

Estado: aceptado (opción A), modificado para los clientes por los ADR 0033 y 0034: la shell y las apps propias usan CelestialKit sobre Freya con Skia; el compositor sigue con GLES + `cosmic-text` + `grade-graphics`.

## Contexto

Cada componente del shell es un cliente Wayland que pinta su superficie layer-shell (ADR 0023). El criterio era compartir el GLSL del squircle y la titlebar entre compositor y shell.

## Decisión

El toolkit (hoy `celestial-kit`) es el stack de la shell y las apps propias, **no del compositor**. Los shaders GLSL compartidos (squircle, hairline, inner line, sombra 9-slice) viven en `grade-graphics`; se comparte el GLSL y sus uniformes, no el código GL de Rust.

Parámetros del toolkit: iconos SVG con `resvg` rasterizados una vez, texto con `cosmic-text` y atlas de glifos, y blur dual-Kawase solo en superficies pequeñas. El ADR 0034 cambió los clientes a Freya + Skia; el compositor nunca enlaza Skia ni `freya-*`.

## Consecuencias

Un solo lenguaje visual compartido por GLSL; el compositor mantiene GLES y la shell paga Skia/Freya.

## Alternativas descartadas

- Skia en ambos lados: obligaba a portar el compositor o duplicar el dibujo.
- Vello/wgpu: stack distinto y blur/filtros aún en alpha.
