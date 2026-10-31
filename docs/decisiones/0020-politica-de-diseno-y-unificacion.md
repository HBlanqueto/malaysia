# 0020 — Política de diseño y unificación: chrome separado, efectos para todos

Estado: aceptado. Complementa al ADR 0019 y supera su exclusión de sombras en ventanas auto-decoradas.

## Contexto

En Linux conviven GTK/libadwaita, Qt, Electron y apps propias; el compositor no puede redibujar su interior. macOS unifica por monopolio del toolkit; GNOME, por reparto entre chrome y preferencias estándar.

## Decisión

1. **Chrome separado:** GTK/Libadwaita con headerbar conserva su apariencia y no recibe titlebar. Qt y las demás reciben el chrome unificado server-side. Sin negociación, CSD (ADR 0019).
2. **Efectos para todas las superficies**, CSD o SSD: inner border, sombra (también en CSD) y blur/vibrancy. La sombra usa `xdg_surface.set_window_geometry` cuando exista.
3. **Efectos compositor-side**, no por app: un mecanismo de estilo por superficie (tipo `otto-surface-style` o render interno) aplica radio, sombra, blur y borde.
4. **Apariencia estándar:** `org.freedesktop.appearance` (`color-scheme`, `accent-color`) vía xdg-desktop-portal.
5. **Tokens en `yuki-design`** (ADR 0011). La shell propia usa el mismo lenguaje.

## Consecuencias

Las apps GTK se ven como son, pero quedan enmarcadas por el compositor; un tema transparente funciona en todas. ADR 0019 sigue vigente para la negociación.

## Alternativas descartadas

- Redibujar el interior de las apps: imposible sin reimplementar cada toolkit.
- Efectos por app: rompe la uniformidad.
