# 0035 — Identificación de apps propias y CSD

Estado: **aceptado** (2026-10-23).

## Contexto

Las apps propias de Yuki se dibujan con CelestialKit y llevan su propia
titlebar y sus semáforos (CSD). El compositor también puede añadir chrome
server-side (`window.decorations`, ADR 0019/0020). Con `force_server` esa
política pintaría una segunda barra sobre una app propia.

Hace falta una forma estable de que el compositor reconozca una ventana como
propia y no le añada nunca su titlebar. No puede basarse en el título ni en el
protocolo de decoración por sí solo: una app ajena también puede pedir CSD, y
una app propia podría olvidarse de pedirlo.

## Decisión

1. **Prefijo de app-id.** Una app de CelestialKit usa un `app_id` que empieza
   por `NATIVE_APP_ID_PREFIX = "io.github.hblanqueto.yuki."`, constante de
   `yuki-design`.
2. **CSD explícito.** Antes de su primer commit, la app pide
   `zxdg_toplevel_decoration_v1.set_mode(ClientSide)`, como hace `otto-kit` con
   `WindowDecorations::RequestClient`.
3. **Precedencia.** Para una ventana con el prefijo, el modo efectivo es
   siempre `Client`, incluso con `force_server`. Para el resto, la política no
   cambia.
4. **Orden irrelevante.** El compositor guarda el modo *pedido* por ventana y
   recalcula el efectivo cuando llega `app_id_changed` y antes del configure
   inicial. Una petición KDE que llega antes del `xdg_toplevel` se guarda como
   pedida, no como efectiva.
5. **Aviso.** Si una app propia pidió explícitamente `Server`, el compositor
   avisa una vez por ventana («app propia que no pidió CSD»). Sigue sin barra.
6. **No es seguridad.** El prefijo solo decide si se dibuja una barra. Cualquier
   cliente puede usar ese app-id; no autentica ni aísla nada. Infinity lo
   sustituirá cuando exista.

## Consecuencias

- El compositor comparte la regla pura `effective_decoration(policy, requested,
  native)`; los handlers solo traducen protocolo a modo pedido.
- CelestialKit expone `TrafficLights` para las apps propias. El runner todavía
  no crea `xdg_toplevel`; el contrato para hacerlo queda anotado como
  `TODO(ADR 0035)` en `platform/mod.rs`.
- Una app propia sin CSD no recibe la barra del compositor: se ve sin chrome,
  que es el fallo visible que el aviso pretende detectar.
- `yuki_appearance::traffic` sigue siendo puro y sin Skia; el componente de
  CelestialKit es el único que dibuja.
