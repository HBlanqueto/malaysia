# Documentación de MalaysiaDreams

Índice de la documentación. El estado vive **solo** en
[`roadmap.md`](roadmap.md); el porqué de cada decisión, en los ADR; los
hallazgos abiertos, en [`deuda.md`](deuda.md).

## Mapa

```
docs/
├── README.md            este índice
├── roadmap.md           estado por bloques y sección «Ahora»
├── deuda.md             deuda técnica abierta (id · dónde · qué · lote)
├── glossary.md          nombres del proyecto (BarrierKit, Grade, Infinity)
├── hardware.md          línea base de GPU/EGL de la máquina
├── investigacion.md     conclusiones de macOS, GNOME y Otto
├── arquitectura/        cómo es la UI y el shell HOY
│   ├── interfaz.md          mapa visual completo
│   ├── modelo-de-procesos.md  procesos y reparto compositor ↔ shell
│   ├── ventanas.md          titlebar, marco y decoración
│   ├── wallpaper.md         malaysia-wallpaper (wgpu/Vulkan)
│   ├── menubar.md           barra superior y menús
│   ├── dock.md              dock
│   ├── lock.md              pantalla de bloqueo
│   ├── capturas.md          capturas de pantalla
│   └── malaysia-y-shell.svg diagrama del reparto
├── decisiones/          ADRs (índice en decisiones/README.md)
├── configuracion/
│   └── opciones.md      catálogo canónico de opciones
└── mockups/             lienzos de diseño; no se editan a mano
```

El toolkit (`barrier-kit`, `vow-design`, `vow-appearance`,
`infinity-protocol`) vive en
[`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit)
desde el ADR 0048; su documentación y sus ADR están allí.

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

### Regenerar Skia al subir Freya

1. Actualiza `nix/skia-prebuild/Cargo.toml` y regenera su `Cargo.lock`.
2. Lee la etiqueta de Skia del nuevo `freya-skia-bindings` y actualiza `tag` y
   `hash` de `skia-source` en `nix/skia.nix`.
3. Regenera `nix/skia-externals.json` desde el `DEPS` de esa etiqueta y el hash
   de `nix/skia-deps.nix` si cambiaron las librerías del sistema.
4. Ajusta el guard de `flake.nix` y compila `nix build .#default`.

## Estado

Los bloques 0–15 están hechos o aceptados, salvo los pendientes que el roadmap
detalla; el 16a (fondo en Vulkan) está en curso. Ahora mismo faltan el modo foco,
la captura de ventana con marco, el resto del menú global (14b-2/14e/14f-2), el
portal 12a (`Screenshot`/`ScreenCast`), los servicios 12b, 8b/8c y las opciones
NixOS de polkit/keyring/AT-SPI. Para el detalle, fechas y comandos de prueba,
lee [`roadmap.md`](roadmap.md).

## Cómo leerlo

1. [`roadmap.md`](roadmap.md) — qué hay, qué falta y cómo se prueba.
2. [`arquitectura/interfaz.md`](arquitectura/interfaz.md) — mapa visual.
3. [`glossary.md`](glossary.md) — nombres y equivalencias con macOS.
4. [`decisiones/README.md`](decisiones/README.md) — ADR y decisiones abiertas.
5. [`configuracion/opciones.md`](configuracion/opciones.md) — catálogo de
   opciones y archivos `.domain`.
6. [`investigacion.md`](investigacion.md) — por qué se tomó cada idea de macOS,
   GNOME y Otto.
7. [`AGENTS.md`](../AGENTS.md) — política de diseño y reglas para agentes.
