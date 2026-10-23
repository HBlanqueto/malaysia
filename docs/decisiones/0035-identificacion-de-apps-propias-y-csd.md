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
3. **Precedencia.** ~~Para una ventana con el prefijo, el modo efectivo es
   siempre `Client`, incluso con `force_server`.~~ **Sustituido por la sección
   «Elección de la app»**: una app propia elige `Client` o `Server` y Yuki
   respeta esa elección.
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

## Elección de la app (enmienda 2026-10-23)

Sustituye la regla «native → Client siempre» del punto 3.

- Cada ventana de CelestialKit declara su decoración:
  - `Decorations::Client`, por defecto: dibuja su barra unificada de 44 px
    (`titlebar-h`), con `TrafficLights` y hueco de toolbar.
  - `Decorations::Server`: Yuki dibuja su barra de 32 px (`titlebar-ssd-h`).
- El kit pide **siempre** el modo de forma explícita con
  `xdg-decoration.set_mode` antes del primer commit, junto con
  `set_app_id(NATIVE_APP_ID_PREFIX + nombre)`.
- Para una ventana propia, Yuki respeta el modo pedido y no aplica
  `window.decorations`: `force_server` y `force_client` solo afectan a apps
  ajenas.
- `requested` es `None` hasta que llega `request_mode`; crear el objeto de
  decoración o hacer `unset_mode` no cuentan. Una app propia sin modo negocia
  `Client` y el compositor avisa una vez por ventana («app propia sin modo de
  decoración»).
- Si el compositor no anuncia `xdg-decoration` o responde `ClientSide` a una
  petición `Server`, el kit dibuja su barra de 44 px. Bajo Yuki recibe lo que
  pidió.
- Client es para apps con toolbar o sidebar (Ajustes); Server para utilidades
  simples sin toolbar.
