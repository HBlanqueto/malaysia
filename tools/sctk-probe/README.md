# sctk-probe

Dev-only Wayland clients for driving Neptune while developing. This package is
**not** part of the compositor: it is a workspace member excluded from the
default build (`default-members` in the root `Cargo.toml`). A plain
`cargo build`/`cargo test` at the repository root will not build it; name the
package to build it on purpose.

The clients here connect to whatever `WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` point
at, so they can be run against Neptune or the host session without any change.

Everything here runs inside the dev shell (`nix develop`). The compositor must
be built first:

```sh
nix develop
cargo build -p neptune
```

## `parent_probe`

Demonstrates that decoration insets are a function of a window's *current*
state, not of the state it had at map time. It maps two toplevels and toggles a
parent on the child while both are tracked:

- initial (no parent) — decorated, configured one left-titlebar narrower;
- `set_parent` — the child becomes a dialog, so it is reconfigured wider;
- unset — the child returns to exactly its initial size.

The probe checks the three configure sizes and exits non-zero if the cycle is
not reproduced, so it is a pass/fail check rather than output to read by eye.

Run it against a compositor started with its own runtime directory (the socket
must be the one Neptune created). For example, while Neptune runs under
`./dev.sh run`, point the probe at the same socket it uses:

```sh
XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR/neptune-dev" WAYLAND_DISPLAY=wayland-0 \
    cargo run -p sctk-probe --bin parent_probe
```
