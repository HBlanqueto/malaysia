# 0012 — El backend vive junto al State del compositor

Estado: aceptado (sesión anidada, parte 1).

## Contexto

Direct Session sustituirá winit por DRM/KMS.

## Decisión

`State` no guarda tipos de backend: solo protocolo, escena y señal del loop. El backend (ventana, renderer, damage tracker) vive en `session.rs`, capturado por las fuentes de eventos. Los handlers mutan `State` y piden redibujo; el loop lee el estado al dibujar.

Enmienda 7b: `State` guarda un `LoopHandle` para que los blockers de explicit sync y el release diferido registren sources one-shot. No es un tipo de backend.

## Consecuencias

Cambiar de backend toca `session.rs` y la escena, no los handlers; los handlers son testeables sin pila gráfica.

## Alternativas descartadas

- Campos concretos de winit en `State`: acopla todos los handlers al backend.
