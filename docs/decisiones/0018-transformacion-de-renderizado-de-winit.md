# 0018 — La salida winit se voltea en tiempo de renderizado, nunca en `wl_output`

Estado: aceptado (seguimiento del Bloque 4).

## Contexto

El frame salía boca abajo: el framebuffer de winit tiene origen abajo-izquierda
y el render de Smithay es y-hacia-abajo. Cambiar la transformación del `Output`
haría que los clientes pre-rotaran sus buffers, trasladando el error.

## Decisión

Mantener `Output` en `Normal` y dar al damage tracker una fuente de modo
estática `Flipped180` (`OutputModeSource::Static`). El volteo ocurre solo en la
matriz de proyección del renderer. Un resize construye un tracker nuevo porque
la fuente es estática.

## Consecuencias

- El render y = 0 es la parte superior de la ventana.
- Daño, cursor, hit-testing y geometría siguen en coordenadas lógicas
  y-hacia-abajo.
- Un backend DRM/KMS no copia esto: allí la transformación del `Output` es la
  rotación de scanout y sí pertenece al `Output`.

## Alternativas descartadas

- Poner `Output` en `Flipped180`: los clientes verían la rotación y pre-rotarían
  sus buffers.
- Voltear cada elemento por separado: duplica la corrección y deja sitios sin
  cubrir.
