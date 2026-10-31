# 0027 — La shell presenta por EGL

Estado: aceptado (Bloque 7c).

## Contexto

La shell dibujaba offscreen y presentaba por `wl_shm`: copia CPU a CPU y re-subida a textura. Era barato para CPU y caro para GPU, y la dejaba fuera de dmabuf (ADR 0026).

## Decisión

1. Presenta por EGL sobre `wl_egl_window`; Mesa asigna dmabuf y el compositor lo texturiza sin copias. Es el único camino.
2. El toolkit recibe punteros opacos (`wl_egl_window*`) y sigue Wayland-agnóstico; se elimina el readback offscreen.
3. Menubar, dock y lock componen su RGBA por CPU y lo suben a una textura reutilizada, solo en el rectángulo dañado.
4. El lock mantiene un presenter por superficie.
5. Se elimina `wl_shm` de la shell; el compositor sigue sirviéndolo a clientes.
6. Las animaciones deben renderizarse en GPU; no se sube un frame completo por frame.

## Consecuencias

Un solo camino; la subida de menubar/dock es parcial. Cada componente de texto paga ~18 MB de USS por su contexto GLES; la medida USS+GPU quedó borderline pero no «claramente mayor» que el umbral. El reloj commitea al minuto.

## Alternativas descartadas

- Mantener `wl_shm`: copia CPU y re-subida a GPU.
- Readback offscreen por frame: coste de memoria y ancho de banda.
