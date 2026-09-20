# 0007 — Building outside the dev shell is forbidden

Status: accepted (closes Block 0 open question 5)

## Context

The mold linker is requested by `RUSTFLAGS` set in the dev shell (ADR 0005).
Run `cargo build` outside `nix develop` and the flag is absent: the build uses
the default linker, produces a differently linked binary, and warms a separate
cargo cache. Both outcomes are wrong for reasons that are invisible in the
output — the build succeeds, it is simply not the build the project intends.

ADR 0005 named the dev shell the supported environment but did not stop anyone
from leaving it. This decision closes that gap. On this machine there is no
`rustc` on the `PATH` outside the shell, so the failure is currently moot in
practice, but it is exactly the kind of silent divergence that should fail
loudly if a system toolchain is ever installed.

## Options considered

1. **Unsupported, documented only.** State in the documentation that outside
   builds are not supported. Cheap, but a raw `cargo build` still succeeds
   silently and yields a differently linked binary; the failure is only
   discovered by reading the docs, which is precisely what people do not do.
2. **Forbidden, enforced by a dev-shell marker in `dev.sh`.** The flake exports
   a marker variable; the `dev.sh` commands that invoke cargo refuse to run
   without it and explain why. Covers the sanctioned entry point and fails
   fast. A direct `cargo` invocation outside the shell is still physically
   possible; the policy forbids it.
3. **Forbidden, enforced by a cargo `rustc-wrapper`.** Setting
   `build.rustc-wrapper` reaches every cargo invocation, including raw ones.
   But the wrapper also runs during `nix build` of the package, where the dev
   shell marker is absent, so it needs its own shell detection — a moving part
   inserted into every compile to catch a case that policy already forbids.

## Decision

Building outside the dev shell is forbidden (option 2). The dev shell exports
`NEPTUNE_DEV_SHELL=1`. `dev.sh` gains `require_devshell`, called by every
command that invokes cargo (`build`, `check`, `clean`); without the marker it
fails immediately with a clear message and exit code 7. Commands that do not
compile — `run`, `stop`, `restart`, `status`, `log` — keep working outside the
shell, since managing an already-built binary does not depend on the linker.

Direct `cargo` invocations outside `nix develop` remain forbidden by policy and
unsupported. If this ever needs to be physically impossible rather than
disallowed, option 3 is the path, with the shell detection it entails.

## Consequences

- `./dev.sh build`, `./dev.sh check` and `./dev.sh clean` cannot produce a
  differently linked binary: they refuse before invoking cargo.
- The native package build (`nix build`) is unaffected; it does not go through
  `dev.sh` and does not need the marker.
- `dev.sh` gains exit code 7 (`EXIT_NOT_DEVSHELL`), recorded in the exit-code
  table in `docs/blocks/00-foundations.md`.
- A determined direct `cargo build` outside the shell is still possible. It is
  a policy violation, not a silent accident, and is called out as such here.
