# 0030 — Direct Session 8a: un plano primario y una sola salida

Estado: aceptado (Bloque 8a).

## Contexto

8a debe arrancar desde un TTY en el panel interno sin multi-GPU, overlay/cursor planes ni hotplug; 8b y 8c los añaden después.

## Decisión

- DRM atómico, sin camino legacy.
- Un dispositivo: el primario del seat, abierto por `libseat`; GBM + EGL + `GlesRenderer`. `--drm-device` separa render y scanout solo para desarrollo.
- Un conector interno (eDP) en su modo preferido; si no hay eDP, el primero conectado; si no hay ninguno, falla.
- Un plano: `DrmCompositor` recibe overlay y cursor vacíos; el puntero se compone como elemento.
- Un frame por vblank con damage tracking; sin daño se reprograma un timer.
- libinput es la única entrada, con defaults.
- Cambio de VT con pausa/reanudación de DRM y libinput; `Ctrl+Alt+Backspace` es la salida de emergencia. La sesión bloqueada sigue bloqueada tras volver.
- La inicialización EGL/GBM es la única `unsafe`, en `udev.rs` (excepción al ADR 0003).

`--drm-device` y `--no-input` son solo para pruebas y exigen `--drm`; la prueba usa `LIBSEAT_BACKEND=noop` bajo sudo.

## Consecuencias

8a no soporta hotplug, monitores externos, escala por salida, direct scanout ni XWayland. vkms cubre DRM/KMS, GBM/EGL, import dmabuf y vblank; no cubre eDP, touchpad, VT ni lock + VT. 8b/8c no cambian el contrato de `State`.

## Alternativas descartadas

- Copiar multi-GPU/planos de Otto/anvil: fuera de una sesión mínima.
- Fallback a winit desde DRM: oculta fallos de arranque.
