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

## 3. Phase 1 — dependencies

Researched on 2026-11-08 against crates.io, Smithay `master` and the wgpu
changelog.

| Dependency | Was | Latest | What breaks here | What it brings | Decision |
|---|---|---|---|---|---|
| barrierkit | `1a4ef8d`, no longer on its `main` | `7172d25` | crate names (its ADR 0062), view and menu names (0063), Freya 0.5 | a pin that stays on `main`, Vulkan kit, Skia m153 | done, `45c67ef` |
| `freya-*`, `torin` | `=0.4.3` | `=0.5.0` | `Platform::get().current_window()` | the kit's version | done, `45c67ef` |
| `smithay-client-toolkit` | 0.19.2 | 0.21.1 | nothing at 0.20 | one sctk, calloop and calloop-wayland-source less | 0.20, barrierkit's, `16d4e13` |
| Smithay | `023b78b1` (2026-10-15) | `65aa86fa` (2026-11-03), 27 commits | winit `0.31.0-beta.3` → `beta.4`; virtual-keyboard now hands out a `KeymapFile` (`6b37edc0`), which `virtual_input.rs` uses | fixes: GLES y-inverted texture matrix, leaked drm_syncobj timeline handles, libinput queued events, `Rectangle::subtract_rects_many_in_place`, layer kept on unmap, `wl_output.done` for v1, tablet frame order | done, approved 2026-11-08 |
| `wgpu` | 24.0.5 | 30.0.1 | `InstanceDescriptor` by value with its display handle, `request_adapter` returns `Result`, `get_current_texture` returns `CurrentSurfaceTexture`, `Queue::present`, `PollType`, `immediate_size`, `multiview_mask`, `depth_slice`, `color_space`, `MipmapFilterMode`; the Vulkan backend is now a feature | only the Vulkan backend is compiled; fixes of six releases | done, approved 2026-11-08 |
| `toml`, `png`, `zbus` | 0.8, 0.17, 5 | 1.2, 0.18, 6 | barrierkit pins the old ones | bumping only here compiles both | keep in step with barrierkit |
| `calloop`, `nix`, `cosmic-text`, `libheif-rs`, `zune-jpeg`, `wayland-*`, `xkbcommon` | latest compatible | — | — | — | nothing to do |

## 4. Phase 3 — checks and CI

1. `./dev.sh check` runs `scripts/check-links.sh` (relative links and
   anchors), copied from barrierkit.
2. No GitHub workflow is added. Two reasons:
   - Skia comes from barrierkit's Nix derivation, built from Google's tree;
     with no binary cache every run rebuilds it.
   - `check-client-dispatch.sh` and one session test need
     `EGL_ANDROID_native_fence_sync`, which the software renderer of a
     hosted runner lacks (`debt.md` R-01).

## 5. Before and after

Same machine (4 CPUs) and commands as section 1; "after" is `7e1e8fa`
plus this commit.

| Measurement | Before | After |
|---|---|---|
| Clean build | 687.5 s | 203.0 s (−70 %) |
| Incremental, touch `input.rs` | 3.9–4.0 s | 4.1–4.2 s |
| Incremental, touch `bar.rs` | 2.5–2.6 s | 2.5–2.7 s |
| `cargo clippy` cold | 260 s | 78 s |
| Crates in the graph | 431 | 387 (417 before wgpu 30) |
| `cargo shear` | 6 findings | clean |
| `cargo deny` duplicates | 49 | 37 |
| `cargo deny` sources and yanked | 5 git sources not allowed, 1 yanked | clean |
| Files over 1500 lines | `session.rs` 3517, `input.rs` 3199, `window.rs` 1887, `handlers.rs` 1520 | `input.rs` 1598 |
| Documents | 156 files, every one twice | one English copy each |

The incremental target (−25 %) is not met: the touched crate and the link
dominate, and neither the profile nor the splits change that.

## 6. Pending

1. Licences: `cargo deny` rejects BSL-1.0, CC0-1.0, MPL-2.0 and
   Apache-2.0 WITH LLVM-exception, all already in the graph; allow them
   in `deny.toml` or replace the crates. `bandura.jpg` and the solid-colour
   wallpapers need an origin (`debt.md` R-04).
2. Not run here: a nested or DRM session, `check-client-dispatch.sh` and
   one session test (they need a GPU), `nix build` and the wallpaper
   render tests on hardware.

When the licences are decided, this document is deleted.

