# 0014 — Los insets de decoración son una función del estado de la ventana

Estado: aceptado (sesión anidada, parte 4).

## Contexto

El configure de un toplevel lleva su tamaño útil (ventana menos insets). Los
insets deben existir antes de escribir ese configure. El chrome se decide en
`vow-design` y puede cambiar mientras la ventana vive.

## Decisión

`decoration` posee `Insets { top, right, bottom, left }` en píxeles lógicos
`f64`. `Insets::for_window(role, window, chrome, titlebar)` es el único lugar
que resuelve la precedencia:

1. rol: popups y diálogos no se decoran; un toplevel normal sí;
2. estado: fullscreen no reserva nada; maximizado conserva titlebar;
3. negociación: CSD o petición explícita de no decorar no reservan nada.

El resto recibe `Insets::ZERO`. Los insets se recalculan en cada configure, no
se capturan al mapear. Las sombras no son insets: se dibujan fuera y no
reservan layout. La región de entrada puede extenderse fuera de la ventana con
insets negativos.

## Consecuencias

- El configure inicial pide al cliente una superficie menor por la titlebar.
- El tipo es puro y testeable sin Smithay.
- Añadir un rol o estado es editar el único `match`.

## Alternativas descartadas

- Capturar los insets al mapear: no soporta fullscreen, maximizado ni cambios de
  modo posteriores.
- Incluir la sombra en los insets: movería ventanas adyacentes y rompería el
  solapamiento visual.
