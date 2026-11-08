# 0028 — Smithay is pinned to master by revision

Status: accepted (2026-10-19). Replaces ADR 0010.

## Context

Block 11 needs `ext-image-copy-capture-v1` and `ext-image-capture-source-v1`, which do not exist in 0.7.0.

## Decision

Pin Smithay to `master` by revision:

```
https://github.com/Smithay/smithay
65aa86fa5236e41775c313408102a5d5c2d1e2ca
```

`smithay-otto` is not adopted; it remains only as a reference.

## Consequences

`Cargo.toml` uses `git` + `rev`; `Cargo.lock` pins the revision and `flake.nix` carries `cargoLock.outputHashes`.

## Rejected alternatives

- Staying on 0.7.0: no capture and no background-effect.
