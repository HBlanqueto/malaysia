# 0018 — La salida winit se voltea en tiempo de renderizado, nunca en `wl_output`

Estado: aceptado (seguimiento del Bloque 4).

## Contexto

El frame salía boca abajo: el framebuffer de winit tiene origen abajo-izquierda y el render de Smithay es y-hacia-abajo.

## Decisión

Mantener `Output` en `Normal` y dar al damage tracker una fuente de modo estática `Flipped180` (`OutputModeSource::Static`). El volteo ocurre solo en la matriz de proyección. Un resize construye un tracker nuevo porque la fuente es estática.

## Consecuencias

El render y = 0 es la parte superior; daño, cursor, hit-testing y geometría siguen en lógico y-hacia-abajo.

## Alternativas descartadas

- Poner `Output` en `Flipped180`: los clientes pre-rotarían sus buffers.
