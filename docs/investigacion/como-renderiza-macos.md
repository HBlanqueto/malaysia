# Cómo renderiza macOS su escritorio (a fondo)

Investigación sobre la arquitectura de renderizado y composición de macOS, desde los
procesos que intervienen hasta cómo se compone cada píxel final. El objetivo de este
documento es servir de referencia para entender el modelo que un compositor como
yuki podría emular. La comparación con Wayland y las opciones existentes están en
[`wayland-vs-macos.md`](wayland-vs-macos.md).

> **Aviso de fuentes.** Gran parte del comportamiento interno de macOS está
> documentado (el modelo de Core Animation y el papel de WindowServer son públicos en
> las guías de Apple y en papers de la WWDC), pero los detalles de bajo nivel (el
> diseño del proceso WindowServer, el render tree, la IPC exacta) no son públicos y
> provienen de análisis de ingeniería inversa, patentes de Apple y presentaciones.
> Donde algo no está confirmado se indica.

---

## 1. TL;DR

- macOS usa un modelo **cliente-servidor estricto**: las aplicaciones *dibujan su
  propia ventana* y un único proceso del sistema, **WindowServer**, *compone* todas
  las ventanas en la imagen final y la manda a la pantalla.
- WindowServer es a la vez **display server**, **compositor** y **router de eventos**.
  No ejecuta el código de dibujo de la aplicación (en el caso general): recibe
  *raster* (o capas) de cada app y lo mezcla.
- El mecanismo moderno es **Core Animation**: cada app mantiene un árbol de `CALayer`
  (modelo), Core Animation lo separa en tres árboles (modelo / presentación /
  render), y el render tree vive **en WindowServer**, que es quien ejecuta la
  composición y las animaciones en la GPU usando **Metal**.
- Los buffers se comparten entre app y compositor vía **memoria compartida**
  (backing stores) e **IOSurface** (memoria de GPU), sincronizados con **fences**.
- La comunicación es por **mensajes Mach** (MIG), no por un protocolo de red como X11.
- En resumen: **las apps pintan, WindowServer mezcla**. El compositor es un proceso
  separado, privilegiado y centralizado, exactamente el mismo reparto de trabajo que
  propone Wayland — pero implementado como un único componente privado del sistema.

---

## 2. Vista general del stack gráfico

```
┌────────────────────────────────────────────────────────────────────┐
│  Aplicaciones (AppKit / UIKit, Metal, Core Graphics)               │
│   · cada ventana dibuja su contenido (CPU o GPU)                  │
│   · mantiene un árbol de CALayer / CAMetalLayer                   │
│   · CATransaction.commit() → envía capas a WindowServer           │
└──────────────┬─────────────────────────────────────────────────────┘
               │  buffers (memoria compartida / IOSurface) + mach messages
┌──────────────▼─────────────────────────────────────────────────────┐
│  WINDOWSERVER  (proceso del sistema, user "_windowserver")         │
│   · CoreGraphics thread  → recibe ventanas, eventos, displays      │
│   · QuartzCore thread    → recibe los layer trees (render tree)    │
│   · compone todas las capas en una imagen por pantalla             │
│   · hace las animaciones y efectos (sombras, transparencia, dock)  │
│   · enruta los eventos de entrada a la app correcta                │
└──────────────┬─────────────────────────────────────────────────────┘
               │  comandos de GPU
┌──────────────▼─────────────────────────────────────────────────────┐
│  GPU (Metal) → framebuffer → escaneo al display (vsync)            │
└────────────────────────────────────────────────────────────────────┘
```

---

## 3. El proceso WindowServer

WindowServer es el corazón del sistema gráfico de macOS. Es un proceso único que
arranca junto con `loginwindow` y vive hasta que el usuario cierra sesión. Sin él no
hay GUI: si se cae, la pantalla se congela (compositing *y* entrada) y el sistema
intenta relanzarlo.

- **Usuario**: se ejecuta como el usuario especial `_windowserver` (menos privilegios
  que root, más que las apps normales).
- **Procesos asociados** (todos bajo `_windowserver`):
  - `MTLCompilerService` — compila y gestiona shaders/comandos Metal.
  - `distnoted` — notificaciones del sistema (no gráfico directamente).
- **Ubicación actual**: forma parte del framework privado **SkyLight**
  (`/System/Library/PrivateFrameworks/SkyLight.framework`), junto con el soporte de
  cursor/pointer.
- **Historia**: comenzó como **Quartz Compositor** en Mac OS X 10.0 (2001), pasó a
  formar parte de Core Graphics y hoy vive dentro de SkyLight. El motor de composición
  fue software (CPU) al principio, **Quartz Extreme** (10.2, 2002) lo movió a la GPU
  vía OpenGL, y hoy la composición usa **Metal** (OpenGL quedó deprecado).

### 3.1 Responsabilidades

1. **Composición**: toma las ventanas/capas de todas las apps, las posiciona, aplica
   transparencia, sombras, escala (p.ej. el Dock), y produce una imagen por pantalla.
2. **Display server**: gestiona los displays (configuración de hardware vía Quartz
   Display Services), múltiples monitores, mirroring y capturas de pantalla (de hecho,
   una captura de pantalla es "arrancar" esa composición ya hecha).
3. **Router de eventos**: sabe qué ventana está bajo el cursor o tiene el foco, y
   entrega los eventos (teclado, ratón, trackpad) a la app correcta (Quartz Event
   Services, event taps).

### 3.2 Hilos internos

Internamente (según análisis de ingeniería inversa) WindowServer tiene al menos dos
bucles de servidor dedicados:

- Un hilo que atiende las interfaces **CoreGraphics** (API `CGS*`): ventanas,
  superficies, displays, cursor, sesiones, transiciones.
- Un hilo que atiende las interfaces **QuartzCore** (API `CAS*`): los layer trees de
  Core Animation (el render tree) y las órdenes de composición.

Cada hilo recibe peticiones de *cualquier* proceso por mensajes Mach (MIG) y despacha
a las funciones del lado servidor según un message ID. Muchas de estas interfaces son
accesibles desde apps sandbox (con restricciones según la operación), lo que históricamente
las ha convertido en superficie de ataque (CVE-2014-1314, CVE-2016-1804, etc.).

---

## 4. Cómo pinta una aplicación (el lado cliente)

Una app de macOS, al abrir una ventana, hace esto:

1. Crea un `NSWindow`. Su *backing store* es un buffer (por defecto `CALayer`; con
   Metal, un `CAMetalLayer`).
2. Dibuja su contenido: con **Core Graphics** (2D, CPU) o con **Metal** (GPU). En el
   caso de Metal, la app renderiza directamente en un *drawable* de la capa.
3. Para mostrarlo, "hace commit" de los cambios de la capa. El contenido rasterizado
   queda en un buffer compartido que WindowServer puede leer.

### 4.1 Cómo se comparten los buffers

- **Ventanas clásicas**: el compositor asigna una región de **memoria compartida**
  (`CG backing stores`, regiones `SM=SHM` visibles con `vmmap`) que queda mapeada en el
  espacio de direcciones tanto de la app como de WindowServer. La app dibuja ahí; al
  hacer flush, manda un mensaje Mach a WindowServer para que la componga. Históricamente
  esto era un *backing store por ventana*; hoy es un árbol de capas.
- **GPU (Metal/OpenGL, video)**: los buffers viven en memoria de GPU y se comparten con
  el compositor como **IOSurface** (el equivalente conceptual a un `dma-buf` en Linux).
  Con `CAMetalLayer`, el drawable es la IOSurface que WindowServer usará como textura.
- **Sincronización**: se usan **fences** (cercas) para que WindowServer no lea un
  buffer antes de que la GPU de la app termine de renderizarlo. La API de Core
  Animation expone el equivalente de *completion/presentation callbacks* (fences
  Metal, `createFencePort`).

### 4.2 La IPC

El protocolo entre apps y WindowServer no es un protocolo de red (a diferencia de
X11). Es **mach messaging** generado con MIG (Mach Interface Generator): funciones
cliente del tipo `CGS*`/`CAS*` que empaquetan un mensaje, lo envían por un mach port
y el hilo servidor lo despacha. Las APIs de dibujo (Core Graphics, Metal) no hablan
directamente con WindowServer; Core Animation es el que serializa las capas.

---

## 5. Core Animation: los tres árboles

Core Animation es la razón por la que macOS puede animar la interfaz de forma fluida
**aunque el hilo principal de la app esté ocupado**. La clave es que no es la app la
que hace las animaciones: las declara, y WindowServer las ejecuta.

Cada `CALayer` visible pertenece a tres árboles paralelos:

1. **Model tree** (árbol modelo) — en la app. Contiene los valores "finales" de las
   propiedades. Es la fuente de verdad del programador.
2. **Presentation tree** (árbol de presentación) — en la app (o accesible desde ella).
   Contiene los valores *interpolados en vuelo* durante una animación.
3. **Render tree** (árbol de render) — **privado, vive en WindowServer**. Es lo que
   WindowServer usa para generar comandos de GPU. Se actualiza a la frecuencia del
   display (vsync), independientemente del hilo de la app.

### 5.1 Transacciones (`CATransaction`)

Todos los cambios a capas se agrupan en **transacciones** (implícitas o explícitas).
Al hacer `commit`:

- La app envía a WindowServer el estado de sus capas (contexto identificado por un
  **`CAContext`**, con su `contextID`).
- Un `CAContext` puede alojar el árbol de capas de una app en *otro* proceso (remote
  hosting): es como Safari aloja cada pestaña, o como se alojan las capas de una app
  embebida. Esto es conceptualmente parecido a los *subsurfaces* de Wayland, pero el
  hosting es entre procesos distintos.
- Los **fences** permiten que el commit del host espere a que el proceso remoto
  termine su commit (sincronización de frames entre procesos).

### 5.2 Qué hace WindowServer con el render tree

Según la patente de Apple (US 9,019,300) y el análisis de Core Animation:

- El render tree es una copia de las capas del modelo pero **sin interfaz con la app**.
- Cuando la app cambia propiedades, esos cambios "percolan" al render tree en un modelo
  transaccional (commit).
- El proceso de animación/composición, **independiente de la app**, determina qué
  animación aplicar y la aplica sobre el render tree.
- El render engine recorre el render tree **a la frecuencia del display**, renderizando
  solo las regiones que cambiaron, y dibuja al framebuffer.

Es decir: el compositor puede sintetizar una representación actualizada de las capas
**sin llamar de vuelta a la aplicación**. Esto es una diferencia fundamental con la
mayoría de compositores Wayland, que solo texturizan buffers de píxeles y no guardan un
escenario vectorial retenido de cada ventana (ver `wayland-vs-macos.md` §7).

### 5.3 Mecanismo de animación

- Las animaciones se declaran en las capas (implícitas por defecto al cambiar una
  propiedad, o explícitas con `CAAnimation`).
- Los valores interpolados se exponen en el **presentation tree**.
- El render tree + el proceso de animación, ambos en WindowServer, son los que
  producen cada frame interpolado en la GPU.

---

## 6. El pipeline de composición

El paso final lo hace WindowServer, así:

1. Recibe las capas/buffers de cada app (o los toma de las IOSurfaces ya renderizadas).
2. Construye una **imagen por capas** (layered image): coloca cada ventana según su
   posición, Z-order, opacidad, transformación (escala, rotación) y efectos.
3. Aplica los efectos del sistema: transparencia, **drop shadows**, escala del Dock,
   esquinas redondeadas de las ventanas, transiciones (Mission Control, Exposé,
   Spaces).
4. Pasa la imagen al driver gráfico (a través de **Metal**), que la renderiza en el
   framebuffer y la escanea al display sincronizado con **vsync** (Display Link /
   CADisplayLink para animación al refresco del monitor).
5. Gestiona múltiples displays, mirroring, VRR (ProMotion) y capturas (screencast).

Históricamente:

- **10.0–10.1**: composición por CPU (el compositor "mezclaba" capas en software).
- **10.2+ (Quartz Extreme)**: la composición pasa a la GPU. "El escritorio es solo una
  escena 3D". Cada backing store es una textura; el compositor, vía OpenGL, hace el
  blending, las sombras y el escalado. DMA: la GPU lee los buffers directamente.
- **10.4 (QE2D)**: Quartz 2D acelera también el trazado de la app (las operaciones de
  dibujo 2D se ejecutan en GPU).
- **Hoy**: la composición del sistema se hace con **Metal**; OpenGL está deprecado. La
  app puede renderizar con Metal directamente a IOSurface; WindowServer la compone.

### 6.1 Direct-to-display

Cuando una app está en **pantalla completa, opaca y con `CAMetalLayer`** en Apple
Silicon, su drawable puede ir **directo al display** (direct-to-display): el hardware
compone el drawable a la pantalla sin pasarlo por el paso general de composición, con
escalado de alta calidad. Esto es el equivalente exacto del **direct scanout** de los
compositores Wayland/DRM (ver `wayland-vs-macos.md` §6.2).

---

## 7. Enrutado de eventos

WindowServer es también quien **ruta la entrada**:

- Recibe los eventos HID (teclado, ratón, trackpad, lápiz) del kernel.
- Hace hit-testing sobre su escena: sabe qué ventana está bajo el cursor y qué app es
  la frontal, por lo que entrega cada evento a la app dueña de la ventana
  correspondiente (y con las coordenadas correctas, aunque la ventana esté escalada o
  rotada por un efecto).
- Mantiene el foco de teclado, y expone *event taps* y filtros (Quartz Event Services)
  para observar o alterar el flujo de eventos.
- En iOS ese papel se reparte entre **SpringBoard** (composición/escena) y
  **backboardd** (entrada); en macOS, WindowServer hace ambas cosas.

---

## 8. ¿El compositor "hace algo aparte"?

La pregunta clave. En macOS el reparto es:

| Quién | Qué hace |
|------|----------|
| **La app** | Renderiza su propio contenido (CPU con Core Graphics, o GPU con Metal) en sus buffers/capas. Declara animaciones y transiciones en su árbol de capas. |
| **WindowServer** | No ejecuta el dibujo de la app. **Compone**: mezcla capas/buffers, aplica efectos globales (sombras, transparencia, zoom de pantalla, Dock, transiciones de Spaces), **anima** el render tree de cada app a la frecuencia del display, **gestiona displays** (modo, mirroring, capturas) y **enruta eventos**. |
| **Kernel (IOKit / GPU)** | Framebuffer, modo de video, DMA, GPU. |

El matiz importante: WindowServer **sí tiene una copia retenida del escenario de cada
ventana** (el render tree de Core Animation). Esto le permite:

- Animar geometría/opacidad de cualquier capa sin re-rasterizar ni pedirle nada a la app.
- Re-renderizar contenido retenido en el render tree si es necesario (implicit
  animation, snapshots de ventana, Exposé/Mission Control que "encogen" ventanas).
- Generar capturas de ventana individuales (Command-Shift-4 + espacio) tomando la
  capa ya compuesta.

No ejecuta, en cambio, el código `drawRect:` de la app ni los shaders de juego de la
app: para contenido inválido, la app debe re-dibujar y hacer commit de nuevo.

---

## 9. Resumen del modelo conceptual

```
APP (proceso por app)                          WINDOWSERVER (proceso único)
─────────────────────                          ───────────────────────────
CALayer tree (modelo)        ──commit──►       render tree (por app/CAContext)
  + contenido raster (SHM/IOSurface)           ──Metal──► composición final
  + animaciones declaradas                     + efectos globales
                                               + animaciones (a vsync)
                                               + gestión de displays
                                               + enrutado de eventos
```

Las piezas que un compositor Wayland/yuki debería "copiar" del modelo macOS son
conceptuales, no de implementación:

1. **Composición centralizada en un proceso propio** (separado del WM y de las apps).
2. **Escena retenida por ventana** para poder animar/efectos sin re-render del cliente.
3. **Buffers de GPU compartidos** (IOSurface ≈ dma-buf) + fences de sincronización.
4. **Render a la frecuencia del display** con redibujado incremental (solo daño).
5. **Efectos globales** (sombras, esquinas, transparencia, zoom) aplicados por el
   compositor, no por cada app.
6. **Direct-to-display** para fullscreen opaco (≈ direct scanout).

La comparación detallada con lo que existe y lo que se podría probar en Wayland y en
yuki está en [`wayland-vs-macos.md`](wayland-vs-macos.md).

---

## Fuentes

- Apple, *Core Animation Programming Guide* — los tres árboles (model / presentation /
  render) y `CATransaction`.
- Apple, *Managing your game window for Metal in macOS* — `CAMetalLayer`, drawables,
  direct-to-display.
- Apple, WWDC 2002, *Exploring the Quartz Compositor* (Peter Graffagnino / Ralph Nicosia)
  — arquitectura original, backing stores, Quartz Extreme, DMA.
- Wikipedia, *Quartz Compositor*.
- Keen Security Lab, *WindowServer: The privilege chameleon on macOS* (2016) — proceso,
  hilos CGS/CAS, MIG, sandbox.
- The Eclectic Light Company (Howard Oakley), *WindowServer: display compositor and
  input event router* (2020) y *Explainer: WindowServer* (2022) — SkyLight, roles,
  MTLCompilerService, capturas.
- EthanArbuckle, *ios-rendering-docs* (GitHub) — CAContext / Render Server / backboardd
  (el homólogo iOS del mismo diseño).
- Aditya Vaidyam, *The Secret Life of Core Animation* (2018) — `CA::Render::`, contextos
  remotos, fences.
- Patente de Apple US 9,019,300 — render tree en proceso separado, transacciones,
  redibujado independiente de la app.
- John Siracusa / Ars Technica, *Mac OS X DP4* (2000) — Core Graphics Services y el
  modelo de composición por capas.
- flylib.com, *Mac OS X Internals* §2.10 — Quartz Extreme / QE2D / Core Image.