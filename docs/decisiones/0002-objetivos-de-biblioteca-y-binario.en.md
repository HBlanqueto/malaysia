# 0002 — Expose `crates/yuki` as a library and as a binary at the same time

Status: accepted (Block 0).

## Context

A binary-only crate has no external interface: `dead_code` punishes unused `pub` items, and neither `tests/` nor doctests can import its modules.

## Decision

One package with `src/lib.rs` (reusable modules) and `src/main.rs` (startup that uses the library). `lib.rs` carries `#![deny(unsafe_code)]` and `#![warn(missing_docs)]`. `cli` and `logging` belong to the binary.

## Consequences

Future modules are testable without `#[allow(dead_code)]`; the tree gains `src/lib.rs`, a deliberate deviation.

## Rejected alternatives

- Binary only: it required fake uses or a compile-fail test that never runs.
