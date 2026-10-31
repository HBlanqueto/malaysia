# 0030 — Direct Session 8a: un plano primario y una sola salida

Estado: aceptado (Bloque 8a).

## Contexto

8a debe arrancar desde un TTY en el panel interno sin arrastrar multi-GPU,
overlay/cursor planes ni hotplug; 8b y 8c los añaden después.

## Decisión

- DRM atómico únicamente, sin camino legacy.
- Un solo dispositivo: el primario del seat, abierto por `libseat`; GBM + EGL +
  `GlesRenderer`. `--drm-device` separa render y scanout solo para desarrollo.
- Un solo conector interno (eDP) en su modo preferido; si no hay eDP, el
  primero conectado; si no hay ninguno, falla al arrancar.
- Un solo plano: `DrmCompositor` recibe overlay y cursor vacíos; el puntero se
  compone como elemento.
- Un frame por vblank con damage tracking; sin daño se reprograma un timer.
- libinput es la única entrada, con defaults de libinput.
- Cambio de VT: `Ctrl+Alt+F1..F12`, pausa/reanudación de DRM y libinput;
  `Ctrl+Alt+Backspace` es la salida de emergencia. La sesión bloqueada sigue
  bloqueada tras volver del VT.
- La inicialización EGL/GBM es la única `unsafe`, acotada a `udev.rs` (excepción
  al ADR 0003).

## Herramientas de desarrollo

`--drm-device <cardN>` y `--no-input` son solo para pruebas y exigen `--drm`.
La prueba usa `LIBSEAT_BACKEND=noop` bajo sudo; no es ruta de producción.

## Consecuencias

- 8a no soporta hotplug, monitores externos, escala por salida, direct scanout
  ni XWayland.
- vkms cubre DRM/KMS, GBM/EGL, import dmabuf y vblank; no cubre eDP, touchpad,
  VT ni lock + VT.
- 8b/8c pueden añadir conectores y planos sin cambiar el contrato de `State`.

## Alternativas descartadas

- Copiar la complejidad de Otto/anvil (multi-GPU, planos): fuera del alcance de
  una sesión mínima.
- Fallback legacy o a winit desde DRM: oculta fallos de arranque.
