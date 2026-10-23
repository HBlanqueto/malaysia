# 0024 — Stack de render del toolkit del shell (propuesta)

Estado: **aceptado** (opción A), **modificado para los clientes por
[0033](0033-skia-en-clientes-y-celestialkit-desde-otto-kit.md) y después por
[0034](0034-celestialkit-sobre-freya.md)**: los procesos cliente usan
CelestialKit sobre los crates de núcleo de Freya con Skia; el compositor sigue
con GLES + `cosmic-text` + `grade-graphics`.

Este ADR presenta las opciones para el stack de render del toolkit de la shell
(ADR 0023) y sus ventajas y desventajas sobre el hardware de la máquina, y fija
la decisión con sus parámetros técnicos.

## Contexto

El ADR 0023 define el shell como binarios Rust propios sobre un toolkit propio
(`<toolkit>`, nombre pendiente), un proceso por componente (wallpaper, menubar,
dock, lock). Cada componente es un cliente Wayland que pinta su propia superficie
layer-shell. El toolkit necesita un stack de render.

El compositor renderiza hoy con `renderer_gl` de Smithay (GLES2 vía glow — ADR
0009). El criterio clave de esta decisión:

> **El compositor y la shell comparten los shaders GLSL** (a través del crate
> `grade-graphics`), para que el SDF del squircle y el dibujo de la titlebar se
> escriban **una sola vez** y se reutilicen en ambos lados (el compositor dibuja
> decoraciones server-side y efectos — ADR 0020 — y la shell dibuja menubar/dock
> con el mismo lenguaje). El toolkit (`celestial-kit`) lo usan la shell y las apps
> propias de Yuki, **no el compositor**. Nombres: [`docs/glossary.md`](../glossary.md).

### Hardware (referencia, `docs/hardware.md`)

- AMD Ryzen 5 PRO 5650U, iGPU **Vega ("renoir")**, memoria unificada (APU).
- GLES **3.2** (radeonsi/ACO, Mesa 26.2.3) y Vulkan (RADV) disponibles.
- Memoria unificada: CPU y GPU comparten RAM; subir texturas es barato. Favorece
  por igual a las tres opciones y no discrimina entre ellas.

### Nota sobre el paso 1 (wallpaper)

El primer cliente (`yuki-shell wallpaper`) casi no depende de esta elección:
subir una textura y dibujarla con escala fraccional no necesita texto ni formas
complejas. La elección pesa de verdad en menubar/dock/lock (texto, squircle,
indicadores, blur), que vienen después.

## Opciones consideradas

### A. GLES (EGL) + `cosmic-text` + shaders propios (SDF) — el stack del compositor

El toolkit abre su propio contexto EGL/GLES (lo que ya hace el compositor), dibuja
texto con `cosmic-text` (rasterizando glifos a un atlas vía `SwashCache`) y las
formas (squircle, esquinas, gradientes) con shaders propios, igual que el
compositor.

Ventajas:

- **Cumple el criterio**: mismo stack (GLES, mismo lenguaje de shaders). El SDF
  del squircle y el dibujo de la titlebar se escriben una vez y viven en un crate
  compartido (`<toolkit>` o `yuki-design`); el compositor (decoraciones y efectos)
  y la shell (menubar/dock) los reutilizan.
- Dependencias nativas y ligeras: `cosmic-text` es Rust puro (shaping con
  harfrust, rasterización con swash, fuentes con fontdb; MIT/Apache). No arrastra
  un motor de render de terceros.
- Ya se conoce el terreno: el compositor renderiza con GLES; el riesgo es
  familiar. GLES 3.2 de esta máquina da compute shaders si hace falta blur con
  compute.
- Control total del dibujo (SDF, blur, alpha) sin luchar contra un canvas ajeno.

Desventajas:

- Hay que escribir mucho a mano: un mini-canvas 2D propio (rectángulos, SDF,
  gradientes, atlas de glifos, composición). Nada de curvas Bézier arbitrarias
  gratis: o se teselan o se dibujan como SDF.
- El texto funciona, pero es un motor de *texto*, no un renderer 2D: cacheo de
  glifos, kerning, emoji — todo integrado a mano contra el pipeline GLES.
- Más código propio que mantener, y la calidad del antialias/curvas depende de
  nosotros.

### B. Skia vía `skia-safe`

El toolkit dibuja sobre un canvas de Skia (backend GLES con Ganesh, feature `gl`),
creado sobre el contexto EGL. Texto con `textlayout` (Harfbuzz + ICU,
`skparagraph`). Es el renderer que usa Otto (`github.com/nongio/otto`).

Ventajas:

- Renderer 2D completo y maduro: formas, curvas, gradientes, blur, antialias,
  texto — todo listo. Skia es el motor de Chrome/Firefox.
- `skia-safe` tiene prebuilt binaries (`binary-cache`) y se mantiene al día
  (0.153.x, septiembre 2026; MSRV 1.85).
- Probado en un compositor Wayland con la misma filosofía (Otto).

Desventajas:

- **Violaría el criterio a menos que el compositor también adopte Skia**: el
  compositor seguiría en GLES y la shell en Skia → el squircle y la titlebar se
  escribirían dos veces, o habría que portar el render del compositor a Skia ahora
  (un cambio grande sobre el camino GLES ya probado).
- Dependencia pesada: Skia es enorme (decenas de MB, ~600K SLoC si se compila).
  Los prebuilt apuntan a glibc de Ubuntu 16+; en esta máquina (CachyOS) es un
  riesgo de ABI — probablemente haya que compilar de fuente (LLVM + Python +
  Ninja, build largo).
- Superficie de API enorme y bindings no perfectos (callbacks/clases virtuales sin
  envolver).

### C. Vello (wgpu)

El toolkit dibuja con Vello (`linebender/vello`), un renderizador vectorial
centrado en compute de GPU sobre wgpu (Vulkan en esta AMD). Es el backend de
render de Xilem.

Ventajas:

- Renderizado vectorial por GPU de alto rendimiento (paths, gradientes) y calidad
  de curva moderna. Tres variantes (GPU clásico, CPU, híbrido).
- Rust nativo del ecosistema Linebender; MIT/Apache. Esta iGPU Vega soporta
  compute (Vulkan/RADV).

Desventajas:

- **Estado alpha declarado por el propio proyecto**: blur y filtros en curso,
  caching de glifos en curso, artefactos de conflación. El blur es un requisito de
  diseño de Yuki (ADR 0020: blur/vibrancy del compositor) — no se puede basar el
  camino crítico visual en algo aún en desarrollo.
- **Stack distinto al del compositor** (wgpu/Vulkan contra GLES): el squircle y la
  titlebar no se comparten; el compositor tendría que adoptar wgpu o duplicar.
- Integrarlo "profundo" en un stack de UI requiere montar el contexto wgpu y el
  blit a la superficie GLES del cliente — más piezas de orquestación.
- MSRV alto (1.92) frente a 1.98 del proyecto — no es problema hoy, pero el ritmo
  de releases de wgpu es alto.

## Comparativa

| | A. GLES + cosmic-text + shaders | B. Skia (skia-safe) | C. Vello (wgpu) |
|---|---|---|---|
| Mismo stack que el compositor | **sí** | no (salvo portar el compositor) | no |
| SDF squircle / titlebar una sola vez | **sí** | no | no |
| Trabajo a mano propio | alto | bajo | medio |
| Texto | cosmic-text (integración propia) | skparagraph | vía swash/skrifa (WIP) |
| Blur / filtros | propio (control total) | completo | **alpha, WIP** |
| Madurez para una shell | alta (control total) | alta | baja-media |
| Peso de dependencia | ligero | muy pesado (ABI/build) | medio-alto (wgpu) |
| Ajuste al hardware (Vega, memoria unificada) | GLES 3.2 nativo | GLES ok; build arriesgado | Vulkan/RADV ok |

## Decisión (Aceptada)

**Se elige la opción A**: GLES (EGL) + `cosmic-text` + shaders propios (SDF), el
mismo stack que el compositor. El squircle SDF y el dibujo de la titlebar se
escriben **una sola vez** y se comparten entre compositor y shell **por el GLSL**
(a través del crate `grade-graphics`), no por el código GL.

El toolkit es **`celestial-kit`** (≈ AppKit): lo usan la shell y las apps propias
de Yuki, **no el compositor** — el compositor consume el GLSL compartido de
`grade-graphics`, igual que la shell. Ver [`docs/glossary.md`](../glossary.md).

Parámetros técnicos de la implementación:

1. **Shaders compartidos por GLSL, no por código GL.** Los shaders GLSL (squircle
   SDF, hairline, inner line, sombra 9-slice) y sus parámetros viven en el crate
   compartido **`grade-graphics`**, que usan el compositor y `celestial-kit`. Lo
   que se comparte es el **GLSL** (el texto del shader y sus uniformes), no el
   código GL de Rust (bindings, contextos, buffers).
2. **Íconos SVG con `resvg`**, rasterizados **una vez** a una textura al tamaño y
   escala de destino; nunca por frame.
3. **Texto con `cosmic-text` y un atlas de glifos.** El texto se re-rasteriza solo
   cuando cambia (cache del atlas); no se re-rasteriza en cada frame.
4. **Blur solo en superficies chicas y acotadas**, con un shader **dual-Kawase**
   (no blur de pantalla completa).

## El cruce de la decisión

El criterio (stack compartido → squircle/titlebar escritos una vez) es el que
separa las opciones: **solo A lo cumple sin cambiar el compositor**. B y C lo
cumplen únicamente si, además, se porta el render del compositor a Skia o wgpu
respectivamente — una segunda decisión de fondo. Para C, además, el estado alpha
del blur/filtros choca con un requisito de diseño ya aceptado (ADR 0020).

Decidir A es elegir "más código propio a cambio de un solo stack". Decidir B o C es
elegir "un renderer completo a cambio de dos stacks (o de portar el compositor)".
Se resuelve por **A**.

## Lo que no decide este ADR

- El nombre del toolkit (pendiente).
- La estrategia de blur/efectos del compositor (ADR 0020, en su momento).
- Si el compositor llegara a portar su render a otro motor (fuera de alcance hoy).

## Referencias verificadas

- `cosmic-text` 0.19.0 — shaping harfrust, rasterización swash (opcional), fuentes
  fontdb; Rust puro; MIT/Apache. El renderer se integra por el lado del consumidor.
- `skia-safe` 0.153.x — bindings de Skia; backend GLES con Ganesh (feature `gl`),
  `textlayout`/`skparagraph`; `binary-cache` con prebuilt para glibc Ubuntu 16+.
  Lo usa Otto (compositor completo sobre Skia).
- `vello` 0.10.0 — renderer por compute sobre wgpu 29; alpha declarado (blur y
  filtros, caching de glifos, memoria en curso); MSRV 1.92; backend de Xilem.
- Stack actual del compositor: smithay `renderer_gl` (GLES2, ADR 0009), hardware
  Vega "renoir" GLES 3.2 (ADR 0009 / `docs/hardware.md`).