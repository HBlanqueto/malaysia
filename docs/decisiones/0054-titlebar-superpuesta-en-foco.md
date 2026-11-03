# 0054 — Titlebar superpuesta en foco

Estado: aceptado (2026-11-01). Implementado en barrierkit (`ce12d36`,
`3841a09`), en el compositor y en el fork de libadwaita (`605edc1`); falta
aceptarlo en sesión. Cambia el ADR 0051 §6 para las apps CSD.

## Contexto

En modo foco, con el puntero arriba, el compositor bajaba 28 px la ventana
CSD (barrierkit, libadwaita) para que su barra quedara bajo el panel negro.
La ventana entera se deslizaba y perdía 28 px por abajo; en libadwaita
(ADR 0053) la headerbar además empujaba el contenido al aparecer. En macOS
la ventana no se mueve: la barra de herramientas baja **sobre** el
contenido, como ya hace aquí la titlebar Tengen de una app SSD.

## Decisión

1. **Protocolo** `infinity_focus_v1` **versión 2** (barrierkit):
   - `infinity_focus_surface_v1.set_overlay`: la app dibuja su titlebar
     sobre su contenido, debajo de la barra; el compositor no mueve la
     ventana.
   - Evento `bar_height(height)`, enviado justo antes de cada `bar(1)` a
     quien pidió `set_overlay`: la altura que tapa la barra, en píxeles
     lógicos de la superficie.
   - Un cliente v1, o sin `set_overlay`, sigue como en el ADR 0051: la
     ventana baja.
2. **Compositor:** ofrece la v2. Al bajar la barra sobre una ventana CSD
   en foco, solo la desplaza (escena y dibujo) si no pidió `set_overlay`.
   `FocusReveal` separa la ventana avisada (`notified`) de si se desplazó
   (`shifted`, que se conserva mientras la barra sube para el dibujo).
3. **barrierkit:** liga hasta la v2 y llama a `set_overlay`. En fullscreen
   la `Titlebar` sale del layout (el contenido ocupa toda la ventana); con
   la barra bajada se dibuja encima del contenido a `bar_height`, en una
   capa por delante del contenido y por debajo de menús y popups. Los
   semáforos de la barra lateral bajan lo mismo.
4. **libadwaita (fork):** las `AdwToolbarView` que controla el modo foco
   (ADR 0053) extienden su contenido hasta el borde superior durante todo
   el fullscreen y, con la barra bajada, muestran sus barras superiores
   encima, a `bar_height`, con el estilo `raised` (opaco, con sombra). Es un
   modo privado de la vista: no cambia `extend-content-to-top-edge` ni
   `top-bar-style`. Las vistas que gestiona la app no se tocan.

## Consecuencias

- Ni las apps de barrierkit ni las de libadwaita se deslizan: el contenido
  queda quieto y la titlebar tapa su parte de arriba mientras la barra
  está bajada, como la titlebar SSD.
- Una app de barrierkit en foco ya no muestra su titlebar sin la barra
  (antes la mantenía, solo sin semáforos).
- Una vista de libadwaita que la app gestiona sola (un visor que muestra su
  headerbar al mover el ratón) puede quedar bajo la barra negra con la barra
  bajada, porque la ventana ya no baja.
- Apps con la versión 1 (barrierkit o libadwaita anteriores, otras apps)
  siguen deslizándose: el compositor mantiene ese camino.

## Alternativas descartadas

- **Mantener el deslizamiento solo para barrierkit:** dos comportamientos
  en el mismo modo foco.
- **Que la app adivine la altura de la barra:** `MENUBAR_HEIGHT` existe en
  `vow-design`, pero libadwaita no lo conoce y la barra puede medir otra
  cosa; el compositor la envía.
- **`top-bar-style = raised` y `extend-content-to-top-edge` públicos:**
  notifican a la app y pisan valores que la app puede vigilar.

## Cambios posteriores

Cambios en barrierkit desde que se aceptó este ADR. El detalle de cada caso está
en el [capítulo 7 de su documentación](https://github.com/HBlanqueto/barrierkit/blob/main/docs/barrierkit/07-titlebars.md#maximizada-pantalla-completa-y-modo-foco).

- **Barra de pantalla completa** (`FullscreenBar`, ADR 0057 de barrierkit): la
  titlebar superpuesta se dibuja lisa (`Solid`, por defecto) o con vidrio que
  difumina lo de debajo (`Glass`, `ProgressiveGlass`).
- **Empujar el contenido** (`WindowOptions::fullscreen_push`, `4de4079`): con
  la barra bajada, el contenido baja lo que miden la barra y la titlebar en vez
  de quedar tapado. Se combina con cualquier `FullscreenBar`.
- **En vivo** (`d07d84e`): la barra y el empuje son estados de
  `WindowControls`; la app los cambia sin reabrir la ventana.
- **Rail** (`bcc1ecd`): con `TitlebarLayout::Rail` la titlebar superior no
  sale del layout ni se superpone, y nada baja con la barra. Solo el rail entra
  desde la izquierda, con sus semáforos bajo la barra.
- **Vistas previas** (`d07d84e`): una `Titlebar::preview` dentro del contenido
  no lee el estado de la ventana, así que no se superpone en pantalla completa.
