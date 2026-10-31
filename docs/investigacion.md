# Investigación: conclusiones

Resumen de lo decidido tras leer macOS, GNOME y Otto. El texto completo vive en git, en el commit anterior al borrado de `docs/investigacion/`.

## Cómo renderiza macOS

- Cada app dibuja su ventana y **WindowServer** compone, anima y enruta eventos.
- Core Animation separa modelo, presentación y render tree; el render tree vive en WindowServer, que compone en GPU con Metal.
- Se toma «la app pinta, el compositor mezcla», los render elements + damage de Smithay, los efectos de compositor y, con DRM, direct scanout y dma-buf. No se copia el protocolo privado de Apple.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/como-renderiza-macos.md`

## macOS frente a Wayland

- Comparten el reparto y las optimizaciones (buffers GPU, fences, damage, vsync, direct scanout).
- WindowServer retiene un render tree por ventana; en Wayland el compositor solo tiene texturas.
- Se toma el pipeline de Smithay + efectos compositor-side + direct scanout; no se acopla a un toolkit ni se copia el protocolo de Apple.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/wayland-vs-macos.md`

## Escalado de macOS

- Separa puntos, backing store (1×/2×) y píxeles del panel; compone a 2× y reduce al panel.
- Se toma la separación `Logical`/`Physical`/`Buffer` y la escala por pantalla (8b). No se finge 1.5 ni se renderiza todo a 2× sin medir.
- El supersampling global queda como futuro opcional tras 8b/9/11/12 y con medidas reales.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/escalado-macos.md`

## Otto y sus forks

- No se usa `smithay-otto`: sus parches son Vulkan/Intel y MalaysiaDreams es GLES/AMD.
- De `layers` solo se toma el resorte; `laye-rs` no tiene licencia.
- De `otto-kit` se portan estados y protocolos, no dibujo: bucle, key repeat, escala, menús, DBusMenu, portal, desktop entries, iconos y accesibilidad. Otto se lee para udev/input, fractional scale, PAM/lock, xdg-activation y appmenu; render, planos, backdrop y screencopy no se portan.
- Si se copia una línea MIT, el aviso entra en `NOTICE` en el mismo commit; el ADR 0034 dejó BarrierKit sobre Freya.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/otto-y-forks.md`

## GNOME como referencia

- GNOME es referencia, no dependencia: Mutter/Shell son GPL y los portales LGPL; se reimplementa en Rust.
- De Mutter se toman modelos para 8b/8c/9/11; de GNOME Shell, la UX y el modelo de estados de barra, dock, lock, notificaciones y OSD.
- Portal propio solo para Settings, Screenshot y ScreenCast; GTK para FileChooser/Print/Notification.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/gnome.md`

## Menú global

- Se elige `org_kde_kwin_appmenu` + `com.canonical.dbusmenu`: el compositor enruta y la shell consume el árbol.
- De Otto/Noctalia se toma la dirección del protocolo y el cliente dbusmenu; el dibujo es de BarrierKit.
- Se descarta AT-SPI, el Registrar de Canonical, `xdg-dbus-annotation` y leer menús en el compositor. Las fuentes GPL se leen como comportamiento; las MIT/Apache, con aviso en `NOTICE`.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/global-menu.md`

## Portapapeles de capturas

- Wayland usa *pull*: el dueño publica MIME types y el compositor entrega un fd por pega; la selección sigue el foco.
- MalaysiaDreams es dueño de la captura: PNG en `image/png`, sin fichero si `target = "clipboard"`; default Escritorio.
- De GNOME se toma que el compositor sea el dueño; de macOS, el destino configurable y `Control` (pendiente). La escritura al fd va en un hilo; no hay gestor de portapapeles. `freya-clipboard` sigue arrastrando SCTK/calloop duplicados; el reemplazo vive en barrierkit.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/portapapeles-capturas.md`

## Resize por bordes y cursores

- Implementado: banda exterior de 10 px, «L» de esquina de 20 px (2 px hacia dentro), prioridad de botones, solo SSD y sin maximizado/fullscreen.
- GNOME, Otto y macOS coinciden en banda exterior y cursores por lado; se descarta el resize por cuadrados interiores. Pendiente: sombra/CSS en el área de agarre, esquinas redondeadas y excluir tiled.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/resize-bordes-y-cursores.md`

## ChromaLeon y los acentos

- Se toma el ajuste de luminosidad HSL `L` para contraste WCAG y subir saturación si el acento queda plano.
- No se toma el primer píxel como representante ni su detección de grises; MalaysiaDreams usa histograma ponderado por croma y vecindario 3×3×3.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/chromaleon.md`

## Acento y tinte del wallpaper

- Se deriva el acento y el tinte del dominante del fondo con contraste WCAG contra las superficies.
- Toca barrierkit (helpers de color, `vow-appearance`, tokens) y malaysia (3×3×3 en `dominant()` e integración visual).
- Límite: un amarillo puro con `L = 0.65` no cumple a la vez contraste de ventana ≥3 y texto blanco ≥3; se prioriza la ventana. La prueba visual solo cubrió fondo amarillo; la lógica tiene tests unitarios.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/informe-wallpaper-accent.md`
