# 0028 — Smithay se ancla a master por revisión

Estado: aceptado (2026-10-19). Reemplaza al ADR 0010.

## Contexto

El Bloque 11 necesita `ext-image-copy-capture-v1` y
`ext-image-capture-source-v1`, que no existen en 0.7.0. El fork `smithay-otto`
apunta a Vulkan/Intel y no aporta al camino GLES/AMD.

## Decisión

Anclar Smithay a `master` por revisión:

```
https://github.com/Smithay/smithay
023b78b1c5a05ca512767cacc07efca55fae8be4
```

No se adopta `smithay-otto`; queda solo como referencia.

## Consecuencias

- `Cargo.toml` usa `git` + `rev`; `Cargo.lock` fija la revisión y `flake.nix`
  lleva `cargoLock.outputHashes`.
- La API de la sección Unreleased obliga a migrar: `with_committed_state()`,
  estados cacheados, `delegate_dispatch2!` y firmas de DnD/selección.
- El DnD iniciado por cliente lo crea el compositor en `dnd_requested`.
- Un bump es un bump de `rev`; el árbol anclado es la autoridad, no docs.rs.

## Alternativas descartadas

- Quedarse en 0.7.0: sin captura ni background-effect.
- `smithay-otto`: parches Vulkan/Intel y rama ajena al master actual.
