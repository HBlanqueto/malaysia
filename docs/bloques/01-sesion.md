# La sesión Wayland anidada — Bloques 1, 2 y 3 del roadmap

> **Correspondencia con el roadmap.** Este documento es el registro detallado de los **Bloques
> 1 (un cliente se conecta), 2 (una ventana en pantalla) y 3 (entrada)** del roadmap,
> construidos como un único hito que los documentos antiguos llaman "Bloque 1". No es el Bloque 1 del
> roadmap, y las Partes siguientes son Partes de esta sesión, no números de bloque. El backend
> DRM/KMS que reemplaza a winit es el **hito Direct Session**, que el
> roadmap no numera.

Estado: **en curso.** Este documento se completa cuando la sesión lo esté.

## Propósito

La sesión anidada convierte a Yuki en un compositor: un cliente Wayland puede conectarse,
y su ventana aparece dentro de la ventana winit que el trabajo de Cimientos (Bloque 0 del
roadmap) preparó. Está deliberadamente anidada — el compositor corre como cliente de
la sesión host — de modo que ninguna prueba es un cierre de sesión y no se necesita ningún seat
ni privilegios elevados (ADR 0009).

La sesión se construye en partes, cada una revisada por separado:

1. El `State` del compositor, con el backend a su lado (ADR 0012).
2. Display, socket de escucha y dispatch de clientes (ADR 0013).
3. Globals: `wl_compositor`, `wl_subcompositor`, `wl_shm`, `wl_output`,
   `wl_seat` y `xdg_wm_base`.
4. El tipo de decoración, escrito antes de que se mapee la primera ventana.
5. Ciclo de vida de toplevels `xdg_shell`: configure inicial, ack, y mapeo en el
   primer commit con buffer — *no* en el ack. El enunciado de la Parte 5 del resumen de la sesión
   ("ack_configure → map") es incorrecto; véase Divergencias 1, que es lo autoritativo.
6. Renderizado de superficies de clientes y reenvío de la entrada de winit al seat.

Las partes cubren tres bloques del roadmap; este documento es el registro a nivel de sesión
y el registro del Bloque 1, y los Bloques 2 y 3 tienen los suyos propios:

| Bloque del roadmap | Partes | Registro |
| --- | --- | --- |
| 1 — Conseguir que un cliente se conecte | 1–3 | este documento |
| 2 — Una ventana en pantalla | 4–6 (renderizado) | `02-ventana-en-pantalla.md` |
| 3 — Entrada | 6 (entrada), ampliado después con el soporte de cursor | `03-entrada.md` |

Los bloques posteriores tienen sus propios documentos: `04-ventanas-manipulables.md`
para el Bloque 4. El Bloque 5 no está iniciado.

Fuera del alcance: popups, move/resize, portapapeles, XWayland, DRM/KMS.

## Divergencias con respecto al resumen de la sesión

1. **El mapeo ocurre en el primer commit con buffer, no en `ack_configure`.** El enunciado de la Parte 5
   dice "ack_configure → map". El protocolo mapea una superficie cuando tiene un
   buffer no nulo commiteado, no cuando se hace ack de un configure; mapear en el ack
   mostraría una superficie sin contenido. La ventana se marca como lista con el ack
   (y solo con el serial que coincide, véase ADR/Parte 6) y se mapea con el primer
   commit que porta un buffer. El enunciado literal del resumen de la sesión es incorrecto en
   este punto y se anula aquí, deliberadamente.

## Fallos de la revisión de la Parte 5

La revisión de la Parte 5 produjo seis fallos. Cinco cambiaron código y están registrados en
sus propios commits: anunciar `wl_data_device_manager`, un accessor para la
conversión de output lógico, hacer coincidir los serials de configure antes de marcar una ventana
como lista, mapear en el primer commit con buffer (Divergencias 1), y un probe
`set_parent` en vivo (ahora `tools/sctk-probe`, véase Verificación). El sexto no cambió
nada, por lo que se registra aquí.

**Fallo 6 — el tracker de ventanas conserva `Window::location` y `Windows::iter`.**
Ambos se cuestionaron como API sin consumidor todavía. Hoy tienen consumidores:
el camino de renderizado lee cada `Window::location` para colocar su superficie e itera
`Windows` para recolectar las superficies que dibujar. El tracker en sí es un placeholder
para `yuki-scene` en el hito Direct Session — este módulo es lo que se reescribirá entonces — de modo
que los dos métodos son la superficie más pequeña que sirve al camino de dibujo y nada
se construye sobre ellos.

## Verificación

Las comprobaciones de la sesión son comandos cuyo código de salida es el resultado, no un log leído
a ojo:

- **`./dev.sh check`** — rustfmt, `clippy -D warnings` en todo el workspace, y
  cada prueba. Incluye las pruebas del parser de protocolo del harness de probes, que cubren
  cada afirmación de mapeo siguiente, incluido que una finalización de `wl_display.sync` *no*
  se acepta como callback de frame.
- **`cargo run -p sctk-probe --bin parent_probe`** — maneja dos toplevels contra
  un compositor en ejecución y comprueba que establecer y quitar un padre
  reconfigura al hijo (`1248×800 → 1280×800 → 1248×800` en un output de 1280×800).
  Sale con código no cero si el ciclo no se reproduce.
- **`cargo run -p sctk-probe --bin smoke`** — lanza el compositor y `foot`,
  captura el rastro `WAYLAND_DEBUG` del cliente, y comprueba, en orden:
  `xdg_toplevel.configure` antes del primer attach no nulo; un serial de
  `ack_configure` coincidente antes de ese attach; un commit con buffer no nulo que el
  compositor reporta como mapeado; `wl_keyboard.enter` en esa superficie después del
  commit; y la finalización del id de callback resuelto desde `wl_surface.frame`. Sale
  con código no cero en el primer paso faltante o fuera de orden.
- **`RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`** — la
  compilación de documentación está libre de warnings.

Ambos binarios viven en `tools/sctk-probe`, un miembro del workspace solo de desarrollo excluido
de la compilación por defecto (véase su README para los comandos exactos). Necesitan el
dev shell y un display host.

No verificado por un comando, y dejado a la comprobación manual: que la escritura llega al
cliente con el layout correcto, que el puntero responde, y que el output del cliente
y el cursor dibujado son legibles. El harness de smoke no puede mover el puntero
host, de modo que no puede hacer que un cliente establezca un cursor en vivo; comprueba el
puntero por defecto del compositor y el emparejamiento de seriales `set_cursor`/enter del cliente, no los
píxeles finales.

## Preguntas abiertas

1. **La política de `deny(unsafe_code)` debe redefinirse antes del hito Direct
   Session.** Actualmente es absoluta ("prohibido"), lo que ya dobló la
   sesión anidada en dos lugares: el código fuente del dispatch de clientes no puede usar el
   `Generic::get_mut` de calloop y tuvo que consultar un descriptor duplicado en su lugar
   (ADR 0013), y el compositor no puede llamar a `std::env::set_var` para exportar
   `WAYLAND_DISPLAY`, por lo que pasa la variable a los hijos generados con
   `Command::env` en su lugar. El hito Direct Session trae dmabuf, GBM y
   EGL, donde unsafe no es evitable — las APIs de fd crudo y mapeo no tienen envoltorio
   seguro. La política debería volverse "cercada": unsafe se permite solo en módulos
   marcados explícitamente para ello, nunca en lógica de protocolo, y cada uso lleva un
   comentario `// SAFETY:` que declara el invariante que sostiene. Eso es lo que hacen niri y
   Smithay. Decide esto en el límite entre la sesión anidada y Direct Session, no
   a mitad de hito. *(Planteado en la Parte 2.)*