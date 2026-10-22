# Yuki + shell nativa: cómo se reparten el trabajo

Este documento fija cómo actúa la shell (**`yuki-shell`**, un proceso por
componente, ADR 0023/0025) dentro del compositor (**Yuki**), quién decide cada
cosa y cómo se comunican. Es la implementación de la política de diseño
(ADR 0020) y de la elección de shell nativa (ADR 0025). El diagrama está en
[`yuki-y-shell.svg`](yuki-y-shell.svg).

## La idea en una frase

**La shell dibuja el contenido de la shell; Yuki decide dónde va, en qué capa,
de qué tamaño y cómo se rodea.** Ninguno sobrescribe al otro: cada uno manda en
su capa, como en macOS (el Dock dibuja, WindowServer compone).

## División de trabajo

| Aspecto | Quién decide | Cómo |
|---|---|---|
| Protocolos (wl_surface, xdg_shell, layer-shell...) | **Yuki** | es el display server |
| Orden de composición (Background → Ventanas → Top → Overlay) | **Yuki** | scene + damage |
| Geometría de cada superficie (dónde, tamaño, anchors, exclusive zone) | **Yuki** | valida y coloca; la shell *pide* |
| **Contenido** de las superficies de la shell (wallpaper, barra, dock, lock) | **la shell** | `celestial-kit` → píxeles |
| Efectos de contorno (sombra, blur, inner border — ADR 0020) | **Yuki** | envuelve cualquier superficie |

## Capturas: una capacidad del compositor

Las capturas de pantalla (`Shift+Super+3/4`, PrintScreen) están al nivel del
dock o del wallpaper como función del escritorio, pero **no tienen proceso de
shell**: las hace Yuki. El compositor dibuja el overlay de selección, vuelve a
renderizar la escena offscreen y guarda el PNG (o, en el futuro, lo copia al
portapapeles). La shell no participa. Ver
[`capturas.md`](capturas.md).

## Cómo se comunican

1. **El almacén `.domain`** (ADR 0021): Yuki es el dueño de la configuración;
   cada componente de la shell lee el suyo de `~/.config/yuki/` (preferencias)
   y `~/.local/share/yuki/` (estado). El componente **no** escribe su propia
   configuración; solo la consume.
2. **Los protocolos**: la shell crea superficies layer-shell y las configura
   con lo que Yuki responde (anchors, tamaño, exclusive zone). Yuki publica lo
   que la shell necesita ver: la app activa (`window.domain`), la lista de
   ventanas (`ext-foreign-toplevel-list-v1`) y el control del dock
   (`zwlr_foreign_toplevel_management_v1`), y el bloqueo de sesión
   (`ext-session-lock-v1`).
3. **Los efectos** (ADR 0020): el compositor envuelve las superficies de la
   shell con los mismos efectos que las ventanas — nada que la shell deba
   dibujar por su cuenta.

## El diagrama

Cada componente (`wallpaper`, `menubar`, `dock`, `lock`) es un proceso
`yuki-shell` que dibuja en su capa; Yuki compone, coloca, da input y aplica los
efectos. Las capturas no son un proceso aparte: son una capacidad de Yuki. La supervisión (`yuki-shell@.service`) relanza cualquier componente
que muera sin tocar a los demás.