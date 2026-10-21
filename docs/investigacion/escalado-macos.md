# Cómo usa macOS el escalado en el escritorio

Investigación sobre el modelo de escala de macOS y qué se puede aprender para el
Bloque 9 de Yuki. La conclusión corta: **macOS no hace escala fraccional por
aplicación**. Renderiza el escritorio completo a 2× de una resolución lógica y
luego reescala el framebuffer virtual al panel. Es supersampling global, no
`wp_fractional_scale_v1` por superficie.

## 1. Tres espacios distintos

macOS separa tres cosas que en Wayland suelen mezclarse:

| Espacio | Qué es | API |
|---|---|---|
| Puntos lógicos | Coordenadas de la app y del escritorio; "Looks like 1440×900" | `NSScreen.frame`, `NSWindow.frame` |
| Backing store | Píxeles reales que dibuja la app; 2× los puntos en Retina | `backingScaleFactor`, `convertRectToBacking` |
| Framebuffer virtual | Imagen compuesta por WindowServer antes del escaneo | `CGDisplayModeGetPixelWidth/Height` |
| Panel físico | Píxeles nativos del monitor | EDID / modo nativo |

En un Mac Retina, `backingScaleFactor` es **1.0 o 2.0, nunca 1.5**. Apple lo
documenta así: el factor relaciona puntos con píxeles de *backing*, no con los
píxeles físicos del panel. El escalado final a la resolución nativa es otro paso,
hecho por WindowServer/GPU.

## 2. El pipeline

1. La app dibuja en puntos lógicos. AppKit multiplica por el backing scale (1 o
   2) y le da un backing store del tamaño resultante.
2. WindowServer compone todas las ventanas y capas en un **framebuffer virtual**
   a la resolución del backing store (por ejemplo, 5120×2880).
3. Si ese framebuffer no coincide con la resolución nativa del panel, macOS lo
   reescala (downsample/upsample) a la resolución nativa antes de enviar la
   señal. En un panel 4K de 3840×2160, un escritorio "looks like 2560×1440" se
   compone a 5120×2880 y se reduce a 3840×2160.
4. La app nunca ve ese último paso: sigue viendo puntos y un backing scale de 2.

Es decir: **la escala fraccional es del compositor, no del cliente**. Todas las
apps, incluidas las que solo entienden 1×/2×, se benefician (o sufren) el mismo
reescalado final.

## 3. Ejemplos

| Panel | "Looks like" | Backing store | Escala final al panel |
|---|---|---|---|
| 5K 5120×2880 | 2560×1440 | 5120×2880 | 1:1 |
| 4K 3840×2160 | 1920×1080 | 3840×2160 | 1:1 |
| 4K 3840×2160 | 2560×1440 | 5120×2880 | 0.75× (downsample) |
| 4K 3840×2160 | 3008×1692 | 6016×3384 | ≈0.638× |
| MBP 14" 3024×1964 | 1512×982 | 3024×1964 | 1:1 |
| MBP 14" 3024×1964 | 1800×1169 | 3600×2338 | ≈0.84× |
| 1920×1200 no-Retina | 1920×1200 | 1920×1200 | 1:1 (LoDPI) |

Las opciones "Más espacio" / "Texto más grande" cambian la resolución lógica.
El backing scale sigue siendo 2 en un panel Retina; lo que cambia es el tamaño
del framebuffer virtual y, por tanto, cuánto se reduce al final.

## 4. Por qué macOS lo hace así

- **Compatibilidad:** las apps solo necesitan soportar 1× y 2×. No hay que
  reescribir layouts para 1.25 o 1.5.
- **Tamaño consistente:** el usuario elige "cuánto ocupa" la interfaz; el
  compositor se encarga del resto.
- **Calidad:** renderizar a 2× y reducir suele verse mejor que renderizar
  directamente a una escala fraccional con clientes que no la soportan.
- **Costo:** se renderizan más píxeles de los que el panel muestra (hasta 2×
  lineales, 4× área), y el downsample consume GPU. Es un compromiso aceptado;
  las pruebas de rendimiento de terceros muestran impactos pequeños pero reales.
- **Desventaja:** en modos no nativos hay pérdida de nitidez, moiré y artefactos
  al hacer scroll; no hay mapeo 1:1. Los modos "Default" suelen ser los que sí
  dan 1:1.

## 5. Multi-monitor

El backing scale es una propiedad de la ventana/pantalla, no global. Cada
pantalla tiene su propio framebuffer y su propio factor (1× o 2×). Al mover una
ventana de un monitor a otro, macOS cambia la propiedad del backing store cuando
la ventana está mayoritariamente en el destino y vuelve a dibujar el contenido a
la nueva escala. En un setup mixto, una ventana puede pasar de 1× a 2× y
viceversa.

Para Yuki/8b esto implica: la escala no puede ser una constante global; cada
salida necesita su `Scale`, y una ventana que cruza outputs debe reconfigurarse
y volver a renderizar a la escala de la salida que la contiene. La parte
"fraccional global" de macOS no se traslada directamente a Wayland.

## 6. Cómo encaja con Wayland y con Yuki

Wayland ofrece dos mecanismos:

- **`wl_output.scale` entero** (1, 2, ...): el cliente dibuja a ese múltiplo.
- **`wp_fractional_scale_v1` + `wp_viewporter`**: el cliente recibe una escala
  fraccional (1.25, 1.5, ...) y decide cómo renderizar y cómo mapear su buffer
  al tamaño lógico.

Yuki (Bloque 9) sigue el modelo Wayland: escala por salida, `preferred_scale`
fraccional a cada superficie y viewporter en la shell. Eso permite 1:1 en los
clientes que lo soportan, pero un cliente que solo entiende 1×/2× queda borroso
o mal dimensionado. macOS evita ese problema a costa de renderizar todo a 2× y
reducir al final.

Opciones para Yuki:

1. **Wayland puro (actual):** escala por salida/superficie, clientes modernos a
   1.25/1.5 con viewporter; clientes antiguos a 1× o 2×. Es el camino correcto
   para Wayland y el que ya está implementado.
2. **Supersampling tipo macOS (futuro):** renderizar la escena completa a 2× de
   la resolución lógica en un framebuffer offscreen y reducir al panel. Da
   nitidez uniforme a todas las apps y elimina el problema de clientes sin
   fractional-scale, pero añade memoria/GPU y un blit final. No es necesario
   para 8a/9; podría ser una opción de calidad en el futuro.
3. **Híbrido:** escala entera para clientes y compositor downsample. Es
   parecido a (2) pero sin forzar 2× en todos los casos.

Para el ThinkPad (1920×1200, 1×) no hay diferencia práctica. Si se elige
"looks like 1440×900" en Yuki, el modelo Wayland puro renderiza a 1.333; el
modelo macOS renderizaría a 2880×1800 y reduciría a 1920×1200. La decisión
depende de si se prioriza compatibilidad/nitidez uniforme (macOS) o el modelo
Wayland y coste menor (Yuki actual).

**Decisión:** la opción (2) queda como **futuro opcional**. No se implementa en
el Bloque 9 ni antes de que el escritorio sea competente (8b/9/11/12 cerrados y
uso diario estable). Se reevalúa con medidas reales de GPU/memoria, no por
estética.

## 7. Lo que sí conviene copiar de macOS

- **Separar puntos, backing y panel** en la documentación y en el código. Yuki
  ya tiene `Logical`, `Physical` y `Buffer`; el siguiente paso es no confundir
  "tamaño lógico de la salida" con "píxeles del panel".
- **Escala por pantalla**, no una constante global. En 8b cada output debe
  llevar su escala.
- **Un modo por defecto que sea 1:1** cuando el panel lo permita; los modos
  fraccionales son una elección explícita.
- **Avisar del coste** de un modo fraccional: downsample, blur y GPU.

## 8. Lo que no conviene copiar

- **No hay que fingir un `backingScaleFactor` de 1.5.** En macOS es siempre
  1 o 2; el 1.5 es el resultado de 2× lógico + downsample, no una escala de
  backing. En Wayland eso corresponde a `preferred_scale=1.5` en el cliente o a
  un supersample del compositor, no a mentir sobre el factor.
- **No conviene renderizar todo a 2× sin medir.** En una iGPU Vega el coste de
  memoria y ancho de banda no es trivial; el modelo Wayland con clientes que
  soportan fractional-scale es más barato.
- **No se copia código de WindowServer/SkyLight**: es privado y no portable.

## Fuentes

- Apple, *High Resolution Explained* (guía archivada): puntos, backing store,
  `backingScaleFactor` 1.0/2.0, framework-scaled vs magnified.
  <https://developer.apple.com/library/archive/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/Explained/Explained.html>
- Apple StackExchange, *How does display scaling work on macOS?*: framebuffer
  virtual, downsample a la resolución nativa, modos HiDPI.
  <https://apple.stackexchange.com/questions/421858/how-does-display-scaling-work-on-macos>
- BetterDisplay wiki, *MacOS scaling, HiDPI, LoDPI explanation*: framebuffer 2×,
  bitmap-raster scaling y modos fraccionales.
  <https://github.com/waydabber/BetterDisplay/wiki/MacOS-scaling,-HiDPI,-LoDPI-explanation>
- winit issue #2048: `scale_factor` siempre 2.0 en macOS; el backing store es 2×
  y el sistema reescala por separado.
  <https://github.com/rust-windowing/winit/issues/2048>
