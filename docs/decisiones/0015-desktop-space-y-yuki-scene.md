# 0015 — El layout de ventanas permanece en `yuki-scene`; no se adopta `desktop::Space` de Smithay

Estado: **aceptado.** Este registro resuelve una divergencia que el roadmap dejó sin explicar: el brief del Bloque 2 del roadmap pedía "un módulo dedicado que envuelva `desktop::Space`", la implementación mantuvo la feature `desktop` desactivada y definió sus propios tipos en `yuki-scene` en su lugar, y ningún documento explicó el porqué. Se escribe antes del Bloque 4, donde revertir la elección se volvería caro.

## Contexto

Smithay incluye un helper de gestión de ventanas, `desktop::Space`, detrás de su feature `desktop`. Yuki compila Smithay con `default-features = false` y **no** habilita `desktop` (ADR 0009, ADR 0010). Los toplevels se rastrean a mano en `crates/yuki/src/window.rs`, y la crate `yuki-scene` es dueña de los tipos de coordenadas propios del proyecto (ADR 0001).

El Bloque 2 del roadmap pidió un módulo que envuelva `Space`. La implementación no hizo eso; este registro vuelve deliberada esa elección. Tiene que dar cuenta del modelo de decoración del proyecto, que el brief precede: Yuki dibuja su propio chrome — esquinas squirculares, un borde fino oscuro, una línea interior clara, una sombra de dos capas y una barra de título vertical — por lo que una ventana **no** es un rectángulo y la geometría de una ventana, la superficie de su cliente y su región de entrada no coinciden.

## Qué proporciona `yuki-scene` hoy

Concretamente, y por tipo:

- `geometry::Logical` (`f64`), `geometry::Physical` (`i32`) y `geometry::Buffer` (`i32`) — marcadores a nivel de tipo para los tres espacios de coordenadas.
- `geometry::Point<N, Space>`, `geometry::Size<N, Space>` y `geometry::Rect<N, Space>` — valores parametrizados por esos marcadores. Los puntos y tamaños se suman y restan solo dentro de un mismo espacio; las conversiones son métodos explícitos `to_physical(scale)` / `to_buffer(scale)` / `to_logical(scale)`. `Rect` expone `location`, `size`, `x`, `y`, `width`, `height`.
- La crate **no tiene dependencias**, es `#![forbid(unsafe_code)]`, y su aritmética entre espacios se mantiene en su lugar mediante un doctest `compile_fail`.

Su responsabilidad hoy es el vocabulario de coordenadas en la única frontera donde el compositor alimenta la escena (ADR 0001). **Aún no tiene scene graph, ni layout, ni z-order ni hit-testing** — el graph es un hito posterior. En particular `Rect` no tiene `contains`/`intersection`, y ningún código rastrea dónde está una ventana excepto `Window` en el compositor.

## Qué proporcionaría `desktop::Space` que `yuki-scene` no tiene

`Space<E>` es una colección ordenada de elementos con ubicaciones por elemento. Proporciona: `map_element`/`unmap_elem`; `elements` en z-order; `raise_element`; hit-testing por punto (`element_under`); `element_location`, `element_geometry` y `element_bbox`; mapeo de salidas (`map_output`, `output_under`, `outputs_for_element`, `output_geometry`); `refresh`; y generación de render-elements (`render_elements_for_region`, `render_elements_for_output`, `space_render_elements`, `render_output`). Habilitar `desktop` también trae el elemento concreto `WindowElement`/`Window` (un elemento de árbol de superficie con `geometry`, `bbox`, `bbox_with_popups`, `set_activate`, enter/leave de salida), el helper de seguimiento de popups `PopupManager` con `PopupGrab`, y los tipos de layer-shell.

`yuki-scene` no proporciona nada de eso, y tendría que hacer crecer un subconjunto de ello.

## Qué necesita el Bloque 4, y qué ofrece cada lado

El Bloque 4 (move, resize, close, stacking, click-to-focus) necesita cuatro cosas:

1. **Hit-testing** de un punto lógico hasta la ventana superior. `Space::element_under` lo hace a partir del `bbox` de cada elemento. `Windows::surface_under` de Yuki ya lo hace a partir de `Window::contains`, de atrás hacia adelante.
2. **Orden de apilamiento** con raise-on-click. `Space` mantiene el orden y tiene `raise_element`; un `Vec` con mover-al-final hace lo mismo para una salida.
3. **Focus-on-click**, que es hit-testing más el contador central de serial existente (`input.rs`).
4. **Grabs de move y resize.** Estos son pointer grabs de Smithay, impulsados desde el rectángulo *exterior* de la ventana y sus bordes de decoración. `Space` puede informar la geometría de superficie de un elemento, pero un borde de decoración (la barra de título de 32 px, las bandas de resize) no es parte de la geometría de `WindowElement`, por lo que la matemática del grab se sentaría fuera de `Space` de todos modos.

La maquinaria extra de `Space` — solapamiento y daño por salida, activación de elementos, `render_elements_for_output` — apunta a muchas ventanas sobre varias salidas. Yuki tiene una sola salida hoy (ADR 0009), y la ruta de dibujo ya funciona a través de `OutputDamageTracker::render_output` con un slice de elementos plano.

## Qué necesita el Bloque 5, y qué ofrece cada lado

El Bloque 5 (popups) necesita dos cosas que el brief nombra:

1. **Posicionamiento contra el padre.** Esto es el rectángulo de la ventana padre más el `PositionerState` del popup (anchor, gravity, offset, size). `Space` puede informar el `element_geometry`/`bbox_with_popups` del padre; no coloca el popup.
2. **Ajustes de restricción** (flip, slide, resize) contra el área útil del monitor. Estos *sí* están disponibles bajo el conjunto de features actual: `smithay::wayland::shell::xdg::PositionerState::get_unconstrained_geometry` (`src/wayland/shell/xdg/mod.rs`, condicionado por `wayland_frontend`, que Yuki habilita) implementa el orden flip/slide/resize y toma el rectángulo objetivo que el compositor suministra. `PositionerState::get_geometry` es la variante sin ajustar; `get_anchor_point` también es público allí. Así que la matemática de restricción no se pierde por mantener `desktop` desactivado; solo el rectángulo objetivo del llamador — los límites decorados del padre y el área útil de la salida — le toca calcularlo a Yuki.

Lo que mantener `desktop` desactivado le cuesta al Bloque 5 es el bookkeeping de popups y los grabs, que viven detrás de esa feature:

- `PopupManager` (`src/desktop/wayland/popup/manager.rs`), que rastrea el árbol de popups, mapea popups en commit y envía `popup_done`.
- `PopupGrab`, `PopupGrabError`, `PopupUngrabStrategy` y los grabs por defecto `PopupKeyboardGrab`/`PopupPointerGrab` (`src/desktop/wayland/popup/grab.rs`), que implementan los comportamientos `KeyboardGrab`/`PointerGrab` y el orden de descarte por click-afuera/Esc.
- `PopupKind` (`src/desktop/wayland/popup/mod.rs`), el wrapper de popups xdg/input-method.

Los tres están condicionados por `#[cfg(feature = "desktop")]` en `pub mod desktop` (`src/lib.rs`). Con él desactivado, Yuki debe ser dueño de un árbol de popups en `yuki-scene` y escribir sus propios keyboard y pointer grabs, mientras aún usa `PopupSurface`, `XdgPopupSurfaceData` y `PositionerState` de Smithay desde `wayland_frontend`.

Así que `desktop` le compra al Bloque 5 el árbol y los grabs — trabajo real — pero no la colocación ni el ajuste de restricciones, que Smithay ya proporciona.

## Cómo los requisitos de decoración afectan a la propiedad de la geometría

Una ventana de Yuki tiene al menos cuatro figuras distintas, y no coinciden:

- el rectángulo **exterior**: la barra de título, el borde y la extensión de la sombra más el cliente;
- el rectángulo de la **superficie del cliente**, dentro de los insets;
- la **región de entrada**, que excluye los píxeles de sombra y no es la superficie del cliente cuando el puntero está sobre la barra de título;
- los **límites visuales**, donde la sombra de dos capas se extiende más allá del rectángulo exterior.

Los insets son una función del estado de la ventana y se recalculan en cada commit (ADR 0014): un `set_parent` puede llegar después del mapeo, el fullscreen puede cambiar, por lo que la relación exterior/interior nunca se congela. La geometría de elemento de `Space` es la del árbol de superficies del cliente — `WindowElement::geometry`/`bbox` provienen de la superficie, sin lugar para insets del lado del servidor. Cualquier código que pase por `Space` necesitaría por tanto una tabla lateral de insets para corregir la colocación, el hit-testing y los grabs, lo que recrea el problema de la propiedad *dentro* de `Space` en lugar de resolverlo.

La alternativa es una única autoridad: `yuki-scene` (o un tipo de ventana del que sea dueño) sostiene el rectángulo **exterior** y deriva de él el rectángulo del cliente, la región de entrada y los límites visuales usando los insets. Todo consumidor — hit-testing, grabs, colocación de popups, dibujo — lee esa única geometría.

## Coste de revertir la decisión después del Bloque 4

Si `desktop::Space` se adoptara para el Bloque 4 y la decisión se revirtiera después, la reversión toca cada lugar que introduce el Bloque 4: hit-testing y click-to-focus en `input.rs`, stacking y bookkeeping de ventanas en `window.rs`, la ruta de dibujo en `session.rs` (que estaría alimentando `WindowElement`s a `Space`), y todos los rectángulos padre del Bloque 5. La geometría de decoración tendría que re-derivarse desde cero, porque `Space` habría sido la cosa que la sostenía. El proyecto haría el trabajo de layout dos veces.

Revertir en el otro sentido (empezar con `yuki-scene` y pasar a `Space` más tarde) es más barato solo en que los tipos de coordenadas de `yuki-scene` sobreviven de cualquier manera — pero aun así significaría reescribir los mismos consumidores. Cualquiera de las dos direcciones es una reescritura real; la decisión más barata es la que se toma antes del Bloque 4 en lugar de después.

## Decisión

**No** habilitar la feature `desktop` de Smithay y **no** adoptar `desktop::Space`. Hacer crecer `yuki-scene` hasta convertirla en la autoridad del layout de ventanas: un conjunto ordenado de ventanas, cada una dueña de su geometría exterior y derivando su rectángulo de cliente, su región de entrada y sus límites visuales de los insets de decoración, exponiendo hit-testing por punto, stacking y raises. Usar los tipos de `wayland_frontend` de Smithay (`ToplevelSurface`, `PopupSurface`, `PositionerState`) directamente para el trabajo de protocolo. Para el Bloque 5, computar el rectángulo objetivo en `yuki-scene` y alimentarlo a `PositionerState::get_unconstrained_geometry`; ser dueños del árbol de popups y de los keyboard/pointer grabs que `desktop` proporcionaría de otro modo.

**Recomendación:** mantener `desktop` desactivado. La abstracción central de `Space` — un elemento cuya geometría es su superficie de cliente — es la unidad equivocada para un compositor cuyas ventanas se decoran con chrome que no coincide con la superficie del cliente; adoptarla añadiría una dependencia y una capa de elementos solo para envolverla con los insets que no puede representar, mientras que ni los grabs de move y resize del Bloque 4 ni las restricciones de popups del Bloque 5 vendrían gratis. `yuki-scene` ya es dueña de los tipos de coordenadas y es la autoridad única natural para la geometría exterior, por lo que el trabajo de layout pertenece allí, detrás de una frontera, y debería escribirse antes del Bloque 4 en lugar de después.

## Consecuencias

- El rastreo manual de `window.rs` es la semilla del layout de `yuki-scene`, no un placeholder que haya que borrar; el Bloque 4 lo extiende detrás del límite del módulo existente.
- El Bloque 4 debe añadir a `yuki-scene` lo que `Space` habría dado: un conjunto de ventanas ordenado, almacenamiento del rectángulo exterior, contención/Intersección de rectángulos, y la conversión de geometría exterior e insets a rectángulos de cliente y de entrada.
- El Bloque 5 es dueño de su árbol de popups y de sus keyboard/pointer grabs, porque `desktop::PopupManager` y `PopupKeyboardGrab`/`PopupPointerGrab` están detrás de la feature que esta decisión mantiene desactivada. Su ajuste de restricciones usa `PositionerState::get_unconstrained_geometry` de Smithay (disponible bajo `wayland_frontend`), con un rectángulo objetivo que suministra la escena.
- La feature `desktop` permanece desactivada; si un hito posterior necesita layer-shell o solapamiento multi-salida, esta decisión puede revisitarse desde la posición de un requisito concreto y escrito en lugar de desde el redactado del brief del Bloque 2.

## Referencias

- `crates/yuki-scene/src/geometry.rs` — los tipos de coordenadas y sus tests.
- `crates/yuki/src/window.rs` — el rastreo de toplevels actual y el hit-testing de `surface_under`.
- `crates/yuki/src/decoration.rs` — `Insets`, `Role`, `WindowState`; los insets se recalculan por commit (ADR 0014).
- `0001-tipos-de-coordenadas.md` — por qué `yuki-scene` es dueña de la geometría.
- `0009-backend-winit-y-gles2.md` — la feature `desktop` está deliberadamente desactivada.
- Smithay `src/desktop/space/mod.rs`, `src/desktop/space/element/mod.rs`, `src/desktop/wayland/popup/manager.rs`, `src/desktop/wayland/popup/grab.rs` — la API condicionada por `desktop` que esta decisión no adopta.
- Smithay `src/wayland/shell/xdg/mod.rs` — `PositionerState::get_geometry`, `get_unconstrained_geometry` y `get_anchor_point`, disponibles bajo `wayland_frontend`.