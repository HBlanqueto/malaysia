# Decisiones (ADRs)

Índice de las decisiones técnicas del proyecto y la lista de las que siguen
abiertas. Los ADRs son el registro de *por qué* se hizo cada cosa; la numeración es
continua.

## ADRs

| ADR | Título | Estado |
|---|---|---|
| 0001 | Tipos de coordenadas: los de Smithay en el compositor, los nuestros en `yuki-scene` | aceptado |
| 0002 | Exponer `crates/yuki` como biblioteca y como binario a la vez | aceptado |
| 0003 | `deny(unsafe_code)` en el compositor, `forbid` en el resto | aceptado |
| 0004 | Redondear rectángulos lógicos al espacio entero | abierto |
| 0005 | Una máquina, una shell, un linker | aceptado |
| 0006 | El flake es la única fuente de verdad de la toolchain | aceptado |
| 0007 | Compilar fuera de la shell de desarrollo está prohibido | aceptado |
| 0008 | Los design tokens fluyen en una sola dirección, de Nix a Rust | aceptado |
| 0009 | El backend anidado es winit con el renderizador GLES2 | aceptado |
| 0010 | Smithay está anclado a la versión 0.7.0 publicada en crates.io | aceptado |
| 0011 | Los valores de diseño son constantes Rust escritas a mano en `yuki-design` | aceptado |
| 0012 | El backend vive junto al State del compositor | aceptado |
| 0013 | El dispatch de clientes hace poll de un descriptor de archivo duplicado del display | aceptado |
| 0014 | Los insets de decoración son una función del estado de la ventana | aceptado |
| 0015 | El layout de ventanas permanece en `yuki-scene`; no se adopta `desktop::Space` de Smithay | aceptado |
| 0016 | El move y el resize interactivos son estado propio del compositor | aceptado |
| 0017 | xdg-decoration se sirve, y siempre del lado del servidor | aceptado |
| 0018 | La salida winit se voltea en tiempo de renderizado, nunca en `wl_output` | aceptado |
| 0019 | Política de decoración: honrar lo que el cliente negoció | aceptado |
| 0020 | Política de diseño y unificación: chrome separado, efectos para todos | aceptado |
| 0021 | Superficie de configuración y control: valores, comandos e IPC | aceptado |
| 0023 | Los componentes del shell corren como procesos separados | aceptado (supervisión propuesta; overview abierta) |
| 0024 | Stack de render del toolkit | aceptado (opción A: GLES + cosmic-text + shaders propios) |
| 0025 | La shell es nativa (`yuki-shell`); se elimina Quickshell | aceptado |

> El ADR 0022 ("La shell es Quickshell") fue **reemplazado y eliminado** por el
> ADR 0025: la shell se construye como procesos nativos y Quickshell no se usa.

## Decisiones abiertas

- ~~Shell in-process contra clientes layer-shell~~ — **resuelta por el ADR 0023**:
  clientes layer-shell, un proceso por componente.
- ~~Stack de render del toolkit~~ — **resuelta por el ADR 0024** (opción A: GLES +
  cosmic-text + shaders propios, mismo stack que el compositor).
- **Stack de render del compositor** — abierta (hoy winit + GLES2; ver hito Direct
  Session).
- **Si el repositorio es privado o abierto** — abierta.