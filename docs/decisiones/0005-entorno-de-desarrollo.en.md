# 0005 — One machine, one shell, one linker

Status: accepted (Block 0).

## Context

The project targets a single machine.

## Decision

- The development shell is the supported environment. mold is installed there and `RUSTFLAGS = "-C link-arg=-fuse-ld=mold"` is defined in `flake.nix`; no linker is versioned in `.cargo/config.toml`.
- `dev.sh` signals only the saved PID, never the group, and checks that `/proc/<pid>/comm` is the compositor before acting. `status` follows the LSB codes (0/1/3); `restart` is `stop` + `run`.
- Development files live in `$XDG_RUNTIME_DIR/yuki-dev/` (or `/tmp/yuki-$(id -u)`). The socket is a dedicated `wayland-N`.

## Consequences

Building outside the devShell produces a different binary (forbidden by ADR 0007).

## Rejected alternatives

- Unconditional mold in `.cargo/config.toml`: it breaks `nix build`.
