# sctk-probe

Dev-only Wayland clients for driving Neptune while developing. This package is
**not** part of the compositor: it is a workspace member excluded from the
default build (`default-members` in the root `Cargo.toml`). A plain
`cargo build`/`cargo test` at the repository root will not build it; name the
package to build it on purpose.

The clients here connect to whatever `WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` point
at, so they can be run against Neptune or the host session without any change.
`smoke` and `block4` are different: they launch Neptune itself and drive clients
against it.

Everything here runs inside the dev shell (`nix develop`). The simplest way to
run the smoke test is the project script, which builds both the compositor and
this harness and propagates the result:

```sh
./dev.sh smoke
```

The sections below are the underlying `cargo` commands, for when you want to run
one binary directly.

## `parent_probe`

Demonstrates that decoration insets are a function of a window's *current*
state, not of the state it had at map time. It maps two toplevels and toggles a
parent on the child while both are tracked:

- initial (no parent) — decorated, configured one left-titlebar narrower;
- `set_parent` — the child becomes a dialog, so it is reconfigured wider;
- unset — the child returns to exactly its initial size.

The probe checks the three configure sizes and exits non-zero if the cycle is
not reproduced, so it is a pass/fail check rather than output to read by eye.

Run it against a running compositor. While Neptune runs under `./dev.sh run`,
its socket is in the dev runtime directory; Smithay deliberately starts at
`wayland-1`, so find the socket rather than assuming a name:

```sh
runtime="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/neptune-dev"
sock=$(ls "$runtime"/wayland-* 2>/dev/null | grep -v '\.lock$' | head -n 1)
XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="${sock##*/}" \
    cargo run -p sctk-probe --bin parent_probe
```

## `smoke`

An assertion-based smoke test for the toplevel mapping sequence. It launches the
compositor, starts a real client (`foot` by default) with `WAYLAND_DEBUG=1`,
captures the client's trace, and checks, in order: `xdg_toplevel.configure`
before the first non-null attach; a matching `ack_configure` serial before that
attach; a non-null buffered commit that the compositor reports as mapped;
`wl_keyboard.enter` on the mapped surface; and completion of the *specific*
callback id requested by `wl_surface.frame`. A bare `wl_callback.done` is not
accepted, so a `wl_display.sync` round-trip cannot be mistaken for a frame.

It exits non-zero if any step is missing or out of order.

```sh
./dev.sh smoke          # builds neptune and the harness, then runs it
cargo run -p sctk-probe --bin smoke   # run it directly, if already built
```

Environment overrides: `NEPTUNE_BIN` (path to the compositor binary) and
`NEPTUNE_SMOKE_CLIENT` (the client to drive).

## `block4`

The Block 4 acceptance harness. It launches the compositor and `window_stress`,
which opens fifty decorated toplevels and then destroys them. It asserts the
compositor reported at least fifty `window removed` lines and then `scene empty`,
and it reads the compositor's `VmRSS` before and after, reporting the delta. The
delta is **measured, not asserted**: a threshold would be flaky.

```sh
./dev.sh block4                        # builds neptune and the probes, then runs it
cargo run -p sctk-probe --bin block4   # run it directly, if already built
```

Environment overrides: `NEPTUNE_BIN` and `NEPTUNE_STRESS_BIN`.

## `window_stress`

The client `block4` drives. It opens fifty decorated toplevels with
shared-memory buffers, waits for a configure on each, then closes every one and
exits. On its own it is only useful to watch a compositor's teardown:

```sh
cargo run -p sctk-probe --bin window_stress
```
