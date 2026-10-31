# 0019 — Política de decoración: honrar lo que el cliente negoció

Estado: aceptado. Supera al ADR 0017.

## Contexto

El compositor dibuja chrome para ventanas SSD (ADR 0014).

## Decisión

1. Un cliente que negocia por xdg-decoration o KDE y acepta server-side recibe chrome del compositor.
2. Un cliente que pide client-side, o que nunca negocia, se auto-decora: sin titlebar, hairline ni línea interior. El ADR 0020 añade después sombra para todas.
3. El modo se decide antes del primer attach; cada configure lo lleva. Si cambia después, se recalculan insets, se envía configure y se actualiza la escena.
4. `new_decoration`/`unset_mode` responden server-side; `request_mode` responde lo pedido. El manager KDE anuncia `default_mode = server`, lo que hace que GTK3/GTK4 planas prefieran SSD. Un objeto KDE sin petición se trata como auto-decorador.
5. GTK4 pide el modo antes de que exista su toplevel; se recuerda y se aplica al crear la ventana. GTK/libadwaita con headerbar y Qt frameless conservan CSD; Qt ordinario se difiere al compositor.
6. El modo se guarda por ventana en `yuki-scene`. `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` es el fallback para Qt sin xdg-decoration, solo en clientes lanzados por el compositor.

## Consecuencias

El ADR 0017 se revierte: server-side es el default, pero se honra client-side explícito.

## Alternativas descartadas

- Forzar server-side siempre (ADR 0017): duplica chrome en apps auto-decoradas.
