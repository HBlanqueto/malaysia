# Decisiones (ADRs)

Índice de las decisiones técnicas y de las que siguen abiertas. Los ADR son el
registro del *porqué*; la numeración es continua.

> Los ADR del toolkit (BarrierKit, VowDesign, VowAppearance, Infinity) viven en
> [`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit)
> a partir del ADR 0048. Aquí queda un stub de una línea que remite allí.

## ADRs

| ADR | Título | Estado |
|---|---|---|
| 0001 | Tipos de coordenadas: Smithay en el compositor, los nuestros en `malaysia-scene` | aceptado |
| 0002 | Exponer el compositor como biblioteca y binario | aceptado |
| 0003 | `deny(unsafe_code)` en el compositor, `forbid` en el resto | aceptado |
| 0004 | Redondear rectángulos lógicos al espacio entero | aceptado |
| 0005 | Una máquina, una shell, un linker | aceptado |
| 0006 | El flake es la única fuente de verdad de la toolchain | aceptado |
| 0007 | Compilar fuera de la shell de desarrollo está prohibido | aceptado |
| 0008 | Los design tokens fluyen de Nix a Rust | superado por 0011 |
| 0009 | El backend anidado es winit con GLES2 | aceptado |
| 0010 | Smithay anclado a 0.7.0 | reemplazado por 0028 |
| 0011 | Valores de diseño como constantes Rust ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0012 | El backend vive junto al State del compositor | aceptado |
| 0013 | El dispatch usa un fd duplicado del display | aceptado |
| 0014 | Los insets de decoración son función del estado | aceptado |
| 0015 | El layout vive en `malaysia-scene`; no se adopta `desktop::Space` | aceptado |
| 0016 | Move y resize interactivos son estado propio | aceptado |
| 0017 | xdg-decoration siempre server-side | superado por 0019 |
| 0018 | La salida winit se voltea al renderizar, no en `wl_output` | aceptado |
| 0019 | Política de decoración: honrar lo negociado | aceptado |
| 0020 | Política de diseño: chrome separado, efectos para todos | aceptado |
| 0021 | Configuración y control: valores, comandos e IPC | aceptado |
| 0023 | Los componentes del shell corren como procesos separados | aceptado |
| 0024 | Stack de render del toolkit | aceptado (A); modificado por 0034 |
| 0025 | La shell es nativa (`malaysia-shell`) | aceptado |
| 0026 | linux-dmabuf y explicit sync | aceptado |
| 0027 | La shell presenta por EGL | aceptado |
| 0028 | Smithay se ancla a master por revisión | aceptado |
| 0029 | El backend se elige con un flag explícito | aceptado |
| 0030 | Direct Session 8a: un plano primario y una salida | aceptado |
| 0031 | Portal backend propio | aceptado (Settings; faltan Screenshot/ScreenCast) |
| 0032 | Neptunia como fuente de diseño ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0033 | Skia en clientes y CelestialKit desde otto-kit ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | reemplazado por 0034 |
| 0034 | CelestialKit sobre Freya ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado (reemplaza 0033; modifica 0024) |
| 0035 | Identificación de apps propias y CSD ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0036 | Menú global (`org_kde_kwin_appmenu` + dbusmenu) | aceptado e implementado (14a–14f) |
| 0037 | Marco de ventanas dibujado por el compositor | aceptado |
| 0038 | Reducir transparencia por defecto | aceptado |
| 0039 | Infinity v1: gestión de ventanas para la shell ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0040 | Menú global real: arquitectura propia | aceptado e implementado (14d) |
| 0041 | Barra lateral en isla ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0042 | Ajustes del sistema (`malaysia-settings`) | aceptado |
| 0043 | Rebranding a Malaysia | aceptado |
| 0044 | Fundido de apariencia en la GPU | aceptado |
| 0045 | GTK `gtk_shell1` y `org.gtk.Menus` | aceptado (14f-1; 14f-2 pendiente) |
| 0046 | Tengen: lenguaje visual ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0047 | BarrierKit en capas ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | propuesto |
| 0048 | BarrierKit en su propio repositorio | aceptado |
| 0049 | Fondo de pantalla en Vulkan | aceptado |

> El ADR 0022 ("La shell es Quickshell") fue reemplazado y eliminado por el ADR
> 0025: la shell se construye como procesos nativos y Quickshell no se usa.

## Decisiones abiertas

- ~~Shell in-process contra clientes layer-shell~~ — resuelta por el ADR 0023.
- ~~Stack de render del toolkit~~ — resuelta por el ADR 0024 y modificada por el
  0034: Skia en clientes, GLES en el compositor.
- **Stack de render del compositor** — abierta (hoy winit + GLES2).
- **Si el repositorio es privado o abierto** — abierta.
