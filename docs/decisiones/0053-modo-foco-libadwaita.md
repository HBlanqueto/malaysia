# 0053 — Modo foco en libadwaita

Estado: aceptado (2026-11-01). Implementado en el fork
[`HBlanqueto/libadwaita`](https://github.com/HBlanqueto/libadwaita) (rama
`claude/optimistic-bell-p2v9or`, `f2ce550`) y en el paquete
`libadwaita-global-menu`; falta aceptarlo en sesión con apps reales. Es el
paso 6 del ADR 0051 (17f).

## Contexto

En modo foco (ADR 0051 §5–§6) una app libadwaita conserva su headerbar: la
ventana es fullscreen y la headerbar ocupa la parte de arriba todo el rato.
En macOS, una app en pantalla completa esconde su barra de herramientas y la
muestra solo con la barra de menús. Las apps de barrierkit ya lo hacen con
`infinity_focus_v1` (17e). Esconder la headerbar desde el compositor está
descartado (ADR 0051): no se sabe dónde acaba.

La opción C es que lo haga la app: `AdwToolbarView` ya tiene
`reveal-top-bars`, con su animación, y libadwaita lo documenta para esto.

## Decisión

1. **En el fork** (`src/adw-focus-mode.c`), enganchado en
   `AdwToolbarView::root`/`unroot`. Sin API pública nueva.
2. **Solo con el global.** La primera vista de una ventana liga
   `infinity_focus_manager_v1` por `wl_registry` (una vez por `GdkDisplay`).
   Cada ventana realizada crea un `infinity_focus_surface_v1` con su
   `wl_surface` y lo destruye al desrealizarse. Sin el global, sin el backend
   Wayland de GTK o con `ADW_DISABLE_FOCUS_MODE`, no cambia nada.
3. **Qué hace.** Con la ventana en fullscreen, las vistas tomadas tienen
   `reveal-top-bars = FALSE`; con `bar(1)` pasan a `TRUE` y con `bar(0)`
   vuelven a `FALSE`. Al salir de fullscreen vuelven a `TRUE`. Los avisos
   `bar` se aplican en un idle, así que un `bar(0)` y la salida de fullscreen
   en el mismo lote no esconden la barra para mostrarla enseguida.
4. **Qué vistas.** Las `AdwToolbarView` de la ventana, en su propia
   superficie (no en un popover), fuera de un `AdwDialog`, con alguna barra
   superior visible y sin un `AdwToolbarView` antecesor con barras
   superiores. Así se va la headerbar exterior y, en una vista partida, las
   dos, pero no la de una vista anidada en el contenido de otra.
5. **Apps que ya gestionan `reveal-top-bars`** (visores, reproductores):
   - Las vistas se toman en un idle tras `notify::fullscreened`, cuando los
     handlers y bindings de la app sobre `fullscreened` ya han corrido. Si
     una vista tiene entonces las barras escondidas, es de la app durante ese
     fullscreen.
   - Si la propiedad cambia en una vista tomada y no la cambió el módulo, la
     app la ha tomado: deja de tocarse y no se restaura al salir.
   - Al salir de fullscreen se olvida todo y el siguiente vuelve a mirar.
6. **El compositor no cambia.** Sigue bajando 28 px la ventana CSD con la
   barra: es lo que deja la headerbar recién mostrada bajo el panel, igual
   que los semáforos de barrierkit. **Revisado por el ADR 0054:** con la
   versión 2 del protocolo la ventana no baja y las barras van sobre el
   contenido, a `bar_height`.
7. **Paquete Nix.** `nix/patches/libadwaita-focus-mode.patch` se aplica
   detrás del del menú global sobre el tag `1.9.3`, con `wayland-scanner` y
   `wayland` añadidos al paquete. Usa solo API de GTK <= 4.22 y entra por el
   mismo `system.replaceDependencies` (ADR 0052 §5). El paquete conserva el
   nombre `libadwaita-global-menu` para no tocar el módulo.
8. El XML del protocolo se copia de barrierkit
   (`crates/infinity-protocol/protocols/infinity-focus-v1.xml`) a
   `src/protocols/` del fork, con su aviso MIT y un comentario con el
   origen. Si el protocolo cambia, hay que copiarlo otra vez.

## Consecuencias

- Las apps libadwaita en foco usan toda la pantalla y muestran la headerbar
  con el puntero arriba, como barrierkit y como macOS.
- La headerbar reaparece redimensionando el contenido (no se superpone):
  es lo que hace `reveal-top-bars` sin `extend-content-to-top-edge`.
  Solo con compositores v1; con la v2 se superpone (ADR 0054).
- Al salir del foco con la barra bajada, el compositor envía `bar(0)` al
  empezar la animación y fullscreen termina al final: la headerbar se
  esconde durante la reducción y vuelve después. Pasa lo mismo con los
  semáforos de barrierkit.
- Las apps Flatpak, con la libadwaita de su runtime, no lo reciben.
- Una vista en un panel inferior sin antecesor con barras se tomaría
  también; no se ha visto en apps reales.

## Alternativas descartadas

- **Hacerlo en `AdwApplicationWindow`/`AdwWindow`:** deja fuera las
  `GtkWindow` con `AdwToolbarView` y obliga a recorrer el árbol en cada
  aviso. Las vistas ya saben cuándo entran y salen de una ventana.
- **Decidir por geometría** (vistas pegadas al borde superior): la posición
  no está asignada al enraizar y cambia al esconder barras.
- **Marcar las vistas propias de la app al enraizar:** una app que oculta las
  barras al entrar en fullscreen lo hace después; solo se ve en ese momento.
