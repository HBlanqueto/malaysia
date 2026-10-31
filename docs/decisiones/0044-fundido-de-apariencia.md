# 0044 — Fundido de apariencia en la GPU

Estado: aceptado (2026-10-26).

## Contexto

Al cambiar claro/oscuro, cada superficie cambia en su commit y produce un parpadeo escalonado.

## Decisión

1. Fundido de salida completa en GPU. Antes de aplicar el tema, el compositor renderiza la salida actual (sin cursor) a una textura GLES; actualiza el tema y arranca dos fases:
   - **Esperando:** la instantánea se dibuja opaca encima salvo el cursor hasta que las superficies mapeadas commitean, o 150 ms.
   - **Fundiendo:** alfa decreciente con ease-in-out cúbica durante 420 ms (`duration.slow`); al terminar se suelta la textura.
2. Sin lecturas a CPU: `Offscreen::<GlesTexture>::create_buffer` y el mismo `OutputDamageTracker`; se dibuja como `TextureRenderElement`.
3. Sin composición en CPU ni shader nuevo: el alfa del elemento basta.
4. El cursor no entra en la instantánea y sigue dibujándose encima.
5. Con `appearance.reduced_motion` o sesión bloqueada, el cambio es inmediato.
6. Si llega otro cambio durante el fundido, se captura el cuadro actual y se reinicia en **Esperando**.
7. Duración y espera máxima viven en `vow-design`; sin literales.

## Consecuencias

El cambio se percibe como un fundido global.

## Alternativas descartadas

- Fundido por cliente: requiere protocolo privado y no garantiza sincronía.
