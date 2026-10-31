# 0049 — Fondo de pantalla en Vulkan

Estado: aceptado (2026-10-30).

## Contexto

El fondo vivía en `malaysia-shell wallpaper`, dibujado con BarrierKit/Skia GL.
Es el único sitio que necesitará shaders propios, HEIC y fondos dinámicos; meterlo
en el kit obligaría a BarrierKit a arrastrar un backend alternativo para una
superficie opaca.

## Decisión

1. El fondo pasa a `malaysia-wallpaper`, binario aparte y cliente Wayland propio
   que dibuja con `wgpu` sobre **Vulkan únicamente**. Usa SCTK + calloop, sin
   BarrierKit ni Freya.
2. Excepción acotada a «la shell dibuja con el kit»: el fondo no tiene widgets,
   texto ni accesibilidad. No se extiende a otros componentes.
3. `wgpu` no entra en el compositor ni en `malaysia-shell`; Freya/Skia no entran
   en `malaysia-wallpaper`. `scripts/check-client-only-deps.sh` lo comprueba.
4. El wrapper Nix apunta el loader de Vulkan al RADV del flake con
   `MALAYSIA_ORIG_*`, igual que el wrapper EGL (ADR 0007). `VK_DRIVER_FILES` no
   llega a los clientes del compositor.
5. El contrato no cambia: `wallpaper.domain` guarda la selección,
   `wallpaper.state` publica el color dominante y el namespace layer-shell sigue
   siendo `malaysia-wallpaper`.

## Consecuencias

- Hay un proceso más que supervisar, con `malaysia-wallpaper.service`.
- Sin respaldo GL: si no hay adaptador Vulkan, falla con mensaje claro y sale
  con código distinto de cero.
- 16c debe dejar de enviar frame callbacks a capas `Background` tapadas; el
  ahorro real depende de ese cambio.
- La foto se comparte como una textura con mipmaps en GPU; el RGBA de CPU se
  libera tras la subida.

## Alternativas descartadas

- Mantener el fondo en el kit: contamina menubar/dock/lock con dependencias que
  no necesitan.
- Añadir un backend GL de respaldo: duplica caminos y oculta la falta de Vulkan.
