# Documentación de yuki

Índice de toda la documentación del proyecto. La estructura va del plan general a
lo concreto: primero la visión del proyecto, luego el **roadmap** (el plan de
bloques), después cada **bloque implementado**, y por debajo las **decisiones**
(ADRs) y las **investigaciones**.

## Mapa de la documentación

```
docs/
├── README.md          ← este índice
├── yuki.md         visión general del proyecto y cómo desarrollarlo (dev shell, dev.sh)
├── glossary.md        glosario de nombres (celestial-kit, grade-graphics, ...)
├── roadmap.md         EL ROADMAP: plan maestro por bloques (0–11 + lo siguiente)
├── bloques/           qué se ha hecho e implementado, bloque por bloque
│   ├── 00-cimientos.md
│   ├── 01-sesion.md            (cubre los Bloques 1–3)
│   ├── 02-ventana-en-pantalla.md
│   ├── 03-entrada.md
│   ├── 04-ventanas-manipulables.md
│   ├── 05-popups.md
│   ├── 06-portapapeles.md
│   └── 07-linux-dmabuf-y-explicit-sync.md
├── decisiones/        ADRs: las decisiones técnicas documentadas (0001–0028)
│   └── README.md      índice de ADRs y decisiones abiertas
├── arquitectura/
│   ├── yuki-y-shell.md        cómo se reparten el trabajo compositor ↔ shell nativa
│   ├── yuki-y-shell.svg       diagrama de la división de trabajo
│   ├── modelo-de-procesos.md  distribución de procesos del shell (ADR 0023)
│   ├── wallpaper.md           el componente nativo del wallpaper (ADR 0023/0024)
│   ├── menubar.md             la menubar nativa (app activa + reloj)
│   ├── dock.md                el dock nativo (lista de apps + activar al clic)
│   └── lock.md                el lock nativo (ext-session-lock-v1)
├── configuracion/
│   └── opciones.md    catálogo canónico de opciones del compositor (ADR 0021)
├── hardware.md        línea base de hardware/gráficos de la máquina
└── investigacion/
    ├── como-renderiza-macos.md   investigación a fondo del renderizado de macOS
    ├── wayland-vs-macos.md       comparación con Wayland y qué probar en yuki
    └── otto-y-forks.md           Otto y los forks de HBlanqueto: qué reutilizar
```

> **`AGENTS.md` en la raíz**: guía obligatoria para agentes de IA. Contiene la
> **POLÍTICA DE DISEÑO** (ADR 0020) — chrome separado (GTK/Libadwaita conserva su
> apariencia; Qt y demás reciben el chrome de Yuki) y efectos para todas las
> ventanas (inner border, sombra, blur). Si vas a tocar diseño, léelo.
>
> **Avisos de terceros.** `NOTICE` debe actualizarse en el mismo commit en que se copie código de un tercero (Otto incluido).

## Cómo leerlo

1. **[`yuki.md`](yuki.md)** — qué es el proyecto y cómo compilarlo/ejecutarlo.
2. **[`glossary.md`](glossary.md)** — los nombres del proyecto y su reparto
   (CelestialKit ≈ AppKit, Grade Graphics ≈ Core Graphics/Core Text, Grade
   Animation ≈ Core Animation, Infinity ≈ SkyLight).
3. **[`roadmap.md`](roadmap.md)** — el **ROADMAP básico**: la lista canónica de
   bloques, su estado (hecho / no empezado) y sus pruebas de aceptación. Es la
   fuente de verdad sobre qué hay planeado.
4. **[`bloques/`](bloques/)** — el detalle de **cada cosa implementada**: lo que
   construyó cada bloque, las divergencias con el brief y cómo se verificó.
5. **[`decisiones/`](decisiones/)** — las decisiones de diseño detrás de la
   implementación (ADR 0001–0028, índice en [`decisiones/README.md`](decisiones/README.md),
   que incluye la lista de decisiones abiertas). Referenciadas desde los bloques.
6. **[`arquitectura/`](arquitectura/)** — cómo se reparten el trabajo Yuki
   (compositor) y la shell nativa (`yuki-shell`, ADR 0025): documento + diagrama
   SVG, el [modelo de procesos](arquitectura/modelo-de-procesos.md) del shell
   (ADR 0023), y los componentes nativos: [wallpaper](arquitectura/wallpaper.md),
   [menubar](arquitectura/menubar.md), [dock](arquitectura/dock.md) y
   [lock](arquitectura/lock.md).
7. **[`configuracion/opciones.md`](configuracion/opciones.md)** — el catálogo
   canónico de opciones del compositor, con **nombres y valores idénticos a macOS**
   (secciones tipo System Settings, claves tipo `com.apple.dock`). Un archivo
   **`.domain`** por dominio (extensión propia, como `.plist`); comando **`domain`**
   (sintaxis `defaults`) y app de ajustes después, ADR 0021.
8. **[`hardware.md`](hardware.md)** — referencia del hardware/GPU contra la que se
   comparará el hito Direct Session (DRM/KMS).
9. **[`investigacion/`](investigacion/)** — material de investigación: cómo
   renderiza macOS su escritorio, cómo se compara con las opciones Wayland y qué
   se puede reutilizar de [Otto y sus forks](investigacion/otto-y-forks.md).

## Estado actual en una línea

Los **Bloques 0–6** están implementados (scaffolding, conexión de clientes,
ventana en pantalla, input, ventanas manipulables, popups y portapapeles).
El **Bloque 7** está hecho: **7a** (linux-dmabuf v5 con feedback), **7b**
(explicit sync + sync implícito, release diferido por fence) y **7c** (la shell
presenta por EGL, un solo camino sin `wl_shm`). **7.1** (hecho) arregla el
texto de la shell (`shape_until_scroll` + formato Alpha de swash), mide el daño
real y añade la **captura de depuración** (volcado PNG por `SIGUSR1`).
**7.2** (hecho) corrige el **bug de la matriz UV** del present EGL (column-major
sin flip) que ocultaba los tiles del dock y el texto del lock, y re-verifica
los criterios visuales de los bloques 2–6 por captura. **7.3** (hecho) añade la
**verificación por patrón** (4 cuadrantes), el **test de regresión UV** en
`celestial-kit`, un **verificador por regiones** (`scripts/verify-captures.py`),
determina que **la titlebar nunca se implementó** (solo se reserva la banda), y
corrige el **BAD_ALLOC de la captura** (el readback offscreen desenlazaba la
superficie EGL de la ventana; la captura ahora corre antes del render de la
ventana). Ya corren wezterm,
Firefox, vkcube y mpv acelerados dentro de Yuki anidado.
El **Bloque 8** (Direct Session, DRM/KMS) está planificado. La shell es nativa
(`yuki-shell`, ADR 0025): **wallpaper**, **menubar**, **dock** y **lock**
funcionan como procesos propios. Ver [`roadmap.md`](roadmap.md).