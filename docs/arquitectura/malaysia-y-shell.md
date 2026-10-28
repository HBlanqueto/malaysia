# MalaysiaDreams + shell nativa: cómo se reparten el trabajo

Este documento fija cómo actúa la shell (**`malaysia-shell`**, un proceso por
componente, ADR 0023/0025) dentro del compositor (**MalaysiaDreams**), quién decide cada
cosa y cómo se comunican. Es la implementación de la política de diseño
(ADR 0020) y de la elección de shell nativa (ADR 0025). El diagrama está en
[`malaysia-y-shell.svg`](malaysia-y-shell.svg). El mapa completo de la UI está en
[`interfaz.md`](interfaz.md).

## La idea en una frase

**La shell dibuja el contenido de la shell; MalaysiaDreams decide dónde va, en qué capa,
de qué tamaño y cómo se rodea.** Ninguno sobrescribe al otro: cada uno manda en
su capa, como en macOS (el Dock dibuja, WindowServer compone).

## División de trabajo

| Aspecto | Quién decide | Cómo |
|---|---|---|
| Protocolos (wl_surface, xdg_shell, layer-shell...) | **MalaysiaDreams** | es el display server |
| Render de la shell (Skia, Freya) | **malaysia-shell** | proceso cliente; el compositor nunca enlaza Skia (ADR 0034) |
| Orden de composición (Background → Ventanas → Top → Overlay) | **MalaysiaDreams** | scene + damage |
| Geometría de cada superficie (dónde, tamaño, anchors, exclusive zone) | **MalaysiaDreams** | valida y coloca; la shell *pide* |
| **Contenido** de las superficies de la shell (wallpaper, barra, dock, lock) | **la shell** | `barrier-kit` → píxeles |
| Efectos de contorno (sombra, blur, inner border — ADR 0020) | **MalaysiaDreams** | envuelve cualquier superficie |

## Capturas: una capacidad del compositor

Las capturas de pantalla (`Shift+Super+3/4`, PrintScreen) están al nivel del
dock o del wallpaper como función del escritorio, pero **no tienen proceso de
shell**: las hace MalaysiaDreams. El compositor dibuja el overlay de selección, vuelve a
renderizar la escena offscreen y guarda el PNG (o, en el futuro, lo copia al
portapapeles). La shell no participa. Ver
[`capturas.md`](capturas.md).

## Chrome de ventana

La titlebar de las ventanas server-side la dibuja **MalaysiaDreams**, no la shell:
Tengen arriba (32 px para ventanas SSD), semáforos de 12 y botones de cerrar/minimizar/
maximizar. Ver [`ventanas.md`](ventanas.md). La shell no participa en el chrome
de las apps.

## Cómo se comunican

1. **El almacén `.domain`** (ADR 0021): MalaysiaDreams es el dueño de la configuración;
   cada componente de la shell lee el suyo de `~/.config/malaysia/` (preferencias)
   y `~/.local/share/malaysia/` (estado). El componente **no** escribe su propia
   configuración; solo la consume.
2. **Los protocolos**: la shell crea superficies layer-shell y las configura
   con lo que MalaysiaDreams responde (anchors, tamaño, exclusive zone). MalaysiaDreams publica lo
   que la shell necesita ver: la app activa (`window.domain`), la lista de
   ventanas (`ext-foreign-toplevel-list-v1`) y el control del dock
   (`zwlr_foreign_toplevel_management_v1`), y el bloqueo de sesión
   (`ext-session-lock-v1`).
3. **Los efectos** (ADR 0020): el compositor envuelve las superficies de la
   shell con los mismos efectos que las ventanas — nada que la shell deba
   dibujar por su cuenta.

## El diagrama

Cada componente (`wallpaper`, `menubar`, `dock`, `lock`) es un proceso
`malaysia-shell` que dibuja en su capa; MalaysiaDreams compone, coloca, da input y aplica los
efectos. Las capturas no son un proceso aparte: son una capacidad de MalaysiaDreams. La supervisión (`malaysia-shell@.service`) relanza cualquier componente
que muera sin tocar a los demás.