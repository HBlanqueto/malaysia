# 0029 — El backend se elige con un flag explícito

Estado: aceptado (Bloque 8a).

## Contexto

Yuki tiene dos backends de presentación y entrada: winit anidado y DRM/KMS
directo. El primero es el de desarrollo (`dev.sh`, `smoke`, `block4`,
`popup_probe`); el segundo toma el seat, el DRM master y la pantalla real.

## Decisión

Se eligen con `--winit` y `--drm`, sin autodetección. El default es `--winit`.

- No hay autodetección porque un fallo de detección en un TTY no debe acabar
  tomando el seat o la pantalla por sorpresa.
- Winit es el default porque es el camino que usan el dev shell y toda la
  aceptación automatizada, y funciona sin TTY, sin libseat y sin DRM master.
- `--drm` falla con un error claro si no hay seat, GPU o conector interno; no
  cae de vuelta a winit.

## Consecuencias

- El archivo de sesión y el arranque manual desde un TTY deben pasar `--drm`.
- `dev.sh run` y las pruebas siguen usando el default, sin tocar sus comandos.
- Un futuro autodetectado, si se quiere, será otra decisión y otro flag.
