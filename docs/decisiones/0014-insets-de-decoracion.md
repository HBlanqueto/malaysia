# 0014 — Los insets de decoración son una función del estado de la ventana

Estado: aceptado (sesión anidada, Parte 4)

## Contexto

Antes de que Yuki mapee su primera ventana debe saber cuánto espacio ocupa su propio chrome alrededor de la superficie del cliente. El número se necesita antes de lo que parece: el configure que recibe un toplevel lleva su tamaño *útil*, que es el tamaño de la ventana menos los insets. Si los insets no existen cuando se escribe ese configure, este se escribe con el tamaño completo y cada tamaño posterior tiene que ser corregido.

El chrome de Yuki se decide en `yuki-design`: una barra de título con una posición (izquierda, derecha, arriba, abajo o ninguna) y un grosor. No todas las ventanas se decoran de la misma manera, y lo que recibe una ventana puede cambiar mientras vive.

## Decisión

`crate::decoration` es dueño de un `Insets { top, right, bottom, left }` en píxeles lógicos `f64`, y `Insets::for_window(role, window, chrome, titlebar)` es el único lugar donde se resuelve la decisión. Su precedencia es:

1. **Rol.** Un popup nunca se decora. Un diálogo — un toplevel con un padre — nunca se decora. Un toplevel normal puede decorarse.
2. **Estado de la ventana.** Fullscreen no reserva nada. Maximizado conserva su barra de título hoy; eso puede cambiar cuando exista el tiling.
3. **Negociación del cliente.** Un cliente que dibuja su propio chrome, o que pidió a través de `xdg_decoration` no ser decorado, no reserva nada.

Cualquier cosa que no sea un toplevel normal o maximizado con chrome del lado del servidor recibe [`Insets::ZERO`]. El borde y el grosor de la barra de título provienen de `yuki_design`; ningún llamador pasa un ancho, y ningún código puede asumir que una barra de título tiene una cantidad concreta de píxeles de alto.

## Las cuatro restricciones

1. **Los insets son una función del estado actual, no un campo.** `for_window` se llama en cada configure y lee el estado de la ventana cada vez. Nada se captura en el momento del mapeo. Fullscreen pone los insets a cero hoy porque la función se reevalúa; una ventana maximizada o en tile puede cambiarlos más adelante sin mover datos.
2. **Un solo lugar resuelve la precedencia.** Rol, luego estado, luego negociación del cliente, en el único `match` de `Insets::for_window`. Añadir un rol o un estado significa editar ese `match`, no esparcir condiciones por el código de mapeo.
3. **La región de entrada no coincide con la región visual.** Los bordes de resize se extienden unos pocos píxeles *fuera* de la ventana visible, por lo que la región de entrada puede ser más grande que la ventana, cosa que los insets visibles nunca expresan. Esto no se necesita hasta que aterricen move y resize, pero es lo que podría forzar a cambiar la forma de este tipo, así que se registra ahora: la región de entrada será un cálculo hermano, y sus insets pueden ser negativos (extendiéndose hacia afuera). Los campos `f64` ya lo permiten.
4. **Las sombras no son insets.** Se dibujan fuera de la ventana y no reservan espacio de layout; las sombras de ventanas adyacentes se solapan y no se empujan mutuamente. Las sombras pertenecen al scene graph, y los valores `SHADOW` de `yuki-design` no se leen aquí.

## Consecuencias

- El configure inicial de la Parte 5 calcula el tamaño útil como el tamaño de la ventana menos `horizontal()` y `vertical()`, de modo que una ventana decorada pide a su cliente una superficie más pequeña que la ventana exactamente por la barra de título.
- En esta parte no se dibuja nada. Cuando aterrice el renderizado (Parte 6), el área reservada se muestra como el fondo del compositor detrás de una superficie desplazada: una ventana normal tiene una franja visible donde se dibujará la barra de título, un diálogo no tiene ninguna. Esa es la verificación de que los insets están en la ruta del tamaño.
- La negociación `xdg_decoration` del cliente alimenta `Chrome`; hasta que se conecte, todo toplevel se trata como `Chrome::Server`, que es el valor por defecto del protocolo.
- El tipo es puro y está testeado con unit tests contra los valores de `yuki_design`; no tiene dependencia de Smithay ni de una superficie viva, por lo que puede ejercitarse antes de que exista cualquier ventana.