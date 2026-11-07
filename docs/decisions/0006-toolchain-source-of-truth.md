# 0006 — The flake is the single source of truth for the toolchain

Status: accepted (closes open question 4 of Block 0).

## Context

`rust-toolchain.toml` and the devShell declared Rust versions that could diverge; without rustup on the machine, the file was documentation pretending to be configuration.

## Decision

The flake is the single source. `rust-toolchain.toml` is removed; the devShell provides `cargo`, `rustc`, `rustfmt` and `clippy` from the nixpkgs pinned by `flake.lock`. `workspace.package.rust-version` stays as the MSRV floor and must not exceed the flake's version. To bump the toolchain: update nixpkgs, check versions, adjust the MSRV if needed, run `./dev.sh check` and commit `flake.lock` (and `Cargo.toml`) together.

## Consequences

A toolchain bump is a bump of the flake input; editors and tools run in the devShell (ADR 0005).

## Rejected alternatives

- `rust-toolchain.toml` with rust-overlay/fenix: a second mechanism, and it does not work without rustup.
