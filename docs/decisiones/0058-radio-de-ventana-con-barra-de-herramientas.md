# 0058 — Radio de ventana con barra de herramientas

Estado: sustituido por 0059 (2026-11-06). Implementado en barrierkit (`de059fc`) y en
el compositor; falta probarlo en sesión.

## Contexto

En macOS Tahoe el radio de una ventana depende de su barra. Medido sobre
capturas de macOS 26: la Terminal (solo título) tiene esquinas de 16 pt y
Finder o Ajustes del sistema (barra de herramientas con barra lateral), de
26 pt. La isla de la barra lateral sigue la esquina de la ventana: 26 − 8
de margen = 18.

El compositor redondea todas las ventanas (ADR 0020) con
`vow_design::WINDOW` (16) y no sabe qué ventanas CSD tienen barra de
herramientas.

## Decisión

1. **Protocolo** `infinity_window_style_v1` (barrierkit), un global sin
   permisos con una petición, `set_toolbar(wl_surface)`: el toplevel tiene
   barra de herramientas. Dura lo que la `wl_surface`. Sin el global, o sin
   la petición, la ventana tiene la esquina de título.
2. **Tokens** (`vow-design`): `WINDOW_TOOLBAR` con radio 26
   (`components.window.toolbar-radius`); la isla pasa a radio 18
   (`size.sidebar-radius`).
3. **Compositor:** ofrece el global y guarda la marca en los datos de la
   superficie. El marco (esquina, sombra, borde y blur) y la transición del
   modo foco usan `WINDOW_TOOLBAR` para esas ventanas y `WINDOW` para el
   resto. Los diálogos siguen con `DIALOG` y la pantalla completa, sin
   esquinas.
4. **barrierkit:** liga el global y una `Titlebar` que no es `close_only`
   pide `set_toolbar` al montarse. La barra de 44 px (con o sin barra
   lateral) es de herramientas; la de 32 px de «Acerca de» es de título.

## Consecuencias

- Las apps de barrierkit con barra de herramientas tienen la esquina de
  Tahoe y su isla queda concéntrica con la ventana.
- Las ventanas SSD, GTK y Qt siguen con 16: no envían la petición.
- Una app de barrierkit anterior a `de059fc` sigue con 16 y la isla de 12.

## Alternativas descartadas

- **Radio 26 para todas las ventanas:** la Terminal y los diálogos de solo
  título quedarían más redondos que en macOS.
- **Petición en `infinity_focus_surface_v1`:** ese objeto es de avisos del
  modo foco; mezclaría dos motivos de cambio.
- **Que la app comunique un radio libre:** el valor de diseño vive en
  `vow-design` (ADR 0011); la app solo dice qué tipo de ventana es.
