# 0039 — Infinity v1: gestión de ventanas para la shell

Estado: **aceptado**.

## Contexto

La menubar de Yuki (`yuki-shell menubar`) muestra el menú **Ventana** con
entradas para colocar la ventana activa en mitades, cuartos, disposiciones,
rellenar, centrar, restaurar y pantalla completa. Hoy esas entradas salen en
gris porque no existe un canal por el que la shell le pida al compositor esas
operaciones de geometría.

Infinity (ver [glosario](../../docs/glossary.md)) es el protocolo privado
app↔compositor de Yuki. La primera pieza que se implementa es la gestión de
ventanas para los componentes de la shell, empezando por la menubar.

## Decisión

1. **Protocolo privado `infinity_window_manager_v1`.** Se define en
   `protocols/infinity-window-manager-v1.xml`. Es una extensión Wayland
   server-side generada con `wayland-scanner` en el compositor y con la
   generación cliente en `celestial-kit`.
2. **Solo los componentes de `yuki-shell` pueden ver el global.** El filtro de
   visibilidad de Smithay (`GlobalDispatch::can_view`) lo permite solo cuando
   el ejecutable del cliente (`/proc/<pid>/exe`) coincide con uno de los
   binarios de confianza:
   - `yuki-shell` en el mismo directorio que el compositor, resuelto desde
     `std::env::current_exe()`.
   - `.yuki-shell-wrapped` en ese mismo directorio, para los envoltorios de
     Nix que renombran el binario real.
   - El path exacto indicado por la variable de entorno `YUKI_SHELL_EXE`, que
     un wrapper puede exportar cuando la heurística anterior no es suficiente.
   - `YUKI_DEV=1` en el entorno del compositor desactiva el filtro para
     desarrollo.

   No es seguridad fuerte: cualquier proceso que pueda hacerse pasar por
   `yuki-shell` o ejecutarse con `YUKI_DEV=1` podría usar el protocolo. La
   decisión de no encriptar ni autenticar con un token viene del ADR 0035:
   Yuki confía en sus propios procesos de shell por identidad de binario y
   entorno de desarrollo, no por un mecanismo criptográfico.
3. **Las requests actúan sobre un `zwlr_foreign_toplevel_handle_v1` existente.**
   La shell ya maneja esos handles para el dock y los menús de app. Reutilizar
   el mismo identificador evita añadir un segundo namespace de ventanas.
4. **Requests:**
   - `arrange(handle, layout)`: layout = `fill`, `center`, `left`, `right`,
     `top`, `bottom`, `top_left`, `top_right`, `bottom_left`, `bottom_right`.
   - `arrange_pair(layout)`: `side_by_side`, `side_by_side_reversed`,
     `stacked`, `stacked_reversed`. La ventana activa ocupa la primera mitad y
     la anterior en orden de foco la segunda.
   - `arrange_quarters`: las cuatro ventanas visibles más recientes en
     cuartos.
   - `restore(handle)`: vuelve al rectángulo anterior al primer `arrange` y
     borra el rectángulo guardado.
   - `set_fullscreen(handle, bool)`: entra o sale de pantalla completa real.
5. **Diferencias con Maximizar y con Pantalla completa.** Maximizar (botón
   verde, doble clic) conserva el marco de Yuki y ocupa el área de trabajo.
   Pantalla completa (`set_fullscreen`) es sin marco, ocupa toda la salida y
   se refleja en el estado `Fullscreen` de `zwlr_foreign_toplevel_handle_v1`.
   El modo foco sigue fuera de este protocolo; se manejará por otro canal.
6. **Evento `restorable(handle, bool)`.** Se envía cuando cambia si una ventana
   tiene un rectángulo de restore guardado. La shell lo usa para habilitar o
   deshabilitar la entrada "Regresar al tamaño anterior". Se eligió un evento
   del protocolo en lugar de leerlo de `ToplevelInfo` porque el estado de
     restore no forma parte del estado de toplevel que publica
     `zwlr_foreign_toplevel_management_v1`; es interno de Infinity. Centralizar
     la señal en el mismo protocolo que produce el estado evita que la shell
     tenga que adivinar o sondear.
7. **Geometrías.** Los cálculos usan `workspace()` del compositor (salida menos
   las zonas exclusivas de la barra y el dock) con un margen de 8 px alrededor
   y 8 px entre ventanas. `fill` ocupa el área disponible; `center` mantiene el
   tamaño actual y centra la ventana.

## Consecuencias

- `yuki-shell menubar` puede habilitar las entradas de geometría cuando
  Infinity está disponible y llamar a las requests sobre la ventana activa.
- El compositor gana un protocolo server-side más, generado con
  `wayland-scanner`.
- `celestial-kit` incluye la generación cliente del protocolo como capacidad
  opcional: si el compositor no anuncia el global, las funciones de Infinity
  en `Toplevels` simplemente no hacen nada.
- Un cliente externo cualquiera que intente bindear
  `infinity_window_manager_v1` no verá el global.

## Referencias

- [`0034`](0034-celestialkit-sobre-freya.md) — CelestialKit sobre Freya.
- [`0035`](0035-identificacion-de-apps-propias-y-csd.md) — apps propias y CSD.
- [`0036`](0036-menu-global.md) — menú global.
- [`0037`](0037-marco-de-ventanas-dibujado-por-el-compositor.md) — marco de
  ventanas dibujado por el compositor.
- [`glossary.md`](../../docs/glossary.md) — definición de Infinity.
- `protocols/infinity-window-manager-v1.xml`.
