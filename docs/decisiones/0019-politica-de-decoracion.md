# 0019 — Política de decoración: honrar lo que el cliente negoció

Estado: **aceptado.** Supera al ADR 0017, que configuraba el modo server-side para cada cliente que vinculaba `zxdg_decoration_manager_v1` y así ponía la barra de título de Yuki encima de la de un cliente que se auto-decoraba.

## Contexto

Yuki dibuja chrome para ventanas decoradas por el servidor (ADR 0014): una barra de título, un hairline, una línea interior, una sombra. Un cliente solo deja ese chrome al compositor si ambos se ponen de acuerdo; si no, ambos dibujan y la ventana muestra dos barras de título. Los protocolos de acuerdo son `zxdg_decoration_manager_v1` y el más antiguo `org_kde_kwin_server_decoration_manager`. Los clientes difieren en cuál hablan y en si aceptan el modo server-side en absoluto.

### Cómo deciden otros compositores

- **sway.** Crea ambos managers con el modo por defecto de KDE `server` (`sway/server.c:387-412`). `set_xdg_decoration_mode` por defecto es `SERVER_SIDE`; para una vista **flotante** honra el modo solicitado por el cliente, y para una vista en tile mantiene server-side y dibuja su propio borde (`sway/xdg_decoration.c:66-92`). Un cliente sin objeto xdg-decoration se trata como decorado por el cliente: `handle_map` cae al objeto KDE y `csd = !deco || deco->mode == CLIENT` (`sway/desktop/xdg_shell.c:479-495`); el listener de modo KDE hace lo mismo (`sway/decoration.c:37-63`), y `view_update_csd_from_client` cambia el borde del contenedor a `B_CSD` (`sway/tree/view.c:526-538`).
- **niri.** `new_decoration` y `unset_mode` eligen server-side; `request_mode` almacena **lo que sea que el cliente pidió**, con un comentario sobre un bug de SDL2 en el que forzar un modo distinto rompe la creación de ventanas (`src/handlers/xdg_shell.rs:978-1025`). El handler KDE responde con el modo del cliente y registra si era server (`:1032-1067`). niri no dibuja ninguna barra de título en absoluto: con `prefer-no-csd` expone los globals de decoración para que los clientes omitan sus CSD y dibuja un anillo de focus/borde alrededor de ellos (`src/niri.rs:2396-2412`, `resources/default-config.kdl:150-154`).
- **KWin.** La política por defecto de la ventana es `ClientPreference` (`src/xdgshellwindow.h:262`). `preferredDecorationMode` (`src/xdgshellwindow.cpp:1391-1443`) devuelve: client cuando no existe un plugin de decoración o el cliente no tiene ningún objeto de decoración en absoluto; server cuando el cliente creó un objeto xdg-decoration pero nunca llamó a `set_mode`; y en caso contrario el modo que el cliente pidió por xdg-decoration, cayendo al objeto KDE. El modo elegido se envía de vuelta con el configure (`:1500-1530`).

Los tres tratan la *no negociación* como auto-decoración, y ninguno fuerza server-side contra una petición explícita de client-side.

### Qué hacen los toolkits

- **Qt 5.15 y Qt 6** usan xdg-decoration. Un toplevel crea un objeto de decoración salvo que sea frameless, y luego llama a `unset_mode`, dejando la elección al compositor (`qwaylandxdgshell.cpp:29-33,180-186`; el helper `requestMode` existe pero no se llama en 5.15 ni en 6.8). Suprime las decoraciones del lado del cliente mientras la decoración no está configurada **o** una vez que el compositor ha configurado server-side (`qwaylandxdgshell.cpp:113-120` en ambos). Sin el global cae a su plugin de decoración, que `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` desactiva (`qwaylanddisplay.cpp:888-903`). Así que el "request server-side" del brief es impreciso: Qt **se difiere** al compositor y solo dibuja CSD si se le dice client-side o si el global falta.
- **GTK3** nunca usa xdg-decoration. Crea un `org_kde_kwin_server_decoration` por toplevel y pide `client` cuando dibuja CSD, `server` cuando no lo hace (`gdk/wayland/gdkwindow-wayland.c:2509-2535`). Cuál dibuja se decide en realize: una ventana con un `GtkHeaderBar` (establecido a través de `gtk_window_set_titlebar`) tiene `client_decorated` marcado (`gtk/gtkwindow.c:4198,4256`) y anuncia CSD; una ventana plana anuncia SSD (`gtk/gtkwindow.c:7592-7602`). GTK solo *prefiere* SSD si el evento `default_mode` del manager dijo `server` (`gdk/wayland/gdkdisplay-wayland.c:366-393`, `gtk/gtkwindow.c:6132-6159`); si no, `gtk_window_should_use_csd` es true e incluso una ventana plana dibuja CSD.
- **GTK4** es el mismo panorama, no "siempre client-side": también usa el protocolo KDE y ningún xdg-decoration en absoluto (el árbol no contiene ninguna referencia `zxdg_decoration`). `gdk_wayland_toplevel_set_decorated` crea el objeto KDE y pide `server`/`client` (`gdk/wayland/gdktoplevel-wayland.c:1434-1456`), manejado por `decorated && !client_decorated` (`gtk/gtkwindow.c:3264,4547`), con la misma lógica de preferencia `default_mode` (`gdk/wayland/gdkdisplay-wayland.c:445-463,1273-1282`, `gtk/gtkwindow.c:3975-4006`). Una ventana GTK4 plana acepta por tanto server-side cuando el manager dice `server` (verificado con un cliente mínimo, y esto es lo que hace especial a GTK4: crea el objeto KDE **durante realize, antes de que exista el xdg_toplevel**); una ventana con headerbar conserva CSD. GTK4 no instala un listener en el objeto de decoración por superficie, por lo que la respuesta `mode` del compositor se descarta del lado del cliente y solo importa la geometría del configure. `gtk4-demo` conserva CSD por su propia cuenta (nunca crea un objeto de decoración), que la política deja en paz.
- **libadwaita** las ventanas siempre tienen una headerbar: `AdwWindow` y `AdwApplicationWindow` llaman a `gtk_window_set_titlebar` con ella (`src/adw-window.c:328`, `src/adw-application-window.c:280`) y fallan con error si una aplicación llama a `gtk_window_set_titlebar` por sí misma (`:176`, `:128`). Así que Nautilus (GTK4 + libadwaita) siempre está decorado por el cliente.
- **Aplicaciones frameless** como OnlyOffice dibujan su propia área de título. Una ventana frameless de Qt nunca crea el objeto de decoración (`qwaylandxdgshell.cpp:29-33`, protegido por `Qt::FramelessWindowHint`), por lo que nunca negocia y cae bajo la regla de no-negotiation más abajo.

## Decisión

1. Un cliente que negocia a través de xdg-decoration o el protocolo KDE y acepta el modo server-side recibe chrome del compositor.
2. Un cliente que pide explícitamente el modo client-side, o que nunca negocia decoraciones, se auto-decora: sin barra de título del compositor, sin hairline ni línea interior. **Tampoco sombra del compositor.** Se desconoce la forma de la superficie del cliente, así que una sombra se vería a través de esquinas redondeadas o transparentes; GTK y Qt dibujan sus propias sombras con sus CSD.
3. El modo se decide antes del primer attach de buffer. Cada configure lleva el modo, y el configure inicial se envía cuando se crea el toplevel. Si el cliente negocia después de eso, se envía un configure nuevo, y la ventana no se mapea hasta que se reconozca el configure más reciente (`Windows::ack_configure` acepta solo el serial más reciente).
4. Si un cliente cambia de modo después del mapeo, el compositor recalcula los insets, envía un configure con el nuevo tamaño útil, y actualiza el modo almacenado en `yuki-scene` en el mismo paso, de modo que el hit-testing y la zona de move cambian con el chrome y no queda ninguna barra de título obsoleta.

Los handlers de xdg-decoration responden `new_decoration` y `unset_mode` con server-side (el cliente pidió al compositor que eligiera), y `request_mode` con el modo que pidió el cliente. El manager KDE anuncia `default_mode = server`, por lo que las ventanas GTK3/GTK4 planas prefieren server-side, y responde cada petición con el modo solicitado; un cliente que crea el objeto pero nunca pide un modo se trata como auto-decorador. Como GTK4 pide el modo antes de que exista su toplevel, un modo para una superficie no rastreada se recuerda y se aplica cuando se crea el toplevel, de modo que su configure inicial ya lleva el modo y el inset.

El modo se almacena por ventana en `yuki-scene`, junto a la geometría, y los insets, el hit-testing y las zonas de borde lo leen desde allí.

`QT_WAYLAND_DISABLE_WINDOWDECORATION=1` es el fallback para versiones de Qt que carecen de xdg-decoration; se establece solo en el entorno de los clientes que Yuki genera, nunca en el entorno de la sesión.

## Consecuencias

- El ADR 0017 se revierte: server-side es el valor por defecto y la respuesta a un modo no establecido, pero una petición explícita de client-side se honra.
- Las ventanas GTK3/GTK4 con headerbar, las ventanas libadwaita y las ventanas Qt frameless conservan su propio chrome; las GTK3/GTK4 planas y las ventanas Qt ordinarias reciben el de Yuki.
- **Yuki reserva la banda de la barra de título pero aún no pinta chrome.** No existe un renderer de barra de título, hairline o sombra; "server-side" hoy significa que la superficie del cliente está inseteada y que el modo se negocia. Hasta que ese renderer exista, una ventana decorada por el servidor muestra una banda vacía donde estará su barra de título. La política aquí es lo que el renderer dibujará en ese lugar.
- Una sombra para ventanas auto-decoradas está deliberadamente fuera de alcance; si se quisiera más adelante necesitaría la forma de la superficie, que el protocolo no proporciona.
- Yuki aún no tiene una ruta de generación de clientes, por lo que la variable de Qt no se establece en ningún sitio hoy; la regla anterior entra en vigor cuando exista una. El harness de tests la establece para los clientes Qt que lanza y así lo indica.

## Referencias

- [0017-xdg-decoration-del-lado-del-servidor.md](0017-xdg-decoration-del-lado-del-servidor.md) — la política superada.
- Smithay `src/wayland/shell/xdg/decoration.rs`, `src/wayland/shell/kde/decoration.rs`.
- `crates/yuki/src/handlers.rs` — ambos handlers de decoración.
- `crates/yuki/src/window.rs`, `crates/yuki-scene/src/scene.rs` — dónde vive el modo por ventana y qué lo lee.