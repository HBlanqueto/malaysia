# 0011 — Los valores de diseño son constantes Rust escritas a mano en `yuki-design`

Estado: aceptado (bloque 0, supera a la ADR 0008)

## Contexto

La ADR 0008 definió un contrato en el que `design/v1.nix` era la única fuente
creada a mano y un generador emitiría un módulo Rust versionado dentro de
`yuki-design`. Ese contrato se escribió antes de que existiera ningún valor.

El bloque 0 tuvo entonces que poblar el crate. Aún no hay renderizado ni
consumidor, hay exactamente un aspecto en dos modos, y el proyecto
explícitamente no quiere todavía theming en tiempo de ejecución, un archivo de
configuración ni reglas por aplicación. Construir un generador ahora sería
maquinaria para un problema que no ha aparecido: añadiría un paso Nix-a-Rust,
una comprobación de obsolescencia y un comando de regeneración para mantener un
puñado de constantes sincronizadas con una segunda declaración de sí mismas.

Los valores en sí son los valores por defecto del sistema: métricas de la
barra de título, forma de la ventana, elecciones de tipografía, paletas y
sombras. Son elecciones de diseño, no constantes algorítmicas, así que
pertenecen a `yuki-design` en lugar de junto a sus futuros consumidores.

## Opciones consideradas

1. **Mantener la ADR 0008: `design/v1.nix` más un generador.** Un único lugar
   creado a mano con un diff generado revisable. Pero son dos representaciones
   de las mismas constantes y un paso de compilación que mantener, para valores
   con un único consumidor que aún no existe, y sin theming en tiempo de
   ejecución que justifique la capa.
2. **Escribir las constantes a mano en `yuki-design`.** Una sola fuente, en
   el lenguaje que la usa, revisable y directamente comprobable. Sin generación,
   sin `design/v1.nix`, sin dependencia de compilación. El coste es que si un
   consumidor no-Rust (por ejemplo una futura canalización de documentación o de
   assets) llega a necesitar los valores, habrá que introducir un generador más
   tarde.
3. **Constantes en el punto de uso.** Lo que la ADR 0008 se escribió para
   evitar: el mismo color copiado en varios módulos y desincronizándose.
   Rechazada.

## Decisión

Opción 2. `yuki-design` es dueño de los valores de diseño como constantes
públicas en tiempo de compilación: `Rgba`, `Palette`, `LIGHT`, `DARK`,
`TITLEBAR`, `WINDOW`, `TEXT` y `SHADOW`. No hay `design/v1.nix`, ni generador,
ni anulación en tiempo de ejecución. Cada valor se documenta con su unidad, y
`yuki-design` es el único crate que puede definir un valor de diseño.

Los dos estados de una ventana se modelan explícitamente: un color que difiere
entre estados de foco es un `FocusPair`, y la línea interior es un
`Option<Rgba>` para que un renderizador pueda saltarse el pase en modo claro en
lugar de dibujar una línea totalmente transparente.

Esto invierte el contrato de la ADR 0008. Si aparece un segundo consumidor
no-Rust de estos valores, la decisión del generador puede revisarse entonces,
con un consumidor concreto que la justifique.

## Consecuencias

- La ADR 0008 queda superada; `design/v1.nix` se elimina.
- Un cambio de diseño es un diff de Rust ordinario y el compilador lo comprueba
  como cualquier otra constante.
- No hay un camino preparado para un consumidor no-Rust. Introducir uno más
  tarde significa escribir el generador que describió la ADR 0008, ahora contra
  una necesidad real.
- `yuki-design` sigue sin dependencias y sin comportamiento en tiempo de
  ejecución; es solo datos y los constructores que hacen legibles los literales.
- Las reglas por aplicación y el theming en tiempo de ejecución siguen
  explícitamente fuera de alcance y como decisiones no tomadas, tal y como se
  registra en el documento de Cimientos.