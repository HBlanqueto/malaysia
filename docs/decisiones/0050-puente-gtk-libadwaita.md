# 0050 — Puente de apariencia para GTK y libadwaita

Estado: aceptado (2026-10-31).

## Contexto

El portal (ADR 0031) publica `color-scheme` y `accent-color`. Con eso
libadwaita sigue claro/oscuro, pero redondea el acento al más cercano de sus
nueve predefinidos y no conoce el tinte de ventana (`window_color` = `accent` o
`wallpaper`). Las apps de barrier-kit sí reciben el acento exacto y el tinte
porque leen `appearance.domain` y `wallpaper.state` con `vow-appearance`.
GTK 3 no lee el portal para el modo oscuro.

La extensión de GNOME [ChromaLeon](https://github.com/Fabito02/ChromaLeon)
resuelve lo mismo escribiendo CSS de usuario de GTK. Se toma la idea; no se
copia código ni plantillas (ChromaLeon es GPL-3.0, Malaysia es MIT).

## Decisión

1. `xdg-desktop-portal-malaysia` mantiene el puente. Al arrancar y en cada
   cambio de `appearance.domain` o `wallpaper.state` escribe
   `$XDG_CONFIG_HOME/gtk-{3,4}.0/malaysia.css` y deja al principio de cada
   `gtk.css` un bloque marcado `malaysia:gtk-bridge` con
   `@import url("malaysia.css")`. Las reglas del usuario quedan debajo y siguen
   ganando.
2. Colores con `vow_appearance::resolve`, las mismas reglas que el kit: el
   acento siempre (`accent_bg_color`, `accent_fg_color`, `accent_color`); los
   fondos de Adwaita (ventana, headerbar, vista, barras laterales, tarjetas,
   diálogos, popovers) solo si hay tinte, mezclados hacia él con el factor de
   Tengen y las capas (`LAYER_FACTOR_*`). Respeta `tint_parts`. Con
   `contrast = true` solo se escribe el acento. Sin tinte, Adwaita queda tal
   cual (ADR 0020 §1).
3. GTK 4: variables CSS (`--accent-bg-color`, libadwaita ≥ 1.6) y
   `@define-color` (versiones anteriores), con un bloque
   `@media (prefers-color-scheme: light|dark)` por esquema. Así claro/oscuro
   cambia en vivo, también en apps que fuerzan un esquema.
4. GTK 3: solo el esquema actual, más los nombres `theme_*`, y
   `gtk-application-prefer-dark-theme` en `gtk-3.0/settings.ini`.
5. El portal publica además `org.gnome.desktop.interface` `color-scheme`
   (`default` / `prefer-dark`) para apps que solo leen esa clave.
6. `malaysia-session.target` arranca el portal
   (`xdg-desktop-portal-malaysia.service`, `Type=dbus`; la activación D-Bus
   usa la misma unidad con `SystemdService=`). Al recibir `SIGTERM`/`SIGINT`
   quita los bloques y los `malaysia.css`, para que otra sesión (GNOME) no los
   herede. `settings.ini` conserva su último valor.
7. `appearance.gtk_bridge = false` desactiva el puente y lo limpia. Un
   `gtk.css` o `settings.ini` que sea enlace simbólico (home-manager) no se toca.

## Consecuencias

- Las apps libadwaita ven el mismo acento y tinte que las de barrier-kit.
- GTK no relee `gtk.css`: un cambio de acento o tinte se ve al reabrir la app.
  Claro/oscuro sí es en vivo en GTK 4. Mientras tanto, libadwaita sigue
  recibiendo el acento redondeado por el portal, pero la variable del puente
  manda.
- El portal escribe fuera de `~/.config/malaysia`, solo en `gtk-3.0` y
  `gtk-4.0` y solo dentro de su bloque y sus archivos.

## Alternativas descartadas

- Tema GTK propio (`GTK_THEME`): libadwaita lo ignora y rompe apps GTK 3.
- Escribir en dconf (`org.gnome.desktop.interface`): cambia la configuración de
  GNOME para todas las sesiones.
- Forzar la recarga alternando el alto contraste (como ChromaLeon): parpadea en
  todas las apps.
