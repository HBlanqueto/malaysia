# 0008 — Design tokens flow in one direction, from Nix to Rust

Status: superseded by ADR 0011. It stays as a record of what was rejected: the tokens ended up as hand-written Rust constants in `yuki-design`, not generated from Nix.

## Context

Without tokens, the same color or radius is copied in several places.

## Decision

`design/v1.nix` owned the values; a generator emitted a single versioned Rust file in the design crate; only that crate re-exported the tokens; no other crate read Nix or the generated file; hand-written design constants were not allowed.

## Consequences

The generator and the staleness check were never written; ADR 0011 reversed the decision.

## Rejected alternatives

- Generating into `OUT_DIR` with `build.rs`: design diffs stop being visible.
