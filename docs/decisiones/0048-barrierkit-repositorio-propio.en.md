# 0048 — BarrierKit in its own repository

Status: accepted (2026-10-28).

## Context

BarrierKit, Tengen (`vow-design`) and `vow-appearance` lived in the desktop workspace; a complete toolkit (ADR 0047) has its own life cycle and must not be coupled to the compositor.

## Decision

1. **Split.** `github.com/HBlanqueto/barrierkit` (MIT) contains `barrier-kit`, `vow-design` (Tengen), `vow-appearance`, `infinity-protocol`, `apps/tengen-app-show`, their docs, flake, Skia and scripts. `malaysia` keeps the compositor, scene, `grade-graphics`, shell, Settings, portal, `protocols/gtk-shell.xml`, flake, `dev.sh`, `share/` and docs.
2. **Git dependency** with the same `rev` for the four crates, like Smithay (ADR 0028). Updating the kit is `chore(deps): barrierkit <hash>`.
3. **Joint development:** `./dev.sh kit-local` writes an unversioned `.cargo/config.toml` with a `[patch]` to `~/src/barrierkit`; `kit-remote` deletes it.
4. **Nix:** barrierkit is a flake input; crane vendors it with `outputHashes`. Skia is a barrierkit output and malaysia reuses it.
5. **What does not change:** the compositor does not link Freya or Skia; it only uses `vow-design`, `vow-appearance` and `infinity-protocol` (server), which do not depend on Freya.
6. **History and ADRs:** the crates move with `git filter-repo`; a single numbering across repos and a stub here for the moved ADRs. This ADR exists in both.
7. **Order:** close L1b; split in one step; then L1c/L1d and ADR 0047 in barrierkit.

## Consequences

Changing the kit and using it are two commits, although `kit-local` lets you test them together.

## Rejected alternatives

- Keeping the kit in the same repo: a dependency cycle and coupled rhythms.
