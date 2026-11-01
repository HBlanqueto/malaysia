# Informe de diagnóstico — libadwaita en MalaysiaDreams

> Incluido en este directorio: este informe, `capturas/`,
> `compositor-foco-menus.txt` y `menubar-filtrado.txt`. Los logs completos
> (`compositor.log`, `demo-*.log`), `check.txt` y las capturas originales
> quedaron en `/tmp/malaysia-diag/` y no se suben al repo.

Fecha: 2026-11-01. Diagnóstico únicamente: no se ha cambiado código, ni se ha
hecho commit ni push. `Cargo.lock` ya estaba modificado al empezar (`git status`
lo mostró tras el `pull`; 4 líneas borradas) y se ha dejado como estaba.

## 1. Commits

| Repo | Commit | Título |
|---|---|---|
| `~/src/malaysia` | `e476fbb` | `fix(compositor): «Activar pantalla completa» de la barra entra en modo foco` |
| `~/src/barrierkit` | `c8965c3` | `feat(infinity): infinity_focus_v1 y semáforos que aparecen con la barra en foco` |

`./dev.sh kit-local` quedó activo (`.cargo/config.toml` apunta a
`../barrierkit`). La app libadwaita usada carga la libadwaita parcheada del dev
shell (`/nix/store/j0kyj09ia2y0w793hx91llhqaq090qlg-libadwaita-1.9.3`), tanto
`gnome-calculator` como `gnome-text-editor`.

## 2. `./dev.sh check`

Salida completa: `/tmp/malaysia-check.txt`. **Exit code 0. No hubo fallos.**

- `check_display_env`, `cargo fmt --all -- --check`, comprobaciones de mockups
  y tokens, `check-client-only-deps.sh` y `check-client-dispatch.sh`: OK.
- `cargo clippy --workspace --all-targets -- -D warnings`: OK.
- `cargo test --workspace`: todas las suites OK, sin fallos. Totales:
  17, 259 (1 ignored), 15, 51, 19, 126, 24 (1 ignored), 16 y 1 doc-test.
- La línea `zwp_linux_buffer_params_v1#8: error 7: create_immed failed...` que
  aparece en el log es la salida esperada del test
  `session::tests::a_failed_create_immed_is_drained_and_posts_the_protocol_error`,
  que pasa.

## 3. Entorno de ejecución (importante para reproducir)

- No hay interacción humana. El compositor se condujo con
  `MALAYSIA_TEST_INPUT` (pasa por la ruta real de puntero/teclado) y las
  capturas se pidieron con `Shift+Super+3`, redirigidas con
  `MALAYSIA_SCREENSHOT_DIR=/tmp/malaysia-capturas`.
- El runtime real de `dev.sh` es
  `/run/user/1000/malaysia-dev-2486929318/`, no `$XDG_RUNTIME_DIR/malaysia/`.
  El socket anunciado en el log se usó por nombre exacto (`wayland-1` en la
  ejecución final; `wayland-2..10` en ejecuciones previas). Nunca se usó
  `wayland-0`.
- `nautilus --new-window` **no llegó a mapear ventana** en ~85 s (solo Tracker
  y el error de a11y; el compositor nunca registró `app_id="org.gnome.Nautilus"`).
  Se sustituyó por `gnome-text-editor` 50.1 de nixpkgs, lanzado desde el dev
  shell para que cargue la libadwaita parcheada (verificado con `ldd`).
- Las apps se lanzaron con `env -u DISPLAY WAYLAND_DISPLAY=<socket>
  XDG_SESSION_TYPE=wayland`, desde el mismo shell de nix para compartir el bus
  de sesión con la menubar.
- El log final se recogió con la demo todavía en marcha; después se pararon
  todos los compositores anidados propios. No queda ningún proceso de la demo.

## 4. Fallo A — fullscreen / modo foco con libadwaita

Apps: `gnome-calculator` 50.0 y `gnome-text-editor` 50.1 (ambas libadwaita).
Capturas en `capturas/` (S* = calculadora, E* = editor).

| Forma | gnome-calculator | gnome-text-editor |
|---|---|---|
| **(a) Botón de maximizar de la headerbar** | **No existe.** La headerbar solo tiene undo/redo, desplegable «Básico», hamburguesa y cerrar. No hay botón verde/rojo/amarillo. No aplicable. | **No existe.** La headerbar solo tiene «Abrir», nuevo, info, hamburguesa y cerrar. No aplicable. |
| **(b) Doble clic en la headerbar** | **FALLO.** El compositor recibe `gtk_surface titlebar double_click` y ejecuta `toggle_maximize maximize=true` (`state.rs:743`), es decir, **maximiza clásico, NO entra en modo foco**. La ventana queda en `(0,28) 1280x708`: la barra normal sigue visible y el dock sigue visible encima del borde inferior; no hay espacio aparte ni barra negra. Se sale con otro doble clic en la headerbar y vuelve a `(460,74) 360x616` correctamente. Capturas `S2` (maximizada) y `S3` (restaurada). | **FALLO, igual.** `toggle_maximize maximize=true`, `configure 1280x708`; barra y dock normales, sin modo foco (`E2`). Se sale con otro doble clic y restaura (`E3`). En una ejecución anterior el cliente devolvió un segundo configure a `1024x548` y la barra perdió los menús de la app (ver fallo B). |
| **(c) F11** | **No inyectable** con `MALAYSIA_TEST_INPUT`: el parser solo admite flechas, Return, Escape, Home, End, Space, alfanuméricos y modificadores; no hay F11 ni PrintScreen. Calculator tampoco ofrece «Pantalla completa» en su menú. No probado por esta vía. | **Probado por la vía equivalente:** menú hamburguesa → «Pantalla completa (F11)», que llama al mismo `xdg_toplevel.set_fullscreen`. **FALLO:** se crea el espacio de foco (barra y dock ocultos), pero la ventana **no crece**: el log dice `focus transition handed over ... committed=1024x548 output=1280x800 late_ms=310/312` y la captura `E4` muestra la ventana en `(0,0) 1024x548` con el resto del espacio en negro. Se sale con `Ctrl+Super+F` y restaura (`E5`). |
| **(d) Ctrl+Super+F** | **OK.** `focus mode entered`, ventana `1280x800`, barra y dock ocultos (`S4`). Al repetir, `focus mode left`, vuelve a `(460,74) 360x616` (`S5`). | **OK.** `focus mode entered`, ventana `1280x800`, barra y dock ocultos. Al repetir, restaura a `(128,108) 1024x548`. |
| **(e) Barra › Ventana › Ventana a pantalla completa › Activar pantalla completa** | **OK en la entrada** (el commit `e476fbb` lo arregla): `infinity set fullscreen fullscreen=true` → `focus mode entered` → `1280x800`, barra y dock ocultos (`S7`). La salida por el mismo menú es frágil: al bajar la barra en el espacio de foco, a veces la barra cae al menú de escritorio y la opción de la app desaparece (ver fallo B). En la ejecución final la barra conservó los menús y «Ventana» abrió bien (`S8`, `S9`), pero se salió con `Ctrl+Super+F`. | **No disponible de forma fiable:** al enfocar el editor la barra mostró solo «TextEditor» (sin «Ventana»/«Ayuda»); tras cambiar de escritorio sí aparecieron «Ventana» y «Ayuda» (`S13`), pero no se volvió a probar el submenú. Para el editor se usó su menú propio «Pantalla completa» (caso c). |

Resumen del fallo A: **doble clic y F11 (petición del cliente) no dan el modo
foco correcto**; `Ctrl+Super+F` y la entrada por Infinity sí. El doble clic
entra en maximizado clásico; la petición `set_fullscreen` del cliente crea el
espacio de foco pero deja la ventana al tamaño anterior (negro alrededor).

## 5. Fallo B — menú global con libadwaita

Secuencia exacta (ejecución final, `S1..S15`; los tiempos son del
`compositor.log` final):

1. Se abren Calculator y TextEditor; TextEditor mapea después y se queda con el
   foco. Se enfoca Calculator con clic en `(600,100)`.
   Barra superior: **`Calculadora · TextEditor · Ventana · Ayuda`**.
   El ítem **TextEditor sobra**: la ventana enfocada es Calculator. Ya está mal
   desde el estado inicial (`S1`). No es un fallo transitorio de una sola
   ejecución: se repite en la mayoría de las pruebas con las dos apps abiertas;
   en otras el segundo ítem aparece como **«Escritorio»** (también incorrecto:
   abre el menú de app libadwaita de Calculator).
2. Se abre «Calculadora» (menú de app tipo macOS: Acerca de, Ajustes, Servicios,
   Ocultar, Salir) y responde (`S6`). También se abrió «Escritorio» (el menú de
   app libadwaita; ver más abajo) y «Ventana».
3. `Ctrl+Super+F`: modo foco correcto, barra/dock ocultos (`S4`). Al salir, la
   barra vuelve con el mismo ítem espurio `TextEditor` (`S5`).
4. Se abre `Ventana › Ventana a pantalla completa › Activar pantalla completa`:
   entra en modo foco correctamente (`S7`).
5. Se mueve el puntero a `y=0` para bajar la barra. Log:
   `focus space bar window=5 shown=true`. **Aquí aparece el fallo:** en la
   ejecución final la barra conservó `Calculadora · TextEditor · Ventana ·
   Ayuda` (`S8`), pero **no se pintó negra** (siguió el gris normal). En una
   ejecución anterior, en este mismo momento la barra cayó al conjunto de
   escritorio **`Escritorio · Archivo · Edición · Visualización · Ir · Ventana ·
   Ayuda`**, desapareció la opción de la app y cambiaron las posiciones. En esa
   ejecución, el clic en la antigua `x=236` (que era «Ventana») abrió **Edición**;
   el menú no está muerto, pero deja de responder a lo que el usuario espera
   porque el conjunto y las posiciones cambian. Log de esa ejecución:
   `/tmp/malaysia-diag/calc-e/malaysia-compositor.log` (popup de capa en
   `x=191..411`, distinto del de Calculator).
6. `Ctrl+Super+F` para salir. La barra vuelve con `Calculadora · TextEditor ·
   Ventana · Ayuda` (`S10`). Se hace clic en **TextEditor** (`x=167`) con
   Calculator enfocada: se abre el **menú de app de Calculator** (Ventana nueva,
   Limpiar histórico, Automático, Fijo, Científico, Ingeniería, Preferencias,
   Atajos, Ayuda, Acerca de Calculadora) **bajo el título TextEditor** (`S11`).
   Es decir, el ítem espurio está cableado al modelo de la app enfocada.
7. `Ctrl+Alt+N` crea el escritorio 2 y se muestra; `Ctrl+Alt+←` vuelve al 1.
   Log: `space shown index=1 ... desktop=2` y `space shown index=0 ...
   desktop=1`. Tras volver, el menú sigue respondiendo (`S12`). No se rompe por
   cambiar de escritorio.
8. Se enfoca TextEditor con clic en `(900,150)`: barra `TextEditor · Ventana ·
   Ayuda` (`S13`). Se vuelve a Calculator con clic en `(640,86)`: barra
   `Calculadora · TextEditor · Ventana · Ayuda` otra vez (`S14`) y el menú
   «Calculadora» sigue respondiendo (`S15`). El ítem espurio no se limpia al
   cambiar de foco; los menús no se quedan muertos, pero muestran una app que no
   es la enfocada.

Conclusión del fallo B: el fallo reproducible es **la barra muestra un ítem de
menú de app que no corresponde / duplicado (`TextEditor` junto a
`Calculadora`)**, y en modo foco la barra puede caer al menú de escritorio y
cambiar las posiciones, con lo que la opción de la app desaparece. Además, el
menú de app libadwaita aparece a veces titulado **«Escritorio»** en lugar del
nombre de la app (visto en `/tmp/malaysia-diag/calc-menus`), y para TextEditor
faltan «Ventana»/«Ayuda» en la barra hasta que se cambia de escritorio.

No se observó un cuelgue total del menú; lo que se observó es desaparición de
la opción/conjunto correcto y desplazamiento de posiciones.

## 6. Logs finales

En `/tmp/malaysia-diag/` quedaron los artefactos completos:

- `compositor.log` (619 líneas; la primera línea es el aviso de nix por el
  árbol sucio, no del compositor).
- `compositor-foco-menus.txt` (21 líneas; se incluye entero: son las últimas
  150).
- `menubar-filtrado.txt` (64 líneas; se incluye entero).
- `demo-*.log`: se copiaron todos los del runtime. Ojo: el runtime conserva
  logs de una sesión anterior (`lab`, 07:34–07:37): `demo-contador.log`,
  `demo-firefox.log`, `demo-zenity.log`, `demo-nautilus.log` y
  `demo-gnome-calculator.log` no son de esta reproducción. El
  `demo-menubar.log` sí es el de la ejecución final (14:29–14:30).

### 6.1 `compositor-foco-menus.txt` (completo)

```text
92:2026-11-01T14:29:47.426804Z  INFO gtk_menus set window=5 bus=:1.2825 menubar=/org/gnome/Calculator/menus/menubar appmenu=
94:2026-11-01T14:29:47.426921Z  INFO unmaximize_request window=5
101:2026-11-01T14:29:47.463488Z  INFO gtk_menus set window=6 bus=:1.2826 menubar=/org/gnome/TextEditor/menus/menubar appmenu=
149:2026-11-01T14:29:57.571410Z  INFO toggle_maximize window=5 maximize=true origin="crates/malaysia-compositor/src/state.rs:743"
190:2026-11-01T14:30:01.814314Z  INFO toggle_maximize window=5 maximize=false origin="crates/malaysia-compositor/src/state.rs:743"
219:2026-11-01T14:30:05.675519Z  INFO focus mode entered window=5 index=1
220:2026-11-01T14:30:05.675549Z  INFO space shown index=1 spaces=2 focus=true
233:2026-11-01T14:30:06.155687Z  INFO focus transition handed over window=5 committed=Some(Size<smithay::utils::geometry::Logical> { w: 1280, h: 800 }) output=Size<smithay::utils::geometry::Logical> { w: 1280, h: 800 } late_ms=0
254:2026-11-01T14:30:09.237672Z  INFO focus mode left window=5
255:2026-11-01T14:30:09.237700Z  INFO space shown index=0 spaces=2 desktop=1 focus=false
268:2026-11-01T14:30:09.719798Z  INFO space shown index=0 spaces=1 desktop=1 focus=false
339:2026-11-01T14:30:18.284245Z  INFO infinity set fullscreen window=5 fullscreen=true
340:2026-11-01T14:30:18.284329Z  INFO focus mode entered window=5 index=1
346:2026-11-01T14:30:18.284534Z  INFO space shown index=1 spaces=2 focus=true
354:2026-11-01T14:30:18.766277Z  INFO focus transition handed over window=5 committed=Some(Size<smithay::utils::geometry::Logical> { w: 1280, h: 800 }) output=Size<smithay::utils::geometry::Logical> { w: 1280, h: 800 } late_ms=1
371:2026-11-01T14:30:22.062252Z  INFO focus space bar window=5 shown=true
427:2026-11-01T14:30:27.479239Z  INFO focus mode left window=5
428:2026-11-01T14:30:27.479260Z  INFO space shown index=0 spaces=2 desktop=1 focus=false
441:2026-11-01T14:30:27.963487Z  INFO space shown index=0 spaces=1 desktop=1 focus=false
501:2026-11-01T14:30:33.921846Z  INFO space shown index=1 spaces=2 desktop=2 focus=false
518:2026-11-01T14:30:35.544935Z  INFO space shown index=0 spaces=2 desktop=1 focus=false
```

### 6.2 `menubar-filtrado.txt` (completo)

```text
1:2026-11-01T14:29:42.184149Z  WARN Requesting name `com.canonical.AppMenu.Registrar` before setting up the object server. Method calls arriving before interfaces are registered may be lost. Consider using `connection::Builder::serve_at()` and `::name()` instead.
29:2026-11-01T14:29:42.806690Z  INFO menubar: app_menu none app_id=foot
30:2026-11-01T14:29:42.806733Z  INFO menubar: gtk_menus none app_id=foot
39:2026-11-01T14:29:42.888781Z  INFO menubar: app_menu none app_id=io.github.hblanqueto.malaysia.demo.ssd
40:2026-11-01T14:29:42.888824Z  INFO menubar: gtk_menus none app_id=io.github.hblanqueto.malaysia.demo.ssd
42:2026-11-01T14:29:42.908882Z  INFO menubar: app_menu none app_id=io.github.hblanqueto.malaysia.demo.csd
43:2026-11-01T14:29:42.908919Z  INFO menubar: gtk_menus none app_id=io.github.hblanqueto.malaysia.demo.csd
57:2026-11-01T14:29:46.198162Z  INFO menubar: app_menu none app_id=io.github.hblanqueto.malaysia.demo.parent
58:2026-11-01T14:29:46.198227Z  INFO menubar: gtk_menus none app_id=io.github.hblanqueto.malaysia.demo.parent
75:2026-11-01T14:29:47.463918Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.Calculator generation=6
76:2026-11-01T14:29:47.463952Z  INFO menubar: gtk_menus app_id=org.gnome.Calculator bus=:1.2825 menubar=/org/gnome/Calculator/menus/menubar appmenu=
78:2026-11-01T14:29:47.465106Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[0] menus=1
79:2026-11-01T14:29:47.465475Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[1] menus=5
80:2026-11-01T14:29:47.465582Z  INFO menubar: org.gtk.Menus root snapshot generation=6 bus=:1.2825 titles=1 ms=1
82:2026-11-01T14:29:47.511822Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.TextEditor generation=7
83:2026-11-01T14:29:47.511861Z  INFO menubar: gtk_menus app_id=org.gnome.TextEditor bus=:1.2826 menubar=/org/gnome/TextEditor/menus/menubar appmenu=
85:2026-11-01T14:29:47.515891Z  INFO menubar: org.gtk.Menus End bus=:1.2825 path=/org/gnome/Calculator/menus/menubar groups=[0, 1]
86:2026-11-01T14:29:47.527325Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/TextEditor/menus/menubar groups=[0] menus=0
87:2026-11-01T14:29:47.527372Z  INFO menubar: org.gtk.Menus root snapshot generation=7 bus=:1.2826 titles=0 ms=11
96:2026-11-01T14:29:54.270993Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.Calculator generation=8
97:2026-11-01T14:29:54.271036Z  INFO menubar: gtk_menus app_id=org.gnome.Calculator bus=:1.2825 menubar=/org/gnome/Calculator/menus/menubar appmenu=
99:2026-11-01T14:29:54.272110Z  INFO menubar: org.gtk.Menus End bus=:1.2826 path=/org/gnome/TextEditor/menus/menubar groups=[0]
100:2026-11-01T14:29:54.276038Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[0] menus=1
101:2026-11-01T14:29:54.276690Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[1] menus=5
102:2026-11-01T14:29:54.276829Z  INFO menubar: org.gtk.Menus root snapshot generation=8 bus=:1.2825 titles=1 ms=4
143:2026-11-01T14:30:09.238418Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.TextEditor generation=9
144:2026-11-01T14:30:09.238463Z  INFO menubar: gtk_menus app_id=org.gnome.TextEditor bus=:1.2826 menubar=/org/gnome/TextEditor/menus/menubar appmenu=
147:2026-11-01T14:30:09.239457Z  INFO menubar: org.gtk.Menus End bus=:1.2825 path=/org/gnome/Calculator/menus/menubar groups=[0, 1]
148:2026-11-01T14:30:09.244093Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/TextEditor/menus/menubar groups=[0] menus=0
149:2026-11-01T14:30:09.244129Z  INFO menubar: org.gtk.Menus root snapshot generation=9 bus=:1.2826 titles=0 ms=4
167:2026-11-01T14:30:09.721247Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.Calculator generation=10
168:2026-11-01T14:30:09.721290Z  INFO menubar: gtk_menus app_id=org.gnome.Calculator bus=:1.2825 menubar=/org/gnome/Calculator/menus/menubar appmenu=
170:2026-11-01T14:30:09.725838Z  INFO menubar: org.gtk.Menus End bus=:1.2826 path=/org/gnome/TextEditor/menus/menubar groups=[0]
171:2026-11-01T14:30:09.727773Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[0] menus=1
172:2026-11-01T14:30:09.728322Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[1] menus=5
173:2026-11-01T14:30:09.728464Z  INFO menubar: org.gtk.Menus root snapshot generation=10 bus=:1.2825 titles=1 ms=2
461:2026-11-01T14:30:27.479979Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.TextEditor generation=11
462:2026-11-01T14:30:27.480028Z  INFO menubar: gtk_menus app_id=org.gnome.TextEditor bus=:1.2826 menubar=/org/gnome/TextEditor/menus/menubar appmenu=
465:2026-11-01T14:30:27.481352Z  INFO menubar: org.gtk.Menus End bus=:1.2825 path=/org/gnome/Calculator/menus/menubar groups=[0, 1]
466:2026-11-01T14:30:27.487060Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/TextEditor/menus/menubar groups=[0] menus=0
467:2026-11-01T14:30:27.487091Z  INFO menubar: org.gtk.Menus root snapshot generation=11 bus=:1.2826 titles=0 ms=5
485:2026-11-01T14:30:27.965237Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.Calculator generation=12
486:2026-11-01T14:30:27.965302Z  INFO menubar: gtk_menus app_id=org.gnome.Calculator bus=:1.2825 menubar=/org/gnome/Calculator/menus/menubar appmenu=
488:2026-11-01T14:30:27.969486Z  INFO menubar: org.gtk.Menus End bus=:1.2826 path=/org/gnome/TextEditor/menus/menubar groups=[0]
489:2026-11-01T14:30:27.970686Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[0] menus=1
490:2026-11-01T14:30:27.971142Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[1] menus=5
491:2026-11-01T14:30:27.971272Z  INFO menubar: org.gtk.Menus root snapshot generation=12 bus=:1.2825 titles=1 ms=1
575:2026-11-01T14:30:34.169207Z  INFO menubar: org.gtk.Menus End bus=:1.2825 path=/org/gnome/Calculator/menus/menubar groups=[0, 1]
587:2026-11-01T14:30:35.545583Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.Calculator generation=14
588:2026-11-01T14:30:35.545633Z  INFO menubar: gtk_menus app_id=org.gnome.Calculator bus=:1.2825 menubar=/org/gnome/Calculator/menus/menubar appmenu=
590:2026-11-01T14:30:35.550383Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[0] menus=1
591:2026-11-01T14:30:35.550857Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[1] menus=5
592:2026-11-01T14:30:35.550991Z  INFO menubar: org.gtk.Menus root snapshot generation=14 bus=:1.2825 titles=1 ms=5
613:2026-11-01T14:30:38.195747Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.TextEditor generation=15
614:2026-11-01T14:30:38.195787Z  INFO menubar: gtk_menus app_id=org.gnome.TextEditor bus=:1.2826 menubar=/org/gnome/TextEditor/menus/menubar appmenu=
616:2026-11-01T14:30:38.197141Z  INFO menubar: org.gtk.Menus End bus=:1.2825 path=/org/gnome/Calculator/menus/menubar groups=[0, 1]
617:2026-11-01T14:30:38.201996Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/TextEditor/menus/menubar groups=[0] menus=0
618:2026-11-01T14:30:38.202026Z  INFO menubar: org.gtk.Menus root snapshot generation=15 bus=:1.2826 titles=0 ms=4
629:2026-11-01T14:30:41.603769Z  INFO menubar: org.gtk.Menus focus app_id=org.gnome.Calculator generation=16
630:2026-11-01T14:30:41.603824Z  INFO menubar: gtk_menus app_id=org.gnome.Calculator bus=:1.2825 menubar=/org/gnome/Calculator/menus/menubar appmenu=
632:2026-11-01T14:30:41.605579Z  INFO menubar: org.gtk.Menus End bus=:1.2826 path=/org/gnome/TextEditor/menus/menubar groups=[0]
633:2026-11-01T14:30:41.608775Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[0] menus=1
634:2026-11-01T14:30:41.609157Z  INFO menubar: org.gtk.Menus Start path=/org/gnome/Calculator/menus/menubar groups=[1] menus=5
635:2026-11-01T14:30:41.609259Z  INFO menubar: org.gtk.Menus root snapshot generation=16 bus=:1.2825 titles=1 ms=3
```

## 7. Errores, warnings y pánicos vistos

1. **Final2 (compositor.log):** solo
   `WARN backend_winit: xdg_toplevel_icon_manager_v1 is not supported`
   (esperado en el backend winit). Sin ERROR ni pánico en la ejecución final.
2. **`ERROR surface missing from known popups`** (error real del compositor, en
   ejecuciones suplementarias): aparece al destruirse un popup GTK durante una
   transición a fullscreen o al cerrar un menú de app. Ejemplos:
   - `/tmp/malaysia-diag/editor-c/malaysia-compositor.log:132`
     (`14:23:19.520388`, justo antes de `fullscreen_request`).
   - `/tmp/malaysia-diag/editor-final/malaysia-compositor.log:231`
     (`14:35:35.197898`, justo antes de `fullscreen_request`).
   - `/tmp/malaysia-diag/calc-bd/malaysia-compositor.log:235` y
     `/tmp/malaysia-diag/hamburgers/malaysia-compositor.log:147,155`.
   El popup se cierra correctamente, pero el compositor registra el error.
3. **`sync` fallido de Popup grab en menús de ventana:** en
   `/tmp/malaysia-diag/editor-c/...` aparece
   `rejected popup grab: no button press with this serial is held`. Es
   probablemente un artefacto del input guionizado (el botón se suelta a los
   150 ms), pero conviene saberlo: con pulsaciones muy cortas el grab del popup
   se rechaza; el menú se abre igualmente.
4. **Cliente:** ambos apps registran
   `Gtk-CRITICAL: Unable to connect to the accessibility bus at
   'unix:path=/run/user/1000/at-spi/bus_0': Connection refused`. Es del entorno
   (el bus a11y del host no acepta la conexión desde esta sesión), no del
   compositor. También salen `Adwaita-WARNING` por `style-dark.css` /
   `style-hc.css` obsoletos.
5. **Wallpaper:** `WARN InstanceFlags::VALIDATION requested, but unable to find
   layer: VK_LAYER_KHRONOS_validation` (no está instalada la capa de validación;
   no impide dibujar).
6. **Pánico de una sesión anterior, no de esta reproducción:**
   `/tmp/malaysia-diag/demo-contador.log` (283 MB) termina con
   `Io error: Broken pipe (os error 32)` y
   `thread 'main' (177007) panicked at
   crates/barrier-kit/examples/contador.rs:294:6:
   contador: EventLoop("calloop event loop failed")`.
   Es del `lab` anterior (07:37), no de nuestras ejecuciones. Los
   `demo-nautilus.log`, `demo-zenity.log`, `demo-gnome-calculator.log` y
   `demo-firefox.log` copiados también son de esa sesión anterior.

## 8. Capturas

En `capturas/` (también en `/tmp/malaysia-diag/capturas/`):

- `S1-calc-focus-baseline.png`: Calculator enfocada; barra con el ítem espurio
  `TextEditor`.
- `S2-A-b-doubleclick-maximize.png`: doble clic → maximizado clásico, barra y
  dock visibles.
- `S3-A-b-unmaximize.png`: restaurada tras el segundo doble clic.
- `S4-A-d-ctrl-super-f-focus.png`: modo foco correcto por `Ctrl+Super+F`.
- `S5-A-d-exit.png`: restaurada.
- `S6-B-calc-appmenu.png`: menú «Calculadora» abierto.
- `S7-A-e-menu-focus.png`: modo foco correcto por «Activar pantalla completa».
- `S8-B-focus-bar-revealed.png`: barra bajada en modo foco (gris, no negra;
  conserva los menús en esta ejecución).
- `S9-B-focus-bar-ventana-menu.png`: menú «Ventana» abierto sobre el foco.
- `S10-B-after-exit-focus.png`: tras salir del foco, barra con el ítem espurio.
- `S11-B-stale-texteditor-appmenu.png`: el ítem «TextEditor» abre el menú de
  Calculator.
- `S12-B-after-desktop-switch.png`: tras crear el escritorio 2 y volver.
- `S13-B-editor-focused.png`: TextEditor enfocado (`TextEditor · Ventana ·
  Ayuda`).
- `S14-B-calc-refocused.png`: Calculator reenfocada.
- `S15-B-calc-appmenu-after-refocus.png`: menú de Calculator tras reenfocar.
- `E1-editor-focus-baseline.png`: TextEditor enfocado.
- `E2-A-b-doubleclick-maximize.png`: doble clic → maximizado clásico (1280x708).
- `E3-A-b-after-unmaximize.png`: restaurado.
- `E4-A-c-client-fullscreen-broken.png`: fullscreen del cliente (menú
  «Pantalla completa»/F11) deja la ventana en 1024x548 sobre fondo negro.
- `E5-A-c-after-exit.png`: restaurado tras `Ctrl+Super+F`.

## Resolución (2026-11-01)

- **A(b) doble clic en la headerbar:** `apply_titlebar_double_click` (GTK
  `titlebar_gesture` y titlebar SSD) entra o sale del modo foco con
  `green_button = "focus"`; las ventanas de ancho fijo siguen estirándose en
  vertical. Commit `d92b64c`.
- **A(c) F11 / «Pantalla completa» del cliente a 1024x548:** carrera en
  `Windows::commit`. GTK dibuja un frame más a su tamaño viejo (al cerrar el
  popover) antes de confirmar el configure de fullscreen; el compositor miraba
  el estado comprometido (aún Normal), adoptaba ese tamaño y lo reenviaba con
  el estado fullscreen. Ya no sigue el tamaño del cliente si ha pedido
  fullscreen o maximizado. Commit `d92b64c`.
- **B ítem espurio «TextEditor»/«Escritorio» en la barra:** el menú salía de
  la ventana activa, pero el nombre con el que se titulaba el menú primario de
  libadwaita (y el título en negrita) salía de `active_app`, que puede ir por
  detrás al cambiar el foco; el menú no se plegaba en el título. Ahora ambos
  salen de la misma ventana.
- **Salida del foco ampliada y borrosa:** la app recibía el «ya no eres
  pantalla completa» al empezar la animación, redibujaba pequeña y esa imagen
  se estiraba al rectángulo grande. Ahora sigue en pantalla completa mientras
  se encoge (su imagen solo se reduce) y se la suelta al final; el compositor
  mantiene la imagen en su sitio hasta que llega el frame pequeño (300 ms
  máx., log `focus exit handed over`). La animación usa el rectángulo del
  contenido guardado al entrar.
- **Barra que cae al menú de escritorio en foco:** causa probable encontrada.
  En cada commit, si la ventana activa «no era visible» se borraba la ventana
  activa, y «visible» incluía que la geometría cupiera en el buffer: el frame
  transitorio de GTK al pasar a pantalla completa dejaba el escritorio sin
  ventana activa. Ahora solo se quita el foco a una ventana desmapeada,
  oculta, minimizada o sin buffer. Si vuelve a pasar, el log dice desde dónde:
  `active window cleared` (compositor, con `origin`) y
  `menubar: no active window, Escritorio menu` (barra).
- **`surface missing from known popups`:** lo registra Smithay, no Malaysia;
  es inofensivo. Detalle en `docs/deuda.md` (S-01).
