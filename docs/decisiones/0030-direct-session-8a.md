# 0030 — Direct Session 8a: un plano primario y una sola salida

Estado: aceptado (Bloque 8a).

## Contexto

El Bloque 8 se dividió en 8a (sesión mínima), 8b (hotplug y monitores
externos) y 8c (planos y direct scanout). 8a debe arrancar desde un TTY en el
panel interno del ThinkPad sin arrastrar la complejidad de Otto o anvil:
multi-GPU, overlay planes, cursor planes y hotplug quedan fuera.

## Decisión

- DRM atómico únicamente. Sin camino legacy y sin fallback.
- Un solo dispositivo: el primario del seat, abierto por `libseat`; GBM + EGL +
  `GlesRenderer`, el mismo renderer que winit. La excepción es la herramienta
  de desarrollo `--drm-device` (abajo), que separa render y scanout.
- Un solo conector: el interno (eDP), en su modo preferido. Si no hay eDP, el
  primer conector conectado; si no hay ninguno, el arranque falla.
- Un solo plano: se pasan a `DrmCompositor` los planos de la superficie con
  `overlay` y `cursor` vacíos. El puntero se compone como elemento, no en un
  plano de cursor.
- Un frame por vblank, con el damage tracking de `DrmCompositor`. Si un frame
  no tiene daño, se reprograma un timer de un refresco.
- libinput es la única entrada. El touchpad usa los defaults de libinput: no
  hay opciones de `input.*` en la configuración todavía.
- Cambio de VT: `Ctrl+Alt+F1..F12` pide `change_vt`; `PauseSession` pausa DRM y
  libinput, `ActivateSession` los reactiva y redibuja. `Ctrl+Alt+Backspace` es
  la salida de emergencia.
- La sesión bloqueada sigue bloqueada tras cambiar de VT y volver: la pausa no
  toca `State::locked` ni el estado de `ext-session-lock-v1`.
- La inicialización de EGL/GBM es la única `unsafe` del compositor, acotada a
  `udev.rs` y permitida en ese módulo. Es una excepción explícita al ADR 0003.

## Herramientas de desarrollo (`--drm-device`, `--no-input`)

Ambas opciones son **solo para pruebas** y exigen `--drm`; sin ellas el
comportamiento de producción no cambia.

- `--drm-device <cardN>` elige el nodo KMS de scanout en vez del primario del
  seat. Yuki busca aparte un render node del GPU de arranque (amdgpu) y monta
  ahí GBM, EGL y `GlesRenderer`; el `DrmDevice`, la superficie y el
  `GbmFramebufferExporter` usan el nodo elegido. Los buffers se asignan en el
  render node y se importan como dma-buf en el nodo de scanout. El tipo de
  `DrmCompositor` no cambia: el allocator y el exporter ya eran parámetros
  independientes. El feedback de dmabuf y el syncobj usan el render node.
- `--no-input` no construye libinput ni llama a `udev_assign_seat`, así que no
  se abre ningún `/dev/input`.
- Con cualquiera de las dos opciones se omite el target del shell: una prueba
  no debe arrancar componentes en la sesión del usuario.
- La prueba no abre el card del GPU de arranque: solo hace `stat` de sus nodos
  para encontrar el render node. El test `dev.sh drm-vkms` lo verifica leyendo
  `/proc/<pid>/fd`; también comprueba que no haya `/dev/input` abierto.
- Bajo `LIBSEAT_BACKEND=noop` y `sudo`, libseat abre los nodos sin seatd. Es
  solo para vkms; no es una ruta de producción.

## Consecuencias

- 8a no soporta hotplug, monitores externos, escala fraccional real, direct
  scanout, XWayland ni spawns.
- vkms cubre el camino DRM/KMS, GBM/EGL en render node, import dma-buf y
  vblank; no cubre eDP, touchpad, cambio de VT, pausa/reanudación de libseat
  ni lock + VT. Eso sigue necesitando el panel real.
- El lock no se desbloquea por perder el VT ni por la muerte del cliente de
  lock; el render de la sesión bloqueada sigue mostrando solo sus superficies.
- 8b y 8c pueden añadir conectores y planos sin cambiar el contrato de `State`.
- El binario de `nix build` y `yuki::spawn` siguen siendo los de ADR 0007;
  esta decisión no los rehace.
