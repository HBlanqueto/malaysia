# macOS vs Wayland: cómo compone cada uno y qué opciones existen (y cuáles probar)

Este documento compara el modelo de renderizado de macOS (descrito a fondo en
[`como-renderiza-macos.md`](como-renderiza-macos.md)) con el ecosistema Wayland: arquitectura, compositores
existentes, y qué se **podría probar** en un compositor propio como yuki para
acercarse al look & feel de macOS.

> Referencia rápida del modelo macOS: una app dibuja su propia ventana (CPU o GPU),
> y un proceso central **WindowServer** compone todas las capas con Metal, hace las
> animaciones/efectos, gestiona displays y enruta eventos. Su escena retenida por
> ventana (render tree de Core Animation) le permite animar sin molestar a la app.

---

## 1. El modelo Wayland en una frase

En Wayland **el compositor ES el display server**: un solo programa hace de servidor
de protocolo, gestor de ventanas, compositor y router de eventos. Las aplicaciones
son clientes que renderizan su propio contenido y le pasan **buffers** al compositor;
este los compone con la GPU y los manda al monitor. Controla **KMS** (modo de video y
page flips) y **libinput** (entrada) directamente, sin un X server en medio.

```
CLIENTE (app)                       COMPOSITOR (un proceso)
─────────────────                   ──────────────────────────────
renderiza su surface                protocolo Wayland (wl_surface, xdg_*)
   wl_shm / linux-dmabuf  ──►       scene graph (escena de superficies)
   wl_surface.commit + damage       EGL/GLES (o Vulkan) → composición
                                    damage tracking → redibujado parcial
                                    DRM/KMS → page flip al display
                                    input (libinput) → hit-testing → eventos
```

Diferencias estructurales clave con macOS:

| Aspecto | macOS | Wayland |
|--------|-------|---------|
| Proceso de composición | Siempre uno: WindowServer | Uno por compositor; *generalmente* un proceso (a veces con threads por output) |
| WM separado del compositor | Dentro de WindowServer | Dentro del mismo proceso del compositor (aunque el "shell" puede ser otro proceso, p.ej. Plasma → KWin) |
| La app "declara" capas/animaciones | Sí: CALayer/CATransaction, y WindowServer las ejecuta | No por protocolo: el cliente entrega píxeles y daño; las animaciones las hace el compositor por su cuenta |
| Escena retenida del cliente en el servidor | Sí: render tree en WindowServer | Parcial: solo subsurfaces jerárquicas y el scene graph del compositor; el contenido del cliente es un buffer, no un escenario vectorial retenido |
| Buffers de GPU compartidos | IOSurface + fences Metal | `linux-dmabuf` + fences (drm-syncobj / in/out fences) |
| Fullscreen sin composición | direct-to-display (CAMetalLayer opaco) | direct scanout (DRM primary/overlay plane) |
| Protocolo | Mach MIG (privado) | Wayland (abierto, extensible vía protocolos `zwp_*`/`wp_*`) |

---

## 2. Cómo se compone un frame en Wayland (en general)

1. **El cliente renderiza** su surface (con EGL/Metal/Vulkan/CPU) en un buffer:
   - `wl_shm` (memoria compartida, CPU) o
   - `zwp_linux_dmabuf_v1` (memoria de GPU, cero copias).
2. Hace `wl_surface.commit()` indicando el **daño** (qué región cambió).
3. El compositor convierte el buffer en una **textura** (EGLImage vía
   `EGL_WL_bind_wayland_display`, o `glEGLImageTargetTexture2D`), espera el **fence**
   del cliente (drm-syncobj) y lo agrega a su **scene graph**.
4. Con **damage tracking** (frame damage + buffer damage con `EGL_EXT_buffer_age`)
   recomposita solo las regiones cambiadas sobre el framebuffer de salida.
5. Manda el resultado con un **page flip** (DRM) alineado al vsync. Si hay un solo
   surface fullscreen opaco, puede usar **direct scanout** y saltarse la composición.
6. Envía `wl_surface.frame` / `wp_presentation_feedback` para que el cliente sepa
   cuándo se mostró su frame (sincronización app-compositor).

### 2.1 Damage tracking (lo que WindowServer también hace, a su modo)

Es el equivalente de "redibujar solo lo que cambió". Conceptos:

- **Frame damage**: lo que cambió entre el frame anterior y el actual (lo da el cliente).
- **Buffer damage**: lo que hay que redibujar al reutilizar un buffer viejo en una
  swapchain (se deriva acumulando frame damage de frames previos con buffer age).
- **Oclusión**: si una ventana opaca cubre a otra, el compositor puede no renderizar
  la de abajo (visibility culling en wlroots, `opaque_regions` en smithay).

---

## 3. Compositores Wayland existentes y su arquitectura

### 3.1 Weston / libweston — el compositor de referencia
Compositor de referencia del proyecto Wayland. Sencillo, sirve de banco de pruebas.
Módulos y un compositor headless/windowed. Demuestra el pipeline mínimo: composición
por EGL/GLES2 + KMS.

### 3.2 wlroots (Sway, Hyprland, river, Wayfire, ...) — la biblioteca reutilizable
No es un compositor, es una librería que da el "cascón": backends (DRM, Wayland
anidado, X11, headless, RDP), renderer (OpenGL/GLES2, Vulkan en desarrollo), e input
(libinput). Sus piezas relevantes:

- **`wlr_scene`**: scene graph declarativo en 2D con nodos (`wlr_scene_buffer`,
  `wlr_scene_rect`, `wlr_scene_tree`). Soporta solo operaciones 2D básicas; para algo
  más complejo el compositor debe renderizar a mano.
- **Damage ring** por output, occlusion culling y **direct scanout** automático
  (si un buffer cumple las condiciones, se salta la composición y va a un plane).
- **DMA-buf feedback**: si un buffer no es apto para scanout, avisa al cliente qué
  formato/modificador usar para que *pueda* serlo.
- Subsurfaces, xdg-shell, layer-shell, drag & drop integrados al scene graph.

### 3.3 Mutter (GNOME) — compositor + toolkit de composición
Compositor del proyecto GNOME (mutter.gnome.org). Corre sobre KMS + libinput.
Componentes: el **display server Wayland** (protocolo + extensiones + Xwayland),
una **capa de composición con scene graph** (historically Clutter, ahora un sistema
de actores/render nodes) con animación y enrutado de input, y una **capa de
aceleración** (MetaRenderer) que gestiona framebuffers, texturas e importación usando
OpenGL. GNOME Shell se ejecuta *dentro* de mutter (enlazado), no como proceso aparte.

### 3.4 KWin (KDE Plasma) — compositor monolítico con framework de efectos
KWin es un servidor Wayland completo. Tiene su **propio scene graph** interno (no
depende de Clutter), un **OpenGL compositor** (con backend GLX/EGL y fallback
QPainter) y un **framework de efectos** (Present Windows, wobbly, blur, etc.) que
opera sobre el scene graph sin tocar OpenGL. Plataformas: DRM (OpenGL vía GBM),
windowed (X11/Wayland anidado), fbdev. Plasma (la "shell") corre como proceso aparte
y habla con KWin por KWayland. En el pasado eliminó el "unredirect fullscreen"
(prefería pasar el buffer directo al DRM = direct scanout).

### 3.5 Hyprland — wlroots + animaciones y efectos
Compositor wlroots con estética "macOS-ADJACENT". Relevante porque muestra cómo se
consiguen efectos tipo macOS sobre wlroots:

- **`CHyprRenderer`** + pipeline de **damage tracking** por buffer de la swapchain
  (daño transformado por viewport/scale/transform, scissor en shaders, discarding de
  regiones ocluidas por superficies opacas).
- **Animaciones** (workspaces, ventanas, bordes) generando daño para redibujar.
- Sombras, esquinas redondeadas y **blur de fondo** con shaders; el blur invalida
  regiones de forma optimizada.
- **Direct scanout bloqueado** cuando hay efectos complejos encima (porque el blur o
  las sombras requieren composición real).
- Renderer OpenGL + **renderer Vulkan experimental** (`RT_VK`).

### 3.6 cosmic-comp (System76) — compositor Rust sobre smithay
El compositor de COSMIC. Es el ejemplo más cercano a lo que yuki podría llegar a
ser (mismo stack: smithay + Rust). Puntos:

- **Backends**: KMS (producción, con un thread por output), X11 y winit (nested).
- **Renderer `GlowRenderer`** (OpenGL ES) con **shaders GLSL propios** para
  decoraciones: esquinas redondeadas (`ClippingShader`), sombras (`ShadowShader`),
  bordes de foco (`IndicatorShader`), fondo de overview (`BackdropShader`) y
  post-procesado (filtros de color/accessibilidad, `PostprocessShader`).
- **Elementos de render** (`RenderElement` de smithay): cada ventana/stack produce
  sub-elementos (decoración Iced, superficie del cliente, superficie con esquinas,
  sombra) y el compositor los ordena por z y los dibuja en orden.
- **Damage tracking** con `OutputDamageTracker` de smithay y **buffer age**.
- **Direct scanout / overlay planes** vía DRM (controlable con `COSMIC_DISABLE_*`),
  **VRR**, **animaciones** (workspace transitions, fullscreen) con easing.
- **Post-procesado y screencopy** integrados en el pipeline.

### 3.7 smithay — la librería Rust (lo que usa yuki)
Kit de herramientas Rust para escribir compositores. Da:
- Backends: `drm`, `winit`, `x11`, `wayland`, `headless`.
- Renderers: `gles2`, `glow`, `multigpu` (y `vulkan` experimental).
- **Sistema de render elements** (`Renderer::render`, `RenderElement`): el compositor
  construye una lista de elementos (superficie, textura, rectángulo, shader) y smithay
  los dibuja con damage/oclusión. `OutputDamageTracker` acumula el daño y aplica
  buffer age.
- `desktop::Space`, layer-shell, session-lock, dmabuf, etc.

### 3.8 niri — smithay + tiling "scrollable"
Compositor scrollable-tiling escrito con smithay. Su pipeline de render construye
elementos de smithay (`crate::compositor::render`) con rectángulos por ventana;
relevante como caso de un compositor pequeño y cuidado sobre el mismo stack que
yuki.

---

## 4. Tabla: concepto de macOS → equivalente Wayland

| Concepto macOS | Equivalente Wayland (existente) |
|----------------|---------------------------------|
| WindowServer (compositor central) | El compositor (Weston, mutter, KWin, wlroots*, smithay*) |
| Backing store / shared memory | `wl_shm` (buffers de memoria compartida) |
| IOSurface (memoria GPU) | `zwp_linux_dmabuf_v1` (dma-buf) |
| Fences Metal | drm-syncobj (`wp_linux_drm_syncobj_v1`), in/out fences |
| CALayer tree (modelo) | Árbol de surfaces del cliente (`wl_surface` + `wl_subsurface`) |
| Presentation tree | (no hay equivalente directo; el compositor interpola él mismo sus animaciones) |
| **Render tree en WindowServer** | **Scene graph del compositor** (`wlr_scene`, KWin scene, smithay elements) — con la diferencia de que el contenido del cliente es un *buffer*, no un escenario retenido |
| CATransaction (commit atómico) | `wl_surface.commit` (estado pending → current, atómico) |
| Damage / redibujado parcial | damage tracking (frame + buffer damage + buffer age) |
| Animaciones declaradas y ejecutadas por WS | Animaciones implementadas por el compositor (KWin effects, Hyprland, cosmic-comp, Wayfire) |
| Efectos globales (sombras, blur, esquinas) | Shaders del compositor (Hyprland, cosmic-comp, KWin) |
| Dock / Mission Control / Exposé | layer-shell + compositor effects (p.ej. overview en cosmic-comp) |
| Direct-to-display | Direct scanout (DRM primary plane) / overlay planes |
| VSync / CADisplayLink | DRM vblank + `wl_surface.frame` + `wp_presentation_feedback` |
| Router de eventos (WindowServer) | Seat del compositor (wl_seat, libinput, hit-testing) |
| Remote layer hosting (CAContext) | Subsurfaces / client-side decorations / layer-shell (parcial) |

---

## 5. Dónde Wayland NO llega hoy (comparado con macOS)

1. **No hay escena retenida del cliente en el compositor.** En macOS, WindowServer
   tiene el render tree y puede re-renderizar o animar la geometría de una capa sin
   avisar a la app. En Wayland, el compositor solo tiene píxeles; si quiere
   "encoger" una ventana (Exposé), debe escalar la *textura* (que es lo que hacen
   KWin/Hyprland), no re-renderizar el contenido. Efectos como blur *de contenido
   detrás* exigen re-composición del compositor, con coste.
2. **Las animaciones de sistema no las conoce el protocolo.** Cada compositor las
   implementa por su cuenta; macOS las tiene integradas en el sistema (window
   server anima genie, minimize, Spaces, etc. con consistencia global). En Wayland,
   la consistencia depende de cada compositor.
3. **El "shell" y el compositor suelen ser el mismo binario.** No es un problema,
   es una elección; pero significa que "el compositor" hace también de WM. En
   macOS el WM y el compositor están en WindowServer igualmente (así que en esto
   son parecidos).
4. **Xwayland como parche**: las apps X11 se renderizan a través de Xwayland
   (más o menos como correr apps de una era distinta dentro del compositor), sin
   equivalente en macOS.

---

## 6. Qué se podría probar en yuki

yuki es un compositor smithay (Rust), con renderer GLES2, backend winit (nested)
hoy, y el **milestone "Direct Session"** (DRM/KMS) planificado en el roadmap. Eso le
da un camino concreto para adoptar piezas del modelo macOS. Ideas ordenadas de menor
a mayor esfuerzo:

### 6.1 Ahora (sobre el stack actual: winit + GLES2)

- **Adoptar el sistema de `RenderElement` de smithay + `OutputDamageTracker`**
  para no redibujar la escena completa cada frame (hoy yuki renderiza por
  bloque; el roadmap ya contempla "damage"). Es la base para todo lo demás y es lo
  que hace cosmic-comp. *(Este es el equivalente del "redibujado incremental" de
  WindowServer.)*
- **Decoraciones y efectos por shader**: sombras y esquinas redondeadas propias
  (como `ShadowShader`/`ClippingShader` de cosmic-comp), usando `yuki-design`
  como fuente de los valores (ADR 0011: valores de diseño como constantes Rust).
  Sería el primer acercamiento al look macOS.
- **Animaciones en el compositor**: transiciones de foco/mapeo/redimensionado
  interpoladas por el compositor (no por el cliente), generando daño en cada frame
  (patrón Hyprland/cosmic-comp). Hoy el proyecto las tiene "hand-checked" y ninguna
  animación; esto es lo que más acerca la sensación macOS.
- **`wl_surface.frame` / `wp_presentation_feedback`** para sincronizar al cliente
  con el vsync (el análogo del "frame done" de Core Animation).

### 6.2 Con Direct Session (DRM/KMS)

- **Direct scanout y overlay planes**: pasar el buffer de una ventana fullscreen
  opaca directamente a un plane DRM (equivalent - direct-to-display de macOS).
  smithay ya expone esto; yuki solo puede usarlo cuando abandone winit.
- **DMA-buf (`zwp_linux_dmabuf_v1`)**: que los clientes compartan memoria de GPU
  (como las IOSurface), eliminando copias y habilitando el scanout.
- **Fences (drm-syncobj)** para sincronizar cliente/compositor correctamente.
- **Layer-shell** para un panel/menú fijo (el germen de una "menubar/dock" estilo
  macOS) y **workspace switching** con animación (el germen de Mission Control).

### 6.3 A más largo plazo / experimental

- **Escena retenida del cliente**: un protocolo (o un enfoque propio) donde el
  cliente declare *contenido vectorial* (rectángulos, texto, gradientes) en lugar
  de solo píxeles, para que el compositor pueda re-renderizar o animar sin que el
  cliente intervenga — el equivalente Wayland del render tree de Core Animation.
  Es un territorio sin protocolo estándar y sería investigación pura.
- **Renderer Vulkan** (smithay tiene uno experimental) para efectos más baratos
  (blur y sombras de alta calidad) — el equivalente de cuando macOS pasó de OpenGL
  a Metal.
- **Un "shell" separado** (proceso distinto hablando por protocolo con el
  compositor, como Plasma ↔ KWin) para un escritorio con la estética y la separación
  de responsabilidades de macOS, en vez de acoplar UI al compositor.

### 6.4 Lo que NO tiene sentido copiar

- El protocolo privado y monolítico de WindowServer (no es abierto, no es extensible).
- Que el compositor ejecute las animaciones de las apps por decreto: en Wayland eso
  sería trabajo de un protocolo futuro; hoy cada compositor decide.
- Acoplar el compositor a un toolkit de UI concreto.

---

## 7. Conclusión

macOS y Wayland comparten el mismo reparto fundamental — **la app dibuja, el
compositor compone** — y casi todas las optimizaciones coinciden por separado:
buffers de GPU compartidos, fences, damage, redibujado a vsync, direct-to-display /
direct scanout. La gran diferencia conceptual es que WindowServer retiene una
**escena (render tree) por ventana** y por eso puede animar y aplicar efectos sin
depender del cliente, mientras que un compositor Wayland solo tiene píxeles y debe
construir sus efectos (y su escena) por su cuenta sobre esas texturas.

Para yuki, el atajo hacia el "feeling macOS" no es reimplementar el protocolo de
Apple, sino: **(1)** adoptar el modelo de render elements + damage de smithay,
**(2)** sumar efectos de compositor (sombras, esquinas, blur) y **animaciones**
propias, **(3)** y, con Direct Session, habilitar direct scanout y dma-buf. Todo eso
es exactamente lo que ya hacen compositores como cosmic-comp sobre el mismo stack.

---

## Fuentes

- Wayland Project, *Architecture* — el modelo compositor=display server, escena,
  damage, KMS/evdev.
- Simon Ser (emersion), *Introduction to damage tracking* (2019).
- wlroots docs y `wlr_scene.h` — scene graph, damage ring, direct scanout, dmabuf
  feedback, occlusion culling.
- LWN, *The Linux graphics stack in a nutshell, part 2* — buffers, dma-buf, fences,
  DRM/KMS, page flip.
- KDE, Martin Gräßlin — arquitectura de KWin (scene graph, backends, EGL, efectos).
- Mutter docs (mutter.gnome.org).
- Hyprland DeepWiki — damage tracking, renderer, direct scanout bloqueado.
- cosmic-comp (System76) y su DeepWiki — pipeline de render, shaders, elementos,
  backends KMS/X11/winit.
- smithay docs y fuentes — render elements, `OutputDamageTracker`, backends.
- El roadmap y ADRs de este mismo repositorio (`roadmap.md`,
  `decisiones/`), sobre todo ADR 0011 (diseño como constantes) y el milestone
  Direct Session.