# 0007 — Building outside the dev shell is forbidden

Status: accepted (closes Block 0 open question 5; guard moved into the build)

## Context

The mold linker is requested by `RUSTFLAGS` set in the dev shell (ADR 0005).
Run `cargo build` outside `nix develop` and the flag is absent: the build uses
the default linker, produces a differently linked binary, and warms a separate
cargo cache. Both outcomes are wrong for reasons that are invisible in the
output — the build succeeds, it is simply not the build the project intends.

ADR 0005 named the dev shell the supported environment but did not stop anyone
from leaving it. The first version of this decision put the guard in `dev.sh`,
which caught the sanctioned entry point but not a direct `cargo build`. That
version only held because there is no `rustc` or `cargo` on the `PATH` outside
the shell on this machine: the guarantee rested on a circumstance of the current
setup, not on a mechanism in the project, and it would lapse the moment a system
toolchain was installed. This revision moves the guard into the build itself.

## Options considered

1. **Unsupported, documented only.** A raw `cargo build` still succeeds
   silently and yields a differently linked binary; the failure is only
   discovered by reading the docs, which is precisely what people do not do.
2. **Marker enforced only in `dev.sh`.** Covers the sanctioned entry point and
   fails fast, but a direct `cargo` invocation outside the shell passes. It
   protected the guarantee only as long as nothing else could compile the crate.
3. **Cargo `rustc-wrapper`.** Setting `build.rustc-wrapper` reaches every cargo
   invocation, including raw ones. But the wrapper also runs during `nix build`
   of the package, where the marker is absent, so it needs its own shell
   detection — a moving part inserted into every compile.
4. **A build script in the crate, with `dev.sh` as a fast path.** The build
   script runs on every `cargo build`/`check`/`test`/`clippy` of the crate, so a
   direct invocation is caught, and it costs nothing beyond the crate's own
   build. `dev.sh` still refuses earlier and more cheaply for the common path.

## Decision

Building outside the dev shell is forbidden (option 4). The dev shell exports
`NEPTUNE_DEV_SHELL=1`.

`crates/neptune/build.rs` refuses to build unless the marker is `1`, printing
the same message `dev.sh` does and pointing at this record. It emits
`cargo:rerun-if-env-changed=NEPTUNE_DEV_SHELL`, so changing the marker forces
the script to run again and a previous pass cannot be cached across the
boundary.

`dev.sh` keeps `require_devshell`, called by `build`, `check` and `clean`. It
fails first, before cargo starts, with exit code 7. It is the fast path, no
longer the only one. Commands that do not compile — `run`, `stop`, `restart`,
`status`, `log` — keep working outside the shell.

The native package derivation sets `NEPTUNE_DEV_SHELL = "1"` on purpose: it is a
sanctioned, hermetic build path, not an interactive one, and it does not use
mold.

## Consequences

- A direct `cargo build`, `cargo check`, `cargo test` or `cargo clippy` with
  the marker unset now fails with the intended message instead of producing a
  differently linked binary.
- `cargo:rerun-if-env-changed` means the guard cannot be defeated by a cached
  successful build: unsetting the marker re-runs the script and fails.
- `dev.sh` remains the earlier, cheaper failure for the common path; the two
  layers do not conflict.
- The package build is not blocked, because it sets the marker deliberately.
- This is a guardrail, not a sandbox: the marker is an ordinary environment
  variable and can be set by hand. It exists to stop an accidental silent
  bypass, which is what the earlier version risked; it is not a security
  boundary.
- The previous version's guarantee — that direct cargo could not build — is
  recorded here as resting on the absence of a toolchain outside the shell,
  and is now replaced by a check in the build.
