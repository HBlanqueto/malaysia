# Documentación de Malaysia

Índice. El estado vive en [`roadmap.md`](roadmap.md); el porqué, en los ADR;
los hallazgos abiertos, en [`deuda.md`](deuda.md).

## Mapa

```
docs/
├── README.md            este índice
├── roadmap.md           estado por bloques y «Ahora»
├── deuda.md             deuda técnica abierta
├── glossary.md          nombres del proyecto (BarrierKit, Grade, Infinity)
├── hardware.md          línea base de GPU/EGL de la máquina
├── investigacion.md     conclusiones macOS/GNOME/Otto
├── arquitectura/        cómo es la UI y el shell HOY
│   ├── interfaz.md          mapa visual completo
│   ├── modelo-de-procesos.md  procesos y reparto
│   ├── ventanas.md          titlebar, marco y decoración
│   ├── wallpaper.md         malaysia-wallpaper (wgpu/Vulkan)
│   ├── menubar.md           barra superior y menús
│   ├── dock.md              dock
│   ├── lock.md              pantalla de bloqueo
│   ├── capturas.md          capturas de pantalla
│   └── malaysia-y-shell.svg diagrama del reparto
├── decisiones/          ADRs
├── configuracion/
    └── opciones.md      catálogo canónico de opciones
```

## Compilar y probar

El dev shell es obligatorio para cargo (ADR 0007):

```sh
nix develop
./dev.sh build            # debug de los miembros por defecto
./dev.sh run              # compositor anidado
./dev.sh demo             # sesión anidada con shell y ventanas de prueba
./dev.sh lab              # sesión anidada con apps reales
./dev.sh check            # fmt + clippy + tests
./dev.sh titlebar         # cerrar/minimizar/maximizar
./dev.sh smoke            # mapeo de toplevels
./dev.sh drm-vkms         # Direct Session sobre vkms
nix build .#default       # paquete instalable
```

El día a día usa el perfil `dev`: nuestro código sin optimizar, dependencias a
`opt-level = 2` y `debug = "line-tables-only"`. El paquete Nix usa `dist` (fat
LTO, `codegen-units = 1`) y separa `buildDepsOnly` de `buildPackage` con crane.
Skia llega precompilada de `nix/skia.nix` por `SKIA_BINARIES_URL`.


## Estado

Los bloques 0–15 están hechos o aceptados, salvo los pendientes que el roadmap
detalla; el 16a (fondo en Vulkan) está en curso. Ahora mismo faltan el modo foco,
la captura de ventana con marco, el resto del menú global (14b-2/14e/14f-2), el
portal 12a (`Screenshot`/`ScreenCast`), los servicios 12b, 8b/8c y las opciones
NixOS de polkit/keyring/AT-SPI. Para el detalle, fechas y comandos de prueba,
lee [`roadmap.md`](roadmap.md).
