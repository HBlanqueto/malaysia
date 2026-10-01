# 0020 — Política de diseño y unificación: chrome separado, efectos para todos

Estado: **aceptado.** Complementa al ADR 0019 (negociación de decoración) y supera una
de sus consecuencias: la que excluía sombras en ventanas auto-decoradas.

## Contexto

Yuki quiere un diseño unificado: el mismo lenguaje en todo el escritorio. Pero en
Linux coexisten toolkits (GTK4/libadwaita, Qt, Electron, ...) y el compositor no
puede re-dibujar el interior de las apps. macOS unifica por monopolio del toolkit
(AppKit dibuja el chrome y la apariencia); GNOME unifica por reparto: chrome de la
ventana (CSD vía libadwaita o SSD del compositor) + preferencias estándar en el
portal `org.freedesktop.appearance`.

Este ADR fija la política de Yuki para ese reparto, con una decisión de diseño
explícita:

- **GTK/Libadwaita (las apps GNOME) se separan a propósito:** conservan su
  apariencia propia — su headerbar, su look — y NO reciben la barra de título de
  Yuki. Ya lo hace ADR 0019; aquí se reafirma como política de diseño.
- **Pero las superficies de esas apps SÍ reciben los efectos del compositor:**
  inner border estilo macOS, sombras y blur. Cuando el usuario haga un tema
  transparente, las apps GTK deben poder apoyarse en el blur del compositor.
- **Qt y demás apps (las que se vayan creando o lleguen de terceros) reciben el
  chrome unificado de Yuki** (barra de título y controles del compositor).

Es decir: el compositor unifica **todo lo que rodea a la ventana** (borde, sombra,
blur, y para Qt también el chrome), y deja el **interior** de las apps GNOME tal y
como son.

## Decisión

### 1. Reparto del chrome de la ventana (heredado de ADR 0019)

- **GTK3/GTK4/libadwaita** con headerbar → **client-side**: conservan su
  apariencia nativa, sin barra de título del compositor, sin controles de Yuki.
- **Qt y otras apps** (Electron, nuevas, sin headerbar propio) → **server-side**:
  barra de título y controles del compositor, unificados.
- Un cliente que no negocia se trata como auto-decorador (ADR 0019).

### 2. Efectos de superficie del compositor para TODAS las ventanas

Los efectos siguientes se aplican a **cualquier superficie**, sin importar el modo
de decoración (CSD o SSD):

- **Inner border estilo macOS** — el borde interior fino alrededor del contenido
  de la ventana.
- **Sombra** — se aplica también a las ventanas auto-decoradas (GTK/libadwaita,
  frameless). **Esto supera** la consecuencia del ADR 0019 que la excluía. Para
  no chocar con la forma desconocida de una superficie CSD, la sombra se calcula
  contra la geometría declarada por el cliente (`xdg_surface.set_window_geometry`)
  cuando exista; el detalle de recorte se decide al implementarla.
- **Blur / vibrancy** — el compositor difumina el contenido que queda **detrás**
  de las superficies translúcidas. Habilita un tema transparente: la app pinta
  con alpha y el compositor aplica el blur, a la manera de la vibrancy de macOS.

### 3. Mecanismo

Los efectos de superficie se implementan **compositor-side**, no por cada app:

- Un mecanismo de estilo por superficie (protocolo propio al estilo de
  `otto-surface-style-unstable-v1`, o aplicación interna sobre el render) que
  permite al compositor aplicar corner radius, sombra, blur e inner border sin que
  el cliente pinte nada.
- El render debe soportar transparencia (buffers con alpha) y difuminado de lo que
  hay detrás de cada superficie.

### 4. Preferencias de apariencia estándar

Yuki publica `org.freedesktop.appearance` (`color-scheme` y `accent-color`) vía
xdg-desktop-portal, como hace GNOME, para que cualquier app de cualquier toolkit
lea el dark/light y el accent del sistema.

### 5. Design tokens centralizados

Todos los valores de los efectos (márgenes del inner border, radios, intensidad de
sombra, blur, colores del tema) viven en `yuki-design` (ADR 0011). Un tema
transparente significa que las superficies del cliente son transparentes y el blur
lo aporta el compositor con esos tokens.

### 6. Shell

La UI propia del escritorio (top bar, dock, menús del sistema) usa el mismo
lenguaje de diseño y los mismos tokens que las decoraciones y los efectos.

## Consecuencias

- Las apps GTK/libadwaita se ven **como son** (su headerbar, su interior), pero su
  ventana queda enmarcada por el compositor (inner border, sombra, blur) igual que
  el resto → unificación de lo que rodea a la ventana sin romper su apariencia.
- Un tema transparente funciona para **todas** las apps: la app aporta la
  transparencia, el compositor el blur.
- ADR 0019 sigue vigente en todo lo demás (negociación del modo, insets, recálculo
  tras cambiar de modo); solo queda superada su exclusión de sombras en ventanas
  auto-decoradas.
- Pendiente de implementar (fuera de este ADR): el mecanismo de efectos por
  superficie, el soporte de transparencia en el render, los efectos en sí
  (inner border, sombra, blur), y la publicación del portal de apariencia.
- Este ADR es la política que deben respetar los agentes de IA que trabajen en el
  repo: ver `AGENTS.md`.