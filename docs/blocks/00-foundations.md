# Block 0 — Foundations

Status: **Block 0 complete, uncommitted.** Part 5 is done: this document and
`docs/decisions/0005-development-environment.md` exist and `./dev.sh check`
passes. This document describes what the block contains, what it deliberately
does not, the decisions behind it, and the questions left open for later
blocks.

## Purpose

Block 0 builds the skeleton every later block stands on: a workspace, a
consistent way to build and run it, a coordinate model that makes the three
kinds of position a compositor deals with distinct types, and a minimal binary
that starts, logs, and shuts down. It does **not** implement a compositor. Any
behaviour beyond "start and stop" belongs to a later block.

The guiding rule for the block is that nothing is added without a consumer. A
type, dependency, trait, or API is introduced only when real code needs it, and
each dependency carries a one-line justification in `Cargo.toml`.

## What was built

### Workspace and toolchain

- **Virtual workspace** (`Cargo.toml`). The root holds no crate of its own,
  only shared settings and build profiles. Resolver 3, edition 2024, minimum
  Rust 1.98.
- **Three crates** under `crates/`:
  - `neptune` — the compositor, as both a library and a binary (ADR 0002).
    `#![deny(unsafe_code)]`.
  - `neptune-scene` — the scene graph's home. Block 0 contains only its
    geometry module; the graph itself is Block 2. `#![forbid(unsafe_code)]`,
    no dependencies.
  - `neptune-design` — design tokens and shared visual decisions. Empty on
    purpose until something consumes a token. `#![forbid(unsafe_code)]`.
- **Pinned toolchain** (`rust-toolchain.toml`) — Rust 1.98.1 with rustfmt and
  clippy, so every checkout compiles with the same compiler.
- **Nix dev shell** (`flake.nix`) — the single supported build environment. It
  provides the Rust tools, mold and shellcheck, and the native libraries later
  blocks will need (libinput, libglvnd, seatd, libxkbcommon, mesa, wayland,
  udev). It sets `RUSTFLAGS` to request the mold linker (ADR 0005).
- **Cargo configuration** (`.cargo/config.toml`) — `git-fetch-with-cli` for
  environments without a usable libgit2, and a comment recording why the
  linker flag is *not* here.

### Build profiles

- `dev` — Neptune's own code unoptimised, dependencies lightly optimised, so
  incremental rebuilds of our code stay cheap without third-party code being
  painfully slow.
- `dev-fast` — inherits `dev`, disables debug info for faster links. For
  iterating on compile errors.
- `release` — thin LTO, one codegen unit, panic = abort, symbols stripped.

### Coordinate types

`crates/neptune-scene/src/geometry.rs` defines `Point<N, Space>`,
`Size<N, Space>` and `Rect<N, Space>` for the three spaces `Logical`,
`Physical` and `Buffer`. Arithmetic is locked to a single space, so mixing
spaces is a compile error — verified by a `compile_fail` doctest — and every
conversion is a named `to_*` method taking an explicit scale. Logical
coordinates are `f64`; physical and buffer coordinates are `i32`. See ADR 0001
for why these types live here rather than in the compositor, and ADR 0004 for
the unfinished question of rounding rectangles into integer space.

### The binary

`crates/neptune` starts, installs logging, emits one `neptune starting` line,
blocks until `SIGTERM` or `SIGINT`, then emits `neptune stopped` and exits.
`--log-level` selects verbosity; `--help` and `--version` are also understood.
Argument parsing is hand-written against `std::env::args`: a single option does
not justify a parsing dependency. Tests cover the parser (8) and the log-level
mapping (4).

### `dev.sh`

A POSIX shell wrapper around cargo and a background instance of the binary.
Run `./dev.sh --help` for the list; the commands are `build [--release]`,
`run [--release]`, `stop`, `restart`, `status`, `log [-f]`, `clean` and
`check`. It uses only coreutils and cargo, is clean under `shellcheck`, and
keeps runtime files under `$XDG_RUNTIME_DIR/neptune-dev/` (falling back to
`/tmp/neptune-$(id -u)`); the directory avoids the name `neptune`, which a
running compositor may use for its Wayland socket. Exit codes are grouped so
callers can branch:

| Code | Meaning |
| ---- | ------- |
| 0 | success |
| 1 | `status`: dead process with a leftover PID file |
| 2 | usage error |
| 3 | `run`: already running; `status`: not running |
| 4 | a required tool is missing |
| 5 | the binary failed to start |
| 6 | `log`: no log file |
| 7 | `build`/`check`/`clean`: not inside the dev shell |

`status` follows the LSB init-script convention (running = 0, dead with PID
file = 1, not running = 3), so it can be used as a boolean in scripts.
`restart` is `stop` followed by `run` and never consults `status`. `stop`
signals the saved PID only — never the process group — and both the running
check and `stop` confirm the PID belongs to a process whose `/proc/<pid>/comm`
is `neptune`, so a reused PID is treated as stale rather than signalled.

## What was deliberately not built

The following are out of scope for Block 0 and must not appear until a block
requires them: the event loop; a `State` struct; protocol handlers; global
registration; rendering and shaders; input handling; window management
(`xdg-shell` and friends); the scene graph itself; configuration loading;
session/logind integration; and any layout logic. `design/v1.nix` is
intentionally empty for the same reason. Recording these here keeps the
temptation to scaffold them early visible instead of acting on it.

## Coordinate spaces

Neptune has three notions of position, and confusing them is a classic source
of compositor bugs:

- **Logical** — desktop coordinates, independent of output scale.
- **Physical** — real pixels on one output, after its scale factor.
- **Buffer** — pixels inside a client's buffer, before mapping onto an output.

Block 0 makes them distinct types. `crates/neptune` does **not** define
geometry: it uses `smithay::utils` directly once Smithay is introduced, because
every Smithay API speaks those types and a wrapper would force conversions in
every callback. `neptune-scene` cannot depend on Smithay, so it owns its own
types. Conversion happens at exactly one boundary, where the compositor feeds
the scene. The full reasoning, including why the original "own the types in the
compositor" decision was reversed, is in ADR 0001.

## Dependencies

Every dependency is listed in `[workspace.dependencies]` with a one-line
justification. Runtime dependencies of `neptune`, in full:

| Crate | Justification |
| ----- | ------------- |
| `anyhow` | Ergonomic error context in `main` only; libraries use typed errors. |
| `thiserror` | Derives typed error enums in library-shaped modules. |
| `tracing` | Structured logging and spans; spans are needed for later measurement. |
| `tracing-subscriber` | Installs the global subscriber that emits `tracing` events. |
| `signal-hook` | Safe SIGTERM/SIGINT handling; std has no signal API and the block forbids `unsafe`. |

`neptune-scene` and `neptune-design` have no dependencies. Transitive
dependencies are locked in `Cargo.lock`. The standard library is preferred
wherever it is sufficient; an argument-parsing and a globbing crate were both
considered and rejected because Block 0 has no need for them.

## Development workflow

Everything happens inside the dev shell:

```sh
nix develop
./dev.sh check          # rustfmt, clippy -D warnings, tests
./dev.sh build          # debug build
./dev.sh run            # start in the background, write a PID file
./dev.sh status         # 0 running, 1 stale PID file, 3 stopped
./dev.sh log -f         # follow the log
./dev.sh stop           # SIGTERM, then SIGKILL after a 5 s grace period
./dev.sh restart        # stop, then run
./dev.sh clean          # stop, remove target/ and runtime files
```

The shell requests the mold linker through `RUSTFLAGS`, so in-shell builds are
identical from run to run and the cargo cache stays warm. Outside the shell the
system linker is used, which is a different build (ADR 0005).

### Verification

`./dev.sh check` is the acceptance command. It runs 12 tests in `neptune`
(argument parsing and log-level mapping), 16 in `neptune-scene` (coordinate
conversions and same-space arithmetic), one `compile_fail` doctest proving
cross-space point arithmetic does not compile, then rustfmt and
`clippy -D warnings`. `cargo doc` builds without warnings because every public
item is documented.

## Open questions

1. **Rounding rectangles into integer space.** The current `to_physical` and
   `to_buffer` round position and size to nearest. Two adjacent surfaces can
   then open a one-pixel seam. Floor-position/ceil-size is the likely fix;
   ADR 0004 holds the decision open until something places or damages a
   surface. *(ADR 0004)*
2. **Revisiting the coordinate split.** ADR 0001 puts Smithay's types in the
   compositor and ours in the scene. That should be re-examined when the first
   Smithay-consuming module lands, in case the boundary conversion turns out
   to be more invasive than expected. *(ADR 0001)*
3. **Direct Physical ↔ Buffer conversion.** The types currently convert
   between logical and the integer spaces only. A direct physical-to-buffer
   path, and buffer transforms, will be needed once clients' buffers are mapped
   onto outputs; the current API does not express a surface transform at all.
4. **Toolchain source of truth — resolved.** The flake is the one source:
   `rust-toolchain.toml` is removed and the dev shell's nixpkgs `cargo`/`rustc`
   (pinned by `flake.lock`) are what build. `Cargo.toml` keeps `rust-version`
   as the MSRV floor. A bump is `nix flake update nixpkgs`, confirm the
   versions, adjust the floor if needed, then `./dev.sh check`. *(ADR 0006)*
5. **Building outside the dev shell — resolved.** It is forbidden, not merely
   unsupported: the dev shell exports `NEPTUNE_DEV_SHELL=1`, `dev.sh` refuses
   to run `build`, `check` or `clean` without it (exit 7, the fast path), and
   `crates/neptune/build.rs` refuses to compile without it, so a direct
   `cargo build`/`check`/`test`/`clippy` is caught too. Commands that manage an
   already-built binary still work outside the shell. *(ADR 0007)*
6. **`Physical`/`Buffer` are both `i32` and both integer.** They are distinct
   only at the type level. Whether the scene graph ever needs them to carry
   different numeric types (for example `u32` for buffer extents) is not yet
   known.
7. **Design tokens — resolved (contract defined).** `design/v1.nix` is the one
   authored source; a not-yet-written generator will emit a committed module
   inside `neptune-design`, the only crate allowed to read or re-export tokens,
   and hand-written design constants are forbidden everywhere else. No tokens
   are populated and the generator is not written yet. *(ADR 0008)*
8. **Packaging and the session entry.** `share/wayland-sessions/neptune.desktop`
   is present and the flake's `packages.default` still points at `./`, so the
   package would install a session entry whose `Exec` launches the Block 0
   binary — a program that prints one line and waits for SIGTERM. Selecting that
   session at a display manager yields a black screen. Neither the package nor
   the entry has been exercised against a real session. Recorded here, not
   fixed, until there is a compositor worth launching.
9. **A `watch` subcommand for `dev.sh` (blocked).** The intent is to rebuild and
   relaunch Neptune in a nested window on every source change. It cannot be
   built in Block 0: the binary has no winit backend, no event loop and no
   rendering, so there is nothing to show in a window, and `cargo-watch` is
   absent from the dev shell. When a winit backend lands, `cargo-watch` joins
   the dev shell at that same moment — not before — and `watch` is built on it.

## Decision records

- `docs/decisions/0001-coordinate-types.md` — Smithay's types in the
  compositor, ours in `neptune-scene`.
- `docs/decisions/0002-library-and-binary-targets.md` — why `neptune` is both
  a library and a binary.
- `docs/decisions/0003-unsafe-code-policy.md` — `deny` in the compositor,
  `forbid` in the leaf crates.
- `docs/decisions/0004-surface-rect-rounding.md` — rounding, left open.
- `docs/decisions/0005-development-environment.md` — one machine, one shell,
  one linker.
- `docs/decisions/0006-toolchain-source-of-truth.md` — the flake owns the
  toolchain version.
- `docs/decisions/0007-building-outside-the-devshell.md` — outside builds are
  forbidden and fail fast.
- `docs/decisions/0008-design-token-contract.md` — tokens flow from Nix into
  Rust, one way.
