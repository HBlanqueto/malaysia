# Decisiones (ADRs)

Índice de decisiones. Los ADR del toolkit viven en
[`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit)
desde el 0048; aquí queda un stub que remite allí.

| ADR | Título | Estado |
|---|---|---|
| 0001 | Coordenadas Smithay / `yuki-scene` | aceptado |
| 0002 | Biblioteca y binario | aceptado |
| 0003 | `unsafe` acotado | aceptado |
| 0004 | Redondeo floor/ceil | aceptado |
| 0005 | Entorno y linker | aceptado |
| 0006 | Toolchain del flake | aceptado |
| 0007 | Compilar solo en devShell | aceptado |
| 0008 | Tokens desde Nix | superado por 0011 |
| 0009 | Winit + GLES2 | aceptado |
| 0010 | Smithay 0.7.0 | reemplazado por 0028 |
| 0011 | Tokens como constantes Rust ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0012 | Backend junto al State | aceptado |
| 0013 | Dispatch con fd duplicado | aceptado |
| 0014 | Insets de decoración | aceptado |
| 0015 | Layout en `yuki-scene` | aceptado |
| 0016 | Move/resize propios | aceptado |
| 0017 | xdg-decoration server-side | superado por 0019 |
| 0018 | Volteo solo al renderizar | aceptado |
| 0019 | Política de decoración | aceptado |
| 0020 | Chrome separado y efectos | aceptado |
| 0021 | Configuración e IPC | aceptado |
| 0023 | Shell en procesos separados | aceptado |
| 0024 | Stack de render del kit | aceptado |
| 0025 | Shell nativa (`yuki-shell`) | aceptado |
| 0026 | dmabuf y explicit sync | aceptado |
| 0027 | Shell por EGL | aceptado |
| 0028 | Smithay por revisión | aceptado |
| 0029 | Backend por flag | aceptado |
| 0030 | Direct Session 8a | aceptado |
| 0031 | Portal backend propio | aceptado |
| 0032 | Neptunia como diseño ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0033 | Skia/CelestialKit ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | reemplazado por 0034 |
| 0034 | CelestialKit sobre Freya ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0035 | Apps propias y CSD ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0036 | Menú global | implementado |
| 0037 | Marco de ventanas | aceptado |
| 0038 | Transparencia reducida | aceptado |
| 0039 | Infinity v1 ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0040 | Menú global real | implementado |
| 0041 | Barra lateral en isla ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0042 | Ajustes del sistema (`yuki-settings`) | aceptado |
| 0043 | Rebranding a Malaysia | aceptado |
| 0044 | Fundido de apariencia | aceptado |
| 0045 | GTK menus | aceptado |
| 0046 | Tengen ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | aceptado |
| 0047 | BarrierKit en capas ([barrierkit](https://github.com/HBlanqueto/barrierkit)) | propuesto |
| 0048 | BarrierKit en su repo | aceptado |
| 0049 | Fondo en Vulkan | aceptado |
| 0050 | Puente GTK/libadwaita | aceptado |
| 0051 | Espacios y modo foco | aceptado |
| 0052 | Menú global para libadwaita | aceptado |
| 0053 | Modo foco en libadwaita | aceptado |
| 0054 | Titlebar superpuesta en foco | aceptado |

> El ADR 0022 (Quickshell) fue eliminado por el 0025.
