# 0042 — Ajustes del sistema (`yuki-settings`)

Estado: **aceptado** (2026-10-26).

## Contexto

El escritorio necesita una aplicación de preferencias. La primera sección,
**Apariencia**, permite cambiar el esquema de color, el color de acento, el
modo de color de las ventanas y el estilo de los semáforos. Debe integrarse
con el menú del sistema, el empaquetado Nix y el tema Neptunia.

## Decisión

1. **App propia sobre CelestialKit.** El binario se llama `yuki-settings`,
   vive en `crates/yuki-settings` y se dibuja con `celestial-kit` sobre Freya
   (ADR 0034).

2. **`app_id` fijo.** El prefijo de apps nativas es
   `io.github.hblanqueto.yuki.` (constante `NATIVE_APP_ID_PREFIX` de
   `yuki-design`). La app pasa `"settings"` a `WindowOptions`, por lo que el
   `app_id` completo es `io.github.hblanqueto.yuki.settings`.

3. **Ventana con decoración cliente (CSD).** La ventana usa
   `Decorations::Client` y dibuja su propia titlebar con semáforos; el tamaño
   inicial y los límites salen de `yuki-design::SETTINGS`.

4. **Se abre desde el menú ❄.** El item "Ajustes del sistema…" del menú del
   sistema en `yuki-shell menubar` llama a `spawn_app("yuki-settings", &[])`;
   esta función preserva `WAYLAND_DISPLAY` y elimina el entorno Mesa privado
   del compositor, igual que para los componentes de la shell.

5. **Persistencia en `appearance.domain`.** La app lee y escribe
   `~/.config/yuki/appearance.domain` a través de `yuki_appearance::load_appearance`
   y `write_appearance`. Solo edita campos relacionados con apariencia; el
   resto de dominios quedan para fases posteriores.

6. **Empaquetado Nix.** El flake instala el binario `yuki-settings`, lo envuelve
   con el mismo wrapper Mesa que `yuki` y `yuki-shell`, y copia
   `share/applications/io.github.hblanqueto.yuki.settings.desktop`.

## Consecuencias

- Hoy solo existe la sección Apariencia; más dominios se añadirán cuando el
  catálogo de opciones y el backend D-Bus (`org.yuki.Compositor1`, ADR 0021)
  estén listos.
- La app no es un componente `yuki-shell`; es un cliente Wayland independiente.
- El lanzador del menú del sistema no requiere portal ni D-Bus de activación.
- El archivo `.desktop` usa `StartupWMClass` con el `app_id` para que los
  launchers externos puedan agrupar la ventana correctamente.

## Referencias

- [`0021`](0021-superficie-de-configuracion-y-control.md) — configuración y
  control por D-Bus.
- [`0025`](0025-la-shell-es-nativa.md) — la shell es nativa.
- [`0032`](0032-neptunia-es-la-fuente-de-diseno-del-escritorio.md) — Neptunia.
- [`0034`](0034-celestialkit-sobre-freya.md) — CelestialKit sobre Freya.
- [`0041`](0041-barra-lateral-en-isla.md) — barra lateral en isla.
- `crates/yuki-settings/src/main.rs` — implementación.
- `crates/yuki-shell/src/menubar/system.rs` — item del menú del sistema.
- `share/applications/io.github.hblanqueto.yuki.settings.desktop`.
