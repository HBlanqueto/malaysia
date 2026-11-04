# Incidencia 2026-11-04: menubar y dock invisibles por una unidad systemd obsoleta

## Resumen

Tras un reinicio, la barra superior (menubar) y el dock de Malaysia existían y
recibían clics, pero no se veían. No era un fallo de render del compositor: los
dos componentes del shell estaban corriendo un **build viejo** porque un symlink
manual en `~/.config/systemd/user` tapaba la unidad que instala el módulo NixOS.
Ese build era anterior al arreglo de barrierkit que dibuja el primer frame de una
capa. El compositor sí era el build nuevo.

La causa se confirmó con las rutas de `/proc/<pid>/exe`, el `FragmentPath` de
systemd y el contenido de las dos versiones de la unidad. El arreglo fue borrar
el symlink obsoleto, recargar systemd y reiniciar menubar y dock.

## Síntomas

- Menubar y dock no se veían, pero seguían ocupando su franja: los clics
  llegaban a la capa (`press landed on a layer surface`) y la interacción
  funcionaba.
- El wallpaper y las ventanas se veían con normalidad.
- Los logs del shell no mostraban errores ni crashes: solo mensajes normales de
  menú y `new layer surface layer=Top namespace=malaysia-menubar|malaysia-dock`.
- El compositor tampoco registraba errores de EGL, buffers ni protocolo.

## Entorno en el momento de la incidencia

Sesión de GDM con Malaysia sobre DRM, arrancada el 2026-11-04 a las 11:28:18.
Los hashes de `/nix/store` son de ese arranque y sirven solo como evidencia; en
otra generación cambiarán.

| Proceso | PID | Paquete en ejecución | Construido |
| --- | --- | --- | --- |
| `malaysia-compositor` | 2253 | `/nix/store/xhwwnd…-malaysia-0.1.0` | 2026-11-04 10:56:36 |
| `malaysia-shell menubar` | 2305 | `/nix/store/64nwr…-malaysia-0.1.0` | 2026-11-03 18:22:18 |
| `malaysia-shell dock` | 2304 | `/nix/store/64nwr…-malaysia-0.1.0` | 2026-11-03 18:22:18 |

El compositor estaba en la revisión `02ad964` de malaysia. El shell, en un build
del día anterior.

## Cronología

1. **2026-11-03 18:22** — Se ejecutó el `malaysia-drm.sh setup` de entonces. El
   script hacía `nix build` y creaba enlaces en `~/.config/systemd/user` hacia
   `~/src/malaysia/result`, que apuntaba al build recién hecho (`64nwr…`).
2. **2026-11-04 09:46 UTC** — barrierkit `6cb27e8`: «la capa dibuja su primer
   frame al recibir el primer configure».
3. **2026-11-04 09:56 UTC** — malaysia `4850035`: «build: barrierkit en 6cb27e8
   (la shell vuelve a verse)».
4. **2026-11-04 10:45 UTC** — malaysia `02ad964`: la shell arranca desde
   `MALAYSIA_BIN_DIR`; el script deja de enlazar unidades en el home.
5. **2026-11-04 10:56** — Se construye el paquete `xhwwnd…` con `02ad964` y se
   instala en el sistema.
6. **2026-11-04 11:28** — Reinicio. El compositor arranca desde `xhwwnd…`, pero
   menubar y dock arrancan desde `64nwr…` por el symlink viejo.
7. **2026-11-04 11:39–11:43** — Se borra el symlink, se recarga systemd y se
   reinician ambos componentes. Ahora corren desde `xhwwnd…` y vuelven a verse.

## Diagnóstico

### 1. Qué binario ejecutaba cada proceso

```sh
readlink /proc/2253/exe   # compositor -> /nix/store/xhwwnd…-malaysia-0.1.0/bin/…
readlink /proc/2305/exe   # menubar    -> /nix/store/64nwr…-malaysia-0.1.0/bin/…
readlink /proc/2304/exe   # dock       -> /nix/store/64nwr…-malaysia-0.1.0/bin/…
```

La discrepancia entre compositor y shell era la primera pista.

### 2. Qué unidad ganaba

```sh
systemctl --user show -p FragmentPath malaysia-shell@menubar.service
# /home/humbe/.config/systemd/user/malaysia-shell@.service
```

Ese archivo era un symlink:

```
~/.config/systemd/user/malaysia-shell@.service
  -> ~/src/malaysia/result/share/systemd/user/malaysia-shell@.service
  -> /nix/store/64nwr…-malaysia-0.1.0/share/systemd/user/malaysia-shell@.service
```

Mientras tanto, el módulo NixOS sí había instalado la unidad buena:

```sh
ls -l /etc/systemd/user/malaysia-shell@.service
# -> /nix/store/xhwwnd…-malaysia-0.1.0/lib/systemd/user/malaysia-shell@.service
```

### 3. Diferencia entre las dos unidades

Unidad vieja (`64nwr…`), la que corría:

```ini
[Service]
Type=simple
ExecStart=/nix/store/64nwr…-malaysia-0.1.0/bin/malaysia-shell %i
```

Unidad nueva (`xhwwnd…`), la que debía correr:

```ini
[Service]
Type=simple
# The binary comes from MALAYSIA_BIN_DIR, which the compositor imports, so it always
# matches the running compositor; without it, from this package.
ExecStart=/bin/sh -c 'exec "$${MALAYSIA_BIN_DIR:-/nix/store/xhwwnd…-malaysia-0.1.0/bin}/malaysia-shell" "$$1"' sh %i
```

El user manager ya tenía la variable correcta:

```sh
systemctl --user show-environment | grep MALAYSIA_BIN_DIR
# MALAYSIA_BIN_DIR=/nix/store/xhwwnd…-malaysia-0.1.0/bin
```

Pero la unidad vieja **ignoraba** la variable porque fijaba la ruta en el
`ExecStart`. Por eso el arreglo de `02ad964` no surtió efecto por sí solo: hacía
falta que la unidad nueva fuese la que systemd leyera.

### 4. Por qué el build viejo no dibujaba

El build `64nwr…` es anterior a barrierkit `6cb27e8` y a malaysia `4850035`, que
son justamente el arreglo del primer frame de una capa. Una superficie de capa
puede existir, tener región de entrada y recibir clics aunque el cliente no haya
presentado todavía su primer buffer: se ve el fondo, no la capa. Eso explica el
síntoma exacto «existe y se puede interactuar pero es invisible», y también que
no hubiera errores en los logs.

## Causa raíz

Se combinaron tres cosas:

1. **Precedencia de systemd.** Las unidades de usuario en
   `~/.config/systemd/user` tienen prioridad sobre las de `/etc/systemd/user`.
   Un archivo en el home tapa la unidad del sistema para siempre.
2. **Un symlink a un artefacto de desarrollo.** El `setup` viejo enlazaba la
   unidad a `~/src/malaysia/result`, un symlink que solo cambia cuando se ejecuta
   `nix build`, no cuando se hace `nixos-rebuild switch`. El enlace quedó fijado
   al build del 3 de noviembre.
3. **`ExecStart` con ruta fija.** La unidad vieja no consultaba
   `MALAYSIA_BIN_DIR`; aunque el compositor y el user manager tuvieran la ruta
   nueva, el shell seguía arrancando el binario viejo.

No fue un problema de NixOS, del módulo ni del compositor: el módulo instalaba
la unidad correcta en `/etc/systemd/user` en cada generación. El problema era el
override manual en el home.

## Arreglo aplicado

```fish
rm ~/.config/systemd/user/malaysia-shell@.service
systemctl --user daemon-reload
systemctl --user restart malaysia-shell@menubar.service
systemctl --user restart malaysia-shell@dock.service
```

Nota operativa: hay que reiniciar **los dos** componentes y comprobar cada PID.
En esta incidencia el primer reinicio se interrumpió y el dock siguió con el
proceso viejo hasta que se reinició aparte.

Verificación:

```sh
systemctl --user show -p FragmentPath malaysia-shell@menubar.service
# /etc/systemd/user/malaysia-shell@.service

ps -eo pid,cmd | grep malaysia-shell
# menubar y dock desde /nix/store/xhwwnd…-malaysia-0.1.0/bin/malaysia-shell
```

El compositor registró las nuevas capas (`namespace=malaysia-menubar` y
`namespace=malaysia-dock`) y ambas volvieron a verse.

## Cómo prevenirlo

### Opción recomendada: que las unidades sean solo del módulo NixOS

`programs.malaysia.enable = true` hace `systemd.packages = [ cfg.package ]`, así
que las unidades viven en `/etc/systemd/user` y se actualizan con cada
`nixos-rebuild switch`. **No crear unidades de estos componentes en
`~/.config/systemd/user`.** El `malaysia-drm.sh setup` actual ya no las crea
(`02ad964`); el riesgo es usar un checkout viejo del script o un enlace manual
antiguo.

### Para builds de desarrollo: `MALAYSIA_BIN_DIR`

La unidad nueva resuelve el binario en tiempo de arranque:

```ini
ExecStart=/bin/sh -c 'exec "${MALAYSIA_BIN_DIR:-<paquete>}/malaysia-shell" "$1"' sh %i
```

El compositor importa `MALAYSIA_BIN_DIR` al user manager. Así, si se arranca un
compositor de desarrollo con `malaysia-drm.sh run`, el shell sale del mismo
paquete sin tocar unidades ni symlinks. Es el mecanismo pensado para evitar la
discrepancia compositor/shell.

### Limpieza en máquinas existentes

Borrar solo enlaces que apunten al `result` del repo, nunca unidades propias:

```fish
for u in ~/.config/systemd/user/malaysia-*.service
    test -L $u; or continue
    string match -q "$HOME/src/malaysia/result/*" (readlink -f $u); and rm -f $u
end
systemctl --user daemon-reload
```

### Guard automático (pendiente, opcional)

Dos sitios posibles:

- `malaysia-drm.sh setup`: borrar enlaces viejos que apunten a `$REPO/result`
  antes de terminar. Es el script que los creaba; debería limpiarlos.
- `home.activation` en dotfiles (se usa home-manager): eliminar enlaces
  obsoletos en cada activación.

También se puede añadir un aviso al arrancar el compositor si detecta una unidad
de shell en `~/.config/systemd/user` que tape la del sistema.

### Diagnóstico rápido

```sh
systemctl --user show -p FragmentPath <unidad>   # qué archivo gana
systemctl --user cat <unidad>                    # contenido efectivo
systemd-analyze --user unit-paths                # orden de búsqueda
readlink /proc/<pid>/exe                         # binario real en ejecución
systemctl --user show-environment | grep MALAYSIA
```

## Referencias

- malaysia `02ad964` — «fix(session): la shell arranca siempre del paquete del
  compositor en marcha».
- malaysia `4850035` — «build: barrierkit en 6cb27e8 (la shell vuelve a verse)».
- barrierkit `6cb27e8` — «fix(kit): la capa dibuja su primer frame al recibir el
  primer configure».
- `nix/malaysia.nix` — `systemd.packages = [ cfg.package ]` y el target
  `malaysia-session`.
- `share/systemd/user/malaysia-shell@.service` — unidad que resuelve
  `MALAYSIA_BIN_DIR`.

## Contexto para continuar

Estado actual: las cuatro unidades de Malaysia resuelven desde
`/etc/systemd/user` y los dos componentes del shell corren desde el mismo
paquete que el compositor. La incidencia está cerrada.

Ideas abiertas para una sesión de Claude:

1. Añadir a `cmd_setup` de `malaysia-drm.sh` la limpieza de enlaces obsoletos
   hacia `$REPO/result`.
2. Añadir un `home.activation` en dotfiles que haga lo mismo en cada rebuild.
3. Añadir al compositor un aviso (log) si detecta una unidad de shell en
   `~/.config/systemd/user` que tape la del sistema.
4. Valorar si conviene exponer el estado de las unidades en `malaysia-drm.sh
   status` (por ejemplo, `FragmentPath` y binario de cada componente).
