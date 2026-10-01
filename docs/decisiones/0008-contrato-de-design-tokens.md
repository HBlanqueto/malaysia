# 0008 — Los design tokens fluyen en una sola dirección, de Nix a Rust

Estado: **superado por la ADR 0011.** Este contrato se escribió antes de que
existiera ningún valor de diseño. El bloque 0 pobló después `yuki-design`
con constantes Rust escritas a mano y sin generación desde Nix, lo que invierte
la decisión siguiente; el razonamiento está en
`0011-valores-de-diseno-como-constantes-rust.md`. El texto se conserva como
constancia de lo que se consideró y se rechazó.

## Contexto

`design/v1.nix` está intencionadamente vacío y `yuki-design` es un crate
vacío. Eso está bien mientras nada tenga un color, un valor de espaciado, un
radio de esquina o una duración de animación. El trabajo de shaders es lo
primero que querrá esos valores, y el camino de menor resistencia allí es
escribir una constante junto al código que la usa. Una vez que existen unas
cuantas, el mismo valor se copia en tres sitios, se desincronizan, y ya no hay
una única respuesta a "¿cuál es el color de acento?" o "¿cuánto dura la
animación de apertura?".

Esta decisión define el contrato antes de que exista ningún token, para que el
primero se cree correctamente en lugar de migrarse más tarde. Define solo el
contrato: no se puebla ningún token ni se escribe ningún generador.

## Opciones consideradas

1. **Generar el Rust en tiempo de compilación en `OUT_DIR` mediante `build.rs`.**
   El código generado nunca queda obsoleto y Nix es una dependencia de
   compilación obligatoria. Pero los tokens dejan de ser revisables en el árbol
   de fuentes — un diff de un color es invisible — y cada compilación necesita
   que el generador tenga éxito.
2. **Generar un archivo Rust versionado a partir de `design/v1.nix` como un
   paso explícito.** Los diffs de los valores de diseño son visibles en el
   archivo generado y se revisan como cualquier otro código, y una compilación
   de cargo simple no necesita Nix más allá de las entradas normalmente
   ancladas. El coste es que el archivo generado puede quedar obsoleto; esto se
   detecta más tarde con una comprobación que regenera y compara.
3. **Sin generación: escribir los tokens a mano en `yuki-design` y mantener
   `design/v1.nix` como documentación.** Otra vez dos fuentes para cada valor,
   sin mecanismo que las mantenga iguales. Rechazada sin más.

## Decisión

Opción 2. El contrato es:

**Lo que posee `design/v1.nix`.** Los valores de diseño autoritativos de la
versión 1 de diseño: cada nombre, valor, unidad y nota semántica breve (para qué
sirve). Es el único lugar donde se crea un valor de diseño. Una versión es un
conjunto congelado: no se permite cambiar el significado de un valor existente
bajo la misma versión; ese cambio pertenece a `v2.nix`.

**Lo que se genera a partir de él.** Un generador, aún no escrito, lee
`design/v1.nix` y emite Rust. El generador y el archivo Nix son las únicas
entradas creadas a mano; nada más contribuye.

**Dónde aterriza la salida.** Exactamente un módulo generado dentro del crate
`yuki-design` (por ejemplo `crates/yuki-design/src/generated.rs`), con una
cabecera que indique que se genera a partir de `design/v1.nix` y que no debe
editarse a mano. Está versionado, de modo que un cambio de diseño es revisable
como un diff normal, y se regenera con un comando explícito. `build.rs`/`OUT_DIR`
se rechaza por la razón anterior.

**Quién puede leerlo.** `yuki-design` es el único propietario y re-exportador
de los tokens. Cualquier otro crate — el compositor, `yuki-scene`, crates
posteriores — lee los valores de diseño solo a través de la API pública de
`yuki-design`. Ningún otro crate puede leer `design/v1.nix`, leer el módulo
generado directamente, ni definir un valor de diseño.

**Lo que está prohibido.** Constantes de diseño escritas a mano en cualquier
lugar del árbol de fuentes: colores, espaciados, tamaños, radios, grosores de
borde, tipografía, opacidad o duraciones de animación, ya sea como constantes
con nombre o literales en línea, en cualquier lenguaje. Un valor de diseño nuevo
se añade a `design/v1.nix` y se regenera; nunca se introduce en el punto de uso.

**Frontera.** Un valor es un token si es una elección de diseño que un humano
razonablemente querría cambiar por sí sola. Las constantes algorítmicas — un
épsilon de matriz, un límite de bucle, un tamaño de byte — no son tokens y
permanecen junto al código que las necesita. Cuando un valor es genuinamente
ambas cosas, se documenta como una.

## Consecuencias

- El primer código de shaders o de layout que necesite un valor de diseño debe
  primero escribir el generador y poblar `design/v1.nix`. Hasta entonces no hay
  constantes de diseño, lo que es coherente con que el bloque 0 no tenga
  renderizado.
- `yuki-design` se convierte en la única superficie pública para los valores
  de presentación, de modo que un color se cambia en un único lugar creado a
  mano.
- El contrato aún no se aplica: el generador, el comando de regeneración y la
  comprobación de obsolescencia no existen. Hasta que existan, la regla se
  mantiene por convención, y esta ADR es la referencia de lo que está permitido.
- Versionar la salida generada mantiene los cambios de diseño visibles en la
  revisión, a costa de un posible archivo obsoleto, que la comprobación futura
  existe para detectar.
- El versionado de diseño es explícito: un cambio semántico es un `vN.nix`
  nuevo, no una edición de uno congelado.