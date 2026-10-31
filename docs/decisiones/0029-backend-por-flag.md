# 0029 — El backend se elige con un flag explícito

Estado: aceptado (Bloque 8a).

## Contexto

Hay dos backends: winit anidado (desarrollo) y DRM/KMS directo (toma el seat y la pantalla real).

## Decisión

Se eligen con `--winit` y `--drm`, sin autodetección. El default es `--winit`. `--drm` falla con un error claro si no hay seat, GPU o conector interno; no cae de vuelta a winit.

## Consecuencias

El archivo de sesión y el arranque manual desde un TTY pasan `--drm`; `dev.sh` y las pruebas usan el default.

## Alternativas descartadas

- Autodetección: un fallo en un TTY podría tomar el seat o la pantalla por sorpresa.
