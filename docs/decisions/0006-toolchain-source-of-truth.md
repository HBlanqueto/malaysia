# 0006 — The flake is the one toolchain source of truth

Status: accepted (closes Block 0 open question 4)

## Context

Two files declared a Rust version. `rust-toolchain.toml` pinned `1.98.1` and
listed rustfmt and clippy; the flake's devShell supplied `cargo` and `rustc`
from nixpkgs. Two declarations of one version drift apart as soon as either
moves, and the symptom is a compiler error that looks inexplicable because the
toolchain that produced it is not the one the file claims.

The situation on the development machine is sharper than the general case:
there is no `rustc` and no `rustup` on the `PATH` outside the dev shell. The
`rust-toolchain.toml` pin cannot be honoured at all, because the program that
would read it — rustup — is not installed and the dev shell's `rustc` is a
plain nixpkgs build, not a rustup proxy. The file was documentation pretending
to be configuration.

`nix develop -c rustc --version` and a plain `rustc --version` run inside the
shell both report `rustc 1.98.1 (48a229cea 2026-09-01)`, so they agree by
construction: they are the same binary.

## Options considered

1. **`rust-toolchain.toml` wins; the devShell follows it.** This would require
   rustup or a `rust-overlay`/`fenix` input so the shell installs exactly the
   pinned release. It adds a flake input and a second toolchain mechanism to a
   project with one machine, and it would still not be honoured outside the
   shell without rustup, which is not installed.
2. **The flake wins; keep `rust-toolchain.toml` as a mirror plus a check.** Two
   sources remain and can still disagree; the best on offer is detecting drift
   after it happens, not preventing it.
3. **The flake wins; delete `rust-toolchain.toml`.** One source: the nixpkgs
   `cargo`/`rustc` pinned by `flake.lock`. Cargo's own `rust-version` in the
   workspace manifest remains as the minimum-supported-version floor that cargo
   enforces during resolution, which is a different concern from the toolchain
   used to build.

## Decision

The flake is the single source of truth for the toolchain (option 3).
`rust-toolchain.toml` is removed. The dev shell provides `cargo`, `rustc`,
`rustfmt` and `clippy` from nixpkgs, and `flake.lock` pins the exact nixpkgs
revision. `workspace.package.rust-version = "1.98"` stays as the MSRV floor and
must not exceed the flake's rustc version.

### Bumping the toolchain

1. Update the nixpkgs input: `nix flake update nixpkgs` (or
   `nix flake lock --update-input nixpkgs`).
2. Confirm the new versions: `nix develop -c rustc --version` and
   `nix develop -c cargo --version`.
3. If the new release raises the minimum the code needs, update
   `workspace.package.rust-version` in the same change.
4. Run `./dev.sh check` and commit `flake.lock` (and `Cargo.toml` if the MSRV
   moved) together.

## Consequences

- There is exactly one place a toolchain version is chosen; `flake.lock`
  records it and a version bump is a flake-input bump.
- Editors and tooling must run inside the dev shell (ADR 0005). There is no
  rustup pin to fall back on, which is already the situation on this machine.
- `cargo` and `rustc` are not always the same point release — nixpkgs shipped
  `cargo 1.98.0` alongside `rustc 1.98.1` here. They travel as a set; the
  release line, not the patch number, is what the project reasons about.
- The MSRV floor in `Cargo.toml` describes what the code requires, not what is
  installed, and the two must be kept consistent when either moves.
