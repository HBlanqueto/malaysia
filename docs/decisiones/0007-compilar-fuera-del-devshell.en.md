# 0007 — Building outside the development shell is forbidden

Status: accepted (closes open question 5 of Block 0; the guard moved into the build).

## Context

Without the shell, `RUSTFLAGS` with mold is missing: the binary links differently and warms a different cache.

## Decision

The shell exports `YUKI_DEV_SHELL=1`. `build.rs` refuses to build unless it is `1`, prints the `dev.sh` message and emits `cargo:rerun-if-env-changed=YUKI_DEV_SHELL`. `dev.sh` keeps `require_devshell` for `build`, `check` and `clean` as an early failure; `run`, `stop`, `restart`, `status` and `log` work outside.

The Nix derivation sets the marker on purpose and wraps the binaries with `makeWrapper`: it sets `__EGL_VENDOR_LIBRARY_DIRS`, `LIBGL_DRIVERS_PATH`, `GBM_BACKENDS_PATH` and `LD_LIBRARY_PATH`, and saves the previous values in `YUKI_ORIG_*`.

## Consequences

A direct `cargo build/check/test/clippy` fails with the expected message.

## Rejected alternatives

- Documentation only: the error is discovered late.
