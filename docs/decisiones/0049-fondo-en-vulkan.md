# 0049 — Fondo de pantalla en Vulkan

Estado: aceptado
Fecha: 2026-10-30

## Contexto

El fondo de escritorio de MalaysiaDreams vive hoy en `malaysia-shell wallpaper` y se dibuja con BarrierKit sobre Skia GL. Eso cumplió en el Bloque 7c/ADR 0027, pero el fondo es el único sitio del escritorio que va a necesitar shaders propios, decodificación HEIC como formato por defecto y fondos dinámicos día/noche con transiciones. Meter eso dentro del kit obligaría a que BarrierKit arrastre un backend de render alternativo solo para una superficie opaca de pantalla completa, y contaminaría a menubar, dock y lock con dependencias que no les corresponden.

## Decisión

1. El fondo pasa a ser un binario aparte, `malaysia-wallpaper`, cliente Wayland propio que dibuja con `wgpu` sobre **Vulkan únicamente**. Usa la misma pila de cliente que el resto de la shell (SCTK + calloop), pero sin BarrierKit ni Freya.
2. Esta es una excepción acotada a la regla "la shell dibuja todo con el kit": el fondo no tiene widgets, texto ni accesibilidad, y su única salida es una imagen escalada a pantalla completa. La excepción no se extiende a otros componentes.
3. `wgpu` no entra en `malaysia-compositor` ni en `malaysia-shell`, y Freya/Skia no entran en `malaysia-wallpaper`. `scripts/check-client-only-deps.sh` lo comprueba.
4. El wrapper de Nix apunta el loader de Vulkan al RADV del flake, con el mismo patrón `MALAYSIA_ORIG_*` que ya usa el wrapper EGL (ADR 0007). La variable `VK_DRIVER_FILES` no llega a los clientes que lance el compositor.
5. El contrato con el resto del escritorio no cambia: `wallpaper.domain` sigue siendo el almacén de selección, `wallpaper.state` publica el color dominante, y el namespace de layer-shell sigue siendo `"malaysia-wallpaper"`.

## Consecuencias

- Hay un proceso más que supervisar, con su propia unidad systemd `malaysia-wallpaper.service`.
- No hay respaldo a GL: si no hay adaptador Vulkan, `malaysia-wallpaper` falla con un mensaje claro y sale con código distinto de cero.
- El Bloque 16c tendrá que dejar de enviar frame callbacks a las capas `Background` totalmente tapadas, porque hoy `session.rs` los envía a todas las layers en cada frame. Como `malaysia-wallpaper` ya no pide frames en reposo, el ahorro real dependerá de ese cambio.
- La foto se comparte como una sola textura para todas las salidas, con mipmaps generados en GPU; el RGBA de la CPU se libera tras la subida.
