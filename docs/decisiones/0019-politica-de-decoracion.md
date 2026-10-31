# 0019 — Política de decoración: honrar lo que el cliente negoció

Estado: **aceptado.** Supera al ADR 0017.

## Contexto

El compositor dibuja chrome para ventanas SSD (ADR 0014). Si cliente y
compositor no acuerdan quién decora, ambos dibujan y aparecen dos barras de
título. Los protocolos son `zxdg_decoration_manager_v1` y el KDE
`org_kde_kwin_server_decoration_manager`; los toolkits difieren en cuál hablan y
en si aceptan server-side.

## Decisión

1. Un cliente que negocia por xdg-decoration o KDE y acepta server-side recibe
   chrome del compositor.
2. Un cliente que pide explícitamente client-side, o que nunca negocia, se
   auto-decora: sin titlebar, hairline ni línea interior del compositor. El ADR
   0020 añade después sombra para todas las ventanas.
3. El modo se decide antes del primer attach; cada configure lo lleva. Si el
   cliente cambia de modo después, se recalculan insets, se envía configure y se
   actualiza la escena.
4. `new_decoration`/`unset_mode` responden server-side (el cliente delegó);
   `request_mode` responde lo que pidió el cliente. El manager KDE anuncia
   `default_mode = server`, lo que hace que GTK3/GTK4 planas prefieran SSD. Un
   objeto KDE sin petición de modo se trata como auto-decorador.
5. GTK4 pide el modo antes de que exista su toplevel; el modo se recuerda y se
   aplica al crear la ventana. GTK/libadwaita con headerbar y Qt frameless
   conservan CSD; Qt ordinario se difiere al compositor.
6. El modo se guarda por ventana en `malaysia-scene`, junto a la geometría.
   `QT_WAYLAND_DISABLE_WINDOWDECORATION=1` es el fallback para Qt sin
   xdg-decoration, solo en clientes lanzados por el compositor.

## Consecuencias

- El ADR 0017 se revierte: server-side es el default, pero se honra una petición
  explícita de client-side.
- Conviven ventanas CSD (GTK con headerbar, libadwaita, Qt frameless) y SSD
  (GTK planas, Qt ordinario).
- Los insets, el hit-testing y las zonas de borde leen el modo desde la escena.

## Alternativas descartadas

- Forzar server-side siempre (ADR 0017): duplica chrome en apps que se
  auto-decoran.
- Forzar client-side siempre: pierde el chrome unificado en Qt y apps sin CSD.
