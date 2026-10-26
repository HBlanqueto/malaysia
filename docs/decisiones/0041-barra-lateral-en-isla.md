# 0041 — Barra lateral en isla

Estado: **aceptado** (2026-10-26).

## Contexto

`celestial-kit` provee una barra lateral reusable (`Sidebar`, `SidebarItem`,
`SidebarLayout`) usada por `yuki-settings` y pensada para futuras apps propias.
El tema Neptunia define dos formas de dibujarla:

- **Isla**: tarjeta flotante con esquinas redondeadas, borde sutil, línea
  interior, sombra y desenfoque de fondo.
- **Plana**: lista empotrada que se confunde con el fondo de la ventana.

La barra debe poder cambiar de modo por configuración sin tocar el código de
las apps.

## Decisión

1. **El estilo de la barra es un token de diseño.**
   `components.window.sidebar.default-style` en
   `crates/yuki-design/neptunia/components.json` vale `"island"`. El modo plano
   es `"flat"`.

2. **Los valores concretos viven en `yuki-design`, nunca en las apps.**
   Neptunia fija inset, radio, colores (`sidebar-island-bg`,
   `sidebar-island-rim`, `sidebar-island-inner`, ...), sombra (`shadow.island`)
   y desenfoque (`30`) en `components.json` y `tokens.json`. `build.rs` los
   exporta como constantes Rust.

3. **`yuki-appearance` resuelve el estilo en tiempo de ejecución.**
   `AppearanceConfig::sidebar_style` (`SidebarStyle::Island | Flat`) se lee de
   `appearance.domain`; por defecto es `Island`. `resolve()` devuelve un
   `SidebarSurface` con los colores y la sombra ya mezclados para el esquema
   actual.

4. **`celestial-kit` dibuja la barra a partir de `SidebarSurface`.** La app
   solo monta `Sidebar::new()` con sus hijos; el kit aplica el fondo, el radio,
   la sombra y los semáforos según el estilo resuelto.

5. **En modo claro la isla no usa borde ni sombra.** En `tokens.json` el color
   del borde y la sombra son transparentes en claro; en oscuro/tinte oscuro se
   activan para separar la isla del fondo de ventana.

## Consecuencias

- Las apps propias obtienen el estilo Neptunia automáticamente.
- Cambiar el estilo por defecto solo requiere editar `components.json`;
  recompilar regenera las constantes.
- El modo plano existe pero no se expone en la UI de Ajustes todavía.
- La barra de `yuki-settings` muestra los semáforos integrados gracias a
  `.traffic_lights(true)`; su posición también sale de los tokens.

## Referencias

- [`0032`](0032-neptunia-es-la-fuente-de-diseno-del-escritorio.md) — Neptunia
  como fuente de diseño.
- [`0034`](0034-celestialkit-sobre-freya.md) — CelestialKit sobre Freya.
- `crates/yuki-design/neptunia/components.json` — definición de `sidebar`.
- `crates/yuki-design/neptunia/tokens.json` — colores de la isla.
- `crates/yuki-appearance/src/lib.rs` — `SidebarStyle` y `SidebarSurface`.
- `crates/yuki-settings/src/main.rs` — uso de `SidebarLayout`.
- `crates/celestial-kit/src/sidebar.rs` — componente del kit.
