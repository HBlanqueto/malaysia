# 0047 — BarrierKit en capas: un toolkit para construir apps completas

Estado: **propuesto** (2026-10-28).

## Contexto

BarrierKit (ADR 0034) nació para dibujar la shell. Hoy tiene:

- un runner Wayland propio;
- la titlebar CSD, los semáforos, la barra lateral y el menú;
- unos pocos controles propios;
- re-exportaciones directas de `freya-components` (`Button`, `Switch`, `Slider`, `Input`…) que se ven con el estilo de Freya y no con el de Tengen.

La auditoría P4 (`docs/auditorias/2026-10-28-ponytail-p4{a,b,c}-*.md`) lo confirma: Ajustes del sistema reimplementa la barra lateral, el buscador, los radios, el desplegable, las muestras de acento y las filas en tarjeta. Cada app nueva tendría que volver a hacerlo.

El objetivo es que BarrierKit sirva para construir **cualquier** app del escritorio, como AppKit o libadwaita, no una sola forma de app:

- Ajustes del sistema;
- un Finder propio;
- apps al nivel de libadwaita;
- una app de muestra con todos los controles (Master Tengen App Show).

Para eso necesita una arquitectura en capas, un catálogo de piezas y reglas que impidan volver a duplicar.

Decisiones del usuario ya tomadas (2026-10-28):

- **API declarativa**, como SwiftUI y como Freya hoy. Nada de controladores al estilo del AppKit clásico.
- **Personalización por variantes de Tengen.** Una app elige variantes del sistema (tamaño, primario o destructivo, isla o plana), no colores ni medidas sueltas.
- **Búsqueda como en macOS.** Encuentra secciones y también opciones dentro del contenido.
- **Navegación atrás/adelante** con historial.
- **Barra lateral completa:** grupos con título, grupos plegables, fila de cuenta e insignias.
- **SplitView:** ancho ajustable, se puede ocultar, se pliega sola en ventanas estrechas y admite un inspector derecho.
- **Infraestructura de app «como macOS»:** acciones unificadas, preferencias por app, instancia única y restauración de estado.
- **Orden capa por capa.** Se termina una capa antes de empezar la siguiente, y Ajustes se rehace al final.

## Decisión

### 1. Cuatro capas y una columna transversal

```
┌─────────────────────────────────────────────────────────────┐
│ Capa 3 · Patrones      PreferencesWindow, BrowserWindow,    │
│                        AboutWindow…                         │
├─────────────────────────────────────────────────────────────┤
│ Capa 2 · Contenedores  SplitView, SourceList, Toolbar,      │  App (transversal)
│          y vistas      NavigationStack, Form, Table,        │  Application, Action,
│                        CollectionView, TabView, Sheet…      │  Defaults, Restoration,
├─────────────────────────────────────────────────────────────┤  UndoManager, Pasteboard
│ Capa 1 · Primitivas    Estilo (paleta semántica, texto,     │
│                        símbolos) + controles                │
├─────────────────────────────────────────────────────────────┤
│ Capa 0 · Plataforma    Runner Wayland, ventanas, popups,    │
│                        entrada, foco, cursor, escala, a11y  │
└─────────────────────────────────────────────────────────────┘
```

**Reglas de dependencia**

1. Una capa solo usa capas inferiores. La capa 1 no conoce `SplitView`, y la 0 no conoce los controles.
2. **La capa 3 solo compone la API pública de la capa 2.** Si un patrón necesita algo que la capa 2 no ofrece, ese algo se añade a la capa 2 y queda disponible para cualquier app. Ningún patrón tiene comportamiento privado.
3. Una app puede usar cualquier capa directamente. Ajustes usa la capa 3, un Finder compone la capa 2 y una vista muy especial puede dibujar con primitivas de Freya (`rect`, `label`) usando la paleta semántica.
4. La columna **App** la usan las capas 2 y 3 (la barra de herramientas y los menús consumen `Action`), pero no depende de ellas.

### 2. Reglas para todo componente del kit

- **Fuente única de diseño.** Todo valor visual sale de `vow-design` (Tengen, ADR 0011/0032/0046). Si falta un token, se añade a `tengen/components.json` y se genera; nunca va un literal en el componente.
- **Variantes, no estilos libres.** Cada componente expone variantes del sistema mediante enums como `ButtonStyle::{Primary, Secondary, Plain, Destructive}` o `ControlSize::{Small, Regular, Large}`. No hay `.background(color)` en los componentes del kit.
- **Controlado y declarativo.** El estado lo tiene la app (`State<T>`). El componente recibe `value` y avisa con `on_change`. Nada de estado oculto que la app no pueda leer.
- **Interacción común.** Todo control interactivo usa el mismo hook `use_interaction()`, que da hover, pressed, foco de teclado y disabled. El anillo de foco (`focus-ring`) se dibuja solo al navegar con teclado.
- **Accesibilidad obligatoria.** Cada componente asigna rol, etiqueta y estado (checked, expanded, selected), y es navegable por teclado. Un control sin etiqueta accesible no compila: la etiqueta es argumento de `new`.
- **Reactivo a la apariencia.** Los componentes leen la apariencia del contexto que provee el runner. Un cambio de claro u oscuro, de acento o de tinte vuelve a pintar sin reiniciar la app.
- **No reinventar Freya.** Cuando `freya-components` 0.4.3 ya tiene la lógica, el kit la envuelve con el aspecto de Tengen en lugar de reescribirla. Casos: `ResizableContainer`, `Table`, `VirtualScrollView`, `DragZone`/`DropZone`, `Tooltip`, `Attached`, `Checkbox`, `RadioItem`, `Select`, `ProgressBar`, `ContextMenu`, `Accordion`. Si el envoltorio no es posible, el porqué se documenta con `// shortcut:` o en el ADR del componente.
- **Nombres de AppKit** cuando existe equivalente (`SplitView`, `SourceList`, `PopUpButton`, `ColorWell`, `Sheet`), en inglés en el código.
- **Cada componente nuevo** llega con:
  - una página en la app de catálogo (ver §6);
  - un test de render en claro, oscuro y tintado;
  - un test de teclado si es interactivo.

### 3. Catálogo por capa

Estado: **existe** = ya está en el kit, aunque haya que arreglarlo · **parcial** · **nuevo**.

**Capa 0 · Plataforma** (`platform/`)

| Pieza | Estado | Nota |
|---|---|---|
| Runner, ventanas, popups, layer-shell, lock | existe | Se parte en módulos después de los lotes L1 (P4a-08) |
| Foco de teclado y Tab | **nuevo** | L1b (F-01) |
| Cursor por componente | **nuevo** | L1b (F-02) |
| Escala fraccional (wp_fractional_scale + viewporter) | **nuevo** | L1c |
| Portapapeles y selección primaria | **nuevo** | `wl_data_device`, `zwp_primary_selection` |
| Arrastrar y soltar entre apps | nuevo | `wl_data_device` + `DragZone` |
| AT-SPI (accesskit_unix) | nuevo | Junto al Bloque 12b |
| Damage y región opaca | nuevo | P4a S-07 |

**Capa 1 · Primitivas**

| Pieza | Estado | Nota |
|---|---|---|
| `Palette` semántica | **nuevo** | `label`, `secondary_label`, `tertiary_label`, `separator`, `window_background`, `under_page_background`, `control_background`, `control_accent`, `selected_content_background`, `destructive`, `success`, `warning`, sacados de Tengen y del tinte (VowAppearance). Reemplaza el `ColorsSheet` aplanado de `theme.rs` |
| Tema de Freya generado desde la paleta | parcial | Para los componentes de Freya que se usen tal cual; reactivo |
| `TextStyle` | parcial | Los 9 estilos de Tengen (Display…Caption, Mono). `Text::new(..).style(Body).secondary()` |
| `Symbol` / registro de iconos | **nuevo** | Iconos por nombre, tamaño ligado al estilo de texto, caché por (nombre, tamaño, color). Sustituye las 6 copias de `svg_viewer` |
| `use_interaction` | **nuevo** | hover, pressed, foco de teclado, disabled |
| `Button`, `IconButton` | **nuevo** | Variantes primary/secondary/plain/destructive y tres tamaños; botón por defecto (Enter) y de cancelar (Esc) |
| `Switch`, `Checkbox`, `Radio` + `RadioGroup` | **nuevo** | `Radio` sale de Ajustes |
| `Slider`, `Stepper` | nuevo | |
| `SegmentedControl` | **nuevo** | Token `segmented` |
| `TextField`, `SecureField`, `SearchField` | parcial | Con anillo de foco, placeholder, botón de borrar y rol de contraseña |
| `PopUpButton`, `ComboBox` | **nuevo** | Sobre `MenuView`; el desplegable de semáforos de Ajustes es un `PopUpButton` |
| `ColorWell` | **nuevo** | Muestras de color con nombre; sale de Ajustes |
| `ProgressBar`, `ProgressRing`, `Spinner` | nuevo | Token `progress` |
| `Badge`, `Label`, `Link`, `Divider`, `Avatar`, `Image` | nuevo | |
| `ToolGroup`, `ToolButton` | existe | Pasan a usar `Action` (§4) |
| Semáforos, titlebar | existe | Se arreglan en L1d |

**Capa 2 · Contenedores y vistas**

| Pieza | Estado | Nota |
|---|---|---|
| `SplitView` | **nuevo** | Barra lateral más detalle, con inspector derecho opcional. Ancho ajustable con mínimo y máximo, guardado en la restauración. Se oculta con un botón de la barra de herramientas y un atajo. Por debajo de un ancho de ventana (token) se pliega sola y se abre como panel superpuesto. Isla o plana (ADR 0041). Lleva dentro los semáforos cuando la barra lateral está visible. Sobre `ResizableContainer` |
| `SourceList` | parcial | La barra lateral: grupos con título, grupos plegables, fila de cuenta, ítems con símbolo o icono de app, insignias y contadores, selección y teclado. Hoy son `Sidebar`/`SidebarItem`/`SidebarSection` |
| `Toolbar` | parcial | Zonas leading, center y trailing; botones de navegación; título y subtítulo; hueco para búsqueda; overflow en menú. Vive en la titlebar CSD (ADR 0035) |
| `NavigationStack` | **nuevo** | Historial atrás/adelante, subpáginas (`push`), título actual para la barra de herramientas, acciones `go-back` y `go-forward` con atajos |
| `Searchable` | **nuevo** | Índice de búsqueda: cada sección y cada fila declaran título y palabras clave; el kit busca y devuelve resultados; elegir uno navega y resalta la fila. Lo usa `SearchField` en la barra lateral o en la barra de herramientas |
| `Form`, `Section`, `Row` | **nuevo** | Token `list-group`. Variantes de fila: etiqueta + control, de navegación (chevron, `push`), de información y destructiva. Encabezado y pie por sección |
| `ScrollView`, `VirtualList` | existe | Freya |
| `Table`, `OutlineView`, `ColumnBrowser` | nuevo | Columnas ordenables, virtualizadas. Base para un Finder |
| `CollectionView` | nuevo | Rejilla de iconos con selección múltiple, selección por arrastre, renombrar y arrastrar |
| `TabView` | nuevo | Pestañas de documento en la ventana |
| `Sheet`, `Alert`, `Popover`, `Tooltip`, `ContextMenu` | parcial | `MenuView` ya existe. Sheet baja de la titlebar. Alert con botones por defecto y cancelar |
| `EmptyState` | nuevo | Token `settings.empty` generalizado |

**Capa 3 · Patrones**

| Pieza | Estado | Nota |
|---|---|---|
| `PreferencesWindow` | **nuevo** | SplitView + SourceList (cuenta, grupos) + SearchField con `Searchable` del contenido + NavigationStack + Form. Opción de ancho fijo (solo cambia de alto) con el semáforo verde «+» |
| `BrowserWindow` | nuevo | SplitView + Toolbar + NavigationStack + Table/CollectionView + inspector. Para un Finder |
| `AboutWindow` | nuevo | Ventana «Acerca de» estándar |

**App (transversal, `app/`)**

| Pieza | Estado | Nota |
|---|---|---|
| `Application` | **nuevo** | `app_id`, arranque, instancia única (`org.freedesktop.Application`, `DBusActivatable`), abrir archivos y URLs, política de qué pasa al cerrar la última ventana |
| `Action` | **nuevo** | id, título, `Shortcut` tipado, estado activa/marcada y handler **siempre en el hilo de UI**. Una acción alimenta el menú global (dbusmenu), la barra de herramientas, los menús contextuales y el teclado. Cadena de respuesta: ventana enfocada → app. Sustituye las cadenas de `MenuItem` y `shortcut_matches` (P4b-10/14) |
| `Defaults` | **nuevo** | Preferencias por app en `~/.config/malaysia/<app_id>.domain` (formato `.domain`, ADR 0021), tipadas con serde, con aviso al cambiar (el mismo watcher que `AppearanceWatch`) |
| `Restoration` | **nuevo** | Tamaño y posición, sección, ancho de barra lateral, pila de navegación y desplazamiento por ventana. Se conectará con `xdg-session-management` cuando el compositor lo tenga |
| `UndoManager` | nuevo | Deshacer y rehacer conectados a las acciones estándar de Edición |
| `Pasteboard` | nuevo | API sobre el portapapeles de la capa 0 |

### 4. Estructura del crate

Un solo crate `barrier-kit` con una carpeta por capa:

```
barrier-kit/src/
  platform/   capa 0  (partido en egl, wayland, window, popup, input, timer, a11y)
  style/      capa 1  (palette, text, symbol, interaction, tema de Freya)
  controls/   capa 1  (button, switch, radio, field, popup_button, color_well…)
  views/      capa 2  (split_view, source_list, toolbar, navigation, search, form, table…)
  patterns/   capa 3  (preferences_window, browser_window, about)
  app/        transversal (application, action, defaults, restoration, undo)
  shell/      solo para la shell, detrás de la feature `shell` (ver abajo)
  prelude.rs  lo que una app importa normalmente
```

**La parte de la shell va separada, como en macOS.** El Dock, la barra de menús y la pantalla de bloqueo de macOS no usan la API pública de AppKit para lo que hacen distinto: usan frameworks privados (SkyLight y compañía). Aquí se reproduce esa separación dentro del mismo crate:

- Todo lo que solo usa `malaysia-shell` vive en su propia carpeta `src/shell/`, en archivos propios y nunca mezclado con el resto:

  ```
  shell/
    mod.rs             punto de entrada, solo existe con la feature
    layer.rs           superficies layer-shell (run_layer, Anchors, Layer…)
    lock.rs            ext-session-lock (run_lock, SessionUnlock)
    toplevels.rs       foreign-toplevel: lista y control de ventanas para el Dock
    infinity.rs        protocolo privado de colocación de ventanas
    appmenu_client.rs  org_kde_kwin_appmenu del lado de quien muestra menús
    activation.rs      tokens xdg-activation para lanzar apps desde el Dock
  ```
- La carpeta solo se compila con la feature `shell` (`#[cfg(feature = "shell")] pub mod shell;`). `malaysia-shell` la activa en su `Cargo.toml`; Ajustes, Master Tengen App Show y cualquier app nueva no. Para ellas `barrier_kit::shell` no existe: ni lo compilan ni pueden llamarlo por error.
- **Dirección de las dependencias:** `shell/` usa `platform/`, pero `platform/` no sabe nada de `shell/`. El runner expone puntos de extensión (registrar un rol de superficie, recibir eventos de registro Wayland), y `shell/` se engancha a ellos. Ningún `#[cfg(feature = "shell")]` suelto dentro de `platform/`, salvo la línea que registra el módulo.
- Las vistas de la shell (menubar, dock, lock, launcher) siguen en `malaysia-shell` y se construyen con las capas 1–3 como cualquier app. `src/shell/` solo aporta los protocolos y tipos de plataforma que esas vistas necesitan.

Los únicos consumidores son `malaysia-shell` y `malaysia-settings`, así que la API puede romperse entre fases sin capa de compatibilidad.

### 5. Orden: capa por capa

| Fase | Contenido | Sale cuando |
|---|---|---|
| 0 | Lotes de bugs L1a–L1d (auditoría P4) | Los informes de L1 revisados y con push |
| 1 | **Capa 0**: escala fraccional, portapapeles, damage y región opaca, partir `platform/`, mover lo de la shell a `src/shell/` con la feature `shell` | Prueba visible en anidado y en el ThinkPad |
| 2 | **Capa 1**: paleta semántica y tema reactivo, `TextStyle`, `Symbol`, `use_interaction`, todos los controles de la tabla | Cada control con su página en el catálogo y sus tests |
| 3 | **App**: `Action`, `Application`, `Defaults` (la capa 2 los necesita) | El menú global y los atajos van por `Action` |
| 4 | **Capa 2**: SplitView, SourceList, Toolbar, NavigationStack, Searchable, Form, Sheet/Alert/Popover. Después Table, CollectionView y TabView | Catálogo completo |
| 5 | **Capa 3**: PreferencesWindow; `Restoration` | — |
| 6 | **Ajustes del sistema rehecho** solo con la capa 3; se borran sus copias locales | Ajustes no define ningún control ni color propio |

La app **Master Tengen App Show** se crea en la fase 2 con sus primeras páginas y crece con cada fase.

Cada fase se divide en lotes pequeños para opencode, con prueba visible en la sesión anidada.

### 6. App de catálogo: Master Tengen App Show

**Master Tengen App Show** (crate y binario `tengen-app-show`, `app_id` `…tengen-app-show`) es la widget-factory del kit, equivalente a gtk4-widget-factory. Usa `PreferencesWindow`, con una sección por capa y una página por componente que muestra todas sus variantes y estados. Es a la vez:

- la documentación viva del kit;
- el sitio de las pruebas visibles de cada lote;
- la fuente de los tests de render (cada página se renderiza en claro, oscuro y tintado).

## Consecuencias

- Ajustes del sistema deja de definir controles: `controls.rs`, `sidebar.rs`, `groups.rs` y sus `svg_viewer`/`rgba` locales desaparecen en la fase 6.
- `barrier_kit::controls` deja de re-exportar componentes de Freya con su aspecto por defecto.
- El menú global, los atajos y la barra de herramientas comparten un único modelo de acciones. Las cadenas de atajo dejan de parsearse en cada tecla.
- Se añaden muchos tokens a `tengen/components.json` (controles, split view, toolbar, tabla…) y el generador de `vow-design` crece.
- El trabajo es largo. Rehacer Ajustes queda al final de la fase 6.

## Referencias

- [`0011`](0011-valores-de-diseno-como-constantes-rust.md), [`0032`](0032-neptunia-como-fuente-de-diseno.md), [`0046`](0046-tengen-lenguaje-visual.md): tokens y Tengen.
- [`0021`](0021-superficie-de-configuracion-y-control.md): dominios `.domain` y D-Bus.
- [`0034`](0034-celestialkit-sobre-freya.md): BarrierKit sobre Freya.
- [`0035`](0035-identificacion-de-apps-propias-y-csd.md): CSD de las apps propias.
- [`0040`](0040-menu-global-real.md): menú global.
- [`0041`](0041-barra-lateral-en-isla.md): barra lateral en isla.
- [`0042`](0042-ajustes-del-sistema.md): Ajustes del sistema.
- `docs/auditorias/2026-10-28-ponytail-p4a-runner.md`, `-p4b-menu.md`, `-p4c-controles.md`.

## Anexo A · Cómo se escribe una app (boceto de la API)

Es orientativo: fija la forma de la API, no las firmas exactas. Cada fase la concreta.

### Ajustes del sistema con la capa 3

```rust
use barrier_kit::prelude::*;

fn main() -> Result<(), RunError> {
    Application::new("org.malaysia.Settings")          // instancia única, Defaults, Restoration
        .run(|| {
            PreferencesWindow::new("Ajustes del sistema")
                .fixed_width()                           // solo cambia de alto; verde = «+»
                .account(Account::current_user())        // fila de cuenta arriba
                .group(SidebarGroup::new()
                    .pane(Pane::new("appearance", "Apariencia")
                        .symbol("paintbrush.fill").tint(SymbolTint::Blue)
                        .content(appearance))
                    .pane(Pane::new("desktop", "Escritorio y Dock")
                        .symbol("dock.rectangle").content(desktop)))
                .group(SidebarGroup::titled("Red").collapsible()
                    .pane(Pane::new("wifi", "Wi-Fi").symbol("wifi")
                        .badge(Badge::count(2)).content(wifi)))
        })
}

fn appearance() -> impl IntoElement {
    let mut cfg = use_defaults::<AppearanceConfig>("appearance");   // dominio del sistema

    Form::new()
        .section(Section::new()
            .row(Row::new("Apariencia")
                .keywords(["tema", "claro", "oscuro", "automático"])
                .control(ChoiceCards::new(cfg.read().color_scheme)   // tarjetas con miniatura
                    .option(ColorScheme::Auto,  "Automático", ThemePreview::auto())
                    .option(ColorScheme::Light, "Claro",      ThemePreview::light())
                    .option(ColorScheme::Dark,  "Oscuro",     ThemePreview::dark())
                    .on_change(move |s| cfg.write().color_scheme = s)))
            .row(Row::new("Color de acento")
                .keywords(["color", "acento"])
                .control(ColorWell::swatches(ACCENT_SWATCHES)
                    .value(cfg.read().accent)
                    .on_change(move |c| cfg.write().accent = c))))
        .section(Section::titled("Ventanas")
            .row(Row::new("Semáforos")
                .control(PopUpButton::new(cfg.read().traffic_style)
                    .item(TrafficLightStyle::Auto,     "Automático")
                    .item(TrafficLightStyle::Standard, "Clásico")
                    .item(TrafficLightStyle::Accent,   "Acento")
                    .item(TrafficLightStyle::Wallpaper,"Fondo")
                    .on_change(move |s| cfg.write().traffic_style = s)))
            .row(Row::navigation("Opciones avanzadas")      // empuja una subpágina
                .push(|| advanced())))
}
```

La búsqueda «como macOS» sale sola: `PreferencesWindow` construye el índice `Searchable` con los títulos de los paneles y los `Row` con sus `keywords`. Elegir un resultado navega al panel, desplaza hasta la fila y la resalta.

### Un Finder mínimo con la capa 2

```rust
fn finder_window() -> impl IntoElement {
    let mut location = use_state(|| Location::home());
    let mut selection = use_state(Selection::default);
    let mut show_info = use_state(|| false);

    Window::new()
        .toolbar(Toolbar::new()
            .leading(NavigationButtons::new())                       // ‹ › con historial
            .title_from_navigation()
            .trailing(ToolGroup::new()
                .action(ViewAs::Icons.action())
                .action(ViewAs::List.action())
                .action(ViewAs::Columns.action()))
            .trailing(ToolButton::action(&SHOW_INFO))
            .search(SearchField::new().on_submit(|q| /* buscar archivos */)))
        .content(SplitView::new()
            .sidebar(SourceList::new(location)
                .group(SidebarGroup::titled("Favoritos")
                    .items(favorites().map(|f| SidebarItem::new(f.name).symbol(f.symbol).value(f.location))))
                .group(SidebarGroup::titled("Ubicaciones").collapsible()
                    .items(volumes().map(|v| SidebarItem::new(v.name).symbol("externaldrive").value(v.location)))))
            .detail(NavigationStack::new(location).view(move |loc| {
                CollectionView::new(entries(loc))
                    .item(|e| IconTile::new(&e.name).image(e.icon()))
                    .selection(selection)
                    .on_open(move |e| if e.is_dir() { location.set(e.location()) })
            }))
            .inspector(Inspector::new(move || info_panel(selection)).visible(show_info)))
}

static SHOW_INFO: Action = Action::new("finder.show-info", "Obtener información")
    .shortcut(Shortcut::ctrl('i'))
    .symbol("info.circle");
```

La misma `SHOW_INFO` aparece en el menú global, funciona con su atajo y se marca como activa en la barra de herramientas sin escribir nada más. El Finder no usa ningún patrón de la capa 3 y no reimplementa nada: solo compone piezas de la capa 2.
