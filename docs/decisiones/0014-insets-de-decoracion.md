# 0014 — Los insets de decoración son una función del estado de la ventana

Estado: aceptado (sesión anidada, parte 4).

## Contexto

El configure de un toplevel lleva su tamaño útil (ventana menos insets); los insets deben existir antes de escribir ese configure.

## Decisión

`decoration` posee `Insets { top, right, bottom, left }` en píxeles lógicos `f64`. `Insets::for_window(role, window, chrome, titlebar)` resuelve la precedencia:

1. rol: popups y diálogos no se decoran; un toplevel normal sí;
2. estado: fullscreen no reserva; maximizado conserva titlebar;
3. negociación: CSD o petición explícita de no decorar no reservan.

El resto recibe `Insets::ZERO`. Se recalculan en cada configure, no al mapear. Las sombras no son insets; la región de entrada puede extenderse fuera con insets negativos.

## Consecuencias

El configure inicial pide al cliente una superficie menor por la titlebar; el tipo es puro y testeable sin Smithay; añadir un rol o estado es editar un único `match`.

## Alternativas descartadas

- Capturar los insets al mapear: no soporta fullscreen, maximizado ni cambios de modo.
