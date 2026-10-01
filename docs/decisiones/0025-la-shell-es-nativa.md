# 0025 — La shell es nativa (`yuki-shell`)

Estado: **aceptado.** Reemplaza y elimina el ADR 0022 ("La shell es Quickshell"):
la shell de Yuki se construye en el propio repositorio, como procesos Rust
nativos (`yuki-shell <componente>`, ADR 0023), y Quickshell se elimina como
dependencia.

## Contexto

El ADR 0022 eligió Quickshell (QtQuick/QML) como toolkit para construir la
shell, con la condición de que el diseño lo decide Yuki. La práctica demostró
que el trabajo real no lo hace el toolkit: cada componente de la shell es
pequeño (dibujar una barra, un dock, un wallpaper, una pantalla de bloqueo) y
el toolkit propio (`celestial-kit`, ADR 0024) ya cubre exactamente eso — GLES
para la geometría y los shaders, `cosmic-text` para el texto, `wl_shm`/dmabuf
para presentar.

Dos vías posibles:

- **Seguir con Quickshell** — mantener una dependencia externa grande (Qt/QML)
  y su sistema de configuración, cuando los componentes nativos ya funcionan.
- **Shell nativa** — cada componente es un proceso `yuki-shell` en Rust sobre
  `celestial-kit` (ADR 0023/0024), que lee el almacén `.domain` igual que el
  compositor; se elimina Quickshell por completo.

**Decisión: shell nativa.** Quickshell queda fuera; no se documenta, no se
soporta, no se usa. La opción `shell.engine` deja de existir: no hay "motor de
shell" que configurar.

## Decisión

1. **La shell es `yuki-shell`**: un binario, un proceso por componente
   (`wallpaper`, `menubar`, `dock`, `lock`, ...), cada uno un cliente Wayland
   propio (ADR 0023), dibujando con `celestial-kit` (ADR 0024).
2. **El compositor se integra con la shell nativa, no al revés.** Yuki sirve
   los protocolos que los componentes necesitan (`wlr-layer-shell-v1`,
   `ext-foreign-toplevel-list-v1`, `zwlr_foreign_toplevel_management_v1`,
   `ext-session-lock-v1`, `wp_viewporter`, `wp_fractional_scale_manager_v1`)
   y expone las opciones de shell (`dock.*`, `menubar.*`, `appearance.*`) por
   el almacén `.domain` (ADR 0021) y los estados en `~/.local/share/yuki/`
   (`wallpaper.domain`, `window.domain`).
3. **El diseño es de Yuki**: los componentes dibujan con `celestial-kit` y los
   tokens de `yuki-design`, siguiendo la política del ADR 0020 (efectos del
   compositor para todas las superficies; la shell incluida).
4. **Sin dependencias de shell externas.** No se copia código de otras shells
   al compositor ni viceversa; solo protocolos y datos.

## Consecuencias

- Se elimina el ADR 0022 y toda referencia a Quickshell en el repositorio
  (código, flake, configuración y documentación).
- `shell.engine` y `shell.config` salen del catálogo de opciones
  (`docs/configuracion/opciones.md`).
- La supervisión usa las unidades systemd del flake (`yuki-shell@.service`,
  `yuki-session.target`, ADR 0023), que relanzan cada componente por separado;
  matar cualquier componente no afecta al compositor ni a los demás.
- El ADR 0021 (configuración) ya no habla de consumidores QML: los consumidores
  del almacén son los procesos `yuki-shell` y la futura app de ajustes.

## Relación

- ADR 0023 (procesos separados por componente), ADR 0024 (stack de render).
- `docs/arquitectura/modelo-de-procesos.md` y `docs/arquitectura/yuki-y-shell.md`
  (división de trabajo compositor ↔ shell).