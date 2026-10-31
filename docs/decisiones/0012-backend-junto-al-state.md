# 0012 — El backend vive junto al State del compositor

Estado: aceptado (sesión anidada, parte 1).

## Contexto

El hito Direct Session sustituirá winit por DRM/KMS. Si los handles del backend
viven dentro de `State`, cada handler que pida un frame o importe un buffer
nombrará tipos de winit y habrá que editarlo al cambiar de backend.

## Decisión

`State` no guarda tipos de backend: contiene solo el estado de protocolo, la
escena y la señal del loop. El backend (ventana, renderer, damage tracker) vive
en `session.rs`, capturado por las fuentes de eventos que lo poseen. La interfaz
es estrecha: los handlers mutan `State` y piden redibujo; el loop lee el estado
cuando el backend va a dibujar.

Enmienda (Bloque 7b): `State` guarda además un `LoopHandle` para que los
blockers de explicit sync y el release diferido registren sources one-shot. No
es un tipo de backend; el `Session` sigue siendo el dueño del loop.

## Consecuencias

- Sustituir winit por DRM/KMS toca `session.rs` y la escena, no los handlers.
- Los handlers son testeables sin pila gráfica.
- El coste es una indirección en el dibujo, que es el ciclo de redibujo que ya
  hacía falta.

## Alternativas descartadas

- Campos concretos de winit en `State`: acopla todos los handlers al backend.
- Enum `Backend` de niri: útil con varios backends simultáneos; aquí hay uno por
  ejecución y sería abstracción especulativa.
