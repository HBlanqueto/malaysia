# 0048 — BarrierKit en su propio repositorio

Estado: **aceptado** (2026-10-28).

## Contexto

BarrierKit vive dentro del workspace de MalaysiaDreams (`crates/barrier-kit`), junto con el compositor y la shell. El ADR 0047 lo convierte en un toolkit completo para construir cualquier app, a la manera de AppKit o GTK + libadwaita. Un toolkit así tiene su propio ciclo de vida, su app de muestra y sus decisiones, y no debería compartir repositorio con el compositor.

GNOME y macOS separan estas piezas del mismo modo:

| GNOME | macOS | MalaysiaDreams |
|---|---|---|
| GTK + libadwaita (con Adwaita y `AdwStyleManager`) | AppKit (con Aqua y `NSAppearance`) | **barrierkit**: barrier-kit + Tengen + vow-appearance |
| gtk4-widget-factory (en el repo de GTK) | — | Master Tengen App Show (en barrierkit) |
| Mutter | WindowServer / SkyLight | malaysia-compositor |
| gnome-shell | Dock, barra de menús | malaysia-shell |
| gnome-control-center | Ajustes del sistema | malaysia-settings |
| wayland-protocols | — | protocolo Infinity |

En los dos casos, **el toolkit se lleva su estilo y la lectura de la apariencia del sistema**. Si `vow-design` y `vow-appearance` se quedaran en malaysia, barrierkit dependería de malaysia y malaysia de barrierkit: un ciclo.

## Decisión

### 1. Reparto de repositorios

**`github.com/HBlanqueto/barrierkit`** (MIT) es la librería:

```
barrierkit/
  Cargo.toml                  workspace propio, mismas versiones fijadas (freya-* =0.4.3, SCTK…)
  crates/barrier-kit/         el toolkit (ADR 0034, 0047), con la feature `shell`
  crates/vow-design/          Tengen: tengen/tokens.json, components.json → constantes Rust
  crates/vow-appearance/      appearance.domain + fondo → paleta resuelta; watcher
  crates/infinity-protocol/   protocols/infinity-window-manager-v1.xml + código generado,
                              features `client` (kit) y `server` (compositor)
  apps/tengen-app-show/       Master Tengen App Show
  docs/                       decisiones del kit, auditorías del kit, glosario del kit
  flake.nix, nix/skia*        Skia precompilado para freya-skia-bindings 0.98.0 + su guardia
  scripts/                    check-client-only-deps, check-design-tokens (parte kit),
                              check-tengen-generated, mockup-extract
```

**`github.com/HBlanqueto/malaysia`** es el escritorio:

```
malaysia/
  crates/malaysia-compositor, malaysia-scene, grade-graphics
  crates/malaysia-shell        (barrier-kit con features = ["shell"])
  crates/malaysia-settings     (Ajustes del sistema, se queda aquí)
  crates/xdg-desktop-portal-malaysia
  protocols/gtk-shell.xml      (solo lo usa el compositor)
  flake.nix, nix/malaysia.nix, dev.sh, share/, docs/ del escritorio
```

### 2. Cómo consume malaysia el kit

Como cualquier librería Rust de otro repositorio, igual que Smithay (ADR 0028):

```toml
# malaysia/Cargo.toml, [workspace.dependencies]
barrier-kit       = { git = "https://github.com/HBlanqueto/barrierkit", rev = "<hash>" }
vow-design        = { git = "https://github.com/HBlanqueto/barrierkit", rev = "<hash>" }
vow-appearance    = { git = "https://github.com/HBlanqueto/barrierkit", rev = "<hash>" }
infinity-protocol = { git = "https://github.com/HBlanqueto/barrierkit", rev = "<hash>" }
```

- Los cuatro llevan **el mismo `rev`**. Actualizar el kit en el escritorio es un commit de malaysia que cambia ese hash: `chore(deps): barrierkit <hash>`.
- **Desarrollo conjunto.** Con `~/src/barrierkit` junto a `~/src/malaysia`, `./dev.sh kit-local` escribe un `.cargo/config.toml` **no versionado** con `[patch."https://github.com/HBlanqueto/barrierkit"]` apuntando a las rutas locales, y `./dev.sh kit-remote` lo borra. Así se prueba un cambio del kit en la sesión anidada antes de publicarlo.
- **Nix.** barrierkit es un input del flake de malaysia. crane vendoriza la dependencia git con su `outputHashes`, igual que Smithay. Skia precompilado pasa a ser un output de barrierkit (`packages.skia-binaries`) y malaysia lo reutiliza: hay una sola guardia de versión, en barrierkit.

### 3. Lo que no cambia

- El compositor sigue sin enlazar Freya ni Skia. Solo usa `vow-design`, `vow-appearance` e `infinity-protocol` (feature `server`), ninguno de los cuales depende de Freya. Cargo compila únicamente los crates que se piden aunque compartan repo. `malaysia/scripts/check-client-only-deps.sh` lo sigue comprobando.
- Los nombres de crates, módulos y tipos se mantienen. Solo cambia de dónde vienen.
- Una sola rama `main` en cada repo.

### 4. Historial y documentación

- `crates/barrier-kit`, `crates/vow-design` y `crates/vow-appearance` pasan con **su historial** (`git filter-repo`). En malaysia se borran con un commit que remite a barrierkit.
- **Numeración de ADR única para los dos repos.** Una decisión nueva toma el siguiente número libre esté donde esté, para que «ADR 0047» signifique lo mismo en ambos. Las ADR del kit (0011, 0032, 0033, 0034, 0035, 0039, 0041, 0046, 0047) se mueven a `barrierkit/docs/decisiones/`. En malaysia queda un archivo de una línea por cada una que remite a la nueva ubicación. Esta ADR (0048) existe en los dos repos.
- Las auditorías y el triaje de la P4 (BarrierKit) se mueven a barrierkit.

### 5. Orden

1. Cerrar y subir el lote L1b en malaysia, como está.
2. Lote de separación, en un solo paso:
   - crear barrierkit con historial;
   - crear `infinity-protocol`;
   - pasar malaysia a la dependencia git;
   - ajustar los flakes y `dev.sh`.
3. L1c, L1d y todas las fases del ADR 0047 se hacen en barrierkit. Cada lote que el escritorio necesite termina con un commit en malaysia que sube el `rev`.

## Consecuencias

- Cambiar el kit y usarlo en el escritorio son dos commits en dos repos, aunque `dev.sh kit-local` permite probarlos juntos antes.
- barrierkit se puede compilar, testear y enseñar (Master Tengen App Show) sin el compositor. Su CI no necesita Smithay.
- Los informes de opencode que toquen los dos repos deben citar el commit de cada uno.

## Referencias

- [`0028`](0028-smithay-por-revision.md): dependencia git con rev fijo.
- [`0034`](0034-celestialkit-sobre-freya.md): BarrierKit sobre Freya.
- [`0047`](0047-barrierkit-en-capas.md): BarrierKit en capas.
