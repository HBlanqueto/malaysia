# 0005 — One machine, one shell, one linker

Status: accepted (Block 0)

## Context

Neptune targets a single development machine and ships to nobody yet. That
removes a set of portability constraints a general-purpose project would have
to respect, and Block 0 chooses to spend that freedom rather than hedge against
environments that do not exist.

Several development decisions interact:

- **The build environment.** The project is built inside `nix develop`, which
  pins the Rust toolchain, the native libraries and the linker. There is no
  supported build path outside the shell.
- **The linker.** mold links the workspace noticeably faster than the default
  linker. The tool must be both installed and actually requested, and a request
  that depends on the ambient `PATH` makes build behaviour branch and flips the
  cargo fingerprint (invalidating the whole build cache) when it changes.
- **Process management.** `dev.sh` starts the compositor in the background and
  must be able to stop exactly that process without affecting the shell that
  invoked it.
- **Runtime files.** The PID and log files need a writable, per-user location.

## Options considered

### Requesting mold

1. **Unconditionally in `.cargo/config.toml`.** Every cargo invocation gets
   the flag, including `nix build` of the package, where mold is not present —
   which breaks that build. It also cannot be confined to the dev shell.
2. **Probe `PATH` in `dev.sh`.** Enables mold only when found, so it works on
   machines without it. But build behaviour then depends on the ambient
   environment, two developers on the same commit can compile differently, and
   toggling the flag invalidates the entire cargo cache — a branching path the
   project consciously avoids.
3. **Install mold in the dev shell and set `RUSTFLAGS` there.** Anyone inside
   `nix develop` has mold by construction and every in-shell build uses the
   identical flag. The flag never reaches the package build.

### Stopping the background process

1. **Signal the process group** (`kill -- -PID`). Convenient for killing a
   whole tree, but it also signals the invoking shell when `dev.sh` is run
   interactively, which can kill the terminal session.
2. **Signal the saved PID.** Safe, but trusts the PID; if the compositor has
   exited and the PID was reused, it would signal an unrelated process.
3. **Signal the saved PID after confirming it is Neptune.** Read
   `/proc/<pid>/comm` and require it to equal `neptune` before signalling or
   reporting "running". Linux-only, which is fine here.

## Decision

- The dev shell is the supported environment. mold is installed there and
  `RUSTFLAGS = "-C link-arg=-fuse-ld=mold"` is set in `flake.nix`. No linker
  configuration is committed to `.cargo/config.toml` and `dev.sh` does not
  probe for mold. Builds inside the shell are identical and cache-stable.
- `dev.sh` signals the saved PID only, never the process group, and both the
  running check and `stop` verify that `/proc/<pid>/comm` is `neptune` before
  acting. A PID that does not belong to Neptune is treated as a stale PID file
  and is removed rather than signalled.
- `status` follows the LSB init-script exit codes (0 running, 1 dead with a
  PID file, 3 not running) so it can be used as a boolean, while `restart` is
  `stop` followed by `run` and never consults `status`.
- Runtime files live under `$XDG_RUNTIME_DIR/neptune/`, falling back to
  `/tmp/neptune-$(id -u)` when `XDG_RUNTIME_DIR` is unset or missing.

## Consequences

- Entering via `nix develop` is the only supported path; a plain `cargo build`
  compiles a different (system-linker, separately cached) build. Accepted for a
  single-machine project, and recorded as an open question in
  `docs/blocks/00-foundations.md` in case CI is added.
- `dev.sh` is Linux-specific because of `/proc`. There is no portability
  requirement to justify an alternative.
- The native package build is unaffected by the developer linker choice.
- A reused PID cannot cause `dev.sh` to stop an unrelated process.
