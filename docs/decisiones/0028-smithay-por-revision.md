# 0028 — Smithay se ancla a master por revisión

Estado: aceptado (2026-10-19). Reemplaza al ADR 0010.

## Contexto

El ADR 0010 ancló Smithay al release `0.7.0` de crates.io. El Bloque 11 necesita
`ext-image-copy-capture-v1` y `ext-image-capture-source-v1`, y el ADR 0020 quiere
`ext-background-effect-v1`; ninguno existe en `0.7.0`. La alternativa era el fork
`smithay-otto`, pero sus parches propios apuntan al renderer Vulkan de Otto y a
DRM/planos de Intel, mientras Yuki renderiza con GLES sobre una Vega (AMD). El fork
tampoco está basado en el master actual y obliga a seguir su rama.

## Decisión

Anclar Smithay a `master` por revisión:

```
https://github.com/Smithay/smithay
023b78b1c5a05ca512767cacc07efca55fae8be4
```

No se adopta `smithay-otto/neptunia`: no aporta nada al camino GLES/AMD de Yuki y
sus parches de Vulkan, i915 y planos no se usan. El fork queda solo como referencia
en `docs/investigacion/otto-y-forks.md`.

## Consecuencias

- `Cargo.toml` declara Smithay por `git` + `rev`; `Cargo.lock` fija la revisión.
- `flake.nix` lleva `cargoLock.outputHashes` para vendorizar el árbol de esa
  revisión.
- La API de la sección **Unreleased** obliga a migrar: `current_state()` pasa a
  `with_committed_state()`, los estados cacheados cambian de forma, los
  `delegate_*!` se sustituyen por `delegate_dispatch2!`, y
  `DataDeviceHandler`/`SelectionHandler` cambian firmas.
- El DnD iniciado por un cliente ya no lo instala Smithay: el compositor crea el
  `DnDGrab` en `WaylandDndGrabHandler::dnd_requested`. Yuki lo restaura con el
  mismo comportamiento que tenía con `0.7.0` (sin icono dibujado, que tampoco
  existía antes).
- Un bump de Smithay es un bump de `rev`, no de versión. No hay actualización
  automática ni docs.rs como autoridad: la fuente es el árbol anclado.
- No se añaden features nuevas en esta decisión; solo se desbloquean los módulos
  que el roadmap ya pedía.
