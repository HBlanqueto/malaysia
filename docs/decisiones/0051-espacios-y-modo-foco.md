# 0051 — Espacios y modo foco

Estado: aceptado (2026-11-01). Pasos 1 y 2 implementados; 3–6 pendientes.

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
     headerbar ellas mismas al recibir `fullscreen`.
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
4. Borde superior: panel negro, titlebar SSD encima y deslizamiento de 28 px.
5. Infinity: espacio actual para la shell y aviso de foco/panel para
   barrierkit.
6. Experimento con un fork de libadwaita
   ([HBlanqueto/libadwaita](https://github.com/HBlanqueto/libadwaita)):
   `AdwToolbarView` oculta sus barras superiores en fullscreen y las vuelve a
   mostrar con el puntero arriba. Se instala con `system.replaceDependencies`
   para no recompilar las apps. Las apps Flatpak no lo reciben.

## Alternativas descartadas

- **Esconder el headerbar desde el compositor** (subir la ventana 47 px):
  ningún protocolo dice dónde termina el headerbar, y con pestañas, barras de
  búsqueda o layouts adaptativos se cortaría contenido.
- **Titlebar SSD encima del headerbar de libadwaita**: deja dos barras con el
  mismo título y tapa los botones de la app.
- **`Ctrl+←/→` sin Alt**: rompe el movimiento por palabras en las apps.
