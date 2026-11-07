# Remake-Redo tracking

- **Goal:** record the measurements and decisions of the Remake-Redo project
  ([prompt](prompts/remake-redo.md)) while it runs.
- **Scope:** from the phase 0 baseline to the close. This document is deleted
  at the close; what stays open moves to [`debt.md`](debt.md) or
  [`roadmap.md`](roadmap.md).
- **Reader:** whoever continues the project.
- **Inputs:** `cargo build --timings`, `cargo tree`, `cargo clippy`,
  `cargo-shear`, `cargo-machete` and `cargo deny`, run inside `nix develop`.
- **Outputs:** the tables below.

## 1. Baseline (phase 0)

1. Build the baseline at `b259840` with barrierkit pinned at `1a4ef8d`.
2. In this container, `nix develop` cannot build Skia 0.98 for that pin: the
   `rust-skia` tarball returns 403. The baseline uses
   `--override-input barrierkit` with barrierkit `b5c31c2`, whose Skia 0.98
   comes from Google's tree and was already in the Nix store. The Rust code
   built is the same; only the Skia source differs.

| Measurement | Baseline |
|---|---|
| Clean build (`cargo build`, default members, 4 CPUs) | 687.5 s |
| Incremental, touch `malaysia-compositor/src/input.rs` (3 runs) | 3.9–4.0 s |
| Incremental, touch `malaysia-shell/src/menubar/bar.rs` (3 runs) | 2.5–2.6 s |
| `cargo clippy --workspace --all-targets -D warnings`, cold | 260 s, clean |
| Crates in the graph (normal and build edges) | 431 |
| `cargo shear` | 6 findings: `wayland-backend` (compositor), `i18n-embed-fl` (settings, shell, workspace), `wayland-client` (wallpaper, workspace) |
| `cargo machete` | 1 finding: `wayland-client` (wallpaper) |
| `cargo deny check` | advisories: 6 unmaintained (`basic-toml`, `ttf-parser`, `zbus_names`, `zvariant`, `zvariant_derive`, `paste`); licenses: `BSL-1.0`, `CC0-1.0`, `MPL-2.0`, `Apache-2.0 WITH LLVM-exception` not allowed; bans: 49 duplicates |

`cargo-shear`, `cargo-machete` and `cargo-deny` are not in the dev shell;
they came from the flake's nixpkgs (`nix build --inputs-from .`).

## 2. Phase 2 — profiles

Clean `cargo build` and incremental builds on barrierkit `7172d25`, same
code, 4 CPUs:

| Profile | Clean | Touch `input.rs` | Touch `bar.rs` |
|---|---|---|---|
| Dependencies at `opt-level = 2` (before) | 637.3 s | 3.8–4.4 s | 2.5–2.7 s |
| `opt-level = 0`, `debug = false`; `2` for 19 run-time crates | 206.6 s | 4.2–4.4 s | 2.6–2.7 s |

The linker is already mold (ADR 0005). The incremental build does not move:
it is the touched crate plus the link.
