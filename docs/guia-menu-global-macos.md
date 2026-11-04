# El menú global de macOS

Transcripción íntegra de la guía interactiva «Menú global — Guía de macOS»
(Rust + egui) conservada en `Craft Anything/revisar`. El texto se reproduce tal
cual aparece en la aplicación, organizado por capítulos.

## Navegación

- Menú global — Guía de macOS
- EMPIEZA: Introducción
- ENTIENDE: Anatomía, Cómo funciona, Comportamiento
- CONSTRUYE: Para desarrolladores
- CONSULTA: Referencia rápida
- IDEA CLAVE: Una barra. Todas las apps.
- 6 capítulos · macOS 13+

Cada página lleva la etiqueta «GUÍA INTERACTIVA».

---

## 1. Introducción

**Guía visual · macOS**

# El menú global de macOS

Una sola barra en la parte superior de la pantalla muestra los menús de la app
que tienes delante. Esta guía explica de quién es, cómo cambia y qué puede hacer.

> **La barra no pertenece a la app**
>
> Pertenece al sistema. macOS la dibuja en el borde superior de cada pantalla y
> le pide a la app activa el menú que debe mostrar. Al cambiar de app cambia el
> contenido; la barra se queda donde está.

### Tres ideas para entenderlo

Si recuerdas estas tres, el resto encaja solo.

- **01 · Una barra, todas las apps.** No hay una barra por ventana. Hay una por
  pantalla, compartida por todas las apps.
- **02 · Manda el foco.** La app activa decide qué títulos se ven. Cambiar de
  ventana cambia el menú.
- **03 · Fuera de la ventana.** Los menús no se dibujan dentro de la ventana;
  viven en el borde superior de la pantalla.

### Recorrido en 30 segundos

Lo que ocurre entre que haces clic en una app y eliges una acción.

- **01 · Activas una app.** Haces clic en su ventana o la eliges con Cmd+Tab.
- **02 · macOS pide el menú.** El sistema consulta el menú principal que la app
  declaró.
- **03 · La barra se actualiza.** Los títulos anteriores desaparecen y aparecen
  los nuevos.
- **04 · La app responde.** Al elegir una acción, macOS se la envía a la app
  activa.

### Lo que no es

- No es una barra de tareas: no muestra ventanas abiertas ni apps para lanzar.
- No es un menú dentro de la ventana: no se mueve con ella ni se puede anclar.
- No hace globales los atajos: los atajos del menú funcionan con la app que está
  activa.

Ilustración: barra con «Cuaderno», menús Archivo, Edición, Formato, Ver, Ventana
y Ayuda, reloj «mié 9:41», ventanas «Cuaderno — Ideas sueltas». Leyendas: «La
barra pertenece al sistema. La app solo aporta sus menús.» y «La ventana cambia;
la barra se queda.»

---

## 2. Anatomía

**Anatomía · la barra**

# Anatomía de la barra

Cinco zonas, dos dueños y una regla: la barra se ve igual, pero su contenido
cambia con la app activa.

Ilustración rotulada «BORDE SUPERIOR DE LA PANTALLA»: barra con «Cuaderno»,
menús Archivo, Edición, Formato, Visualización, Ventana y Ayuda, reloj
«mié 9:41». Leyenda: «La app cambia. La barra se queda.»

Zonas: 1 Sistema, 2 App, 3 Menús, 4 Extras, 5 Reloj.

### Zona 1 · Icono del sistema

El extremo izquierdo abre acciones de todo el Mac: ajustes, bloqueo, reinicio y
apagado. No depende de la app que esté activa.

Siempre está ahí.

### Zona 2 · Menú de la app

Lleva el nombre de la app en la zona izquierda. Reúne lo esencial: Acerca de,
Ajustes, Ocultar y Salir. macOS lo coloca en segundo lugar.

Lo pinta el sistema; lo define la app.

### Zona 3 · Menús de la app

Archivo, Edición, Formato, Visualización… La app decide cuáles aparecen y en qué
orden. Al cambiar de app, este tramo se sustituye entero.

Es la parte que más cambia.

### Zona 4 · Elementos de estado

Los iconos de la derecha. Cada uno pertenece a una app o a un extra que vive en
la barra: Wi-Fi, batería, nubes, reproductores…

No son menús de la app activa.

### Zona 5 · Control y reloj

Centro de control, batería, Wi-Fi y la fecha. Son zonas del sistema. El reloj
abre una vista de notificaciones y calendario.

Siempre a la derecha.

### Los menús que casi siempre están

El orden puede variar, pero estos nombres se repiten porque macOS comparte
convenciones entre apps.

- **Archivo:** Crear, abrir, guardar, exportar y cerrar.
- **Edición:** Deshacer, portapapeles y selección.
- **Formato:** Estilos y atributos del contenido. No todas las apps lo tienen.
- **Visualización:** Qué se ve: barras laterales, zoom o presentación.
- **Ventana:** Minimizar, zoom y traer ventanas al frente. macOS lo trata como
  especial.
- **Ayuda:** Búsqueda dentro de la ayuda y atajos. Suele incluir un campo de
  búsqueda.

### Reglas de la casa

- El primer menú después del icono del sistema lleva el nombre de la app.
- Ventana y Ayuda tienen comportamiento estándar que macOS espera encontrar.
- Servicios aparece como submenú dentro del menú de la app.
- Los atajos se muestran alineados a la derecha, separados del título.
- Un elemento deshabilitado sigue visible, en gris: la acción existe, pero no
  aplica.

---

## 3. Cómo funciona por dentro

**Cómo funciona · por dentro**

# Cómo funciona por dentro

El sistema pone la barra; la app activa pone el contenido. Esta es la
conversación entre ambos.

### El reparto

- **SISTEMA · Dueño de la barra.** La dibuja, la coloca, decide cuándo se oculta
  y la replica en cada pantalla.
- **APP ACTIVA · Dueña del menú.** Declara los títulos, las acciones, los atajos
  y qué elementos están disponibles.
- **FOCO · Dueño del cambio.** Al activar otra app, su menú sustituye al
  anterior. No hay dos barras a la vez.

### La conversación, paso a paso

Cinco momentos que se repiten cada vez que cambias de app o eliges una acción.

- **01 · La app arranca.** Al iniciar, registra un menú principal: una lista
  ordenada de menús y elementos.
- **02 · El sistema detecta el foco.** Al hacer clic en una ventana, macOS marca
  esa app como la activa.
- **03 · Se intercambia el menú.** La barra toma los títulos del menú principal
  de la nueva app activa.
- **04 · Eliges una acción.** El clic viaja al elemento, que ejecuta un método o
  una acción del sistema.
- **05 · La app actualiza el menú.** Según el contexto, puede habilitar,
  deshabilitar o reemplazar elementos.
- **Siempre · La barra no guarda estado.** No recuerda el menú anterior: lo
  vuelve a pedir a la app que está al frente.

### Quién decide qué

- **Qué títulos se ven:** La app activa, a través de su menú principal.
- **Dónde está la barra:** El sistema: siempre en el borde superior.
- **Qué elemento está activo:** La app, mediante la validación de elementos.
- **Cuándo se oculta:** El sistema: pantalla completa u ocultación automática.
- **Qué atajo se muestra:** La app lo declara; macOS lo muestra y lo ejecuta.

> **El foco lo cambia todo**
>
> La barra no conserva el menú de la app anterior. En cuanto cambia la app
> activa, el menú se sustituye. Por eso dos apps pueden tener menús
> completamente distintos en el mismo sitio.

### Acciones, no solo títulos

- **ATAJOS · Teclas equivalentes.** Cada elemento puede declarar una tecla.
  macOS la muestra a la derecha y la ejecuta cuando la app está activa.
- **ESTADO · Validación.** Justo antes de abrir el menú, la app puede habilitar
  o deshabilitar elementos según el contexto.
- **DESTINO · Cadena de respondedores.** Si una acción no tiene destino
  explícito, macOS la ofrece al primer elemento que pueda responder.

Dos menús son especiales: el que lleva el nombre de la app y el menú Ventana.
macOS espera encontrarlos y les da comportamiento estándar, como ocultar la app
o traer ventanas al frente.

---

## 4. Comportamiento

**Comportamiento · simulador**

# Cómo se comporta

Prueba el simulador: cambia de app, abre menús y oculta la barra. Todo lo que ves
aquí reproduce el comportamiento del menú global.

### Simulador

- Etiqueta: SIMULADOR
- Ayuda: «Haz clic en una ventana para cambiar el foco; abre un menú para ver sus
  acciones.»
- Interruptores: «Simular pantalla completa», «Ocultar automáticamente».
- Botón: «Reiniciar».
- Barra oculta: «Acerca el cursor al borde superior».
- Pista con pantalla completa: «La barra está oculta. Llévala al borde superior
  para revelarla, igual que en macOS.»
- Pista con ocultación automática: «La barra se oculta sola. El borde superior
  sigue siendo la zona para mostrarla.»
- Pista normal: «Al cambiar de ventana, fíjate en cómo los títulos de la barra
  se sustituyen.»
- Al elegir una acción: ««{acción}» en {app}».

### Estados que conviene conocer

El menú global parece estático, pero cambia con el foco, el espacio y la
pantalla.

- **Cambio de app.** Los títulos se sustituyen al instante. No hay una transición
  ni una barra por app: la misma barra cambia de contenido.
- **Pantalla completa.** La barra se oculta para dejar todo el espacio a la app.
  Aparece al llevar el cursor al borde superior.
- **Ocultación automática.** Se activa en Ajustes del Sistema > Escritorio y
  Dock. La barra se esconde y se revela al acercarse al borde superior.
- **Varias pantallas.** Cada pantalla puede tener su propia barra. Con «Espacios
  separados», cada una muestra los menús de la app activa en esa pantalla.
- **Elementos deshabilitados.** Siguen visibles en gris: la acción existe, pero
  no está disponible en ese contexto. No es lo mismo que ocultarla.
- **Submenús y marcas.** Una flecha a la derecha indica un submenú. Una marca de
  verificación indica una opción activa.

> **Una pista para mirar**
>
> Abre dos apps y alterna entre ellas con Cmd+Tab. La barra no se mueve: solo
> cambian los títulos que hay entre el nombre de la app y los iconos de la
> derecha.

### Contenido de los menús del simulador

Aplicaciones: **Cuaderno** (menús Archivo, Edición, Formato, Visualización,
Ventana, Ayuda) y **Galería** (menús Archivo, Edición, Imagen, Visualización,
Ventana, Ayuda).

Icono del sistema:

- Acerca de este Mac
- Ajustes del Sistema…
- ---
- Bloquear pantalla
- Cerrar sesión…
- ---
- Reiniciar…
- Apagar…

Menú de la app (Cuaderno):

- Acerca de Cuaderno
- ---
- Ajustes… (Cmd ,)
- ---
- Ocultar Cuaderno (Cmd H)
- Ocultar otras (Cmd Opt H)
- ---
- Salir de Cuaderno (Cmd Q)

Archivo (Cuaderno):

- Nueva nota (Cmd N)
- Abrir… (Cmd O)
- ---
- Guardar (Cmd S)
- Exportar › (submenú)
- ---
- Cerrar ventana (Cmd W) — deshabilitado

Archivo (Galería):

- Importar… (Cmd I)
- Exportar…
- ---
- Guardar una copia (Cmd S)
- ---
- Cerrar (Cmd W)

Edición (ambas apps):

- Deshacer (Cmd Z)
- Rehacer (Cmd Shift Z) — deshabilitado
- ---
- Cortar (Cmd X)
- Copiar (Cmd C)
- Pegar (Cmd V)
- ---
- Seleccionar todo (Cmd A)

Formato (Cuaderno):

- Título
- Lista
- ---
- Negrita (Cmd B)
- Cursiva (Cmd I)

Imagen (Galería):

- Girar a la izquierda
- Girar a la derecha
- ---
- Recortar
- Ajustes de color › (submenú)

Visualización (Cuaderno):

- Mostrar barra lateral
- ---
- Zoom › (submenú)
- Pantalla completa (Ctrl Cmd F)

Visualización (Galería):

- Miniaturas
- Cuadrícula
- ---
- Pantalla completa (Ctrl Cmd F)

Ventana (ambas apps):

- Minimizar (Cmd M)
- Zoom
- ---
- Traer todo al frente

Ayuda (ambas apps):

- Buscar en la ayuda
- ---
- Atajos de teclado

---

## 5. Para desarrolladores

**Para desarrolladores**

# Dónde se define el menú

La app no dibuja la barra: declara un menú. macOS decide dónde mostrarlo y qué
hacer con las acciones.

Pestañas: AppKit, SwiftUI, Extra en la barra.

### Swift · AppKit

```swift
import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ note: Notification) {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        appItem.submenu = NSMenu(title: "Cuaderno")
        mainMenu.addItem(appItem)

        let fileItem = NSMenuItem()
        fileItem.submenu = NSMenu(title: "Archivo")
        fileItem.submenu?.addItem(
            withTitle: "Nueva nota",
            action: #selector(newNote),
            keyEquivalent: "n"
        )
        mainMenu.addItem(fileItem)

        NSApp.mainMenu = mainMenu
    }

    @objc func newNote() {
        // abrir una nota nueva
    }
}
```

### Swift · SwiftUI

```swift
import SwiftUI

@main
struct CuadernoApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
        .commands {
            CommandMenu("Notas") {
                Button("Nueva nota") { crearNota() }
                    .keyboardShortcut("n")
                Divider()
                Button("Exportar…") { exportar() }
            }
        }
    }
}
```

### Swift · MenuBarExtra

```swift
import SwiftUI
import AppKit

@main
struct CuadernoApp: App {
    var body: some Scene {
        MenuBarExtra("Cuaderno", systemImage: "square.and.pencil") {
            Button("Nueva nota") { crearNota() }
            Button("Ajustes…") { abrirAjustes() }
            Divider()
            Button("Salir") {
                NSApplication.shared.terminate(nil)
            }
        }
        .menuBarExtraStyle(.menu)
    }
}
```

El ejemplo usa nombres neutros y omite la localización. En una app real, los
títulos se traducen y los elementos estándar se construyen con las APIs de
AppKit.

### Dónde vive cada cosa

- `NSApplication.mainMenu` — El menú raíz de la app activa.
- `NSMenu` — Un menú con título; contiene elementos y separadores.
- `NSMenuItem` — Una acción, con atajo, estado y submenú opcional.
- `NSStatusItem` — Un icono propio en la zona derecha de la barra.
- `MenuBarExtra` — El equivalente SwiftUI para un extra de la barra.
- `validateMenuItem` — Decide si un elemento se puede elegir en este contexto.

### El orden importa

- El primer elemento del menú raíz es el menú de la app; su título lo pone
  macOS.
- Después vienen los menús que la app declara: Archivo, Edición y los propios.
- Cada elemento puede tener una acción, un atajo y un submenú.
- Los extras viven en la zona derecha, pero no son el menú de la app activa.
- La validación se ejecuta al abrir el menú; ahí puedes cambiar el estado de
  cada elemento.

> **No hay API para incrustar la barra global**
>
> La barra es parte del sistema. Si una app multiplataforma dibuja menús dentro
> de la ventana, está imitando el patrón; no está usando la barra global de
> macOS.

La barra global no es lo mismo que un atajo global. Los atajos del menú funcionan
cuando la app está activa; para atajos que funcionan desde cualquier app se usan
otras APIs del sistema.

---

## 6. Referencia y preguntas

**Consulta rápida**

# Referencia y preguntas

Los términos, los atajos y las dudas que aparecen cuando se trabaja con el menú
global.

### Glosario

- **App activa.** La app que recibe el foco. Es la que aporta el menú que se ve
  en la barra.
- **Menú principal.** La lista raíz de menús que la app declara al arrancar.
  macOS la convierte en la barra.
- **Menú de la app.** El menú con el nombre de la app: Acerca de, Ajustes,
  Ocultar y Salir.
- **Extra de barra.** Un icono propio en la zona derecha. No es el menú de la
  app activa.
- **Respondedor.** El objeto que recibe una acción cuando no tiene un destino
  explícito.
- **Validación.** La comprobación que decide si un elemento del menú está
  disponible.

### Preguntas frecuentes

**¿Por qué mi app no muestra menús personalizados?**
Lo más habitual es que el menú principal no se haya asignado o se haya creado
después de terminar el arranque. En AppKit, asigna NSApp.mainMenu cuando el menú
esté construido; en SwiftUI, añade el modificador .commands a la escena.

**¿Puedo mover la barra dentro de la ventana?**
No. La barra global es parte del sistema y no se puede incrustar en una ventana.
Puedes dibujar un menú dentro de tu ventana, pero es otro patrón y no tendrá el
comportamiento del menú global.

**¿Cómo agrego un icono a la derecha?**
Con un NSStatusItem en AppKit o con MenuBarExtra en SwiftUI. El icono puede abrir
un menú propio; sigue viviendo en la barra aunque cambie la app activa.

**¿Por qué desaparece en pantalla completa?**
macOS oculta la barra para dar más espacio. Se revela al acercar el cursor al
borde superior. La ocultación automática del escritorio funciona de forma
parecida.

**¿Los atajos funcionan si la app no está activa?**
No. Los atajos declarados en el menú pertenecen a la app activa. Para atajos que
funcionan desde cualquier app hacen falta APIs de atajos globales, que son otra
cosa.

**¿Qué ocurre con varias pantallas?**
Cada pantalla puede mostrar su propia barra. Si usas «Espacios separados», cada
pantalla tiene su propio espacio activo y su propia app activa; si no, la barra
principal manda.

### Atajos que verás en los menús

- Cmd , — Ajustes de la app
- Cmd H — Ocultar la app
- Cmd Q — Salir de la app
- Cmd W — Cerrar la ventana
- Cmd M — Minimizar la ventana
- Ctrl Cmd F — Pantalla completa

### Para seguir leyendo

Documentación de Apple: NSMenu, NSMenuItem, NSStatusItem y MenuBarExtra. Busca
las guías de «Menu bar» y «Status bar» en la documentación para desarrolladores.

---

Fin de la guía.
