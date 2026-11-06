# 0059 — Radio por tipo de barra y esquina continua

Estado: aceptado (2026-11-06). Sustituye a 0058. Falta probarlo en sesión.

## Contexto

En macOS Tahoe el radio de una ventana depende de su barra.
`-[NSThemeFrame _getCachedWindowCornerRadius]` en macOS 26.6.2 da
([notas de ingeniería inversa](https://github.com/mansarip/mac-window-corners/blob/main/macos-tahoe-window-corner-radius.md)):

| Ventana de AppKit | Radio |
|---|---|
| Solo titlebar; toolbar `expanded` o `preference` | 16 |
| Toolbar `unifiedCompact` | 20 |
| Toolbar `unified` o `automatic`; sheets y alertas | 26 |
| Paneles y ventanas utilitarias | 15 |

La forma tampoco es un arco. AppKit construye la máscara con
`ceil(r × cornerCurveExpansionFactor)`, unos 1,53 × r: la curva continua de
Core Animation. Esa curva empieza a 1,5287·R del vértice y su curvatura
crece desde 0 ([PaintCode](https://www.paintcodeapp.com/news/code-for-ios-7-rounded-rectangles),
[NativeScript #11452](https://github.com/NativeScript/NativeScript/issues/11452)).
El compositor dibujaba un arco de círculo (`superellipse-exponent` 2,0),
cuya curvatura salta de 0 a 1/R en el borde.

ADR 0058 solo distinguía «con barra de herramientas» (26) y «sin ella» (16).

## Decisión

1. **Protocolo** `infinity_window_style_v1` (barrierkit): `set_toolbar`
   pasa a `set_bar(wl_surface, bar)`, con el enum `bar`: `titlebar`,
   `compact`, `unified` y `utility`. Una petición posterior sustituye a la
   anterior.
2. **Tokens** (`vow-design`): `WINDOW` (16), `WINDOW_COMPACT` (20),
   `WINDOW_TOOLBAR` (26) y `WINDOW_UTILITY` (15).
3. **Compositor**, en `window_style::corner_radius`:

   | Ventana | Radio |
   |---|---|
   | Envía `bar` | el de su barra |
   | Diálogo (toplevel con padre) sin `bar` | 26, como sheets y alertas |
   | CSD sin `bar` (GTK, Electron) | 26: su headerbar es una barra unificada |
   | SSD (titlebar del compositor, 32 pt) | 16 |
   | Pantalla completa | 0 |

4. **Esquina continua:** el marco, la sombra, el borde y el blur usan una
   superelipse que empieza a `CORNER_EXPANSION` (1,52866) × R con
   `superellipse-exponent` 3,34. Se desvía de la curva de Apple como mucho
   0,0076·R (0,2 pt con R = 26); el arco se desviaba 0,014·R. El shader
   limita la extensión a la mitad del lado más corto.
5. **barrierkit:** una `Titlebar` `close_only` envía `titlebar`; con barra
   lateral en isla (52 pt), `unified`; el resto (40 pt), `compact`. Si
   cambia el estilo de la barra lateral, vuelve a enviarlo.

## Consecuencias

- Cada ventana de barrierkit recibe el radio de su barra, como en Tahoe.
- Las ventanas GTK y los diálogos pasan de 16 a 26.
- Los popups del compositor también usan la esquina continua.
- Una app de barrierkit anterior a este cambio envía `set_toolbar`, que ya
  no existe, y el compositor la desconecta. Todas se construyen con el mismo
  rev.
- La isla de la barra lateral usa el suavizado de freya con
  `CORNER_EXPANSION − 1` y sigue concéntrica con la ventana. El resto de
  controles de barrierkit siguen con arco de círculo.

## Alternativas descartadas

- **Mantener `set_toolbar` y añadir `set_bar` en la versión 2:** ninguna
  app la usa ya; sería código muerto.
- **Curva de Apple exacta con tres cúbicas en el shader:** no tiene una SDF
  cerrada. La superelipse ajustada queda por debajo de 0,3 pt.
- **Subir solo el exponente sin ampliar la extensión:** con la extensión en
  R, n > 2 hace la esquina más cuadrada, no más suave.
