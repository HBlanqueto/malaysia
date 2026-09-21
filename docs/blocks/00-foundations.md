# Roadmap Block 0 — Scaffolding (Foundations)

> **Roadmap mapping.** This is the detailed record of roadmap **Block 0**
> (Scaffolding), which it calls "Foundations". Work after it is named, not
> numbered: the **nested session** (`docs/blocks/01-session.md`, roadmap Blocks
> 1–3) and the **Direct Session milestone** (the DRM/KMS backend, unnumbered).

Status: **complete.** Block 0 is six parts:

1. the workspace, toolchain and build profiles;
2. `neptune-scene`'s coordinate types, with their tests and ADRs;
3. the binary's startup, argument parsing and logging;
4. `dev.sh`, its `watch` subcommand, and `docs/hardware.md`;
5. `neptune-design`'s system defaults;
6. this document.

This document describes what the block contains, what it deliberately does not,
the decisions behind it, and the questions left open for later milestones.

## Purpose

Block 0 builds the skeleton every later block stands on: a workspace, a
consistent way to build and run it, a coordinate model that makes the three
kinds of position a compositor deals with distinct types, and a minimal binary
that starts, logs, and shuts down. It does **not** implement a compositor. The
event loop, protocol handlers and rendering are the work of the nested session
(roadmap Blocks 1–3).

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
    geometry module; the graph itself is a later milestone.
    `#![forbid(unsafe_code)]`, no dependencies.
  - `neptune-design` — the system design defaults, as documented compile-time
    constants. `#![forbid(unsafe_code)]`, no dependencies.

  The nested session added a fourth, dev-only member at `tools/sctk-probe`: the probe
  clients used to verify the compositor. It lives outside `crates/` and is
  excluded from the default build, so it is not part of the compositor.
- **Toolchain from the flake.** There is no `rust-toolchain.toml`: the dev
  shell's nixpkgs `cargo`/`rustc`, pinned by `flake.lock`, are what build, and
  `Cargo.toml`'s `rust-version` is the floor (ADR 0006).
- **Nix dev shell** (`flake.nix`) — the single supported build environment. It
  provides the Rust tools, `cargo-watch`, mold and shellcheck, and the native
  libraries later milestones need (libinput, libglvnd, seatd, libxkbcommon, mesa,
  wayland, udev). It sets `RUSTFLAGS` to request the mold linker and exports
  `NEPTUNE_DEV_SHELL=1` (ADR 0005, ADR 0007).
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
and understands `--log-level`, `--help` and `--version`. Argument parsing is
hand-written against `std::env::args`: a single option does not justify a
parsing dependency. Tests cover the parser (8) and the log-level mapping (4).
Block 0's process lifetime was "wait for `SIGTERM`/`SIGINT`"; the nested session
replaced that wait with its event loop, and the signal handling moved with it.

### `neptune-design`

The system defaults: titlebar placement and metrics, button colors, window
shape, typography, the two-layer shadow, and the light and dark palettes. They
are plain constants and small `const` structs — `Rgba`, `FocusPair`,
`Titlebar`, `TitlebarButtons`, `WindowStyle`, `Font`, `ShadowStyle`, `Palette`
— with `LIGHT`, `DARK`, `TITLEBAR`, `WINDOW`, `TEXT` and `SHADOW` as the
exported values. Colors are `f32` in `0.0..=1.0`, built with `rgb8`/`rgba8` so
the literals stay readable and no color is converted per frame.

Two things are modelled deliberately. Colors that differ between a focused and
an unfocused window are a `FocusPair`, so an unfocused value is its own
constant rather than a multiplier applied later. The dark mode inner line is
`Option<Rgba>`, absent in light mode, so a renderer can skip the pass instead
of drawing something invisible. The accent comes from the host somewm
configuration; the module doc records the file, line and original `#rrggbb`
format (ADR 0011).

### `dev.sh`

A POSIX shell wrapper around cargo and a background instance of the binary.
Run `./dev.sh --help` for the list; the commands are `build [--release]`,
`run [--release]`, `watch [--release]`, `stop`, `restart`, `status`, `log [-f]`,
`clean`, `check` and `smoke [--release]`. It uses only coreutils, cargo, `setsid`
and `cargo-watch`,
is clean under `shellcheck`, and keeps runtime files under
`$XDG_RUNTIME_DIR/neptune-dev/` (falling back to `/tmp/neptune-$(id -u)`); the
directory avoids the name `neptune`, which a running compositor may use for its
Wayland socket. Exit codes are grouped so callers can branch:

| Code | Meaning |
| ---- | ------- |
| 0 | success |
| 1 | `status`: dead process with a leftover PID file |
| 2 | usage error |
| 3 | `run`: already running; `status`: not running |
| 4 | a required tool is missing |
| 5 | the binary failed to start |
| 6 | `log`: no log file |
| 7 | `build`/`check`/`clean`/`watch`/`smoke`: not inside the dev shell |
| 8 | `watch`: the watcher exited unexpectedly |
| 9 | `smoke`: the build or the smoke test failed |

`status` follows the LSB init-script convention (running = 0, dead with PID
file = 1, not running = 3), so it can be used as a boolean in scripts.
`restart` is `stop` followed by `run` and never consults `status`. `stop`
signals the saved PID only — never the process group — and both the running
check and `stop` confirm the PID belongs to a process whose `/proc/<pid>/comm`
is `neptune`, so a reused PID is treated as stale rather than signalled.
`start` launches the compositor with `setsid`, in its own session, so a watcher
that kills its own children cannot take the instance down with it; setsid execs
without forking here, so the saved PID is still the compositor's real PID.

`watch` rebuilds on every change under `crates/` (and `src/`, if it exists) and
restarts the background instance; the build and restart are joined by `&&`, so
a failed build leaves the running instance alone and its compiler output on the
terminal. Ctrl-C stops both.

### `docs/hardware.md`

The machine's first GPU contact, recorded from the nested session's first run:
CPU, GPU, kernel, Mesa 26.2.3, OpenGL ES 3.2, EGL 1.5, `PLATFORM_WAYLAND_KHR`
and the verbatim renderer string. It exists as the known-good point to compare
against when the Direct Session milestone moves to direct DRM/KMS and the EGL
platform changes.

## What was deliberately not built

The following are out of scope for Block 0: the event loop; a `State` struct;
protocol handlers; global registration; rendering and shaders; input handling;
window management (`xdg-shell` and friends); the scene graph itself;
configuration loading; session/logind integration; and any layout logic. The
nested session (roadmap Blocks 1–3) began the first of these with the winit
backend; the rest remain for the milestones that require them. There is no
`design/v1.nix` and no token generator (ADR 0011).

## Coordinate spaces

Neptune has three notions of position, and confusing them is a classic source
of compositor bugs:

- **Logical** — desktop coordinates, independent of output scale.
- **Physical** — real pixels on one output, after its scale factor.
- **Buffer** — pixels inside a client's buffer, before mapping onto an output.

Block 0 makes them distinct types. `crates/neptune` does **not** define
geometry: it uses `smithay::utils` directly, because every Smithay API speaks
those types and a wrapper would force conversions in every callback.
`neptune-scene` cannot depend on Smithay, so it owns its own types. Conversion
happens at exactly one boundary, where the compositor feeds the scene. The full
reasoning, including why the original "own the types in the compositor"
decision was reversed, is in ADR 0001.

## Dependencies

Every dependency is listed in `[workspace.dependencies]` with a one-line
justification. The runtime dependencies added in Block 0, in full:

| Crate | Justification |
| ----- | ------------- |
| `anyhow` | Ergonomic error context in `main` only; libraries use typed errors. |
| `thiserror` | Derives typed error enums in library-shaped modules. |
| `tracing` | Structured logging and spans; spans are needed for later measurement. |
| `tracing-subscriber` | Installs the global subscriber that emits `tracing` events. |
| `signal-hook` | Safe SIGTERM/SIGINT handling; std has no signal API and the block forbids `unsafe`. |

The nested session added `smithay` (only the winit backend, GLES2 renderer and
Wayland frontend features) as the compositor framework, pinned to the crates.io
0.7.0 release (ADR 0009, ADR 0010). It also added one dev-only dependency,
`smithay-client-toolkit`, used only by `tools/sctk-probe`; it was already in the
graph through winit. `neptune-scene` and `neptune-design` still have no
dependencies. Transitive dependencies are locked in `Cargo.lock`. The
standard library is preferred wherever it is sufficient; an argument-parsing
and a globbing crate were both considered and rejected.

## Development workflow

Everything happens inside the dev shell:

```sh
nix develop
./dev.sh check          # rustfmt, clippy -D warnings, tests
./dev.sh build          # debug build
./dev.sh run            # start in the background, write a PID file
./dev.sh watch          # rebuild and relaunch on change (Ctrl-C stops)
./dev.sh status         # 0 running, 1 stale PID file, 3 stopped
./dev.sh log -f         # follow the log
./dev.sh stop           # SIGTERM, then SIGKILL after a 5 s grace period
./dev.sh restart        # stop, then run
./dev.sh clean          # stop, remove target/ and runtime files
```

The shell requests the mold linker through `RUSTFLAGS`, so in-shell builds are
identical from run to run and the cargo cache stays warm (ADR 0005). Building
outside the shell is forbidden and caught both by `dev.sh` and by the crate's
build script (ADR 0007).

### Verification

Block 0 was accepted on `./dev.sh check`: 12 tests in `neptune` (argument
parsing and log-level mapping), 16 in `neptune-scene` (coordinate conversions
and same-space arithmetic), one `compile_fail` doctest proving cross-space point
arithmetic does not compile, then rustfmt and `clippy -D warnings`. The nested
session grew the same command with its own tests and the probe harness; its
document records what the command now covers. `cargo doc` builds without warnings because every
public item is documented; `neptune-design` is where that is exercised most,
since it is almost entirely public data.

## Open questions

1. **Rounding rectangles into integer space.** The current `to_physical` and
   `to_buffer` round position and size to nearest. Two adjacent surfaces can
   then open a one-pixel seam. Floor-position/ceil-size is the likely fix, but
   it is not built: ADR 0004 holds the decision open until something actually
   places or damages a surface. *(ADR 0004)*
2. **Direct Physical ↔ Buffer conversion.** The types convert between logical
   and the integer spaces only. A direct physical-to-buffer path, and buffer
   transforms, will be needed once clients' buffers are mapped onto outputs;
   the current API does not express a surface transform at all.
3. **`Physical` and `Buffer` are both `i32`.** They are distinct only at the
   type level. Whether the scene graph ever needs them to carry different
   numeric types — `u32` for buffer extents, say — is not known yet.
4. **`watch --release` was never exercised.** The flag takes the same path as
   `run`, and bad-argument rejection is tested, but no real `--release` rebuild
   was run through the watcher, because a release rebuild of the whole tree is
   long. It is believed correct by construction, not verified by a run.
5. **GTK4/libadwaita draws its own corners and shadow.** A client that paints
   its own decorations ignores `xdg_decoration`, so Neptune's titlebar and
   shadow would be drawn on top of the client's — double chrome. This needs a
   per-window opt-out, and it changes how the decoration protocol is designed:
   the client's intent has to be read before Neptune commits to drawing a
   frame.
6. **`setsid` and the saved PID.** `start` relies on `setsid` execing without
   forking, so `$!` is the compositor's real PID. That holds when the caller is
   not already a process-group leader, which is true under the watcher; it was
   not tested from a direct interactive-shell invocation, where job control
   could change the disposition.
7. **ADR 0001's Smithay boundary is untested.** No Block 0 code consumes
   geometry, and Smithay only entered with the nested session as the winit
   backend, not as a geometry consumer. The first module that both uses Smithay
   and converts into `neptune-scene`'s types will show whether the one-boundary
   split is as small as ADR 0001 claims.
8. **Packaging and the session entry.** `share/wayland-sessions/neptune.desktop`
   is present and the flake's `packages.default` still points at `./`, so the
   package would install a session entry whose `Exec` launches the binary.
   Neither the package nor the entry has been exercised against a real session.
   Recorded, not fixed, until there is a compositor worth launching.

Resolved during Block 0, kept here so the numbering and the ADRs line up:
toolchain source of truth (ADR 0006), building outside the dev shell
(ADR 0007), and the design-value contract (ADR 0011, superseding ADR 0008).
The `watch` subcommand itself was resolved once a window existed; its one
unverified corner is question 4 above.

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
- `docs/decisions/0008-design-token-contract.md` — the Nix-to-Rust token
  contract, superseded by 0011.
- `docs/decisions/0009-winit-backend-and-gles2.md` — the nested backend and
  renderer (nested session).
- `docs/decisions/0010-smithay-source-pin.md` — Smithay pinned to crates.io
  0.7.0 (nested session).
- `docs/decisions/0011-design-values-are-rust-constants.md` — design values are
  hand-written constants in `neptune-design`.
