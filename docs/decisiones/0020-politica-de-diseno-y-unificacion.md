# 0020 — Política de diseño y unificación: chrome separado, efectos para todos

Estado: **aceptado.** Complementa al ADR 0019 y supera su exclusión de sombras en
ventanas auto-decoradas.

## Contexto

En Linux conviven GTK/libadwaita, Qt, Electron y apps propias. El compositor no
puede redibujar el interior de las apps. macOS unifica por monopolio del toolkit;
GNOME, por reparto entre chrome de ventana y preferencias estándar.

## Decisión

1. **Chrome separado.** GTK/Libadwaita con headerbar conserva su apariencia y no
   recibe titlebar del compositor. Qt y las demás apps reciben el chrome
   unificado server-side. Un cliente que no negocia se auto-decora (ADR 0019).
2. **Efectos para todas las superficies**, sean CSD o SSD: inner border estilo
   macOS, sombra (también en CSD) y blur/vibrancy. La sombra se calcula contra
   `xdg_surface.set_window_geometry` cuando exista.
3. **Efectos compositor-side**, no por app: un mecanismo de estilo por superficie
   (tipo `otto-surface-style` o render interno) aplica radio, sombra, blur y
   borde sin que el cliente pinte nada.
4. **Apariencia estándar:** `org.freedesktop.appearance` (`color-scheme`,
   `accent-color`) vía xdg-desktop-portal.
5. **Tokens centralizados** en `vow-design` (ADR 0011).
6. **La shell propia** usa el mismo lenguaje y los mismos tokens.

## Consecuencias

- Las apps GTK se ven como son, pero su ventana queda enmarcada por el
  compositor; un tema transparente funciona en todas porque el blur lo pone el
  compositor.
- ADR 0019 sigue vigente para la negociación; solo se supera su exclusión de
  sombras.
- La política es obligatoria para agentes de IA (ver `AGENTS.md`).

## Alternativas descartadas

- Redibujar el interior de las apps: imposible sin reimplementar cada toolkit.
- Efectos por app: rompe la uniformidad y obliga a cada app a integrarse.
