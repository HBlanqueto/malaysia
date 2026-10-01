# 0023 — Los componentes del shell corren como procesos separados

Estado: **aceptado** (la decisión de fondo). La supervisión queda como propuesta y
la ubicación del overview como abierta, señaladas en sus secciones.

## Contexto

El shell de Yuki (dock, barra superior, notificaciones, launcher, pantalla de
bloqueo) corre como procesos cliente separados, uno por componente. El objetivo es
el aislamiento de fallas: la caída de un componente nunca debe tumbar a otro. Si el
dock muere, la barra superior sigue funcionando.

Este ADR resuelve la pregunta abierta "shell in-process contra clientes layer-shell"
a favor de los clientes layer-shell, un proceso por componente.

## Decisión (Aceptada)

1. **Cada componente del shell es su propio proceso y su propio cliente Wayland.**
2. **Ningún componente depende de otro.** No hay un "daemon del shell" central;
   eso reintroduciría el punto único de falla que este modelo elimina.
3. **Cada componente tolera la ausencia de los demás como estado normal.** Por
   ejemplo, la barra superior oculta el indicador de notificaciones mientras el
   servidor de notificaciones está caído.
4. **El código compartido vive en un crate de biblioteca de toolkit** (nombre
   pendiente, `<toolkit>`) que lee los tokens del crate de diseño (`yuki-design`).
   Las apps propias de Yuki (por ejemplo, Settings) usarán el mismo toolkit.
5. **Empaquetado propuesto: un solo binario con subcomandos** (`yuki-shell dock`,
   `yuki-shell topbar`, ...): un artefacto de compilación, muchos procesos.
6. **`panic = "abort"` en release**: una falla es una salida limpia y un reinicio,
   nunca un intento de seguir corriendo después de `catch_unwind`.

## Opciones consideradas

| Opción | Ventajas | Desventajas |
|---|---|---|
| **A. Shell dentro del proceso del compositor** | Un solo proceso; sin protocolos de shell que implementar; acceso directo al scene graph (texturas, efectos). | Una caída del shell tumba la sesión; el compositor crece con UI que no es suya; no hay frontera de fallo ni de memoria entre shell y compositor. |
| **B. Un solo proceso de shell con todos los componentes** | Aísla shell ↔ compositor; un solo cliente que hablar; menos procesos que C. | No aísla componentes entre sí: un bug en el dock puede tumbar la barra, las notificaciones o el launcher. El "daemon del shell" se vuelve otro punto único de falla. |
| **C. Un proceso por componente (elegida)** | Frontera de fallo por componente: un proceso muerto se reinicia sin tocar a los demás; cada uno es mínimo y especializado. | Más procesos en la sesión y más protocolos que implementar (layer-shell, foreign-toplevel, session-lock, ...); cada componente necesita su propio arranque. |

Solo la opción C da aislamiento de fallas entre componentes: A y B colocan dos o
más componentes bajo una misma frontera de proceso, de modo que un fallo en
cualquiera de ellos mata al resto y, en A, a la propia sesión.

## Ubicación del código (Aceptada)

**Mismo repositorio, mismo workspace de Cargo, crates separados.** La razón: Yuki es
un escritorio fijo que se versiona como un todo; los tokens de diseño, los shaders y
cualquier protocolo privado futuro deben cambiar al mismo tiempo que el compositor.

Separarlo en otro repositorio tendría sentido si se cumple alguna de estas
condiciones:

- abrir el toolkit pero no el compositor (o al revés);
- que existan usuarios externos del toolkit;
- o un ritmo de versiones independiente entre shell y compositor.

## Supervisión (Propuesta, dos opciones)

1. **Unidades de usuario de systemd incluidas en el flake propio de Yuki**:
   `Restart=on-failure`, `RestartSec` corto, límite de reintentos, logs por
   componente en el journal, y un `yuki-session.target` que arranca después de que
   el compositor importe `WAYLAND_DISPLAY` al gestor de usuario.
2. **El compositor lanza y supervisa los procesos él mismo**: más simple bajo winit,
   pero agrega código al proceso que más necesita ser pequeño.

Recomendación: systemd para producción; en desarrollo, `scripts/test.sh` lanza los
componentes a mano.

## Ubicación del overview / exposé (ABIERTA, no se decide aquí)

- **Opción 1 — dentro del compositor.** Necesita las texturas de las ventanas
  directamente; no requiere protocolo. Desventaja: un bug en el overview puede
  tumbar la sesión.
- **Opción 2 — en un proceso aparte**, como hace macOS (Mission Control y Spaces
  viven en Dock.app y hablan con WindowServer mediante la API privada SkyLight,
  compartiendo imágenes de ventanas con IOSurface). En Yuki eso requeriría un
  protocolo privado más intercambio de texturas por dmabuf.

El punto de partida es la opción 1, y solo se revisa si los bugs del overview se
vuelven un problema de estabilidad.

## Casos de falla que se documentan explícitamente

- **El compositor sigue siendo el único punto de falla inevitable.** Si muere,
  mueren todos los clientes. Mantener el shell fuera lo mantiene más chico.
- **Pantalla de bloqueo.** `ext-session-lock-v1` exige que la sesión **siga
  bloqueada** si el cliente de bloqueo muere — el protocolo lo dice literalmente:
  *"If the client dies while the session is locked, the compositor must not unlock
  the session in response"*. El compositor muestra un color sólido y nunca
  desbloquea. Yuki debe implementar esta garantía correctamente; es lo que hace
  seguro al modelo de procesos.
- **Muerte del dock y su zona exclusiva.** El compositor recalcula el área útil y
  las ventanas maximizadas se reacomodan; se vuelven a reacomodar al reiniciar el
  dock. Postura inicial: aceptar el parpadeo. Retener la zona durante un periodo de
  gracia es una posible mejora futura, no un requisito.
- **Reinicio de GPU (amdgpu).** Afecta a todos los procesos con contexto GL;
  separar procesos no protege contra eso.

## Protocolos que Yuki debe implementar para este modelo

Trabajo futuro (registrado también en `docs/arquitectura/modelo-de-procesos.md`),
no parte de los bloques actuales:

- `zwlr_layer_shell_v1` — dock, barra superior, notificaciones, launcher.
- `ext-session-lock-v1` — pantalla de bloqueo.
- `ext-foreign-toplevel-list-v1` — lista de ventanas para el dock y la barra.
- `zwlr_foreign_toplevel_management_v1` — activar, cerrar y minimizar desde el
  dock; su `set_rectangle` le dice al compositor dónde está el ícono del dock, que
  es lo que necesita la animación de minimizar.
- `xdg-activation-v1` — el launcher y el dock le pasan el foco a las apps que
  lanzan.
- `ext-idle-notify-v1` — bloqueo automático.

El estado del sistema (batería, red, audio, brillo) va por D-Bus directo desde cada
componente (UPower, NetworkManager, PipeWire), no por Wayland. Un protocolo privado
de Yuki solo se introduce cuando aparezca una necesidad concreta que los protocolos
estándar no cubran.

## Modelo de referencia: macOS (según versiones recientes de macOS)

| Proceso | Papel | Si se cae |
|---|---|---|
| WindowServer (SkyLight) | compositor | termina la sesión |
| Dock.app | el dock, más Mission Control, Spaces y Launchpad | se relanza |
| Finder | íconos del escritorio | se relanza |
| Barra de menú | los menús los dibuja la app activa; los íconos de estado pertenecen a sus apps, más ControlCenter.app y SystemUIServer | — |
| NotificationCenter.app (+ usernoted) | notificaciones | se relanza |
| Spotlight.app (+ daemons de indexación) | búsqueda | se relanza |
| loginwindow | sesión y bloqueo | termina la sesión |
| launchd | relanza los agentes mediante KeepAlive | — |

`launchd` relanza los agentes mediante `KeepAlive`; es el papel que aquí cumplen las
unidades de usuario de systemd. Todo sale junto en una misma versión de macOS.

### Referencias en Linux

- **Otto** (`github.com/nongio/otto`, MIT): compositor Rust sobre Smithay. La
  topbar es un componente aparte (`otto-bar`, un binario separado); el dock y el
  Exposé viven dentro del compositor — su README lo confirma al listar como
  pendiente *"moving Dock code out of the compositor core"*. *Verificado contra el
  README del repositorio; no revisado línea a línea en el código.*
- **Noctalia v5** (`github.com/noctalia-dev/noctalia`, MIT): shell nativa en
  C++/OpenGL ES, sin Qt ni GTK, que corre sobre compositores externos como cliente
  usando superficies de layer-shell. *Verificado contra el README del repositorio y
  el anuncio de v5; el detalle de cada componente como cliente layer-shell no se ha
  revisado en el código.*

## Consecuencias

- La shell se construye como un conjunto de procesos cliente, no como UI del
  compositor ni como un daemon central.
- El compositor implementa los protocolos de la lista anterior (layer-shell,
  session-lock, foreign-toplevel, activation, idle-notify) como parte de la
  integración del modelo.
- La muerte de cualquier componente es recuperable por supervisión sin tocar a los
  demás; la única caída irrecuperable es la del compositor.
- `panic = "abort"` alinea el modelo de fallos: un componente muere limpio y se
  reinicia, en lugar de intentar continuar en estado corrupto.
- Complementa al ADR 0025 (la shell es nativa): cada componente de
  `yuki-shell` es uno de estos procesos cliente.