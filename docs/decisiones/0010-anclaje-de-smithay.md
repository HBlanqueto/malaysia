# 0010 — Smithay está anclado a la versión 0.7.0 publicada en crates.io

Estado: reemplazado por 0028

## Contexto

Smithay es la única dependencia grande sobre la que se construye el proyecto.
Se publica en crates.io, y su rama `master` (a la que siguen niri, smallvil en
el árbol y Otto) avanza por delante de las versiones. La sesión anidada usa un
trocito pequeño y estable de la API: una fuente de eventos de winit, un
renderizador EGL/GLES2, y los manejadores de protocolo Wayland para compositor,
shm, seat y xdg-shell. El proyecto se compila dentro de un único dev shell de
Nix y valora las compilaciones reproducibles (ADR 0005, ADR 0006).

## Opciones consideradas

1. **crates.io `0.7.0`.** Una API etiquetada y publicada. La resolución de
   versiones normal de Cargo y `Cargo.lock` la anclan. Sin entrada de flake
   extra, sin fetch de git en la compilación, y la documentación de docs.rs
   coincide con el código.
2. **Git `master` anclado por revisión.** Los arreglos más recientes, y la misma
   fuente que usan los compositores de referencia. Pero añade una dependencia de
   git a `flake.nix` y `Cargo.toml`, sigue una API en movimiento, y convierte un
   bump de versión en un bump de revisión.
3. **Git `master` sin anclar.** Rechazada sin más: la compilación cambiaría bajo
   el proyecto sin un commit.

## Decisión

Anclar Smithay a la versión `0.7.0` de crates.io. Subirla deliberadamente cuando
un hito posterior necesite algo más nuevo, del mismo modo que se sube la
toolchain (ADR 0006).

## Consecuencias

- Las compilaciones son reproducibles solo con `flake.lock` y `Cargo.lock`.
- Un arreglo que solo existe en `master` no está disponible hasta una versión o
  un cambio explícito a la opción 2; si la integración con winit resulta
  defectuosa, cambiar a una revisión de git anclada es el plan de respaldo
  registrado.
- El backend winit de la `0.7.0` publicada es contra lo que se escribe el
  código, así que los docs de docs.rs son la autoridad para este proyecto.