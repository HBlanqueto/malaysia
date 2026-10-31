# 0027 — La shell presenta por EGL

Estado: **aceptado** (Bloque 7c).

## Contexto

La shell dibujaba offscreen y presentaba por `wl_shm`: copia CPU a CPU y el
compositor volvía a subir el shm a una textura. Era barato para CPU y caro para
GPU, y dejaba a la shell fuera de dmabuf (ADR 0026).

## Decisión

1. La shell presenta por EGL sobre `wl_egl_window`; Mesa asigna dmabuf y el
   compositor lo texturiza sin copias. Es el único camino de presentación.
2. El toolkit recibe punteros opacos (`wl_egl_window*`) y sigue
   Wayland-agnóstico; el `wl_egl_window` lo posee `malaysia-shell`. Se elimina
   el readback offscreen.
3. Menubar, dock y lock componen su RGBA por CPU y lo suben a una textura
   reutilizada, solo en el rectángulo dañado.
4. El lock mantiene un presenter por superficie.
5. Se elimina `wl_shm` de la shell; el compositor sigue sirviéndolo a clientes.
6. Las animaciones (magnificación, minimizar) deben renderizarse en GPU; no se
   sube un frame completo por frame.

## Consecuencias

- Un solo camino de presentación; la subida de menubar/dock es parcial.
- Cada componente de texto paga ~18 MB de USS por su contexto GLES; la medida
  USS+GPU quedó borderline pero no "claramente mayor" que el umbral pactado.
- El reloj commitea al minuto; en reposo no hay flujo continuo.

## Alternativas descartadas

- Mantener `wl_shm` en la shell: copia CPU y re-subida a GPU; peor para el
  wallpaper y para los componentes con textura.
- Readback offscreen por frame: coste de memoria y ancho de banda.
