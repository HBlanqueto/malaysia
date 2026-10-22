# 0032 — Neptunia es la fuente de diseño del escritorio

Estado: **aceptado** (2026-10-22).

## Contexto

El diseño del escritorio vive en `/home/humbe/dotfiles/kits/neptunia`
(`tokens.json`, `components.json`, `demo.slint` y las salidas de
`generated/`). Neptunia es un sistema de diseño **como datos**, no un toolkit
de runtime: define colores (light, dark, dark-tint), radios, espacios,
tipografía, sombras y las medidas de los componentes. Hasta ahora
`yuki-design` tenía valores tomados de somewm y una titlebar vertical de 32 px,
que no coincidían con ese sistema.

El usuario ha decidido que **Neptunia es el diseño por defecto del escritorio**
mientras Yuki se construye.

## Decisión

1. **Neptunia manda.** El look por defecto es su tema `light`; `dark` y
   `dark-tint` son las otras variantes. Los colores, radios, espacios,
   tipografía, sombras y medidas de ventana salen de sus JSON.
2. **La ventana de Neptunia**: titlebar **arriba**, semáforos de 12 px con 8 px
   de separación y **radio 4** (cuadrados redondeados), radio de ventana
   `radius.lg = 16` y el estilo de semáforos `macos` (colores `traffic-*`). El
   compositor usa **32 px** para las ventanas SSD de terceros; Neptunia define
   44 px para las ventanas propias de apps. Las apps CSD (GTK, wezterm) no
   reciben la barra de Yuki.
3. **Se implementa en Yuki, no en un framework ajeno.** El chrome es
   compositor-side (ADR 0020); el texto de la titlebar usa `cosmic-text` en el
   compositor. `celestial-kit` sigue siendo el toolkit de la shell y de las
   apps, y el compositor no lo usa (ADR 0024): comparten shaders, no contexto.
4. **Snapshot y sincronización.** Por ahora los valores aprobados se copian en
   `yuki-design` y se mantienen sincronizados a mano con los JSON (no hay
   generador todavía). Un cambio de Neptunia se revisa y se traslada; el
   objetivo es generar las constantes desde los JSON para que nadie escriba un
   color a mano.
5. **Otto es referencia, no dependencia.** Se leen sus ideas (ventana, shell,
   launcher, ajustes, portal); **no se adopta Skia** por peso y porque Yuki ya
   tiene GLES + `grade-graphics` + `celestial-kit`. Si algún día se copia
   código MIT de Otto, se añade su aviso a `NOTICE` en el mismo commit.

## Consecuencias

- La titlebar, la menubar y el dock comparten la paleta clara de Neptunia.
- La titlebar del compositor se dibuja con 32 px para ventanas SSD, semáforos de 12 y radio 16; sus botones
  cierran, minimizan y maximizan.
- Quedan pendientes el redondeo real de ventanas, la sombra, los glifos de los
  semáforos al pasar el puntero y el cambio de tema en vivo.
- La launcher y las apps propias se diseñan primero en Neptunia
  (`demo.slint`) y luego se implementan con `celestial-kit`.

## Referencias

- `kits/neptunia/README.md`, `tokens.json`, `components.json`.
- [`0011`](0011-los-valores-de-diseno-son-constantes-rust.md) — los valores
  viven en `yuki-design`.
- [`0020`](0020-politica-de-diseno-y-unificacion.md) — chrome compositor-side.
- [`0024`](0024-stack-de-render-del-toolkit.md) — GLES/cosmic-text, sin Skia.
- [`../arquitectura/ventanas.md`](../arquitectura/ventanas.md) — la titlebar.
