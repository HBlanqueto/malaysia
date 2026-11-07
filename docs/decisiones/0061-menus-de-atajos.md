# 0061 — Menús de atajos para apps sin menú global

Estado: aceptado (2026-11-07). Sin probar en sesión con apps reales.

## Contexto

Algunas apps no publican menú global en Wayland por ninguna vía del ADR 0040:

- Las de cinta dibujada por ellas mismas: WPS Office y ONLYOFFICE, también en
  X11 ([foro de ONLYOFFICE](https://community.onlyoffice.com/t/can-you-tweak-titlebar-style/2969)).
- Las que lo quitaron: JetBrains en 2024 ([IJPL-156149](https://github.com/JetBrains/intellij-community/commit/336265215c1ae9bf9fd7f9c23ebfabc8fc810743)).
- Las que solo lo publican en X11: Electron sin el parche del ADR 0040 y
  Firefox sin sus preferencias.
- Las apps X11 en general: satellite no expone el ID X11 y el Registrar no
  casa ventanas (ADR 0056).
- Los terminales sin menú (Alacritty, foot, kitty, WezTerm, Ghostty).

Para ellas la barra solo muestra el respaldo 14c: menú de la app, Ventana y
Ayuda. `orbit-global-menu` (GPL-3.0) resuelve lo mismo en GNOME con menús de
atajos escritos a mano por app.

## Decisión

1. **Catálogo propio.** Un TOML por app en
   `crates/malaysia-shell/shortcuts/`, embebido al compilar. Cada mapeo se
   escribe desde la documentación oficial o el código de la app y cita sus
   URL en `sources`. No se copia nada de orbit ni de otro proyecto GPL.
2. **Formato.**

   ```toml
   ids = ["onlyoffice-desktopeditors", "ONLYOFFICE"]
   sources = ["https://helpcenter.onlyoffice.com/..."]

   [[menus]]
   label = "edit"
   items = [["undo", "Ctrl+Z"], [], ["copy", "Ctrl+C"]]
   ```

   - `ids` se compara, sin distinguir mayúsculas, con el `app_id` y con el
     id del `.desktop` que encuentra `find_entry`.
   - `[]` es un separador.
   - La etiqueta es la clave Fluent `shortcut-<label>` de `malaysia-shell.ftl`
     (es y en). Un test exige las dos.
   - Las teclas son `Ctrl`, `Shift` y `Alt` y un keysym de xkbcommon en el
     nivel base: `Ctrl+Shift+grave`, no `Ctrl+asciitilde`.
3. **Barra.** Si la app no publica menú ni hay títulos recordados, los menús
   del catálogo van entre el menú de la app y Ventana. Un menú real siempre
   gana.
4. **Infinity v6, `send_shortcut(handle, keys)`.** Al elegir una entrada la
   barra pide al compositor que teclee el atajo en la ventana activa de la
   app. El compositor:
   1. enfoca la ventana y comprueba que tiene el foco de teclado;
   2. busca cada keysym en el nivel base de la distribución activa;
   3. envía pulsaciones y sueltas a la ventana sin pasar por los atajos del
      compositor.

   Si falta una tecla o el foco, ignora la petición y lo registra.
5. **Sin atajos de Super:** Super es de Malaysia (menús, ventanas y espacios).

## Consecuencias

- WPS, ONLYOFFICE, JetBrains, VS Code y los demás del catálogo muestran
  Archivo, Edición y Visualización con atajos reales.
- Una entrada no sabe si la acción aplica: nunca sale en gris ni con ✓.
- Un atajo del catálogo puede quedar viejo si la app lo cambia; las fuentes
  permiten revisarlo.
- Solo la shell ve Infinity (ADR 0039 de barrierkit), así que solo ella
  puede teclear en otra ventana.

## Alternativas descartadas

- **Copiar los mapeos de orbit:** son GPL-3.0 y Malaysia es MIT.
- **`zwp_virtual_keyboard_v1` desde la shell:** exige subir un keymap y
  depende de que el foco vuelva a la app antes de teclear.
- **Menús genéricos para todas las apps** (`global-menu-for-gnome`): Ctrl+C
  no copia en un terminal y Ctrl+L no alinea en una hoja de cálculo.
