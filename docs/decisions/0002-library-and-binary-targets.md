# 0002 — Expose `crates/neptune` as both a library and a binary

Status: accepted (Block 0)

## Context

The requested layout for `crates/neptune` lists only a binary entry point
(`main.rs`, `cli.rs`, `logging.rs`, `geometry.rs`): a binary-only crate. Modern
Rust, however, treats a binary crate as having no external interface, and
therefore applies the `dead_code` lint to `pub` items that nothing in the crate
reaches. A binary-only crate also cannot be exercised by integration tests or
doctests: `tests/` cannot import the binary's modules, and `cargo test` does not
run doctests for binary targets.

This reasoning was originally applied to the coordinate types, which were
public API not yet consumed. Since ADR 0001, those types have moved to the
`neptune-scene` crate, so the compositor's library target is currently empty.
It is kept because the compositor will grow library-shaped building blocks
(state, scene-boundary conversion, tokens) that need to be testable, and
because removing and re-adding the target as those land would be churn.

## Options considered

1. **Binary only.** Match the listed file set exactly, but add a fake use of
   geometry from `main` to silence `dead_code`, and settle for a commented
   compile-fail example. The comment would not actually run.
2. **Library and binary in one package.** Add `src/lib.rs` (the reusable
   compositor) next to `src/main.rs` (a thin startup/shutdown shell), as niri
   does. `pub` items become part of the library interface, and doctests run.
3. **A separate `neptune-core` crate for geometry.** Correct but premature: it
   splits the project into three crates before there is anything to separate.

## Decision

Option 2. `crates/neptune/src/lib.rs` declares the modules that form the
library, and `crates/neptune/src/main.rs` uses the library crate
(`neptune::...`) for reusable pieces. The library carries
`#![deny(unsafe_code)]` and `#![warn(missing_docs)]` (see ADR 0003).

At the time of writing the library exposes no modules, because its only
candidate (`geometry`) moved to `neptune-scene` in ADR 0001. `cli` and
`logging` are binary modules: they are startup concerns, not reusable building
blocks.

## Consequences

- Future library-shaped modules are testable and exempt from the binary
  crate's `dead_code` strictness; `clippy -D warnings` stays green without an
  artificial use or an `#[allow(dead_code)]`.
- The listed directory structure gains one file, `src/lib.rs`. This is a
  deliberate deviation from the Block 0 brief and is flagged for review.
- Startup-only modules (`cli`, `logging`) live in the binary, while reusable
  building blocks live in the library. The split is a convention, not an
  enforced boundary.
- The compile-fail guarantee for coordinate mixing moved with `geometry.rs` to
  `neptune-scene` (ADR 0001); it is no longer a doctest in this crate.
