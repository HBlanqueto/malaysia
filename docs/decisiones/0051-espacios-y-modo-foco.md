# 0051 — Espacios y modo foco

Estado: aceptado (2026-11-01). Pasos 1–6 implementados; el 6 está sin
aceptar en sesión (ADR 0053).

## Contexto

Malaysia no tenía escritorios: todas las ventanas vivían en un solo plano, y el
botón verde en modo `focus` (`window.green_button`) solo registraba la
petición. El modelo de referencia es macOS: Spaces en fila y pantalla completa
como espacio propio. El concepto se fijó con un mockup interactivo
(2026-11-01) que cubre escritorios, foco, borde superior y los tres tipos de
app.

## Decisión

1. **Espacios en fila.** Hay dos tipos: escritorio y espacio de foco. Uno
   nuevo se añade al final de la fila, a la derecha. Máximo 99 escritorios;
   los espacios de foco no cuentan. La fila no da la vuelta.
2. **Ventanas.** Una ventana nueva va al escritorio mostrado. Un diálogo va
   con su padre. Si se muestra un espacio de foco, la ventana nueva va a su
   escritorio y se cambia a él. Activar una ventana de otro espacio (dock,
   xdg-activation) muestra su espacio.
3. **Atajos.** `Ctrl+Alt+←/→` cambia de espacio y `Ctrl+Alt+N` añade un
   escritorio y lo muestra. macOS usa `Ctrl+←/→`, pero en Linux ese atajo mueve
   el cursor por palabras en las apps, así que se añade Alt.
4. **Deslizamiento.** Los espacios se deslizan en horizontal con una barra
   vertical negra entre uno y otro. Cada escritorio tiene su propia barra
   superior, que se va con él. Solo el dock queda fijo y acompaña.
5. **Maximizar = foco.** El botón verde (`window.green_button = "focus"`) crea
   un espacio de foco justo a la derecha del escritorio de la ventana. En una
   sola animación, la ventana crece hasta cubrir la salida mientras el
   escritorio se desliza; el dock no se mueve y al terminar queda tapado. La
   ventana recibe el estado xdg `fullscreen`, no `maximized`, para que las
   apps con modo inmersivo lo apliquen. Salir hace la animación inversa y
   borra el espacio. Cerrar la ventana también lo borra.
6. **Borde superior en foco.** Con el puntero en el borde superior aparece el
   panel en negro puro con texto blanco. Lo que pasa con la ventana depende
   del tipo de app:
   - **App ajena con SSD** (Qt/KDE como Kate, Firefox): el compositor baja su
     titlebar Tengen encima del contenido, debajo del panel.
   - **App de barrierkit con CSD**: el compositor desliza la ventana entera 28
     px hacia abajo y la app vuelve a mostrar sus semáforos, como Finder. La
     app se entera por un evento de Infinity.
   - **libadwaita/GTK**: se usa lo mismo que con barrierkit. La app conserva
     su headerbar (ADR 0020 §1). Las apps con modo inmersivo esconden el
     headerbar ellas mismas al recibir `fullscreen`. Con el fork de
     libadwaita (ADR 0053), la headerbar se esconde en foco y aparece con la
     barra, como los semáforos de barrierkit.
7. **Modelo sin Smithay.** La fila vive en `malaysia_scene::spaces` (ADR
   0001). El compositor saca de la escena las ventanas de espacios no
   mostrados con `Windows::set_off_space`, que es independiente de minimizar,
   y así dibujo, hit-test y foco las ignoran sin más comprobaciones.

## Plan

1. Modelo de espacios y cambio instantáneo con atajos. **Hecho.**
2. Deslizamiento con separador negro, barra por escritorio y dock fijo.
   **Hecho.** La shell sigue teniendo una sola menubar: durante el
   deslizamiento el compositor dibuja esa superficie dentro de cada escritorio,
   junto con su fondo, así que cada uno lleva su barra y se va con ella. Cuenta
   como barra superior el panel Top/Overlay anclado arriba, a lo ancho y con
   zona exclusiva. Duración, curva, ancho y color de la franja son tokens de
   `vow-design` (`SPACE_*`).
3. Modo foco: espacio de foco, estado `fullscreen` y la animación combinada.
   **Hecho.** También entran en foco F11 (`set_fullscreen`), el verde propio
   de una app CSD con `green_button = "focus"` y `Ctrl+Super+F`. Durante la
   animación se dibuja la superficie del cliente recortada a un rectángulo
   redondeado que crece o se encoge, sin la barra SSD; si el cliente aún no
   redibujó a su nuevo tamaño, el resto del rectángulo se ve negro.
4. Borde superior: panel negro, titlebar SSD encima y deslizamiento de 28 px.
   **Hecho** en el compositor: la barra baja sobre una franja negra y la
   ventana CSD se mueve en la escena, no solo en el dibujo. La barra en negro
   puro con texto blanco necesita que la shell sepa que está sobre un foco
   (paso 5).
5. Infinity: espacio actual para la shell y aviso de foco/panel para
   barrierkit. **La parte de la shell está hecha:** Infinity v5 tiene el
   evento `space` con el estado de la barra (`focus_bar`), y sobre un foco la
   menubar se pinta en negro puro con texto blanco (`FOCUS_BAR_*` en
   `vow-design`). **El aviso a las apps también está hecho:** como Infinity
   solo lo bindea la shell de confianza, va por un protocolo aparte,
   `infinity_focus_v1`, abierto a cualquier cliente porque no da geometría
   ni permisos: `bar(1)`/`bar(0)` por superficie cuando la barra baja o sube
   sobre una ventana CSD en foco. El kit oculta sus semáforos en fullscreen y
   los muestra con `bar(1)`.
6. Experimento con un fork de libadwaita
   ([HBlanqueto/libadwaita](https://github.com/HBlanqueto/libadwaita)):
   `AdwToolbarView` oculta sus barras superiores en fullscreen y las vuelve a
   mostrar con el puntero arriba. Se instala con `system.replaceDependencies`
   para no recompilar las apps. Las apps Flatpak no lo reciben. **Hecho**
   (ADR 0053): con el global `infinity_focus_manager_v1`, las
   `AdwToolbarView` exteriores de una ventana fullscreen ponen
   `reveal-top-bars = FALSE` y lo devuelven a `TRUE` con `bar(1)` y al salir
   de fullscreen; las que la app ya gestiona no se tocan. El compositor no
   cambia: sigue bajando 28 px la ventana. Parche
   `nix/patches/libadwaita-focus-mode.patch`, en el mismo paquete que el menú
   global. Falta aceptarlo en sesión.

## Alternativas descartadas

- **Esconder el headerbar desde el compositor** (subir la ventana 47 px):
  ningún protocolo dice dónde termina el headerbar, y con pestañas, barras de
  búsqueda o layouts adaptativos se cortaría contenido.
- **Titlebar SSD encima del headerbar de libadwaita**: deja dos barras con el
  mismo título y tapa los botones de la app.
- **`Ctrl+←/→` sin Alt**: rompe el movimiento por palabras en las apps.
