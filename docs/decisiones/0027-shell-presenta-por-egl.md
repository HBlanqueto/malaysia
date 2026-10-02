# 0027 — La shell presenta por EGL

Estado: **aceptado** (Bloque 7c).

## Contexto

Hasta el Bloque 7c, `yuki-shell` dibujaba offscreen (el wallpaper con GLES, la
barra de menú, el dock y el lock componiendo texto por CPU) y presentaba el
frame por `wl_shm`: un pool, una copia CPU a CPU, y el compositor re-subía ese
shm a una textura para dibujarlo. Era el camino barato para contenido de CPU
pero el costoso para GPU (una copia de vuelta a CPU solo para volver a entrar
por la GPU), y dejaba a la shell fuera de los buffers dmabuf que el Bloque 7a
empezó a servir.

El Bloque 7c decide cómo presenta la shell sus superficies, con una medición de
memoria explícita como criterio (revisión del usuario).

## Decisión (Aceptada)

1. **La shell presenta por EGL sobre `wl_egl_window`.** Cada componente crea un
   `wl_egl_window` en su superficie de layer-shell (libwayland-egl,
   `WlEglSurface::new` con el `ObjectId` del `wl_surface`) y una surface EGL de
   ventana sobre él (`eglCreateWindowSurface`, plataforma Wayland de EGL). Al
   swapear, Mesa asigna buffers dmabuf con acquire/release (Bloque 7a/7b) y el
   compositor los texturiza directamente: **sin copias por CPU ni readback**.
   Es **el único camino** de presentación de la shell.
2. **`celestial-kit` recibe punteros opacos, sigue Wayland-agnóstico.** El
   renderer crea la surface EGL desde un `wl_egl_window*` crudo y presenta una
   textura (`render_cover_to_surface`); el `wl_egl_window` lo posee
   `yuki-shell` (depende de `wayland-egl`). El readback offscreen
   (`render_cover_to_rgba` y su FBO) se elimina.
3. **El contenido por CPU se sube a una textura reutilizada.** Menubar, dock y
   lock componen su frame RGBA por CPU y lo presentan subiéndolo a una textura
   (`upload_texture_rgba`, una por tamaño, sin fugas). Pagan el contexto GLES;
   el wallpaper, que ya lo tenía, no cambia.
4. **El lock mantiene un presenter por superficie** (una por salida).
5. **Se elimina `wl_shm` de la shell** y, con él, `ShmPresenter` y el readback
   offscreen de `celestial-kit`. El compositor sigue sirviendo `wl_shm` para
   sus clientes.
6. **El contenido estático puede rasterizarse por CPU; las animaciones, no.**
   Menubar, dock y lock componen su frame por CPU (cosmic-text) y lo suben a la
   textura **solo en el rectángulo dañado** (`glTexSubImage2D`, la franja del
   reloj o la baldosa cambiada), no el frame entero. Eso vale para contenido
   casi estático; la **magnificación del dock, los efectos de minimizar o
   cualquier animación deben renderizarse en GPU** (draw calls por frame), nunca
   CPU + subida por frame: la CPU no produce 60 fps de una barra sin hundir el
   hilo de dibujo.

## Por qué la medición (USS + GPU) sostiene el cambio

El criterio de la revisión era: si el coste exclusivo real (USS + GPU) de
menubar y dock era ≲ 35 MB cada uno, camino único EGL; si era claramente mayor,
detenerse y reportar. Medido en reposo (`smaps_rollup` + `amdgpu_top -p`):

- **El wallpaper MEJORA** con EGL (RSS −4 %, GTT −3 MiB; USS igual): su textura
  va directo por dmabuf y el compositor no re-uploada shm.
- **menubar/dock**: USS 1.1→19.2 y 1.3→19.5 MB (+18 MB cada uno, el contexto
  GLES), GPU 22 MiB por proceso (contexto del driver + swapchain, **exclusiva
  por proceso**). El RSS (83 MB) era engañoso: casi todo libs compartidas.
- Suma estricta USS + GPU ≈ **41 MB**, **borderline** sobre los 35 pactados;
  **no "claramente mayor"** (el USS — la medida exclusiva — es ~19 MB, bajo el
  umbral; la GPU, pese a ser exclusiva por proceso, está en el borde pero no
  supera el criterio de forma clara). Por eso se procede con la opción 1.

El dato que habría detenido el cambio (wallpaper claramente peor con EGL) no se
dio; al contrario, el wallpaper mejoró.

## Consecuencias

- Un solo camino de presentación en la shell; `wl_shm` desaparece de ella.
- La subida de textura de menubar/dock es **parcial** (la región dañada, con
  `GL_UNPACK_ROW_LENGTH`), no el frame entero: el coste de redibujar el reloj
  cada minuto o cambiar una baldosa es mínimo.
- Las animaciones futuras (magnificación del dock, minimize) son trabajo de
  GPU en `celestial-kit`, no subidas de CPU.
- Cada componente de texto (menubar, dock, lock) paga ~18 MB de USS por su
  contexto GLES. Es el coste aceptado de "presentar por EGL".
- El reloj del menubar commitea al minuto (timer alineado), así que en reposo
  la shell no genera flujo continuo.
- Los buffers de la shell entran por los caminos ya verificados en 7a/7b
  (importación dmabuf en el drenado, blocker pre-commit, release diferido por
  fence). La shell es ahora un cliente dmabuf de pleno derecho.

## Relación

- ADR 0024 (stack de render del toolkit: GLES + cosmic-text) y ADR 0025 (shell
  nativa). ADR 0026 (dmabuf + explicit sync: la shell los consume).
- `docs/bloques/07-linux-dmabuf-y-explicit-sync.md` — el registro por bloque,
  con la tabla de memoria completa.