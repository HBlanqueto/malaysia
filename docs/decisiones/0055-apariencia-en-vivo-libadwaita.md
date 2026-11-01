# 0055 — Acento y tinte en vivo en libadwaita

Estado: aceptado (2026-11-01). Implementado en el fork
[`HBlanqueto/libadwaita`](https://github.com/HBlanqueto/libadwaita) (rama
`claude/practical-ritchie-cez5d7`, `2393318`), en el portal y en el paquete
`libadwaita-global-menu`; falta aceptarlo en sesión con apps reales. Amplía
el ADR 0050 y cierra la deuda G-02.

## Contexto

Con el puente del ADR 0050, una app libadwaita ve el acento exacto y el tinte
de ventana, pero solo al abrirse: GTK lee `~/.config/gtk-4.0/gtk.css` una vez
(`gtksettings.c`, sin vigilar el archivo). Claro/oscuro sí cambia en vivo,
porque libadwaita lleva los dos esquemas tras `@media (prefers-color-scheme)`
y el portal publica `color-scheme`. El acento del portal también llega en vivo,
pero libadwaita lo redondea al más cercano de sus nueve predefinidos
(`adw_accent_color_nearest_from_rgba`) y su `accent-color-rgba` devuelve el
predefinido. Las apps de barrierkit cambian acento y tinte en vivo leyendo
`appearance.domain`.

libadwaita fija `gtk-theme-name = "Adwaita-empty"`, así que un tema GTK no
sirve, y forzar la recarga de `gtk.css` alternando el alto contraste parpadea
(descartado en el ADR 0050).

## Decisión

1. **Namespace propio en el portal.** `xdg-desktop-portal-malaysia` publica
   `org.malaysia.appearance` en `org.freedesktop.impl.portal.Settings`, con
   `SettingChanged` desde el mismo vigilante de `appearance.domain` y
   `wallpaper.state`:
   - `gtk4-css` (`s`): las reglas GTK 4 del puente (`gtk::live_css`), los
     mismos colores que `malaysia.css` pero solo los dos bloques `@media`.
   - `accent-color` (`(ddd)`): el acento exacto, el mismo que
     `org.freedesktop.appearance`.
   Con `appearance.gtk_bridge = false` la hoja va vacía y el acento fuera de
   rango, `(-1, -1, -1)`: libadwaita queda de serie.
2. **Parche del fork** (`src/adw-live-appearance.c`, tercer parche sobre
   1.9.3, `nix/patches/libadwaita-live-appearance.patch`):
   - Un objeto por proceso lee el namespace con `ReadAll` y sigue
     `SettingChanged`. Sin el namespace (cualquier otro escritorio), o con
     `ADW_DISABLE_LIVE_APPEARANCE` o `ADW_DISABLE_PORTAL`, no hace nada.
   - `AdwStyleManager` carga `gtk4-css` en un `GtkCssProvider` propio con
     prioridad `GTK_STYLE_PROVIDER_PRIORITY_USER + 1` y le pone
     `prefers-color-scheme` y `prefers-contrast` como a la hoja de Adwaita.
     GTK evalúa las `@media` al cargar, así que sin eso el bloque oscuro no
     aplicaría. Un cambio recarga solo ese provider, con las transiciones
     apagadas como cualquier cambio de estilo de libadwaita.
   - `accent-color-rgba` devuelve el acento exacto y se notifica al cambiar.
     `accent-color` (el enum) sigue siendo el predefinido más cercano: es API
     pública y no puede dar otro valor.
3. **Por qué por encima del usuario.** `gtk.css` sigue importando
   `malaysia.css` para Flatpak, GTK 3 y la libadwaita sin parche, y esa
   copia queda vieja tras un cambio. Con la misma prioridad, la copia vieja
   podría ganar. Con `USER + 1` gana la viva. A cambio, una regla del usuario
   que redefina **esos mismos** colores (`accent_bg_color`,
   `window_bg_color`…) pierde mientras el puente esté activo. Para usar los
   suyos, `appearance.gtk_bridge = false`. El resto de sus reglas siguen
   ganando.
4. **`accent_color` con contraste.** El puente escribía el acento tal cual
   como `accent_color` (texto e iconos). Ahora aplica la regla de libadwaita
   (`adw_rgba_to_standalone`, OKLab con L ≤ 0,5 en claro y ≥ 0,85 en oscuro),
   tanto en `malaysia.css` como en `gtk4-css`. libadwaita 1.6+ ya deriva
   `--accent-color` así, y la variable no se escribe.
5. **`dark_tint`.** El portal sigue publicando `color-scheme = 1` (oscuro).
   El bloque oscuro de `gtk4-css` lleva los fondos de Adwaita oscuro
   mezclados hacia el tinte con la paleta `dark_tint`, igual que el puente.

## Consecuencias

- Las apps libadwaita del sistema cambian acento, tinte y claro/oscuro en
  vivo, junto con las de barrierkit y dentro del fundido del ADR 0044.
- Las apps Flatpak usan la libadwaita de su runtime, sin el parche: acento
  redondeado en vivo, claro/oscuro en vivo y, si ven `~/.config/gtk-4.0`,
  acento exacto y tinte al reabrir.
- GTK 3 sigue igual: esquema y colores al reabrir.
- En otra sesión (GNOME) el parche no encuentra el namespace y libadwaita se
  comporta de serie. El portal limpia `gtk.css` al parar (ADR 0050 §6).
- Cada app hace un `ReadAll` síncrono más al arrancar, como las lecturas de
  `org.freedesktop.appearance` que libadwaita ya hace.

## Alternativas descartadas

- **Vigilar `malaysia.css` desde libadwaita:** dos fuentes de verdad, y el
  formato del archivo pasaría a ser API entre el portal y el fork.
- **Publicar los colores como diccionario `a{s(dddd)}`:** obliga a duplicar
  en C la generación del CSS (variables, `@define-color`, bloques) que el
  puente ya hace en Rust.
- **Protocolo Infinity (Wayland):** la apariencia ya viaja por el portal
  (ADR 0031/0050) y también la necesitan procesos sin ventana.
- **Provider con prioridad `USER`:** el orden entre providers con la misma
  prioridad depende de cuál se añadió primero. La copia vieja de `gtk.css`
  podría ganar.
