# 0044 — Fundido de apariencia en la GPU

Estado: **aceptado** (2026-10-26).

## Contexto

Cuando el usuario cambia de claro a oscuro (o viceversa) desde Ajustes o desde
`appearance.domain`, las ventanas, la shell y el fondo de pantalla se vuelven a
renderizar con la nueva paleta. Sin una transición, el cambio es instantáneo y
visualmente abrupto: cada superficie cambia en el momento en que hace commit,
lo que produce un efecto de "parpadeo escalonado".

macOS resuelve esto con un fundido de toda la pantalla: un cuadro estático del
estado anterior se dibuja encima de todo con opacidad decreciente mientras el
nuevo estado se renderiza debajo. Cuando el fundido termina, la capa estática
se suelta. Todo ocurre en la GPU; no hay lectura de píxeles a la CPU.

## Decisión

1. **Un fundido de salida completa en la GPU.** Cuando el vigilante de
   `appearance.domain`/`wallpaper.state` dispara un cambio que altera el tema,
   el compositor, **antes** de aplicar el nuevo tema:

   1. Renderiza la salida actual (ventanas, layers, fondo, chrome; sin cursor)
      a una textura GLES del tamaño físico de la salida.
   2. Actualiza el tema (`chrome_theme`, `config`).
   3. Empieza un estado de fundido con dos fases:
      - **Esperando**: la instantánea se dibuja opaca encima de todo salvo el
        cursor. La fase termina cuando todas las superficies mapeadas
        (toplevels y layer surfaces) han hecho un commit posterior al cambio,
        o a los 150 ms, lo que ocurra antes.
      - **Fundiendo**: la instantánea se dibuja con alfa decreciente siguiendo
        una curva ease-in-out cúbica durante 420 ms (`duration.slow` de
        Neptunia). Al llegar al final se suelta la textura y el fundido
        desaparece.

2. **Sin lecturas a la CPU.** La instantánea es una `GlesTexture` creada con
   `Offscreen::<GlesTexture>::create_buffer`, renderizada con el mismo
   `OutputDamageTracker` que el camino normal, y luego dibujada como
   `TextureRenderElement`. No se usa `ExportMem`, `copy_framebuffer`,
   `map_texture` ni `read_pixels` en este camino.

3. **Sin composición en CPU.** El fundido es un elemento de textura más en la
   lista de elementos de la salida, dibujado por el renderer GLES habitual.
   No hay shader nuevo; el alfa del elemento basta.

4. **Cursor excluido.** El cursor no forma parte de la instantánea y sigue
   dibujándose encima del fundido, para que se pueda mover con normalidad.

5. **Modo reducido de movimiento y sesión bloqueada.** Si
   `appearance.reduced_motion` es `true` o la sesión está bloqueada
   (`state.locked`), el cambio es inmediato: no se crea instantánea ni fundido.

6. **Cambios durante un fundido.** Si llega un nuevo cambio mientras hay uno
   en curso, se suelta la instantánea vieja, se captura una nueva del cuadro
   actual (que incluye el fundido a medias) y se empieza de nuevo en
   **Esperando**.

7. **Constantes de diseño.** La duración (420 ms) y la espera máxima (150 ms)
   viven en `vow-design`, generadas desde `neptunia/tokens.json`. El compositor
   las importa; no define literales.

## Alternativas descartadas

- **Fundido por cliente.** Pedir a cada cliente o a la shell que hiciera su
  propio fundido requeriría un protocolo privado y que apps ajenas lo
  implementaran. No garantiza que todas las ventanas cambien al unísono; el
  objetivo es un fundido global de toda la pantalla.

- **Fundido en CPU.** Leer el framebuffer a la CPU, fundir píxeles y volver a
  subir la imagen introduciría latencia, ancho de banda de memoria y complejidad
  de formato/modificador. El camino GPU evita todo eso y reutiliza el renderer
  existente.

## Consecuencias

- El cambio de apariencia se percibe como un único fundido de toda la pantalla,
  sin que ninguna ventana cambie antes que otra.
- El cursor permanece encima del fundido y responde con normalidad.
- Durante el fundido se fuerza un daño de salida completo y se pide un nuevo
  cuadro en cada vblank (DRM) o `request_redraw` (winit) hasta que termina.
- Fuera del fundido, el damage tracker vuelve a comportarse con daño parcial.
- No hay cambios en clientes, `barrier-kit`, protocolos ni en el camino de
  captura de pantalla existente.

## Referencias

- `docs/roadmap.md` — enlazado desde el Bloque 13 como mejora transversal de
  chrome.
- `crates/malaysia-compositor/src/theme_fade.rs` — máquina de estados,
  curva ease-in-out y helpers de instantánea.
- `crates/vow-design/neptunia/tokens.json` — `duration.slow` y
  `duration.wait-max`.
