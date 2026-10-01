# 0018 — La salida winit se voltea en tiempo de renderizado, nunca en `wl_output`

Estado: aceptado (seguimiento del Bloque 4)

## Contexto

El frame completo se renderizaba boca abajo dentro de la ventana winit: el contenido del cliente, la imagen del cursor y el chrome estaban reflejados verticalmente, y la flecha por defecto apuntaba hacia abajo. La entrada no era la causa. La esquina superior izquierda del host ya mapeaba a (0, 0) lógico y la inferior derecha del host al tamaño lógico de la salida, por lo que los dos espacios de coordenadas coincidían mientras la imagen no lo hacía.

La causa es el framebuffer, no las coordenadas. El framebuffer de winit es una superficie OpenGL cuyo origen está abajo a la izquierda, mientras que el espacio de renderizado de Smithay es y-hacia-abajo. El renderer GLES de Smithay compone la transformación que recibe con su propia corrección OpenGL (`gles/mod.rs:2087-2089`: `flip180 * transform.matrix() * renderer`), por lo que una transformación de render de `Normal` pone el render y = 0 en la parte inferior de la ventana, y `Flipped180` lo pone en la superior. El damage tracker invierte la transformación de su fuente de modo antes de pasarla al renderer (`damage/mod.rs:330-335`).

La transformación propia de la salida es algo distinto: `Output::change_current_state` la envía a los clientes en `wl_output.geometry` (`wayland/output/mod.rs:164-177`). Ponerla a `Flipped180` haría que cada cliente viera `flipped_180` y pre-rotara sus buffers — el mismo error en la otra dirección.

## Decisión

Mantener la transformación del `Output` en `Normal`, y dar al damage tracker una fuente de modo estática que lleva `Flipped180` (`OutputModeSource::Static`, `output.rs:496-536`). El volteo ocurre por tanto solo en la matriz de proyección. Un resize construye un tracker nuevo, porque una fuente estática fija su tamaño.

La salida anunciada permanece en `Normal`, por lo que a ningún cliente se le pide rotar nada, y `State::logical_output_size` sigue siendo la única ruta físico-a-lógico.

## Consecuencias

- El render y = 0 es la parte superior de la ventana. Los rectángulos de daño, la geometría de los elementos, la colocación del cursor y el hit-testing de zonas de borde permanecen todos en coordenadas lógicas y-hacia-abajo y no necesitan más volteos.
- El historial de daño se descarta en el resize, que es lo que un resize necesita de todos modos.
- Los tests de píxeles en `session::tests` afirman la orientación a través de la misma fuente de modo, por lo que un cambio futuro de backend no puede des-voltear el frame en silencio.
- Un backend DRM/KMS no debe copiar esto: allí la transformación de la salida es la rotación de scanout y sí pertenece al `Output`.

## Referencias

- Smithay `src/backend/renderer/damage/mod.rs:330-335` — el tracker invierte la transformación de la fuente de modo.
- Smithay `src/backend/renderer/gles/mod.rs:2087-2089` — la composición de proyección.
- Smithay `src/output.rs:496-536` — `OutputModeSource::Static`.
- Smithay `src/wayland/output/mod.rs:164-177` — la transformación enviada a los clientes.
- `crates/yuki/src/session.rs` — `winit_render_transform`, `winit_mode_source`, `render_and_read`.