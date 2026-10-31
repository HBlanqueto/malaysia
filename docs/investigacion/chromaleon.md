# ChromaLeon como referencia para acentos wallpaper

Estado: **investigación, no dependencia.** ChromaLeon es una extensión de GNOME
escrita por Fabito02 bajo **GPL-3.0-or-later**. No se abrió ni copió su código;
este documento solo recoge las ideas descritas públicamente y cómo las
reimplementamos en Malaysia.

## Ideas que se tomaron

- Ajustar la luminosidad (HSL `L`) del acento hasta alcanzar un contraste WCAG
  mínimo contra los fondos reales de la interfaz (ventana, superficies).
- Subir algo la saturación a colores apagados cuando el ajuste de `L` los
  deja planos.

## Qué no se tomó

- ChromaLeon usa el primer píxel de la imagen como representante. Malaysia sigue
  usando un histograma ponderado por croma.
- No se copia su detección de grises ni ningún otro algoritmo concreto.

## Implementación propia

- `vow-appearance::wallpaper_accent` aplica el shift de Tengen, mueve solo `L`
  contra `bg-window` y `on-accent`, y aplica el boost de saturación a colores
  apagados.
- `malaysia-wallpaper::dominant` usa una suma de vecindario 3×3×3 para evitar
  que un tono repartido entre cubetas pierda contra un color minoritario
  concentrado.

## Licencia

GPL-3.0-or-later. Solo ideas, sin código copiado; no hace falta entrada en
`NOTICE`.
