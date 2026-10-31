# Investigación: conclusiones

Resumen de lo decidido tras leer macOS, GNOME y Otto. El texto completo de cada
informe vive en git; se cita el commit anterior al borrado de `docs/investigacion/`.

## Cómo renderiza macOS

- Modelo cliente-servidor estricto: cada app dibuja su ventana y **WindowServer**
  compone, anima y enruta eventos; es display server, compositor y router.
- Core Animation separa modelo, presentación y render tree; el render tree vive
  en WindowServer, que compone en GPU con Metal.
- Los buffers se comparten por memoria compartida/IOSurface y fences; la IPC es
  Mach, no un protocolo de red.
- Se toma el reparto «la app pinta, el compositor mezcla», los render elements +
  damage de Smithay, los efectos de compositor y, con DRM, direct scanout y
  dma-buf. No se copia nada del protocolo privado de Apple.
- Decisión: el «feeling macOS» se construye con escena/efectos propios, no
  reimplementando WindowServer.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/como-renderiza-macos.md`

## macOS frente a Wayland

- Wayland y macOS comparten el reparto: la app dibuja, el compositor compone.
  Coinciden en buffers de GPU, fences, damage, vsync y direct scanout.
- La diferencia es que WindowServer retiene un render tree por ventana y puede
  animar sin el cliente; en Wayland el compositor solo tiene texturas.
- Se toma el pipeline de render elements + damage de Smithay, los efectos
  compositor-side y el direct scanout de DRM. No se acopla el compositor a un
  toolkit ni se copia el protocolo de Apple.
- Por qué: es el camino que ya usan cosmic-comp/KWin sobre el mismo stack y
  evita inventar una escena propietaria.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/wayland-vs-macos.md`

## Escalado de macOS

- macOS separa puntos lógicos, backing store (1× o 2×) y píxeles del panel; el
  escritorio se compone a 2× y se reduce al panel (supersampling global).
- No hace escala fraccional por app: un `backingScaleFactor` de 1.5 no existe.
- Se toma la separación `Logical`/`Physical`/`Buffer` y la escala por pantalla
  (8b). No se finge 1.5 en el cliente ni se renderiza todo a 2× sin medir.
- El supersampling global queda como futuro opcional, solo tras cerrar 8b, 9,
  11 y 12 y con medidas reales de GPU/memoria.
- Por qué: en una iGPU Vega el coste de memoria/ancho de banda no es trivial y
  `wp_fractional_scale_v1` ya resuelve la nitidez por superficie.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/escalado-macos.md`

## Otto y sus forks

- No se usa `smithay-otto`: sus parches son Vulkan/Intel y MalaysiaDreams es
  GLES/AMD. Se sigue Smithay upstream anclado por revisión (ADR 0010/0028).
- De `layers` solo se toma el resorte (duración + rebote), reimplementado en
  Grade Animation; `laye-rs` no tiene licencia y no se usa.
- De `otto-kit` se portan estados y protocolos, no dibujo: bucle, key repeat,
  escala, menús, DBusMenu, portal, desktop entries, iconos y accesibilidad.
- Otto se lee como referencia para udev/input, fractional scale, PAM/lock,
  xdg-activation y appmenu; render, planos, backdrop y screencopy no se portan.
- Si se copia una línea MIT de Otto, el aviso entra en `NOTICE` en el mismo
  commit. El ADR 0034 fijó BarrierKit sobre Freya y dejó `otto-kit` como
  referencia de lógica, no como base.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/otto-y-forks.md`

## GNOME como referencia

- GNOME es referencia, no dependencia: Mutter/Shell/session son GPL y los
  portales LGPL; se leen ideas y contratos y se reimplementa en Rust.
- De Mutter se toman modelos para 8b/8c/9/11 (outputs, escala, planos,
  fractional scale, activation, cursor-shape, constraints, presentation).
- De GNOME Shell se toma la UX y el modelo de estados de barra, dock, lock,
  notificaciones y OSD; no se copia JS/Clutter.
- Portal: backend propio solo para lo que depende del compositor (Settings,
  Screenshot, ScreenCast) y GTK para FileChooser/Print/Notification.
- Por qué: menos código, licencias limpias y un fallback estándar para lo
  genérico.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/gnome.md`

## Menú global

- Se elige `org_kde_kwin_appmenu` + `com.canonical.dbusmenu` (ADR 0036/0040):
  el compositor solo enruta y la shell consume el árbol D-Bus.
- De Otto/Noctalia se toma la dirección del protocolo y el cliente dbusmenu; el
  dibujo sigue siendo de BarrierKit.
- Se descarta AT-SPI (menús perezosos y Firefox se queda fijado), el Registrar
  de Canonical (IDs de X11), `xdg-dbus-annotation` (no aterrizó) y leer menús
  en el compositor (rompe el reparto del ADR 0024).
- Las fuentes GPL se leen como comportamiento; las MIT/Apache, con aviso en
  `NOTICE` en el mismo commit.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/global-menu.md`

## Portapapeles de capturas

- Wayland usa un modelo *pull*: el dueño publica MIME types y el compositor
  entrega un fd por pega; la selección sigue el foco de teclado.
- MalaysiaDreams es el dueño de la captura: PNG en `image/png`, sin fichero si
  `target = "clipboard"`; el default es el Escritorio, como macOS.
- Se toma de GNOME que el compositor sea el dueño y de macOS el destino
  configurable y el modificador `Control` (aún pendiente).
- La escritura al fd va en un hilo para no bloquear el bucle; no hay gestor de
  portapapeles y la selección dura hasta que otro cliente copie.
- `freya-clipboard` sigue arrastrando SCTK/calloop duplicados; el portapapeles
  propio del kit vive en barrierkit.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/portapapeles-capturas.md`

## Resize por bordes y cursores

- Implementado: banda exterior de 10 px, «L» de esquina de 20 px (2 px hacia
  dentro), prioridad de botones de titlebar, solo SSD y sin maximizado/
  fullscreen. Un punto 30 px dentro de una esquina es contenido.
- GNOME, Otto y macOS coinciden en banda exterior y cursores por lado; se toma
  esa geometría y se descarta el resize por cuadrados interiores.
- Pendiente: sumar la sombra/CSS al área de agarre, revisar esquinas
  redondeadas y excluir tiled cuando exista tiling.
- Por qué: la banda exterior no roba clics al contenido y funciona con CSD.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/resize-bordes-y-cursores.md`

## ChromaLeon y los acentos

- Se toma el ajuste de luminosidad HSL `L` para alcanzar contraste WCAG contra
  los fondos reales y subir la saturación si el acento queda plano.
- No se toma el primer píxel como representante ni su detección de grises;
  MalaysiaDreams usa un histograma ponderado por croma y vecindario 3×3×3.
- Por qué: el color dominante debe sobrevivir como acento legible, no solo ser
  el píxel más repetido.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/chromaleon.md`

## Acento y tinte del wallpaper

- Se deriva el acento y el tinte de ventana del color dominante del fondo, con
  contraste WCAG mínimo contra las superficies de la interfaz.
- El trabajo toca barrierkit (helpers de color, `vow-appearance`, tokens) y
  malaysia (vecindario 3×3×3 en `dominant()` e integración visual).
- Límite conocido: un amarillo puro con `L = 0.65` no cumple a la vez contraste
  de ventana ≥3 y texto blanco ≥3; se prioriza la ventana.
- La prueba visual solo cubrió fondo amarillo; la lógica está cubierta por
  tests unitarios. MalaysiaDreams depende de la rama `for-malaysia` de
  barrierkit hasta migrar la API.

Texto completo: `git show 71600868b840a5e308ee645b08e7d26f2f91e51f:docs/investigacion/informe-wallpaper-accent.md`
