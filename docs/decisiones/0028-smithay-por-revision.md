# 0028 — Smithay se ancla a master por revisión

Estado: aceptado (2026-10-19). Reemplaza al ADR 0010.

## Contexto

El Bloque 11 necesita `ext-image-copy-capture-v1` y `ext-image-capture-source-v1`, que no existen en 0.7.0.

## Decisión

Anclar Smithay a `master` por revisión:

```
https://github.com/Smithay/smithay
023b78b1c5a05ca512767cacc07efca55fae8be4
```

No se adopta `smithay-otto`; queda solo como referencia.

## Consecuencias

`Cargo.toml` usa `git` + `rev`; `Cargo.lock` fija la revisión y `flake.nix` lleva `cargoLock.outputHashes`.

## Alternativas descartadas

- Quedarse en 0.7.0: sin captura ni background-effect.
