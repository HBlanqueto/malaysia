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
├── decisiones/        ADRs: las decisiones técnicas documentadas (0001–0027)
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
    └── wayland-vs-macos.md       comparación con Wayland y qué probar en yuki
```

> **`AGENTS.md` en la raíz**: guía obligatoria para agentes de IA. Contiene la
> **POLÍTICA DE DISEÑO** (ADR 0020) — chrome separado (GTK/Libadwaita conserva su
> apariencia; Qt y demás reciben el chrome de Yuki) y efectos para todas las
> ventanas (inner border, sombra, blur). Si vas a tocar diseño, léelo.

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
   implementación (ADR 0001–0026, índice en [`decisiones/README.md`](decisiones/README.md),
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
   renderiza macOS su escritorio y cómo se compara con las opciones Wayland.

## Estado actual en una línea

Los **Bloques 0–6** están implementados (scaffolding, conexión de clientes,
ventana en pantalla, input, ventanas manipulables, popups y portapapeles). El
**Bloque 7** está en curso: **7a** (linux-dmabuf v5 con feedback) y **7b**
(explicit sync + sync implícito, release diferido por fence) hechos — ya corren
wezterm, Firefox, vkcube y mpv acelerados dentro de Yuki anidado — y **7c** (la
shell presenta por EGL) pendiente.
El **Bloque 8** (Direct Session, DRM/KMS) está planificado. La shell es nativa
(`yuki-shell`, ADR 0025): **wallpaper**, **menubar**, **dock** y **lock**
funcionan como procesos propios. Ver [`roadmap.md`](roadmap.md).