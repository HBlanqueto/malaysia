# Remake-Redo for Malaysia

- **Goal:** clean malaysia up from end to end. That means current
  dependencies, one English copy of each document, a clean ADR set, faster
  builds, and code without dead parts, duplicates or files with several
  reasons to change.
- **Scope:** starts with reading the repository and ends with a
  before-and-after table. Covers code, documentation, dependencies, Nix and
  the build. Ends at the repository: dotfiles and curseterm are not
  modified, only reported on.
- **Reader:** the agent (Claude Code, opencode…) that runs the project. It
  works in phases, without asking for permission at every step, except at
  the stop points marked below.
- **Inputs:** this repository at `627e3b3`, barrierkit at `74ba016` (its
  `main`), Smithay at the pinned rev and upstream, and the audit of
  2026-11-03.
- **Outputs:** small commits on `main`, a green `./dev.sh check` after each
  phase, and a 1–3 line result in the chat per phase.

## 0. Read first, write nothing

Do not change a file until this reading is done. Then summarise in the
chat, in 10–15 lines, how the compositor, the shell and the kit fit
together.

1. **Repository rules:** `AGENTS.md` and `CLAUDE.md`. Follow them to the
   letter: branches, design policy, environment rule, «Código mínimo»
   (rules 1–10). This project changes one rule: documentation becomes
   English only (phase 4).
2. **State and debt:** `docs/roadmap.md`, `docs/deuda.md` and the audit
   `docs/auditoria-claude-2026-11-03.md`. Section 12 of the audit lists what
   is already fixed; do not redo it.
3. **Architecture:** `docs/arquitectura/` (process model, interface,
   windows, menubar, dock, lock, wallpaper, captures) and
   `docs/configuracion/opciones.md`.
4. **Decisions:** the index `docs/decisiones/README.md` and ADRs 0007,
   0011, 0019, 0020, 0021, 0023, 0024, 0025, 0028, 0034, 0047, 0048 and 0061.
5. **Build:** the workspace `Cargo.toml`, `crates/*/Cargo.toml`, `dev.sh`,
   `flake.nix`, `nix/malaysia.nix`, `deny.toml` and `scripts/*`.
6. **The kit:** barrierkit `AGENTS.md`, `docs/barrierkit/README.md`,
   `20-glossary.md`, and ADRs 0061–0064 (views as types, short crate names,
   styles). That is what the shell and settings must move to.

## 1. What we already know (starting point; verify it)

- **Workspace:** `malaysia-compositor`, `malaysia-scene`, `grade-graphics`,
  `malaysia-shell`, `malaysia-wallpaper`, `malaysia-settings`,
  `xdg-desktop-portal-malaysia`, plus `tools/sctk-probe`. About 47 400 lines
  of Rust:
  - the compositor has about 22 900; its largest files are `session.rs`
    (3 517), `input.rs` (3 199), `window.rs` (1 887), `handlers.rs` (1 520),
    `chrome.rs` (1 303) and `udev.rs` (1 300);
  - the shell has about 10 300; the largest files are in `menubar/`
    (`dbusmenu.rs` 1 314, `gtkmenus.rs` 1 066, `bar.rs` 1 061) and
    `dialogs.rs` (1 050).

  `AGENTS.md` says a file over ~1 500 lines takes no new function.
- **barrierkit is pinned at `1a4ef8d`** in `Cargo.toml`, in `flake.nix` and
  in `outputHashes`. That commit is **no longer an ancestor of barrierkit
  `main`** (main was force-pushed), so it can disappear from GitHub. This is
  the most urgent item. Since then barrierkit has changed:
  - crate names (ADR 0062): `barrier-kit` → `bk`, `vow-design` →
    `ctdesign`, `vow-appearance` → `ctappearance`;
  - views as types with SwiftUI names (ADR 0061, 0063) and styles (0064);
  - **Freya `=0.5.0`** with Skia m153. The changes include:
    - `StyleExt` → `BackgroundExt` + `DecorationExt`;
    - `blur` → `backdrop_blur`;
    - `on_global_pointer_press` → `on_global_pointer_up`;
    - accessors `x_mut`;
    - private `CornerRadius` fields;
    - `Platform` per window;
    - a declarative cursor;
  - `bk` features: `default = ["a11y"]`; `shell`, `charts`, `print`,
    `video` and `tracy` are opt-in.
- **Direct Freya use:** `malaysia-shell` and `malaysia-settings` depend on
  `freya-core`, `freya-components`, `freya-animation`, `torin` and
  `freya-testing` `=0.4.3`. About 50 files import the kit or Freya. They
  must move to `=0.5.0` together with the kit.
- **curseterm** comes in through `flake.nix` (rev `f4e804c`) for `./dev.sh
  demo`. It pins its own barrierkit (`36e5958`, Freya 0.4.3). Do not modify
  it; report what it needs.
- **Other pins:**
  - Smithay `023b78b1` (ADR 0028);
  - `wgpu` 24, `toml` 0.8 and `png` 0.17;
  - `smithay-client-toolkit` 0.19.2;
  - `nixpkgs` follows `nixos-unstable`.

  Each needs research before it moves.
- **Profiles:** `[profile.dev.package."*"] opt-level = 2` for every
  dependency. In barrierkit, `0` by default plus `2` only where it counts in
  execution took the clean build from ~365 s to ~110 s.
- **Documentation:** 156 files in `docs/`, and each one exists twice
  (`name.md` in Spanish, `name.en.md` in English):
  - the 59 ADRs are 118 files;
  - `guia-menu-global-macos.md` (557 lines) is only in Spanish;
  - the 2026-11-03 audit (848 lines) is a tracking document that is already
    applied;
  - directory and file names are Spanish (`decisiones/`, `arquitectura/`,
    `configuracion/opciones.md`, `deuda.md`, `investigacion.md`);
  - 85 documents mention old names (`yuki`, `otto`, `celestial`,
    `neptunia`, `vow`).
- **barrierkit links into malaysia:** its inherited ADR stubs (0011,
  0032–0035, 0039, 0041, 0046) link to malaysia's `.en.md` files. Renaming
  them here means updating barrierkit in the same push.
- **No CI:** there is no `.github/workflows`.
- **Dev shell:** in cloud sessions, `nix develop` needs
  `NIX_GIT_SSL_CAINFO=/root/.ccr/ca-bundle.crt` and `--option
  extra-sandbox-paths /root/.ccr/ca-bundle.crt`. Skia comes from barrierkit's
  `skia-binaries`. **Never build outside the dev shell**: `build.rs` and
  `dev.sh` reject it on purpose (ADR 0007). The container has no seat, DRM or
  display: say so for everything that needs a real or nested session.

## 2. Phases (in this order)

Each phase ends with small commits on `main`, a green `./dev.sh check` and a
result line in the chat.

### Phase 0 — Measure

1. Take the baseline. Use `cargo build --timings`, clean and incremental;
   for the incremental build, touch `crates/malaysia-compositor/src/input.rs`
   and `crates/malaysia-shell/src/menubar/bar.rs`. Also run
   `cargo clippy --workspace`.
2. Look for unused dependencies with `cargo-shear` and `cargo-machete`. Look
   for advisories, licences and duplicates with `cargo deny check`.
3. Count the crates in the graph (`cargo tree --prefix none | sort -u`).
   Find what inflates the compositor with `cargo-llvm-lines`.
4. Write the table to `docs/remake-redo.md`, in English, with the AGENTS.md
   format. It is the only tracking document, and it is deleted when the
   project ends.

### Phase 1 — Research, then update dependencies

1. **Research first** (rule 10, in order). For each pin, read the upstream
   changelog and code between the pinned version and the latest one:
   barrierkit `main`, Freya 0.5.0, Smithay `master`, `wgpu`, `toml`, `png`,
   `smithay-client-toolkit` and the flake inputs. Write down what breaks and
   what it brings to malaysia.
2. **Stop point:** show the table: dependency → current → target → breaking
   changes → what it brings → decision. Do not bump Smithay without this
   table.
3. **barrierkit to `main`** (mandatory):
   - new names in `Cargo.toml`, `flake.nix` and `outputHashes`;
   - `bk` with `features = ["shell"]` in the shell;
   - renames from ADRs 0061–0064 and the migration to Freya 0.5 in the shell
     and settings;
   - golden images and tests regenerated, with the reason for each
     difference;
   - `use bk::prelude::*;` where the kit prescribes it.
4. **The rest:** one commit per dependency, each with what it changes, and
   only if `./dev.sh check` stays green.

### Phase 2 — Profiles and toolchain

- Apply barrierkit's result: `opt-level = 0` by default for dependencies,
  and `2` only for what counts at run time. That means Smithay, the
  renderers, `freya-*`, `torin`, `cosmic-text`, the image decoders and
  `wgpu`. Measure it.
- Try `default-features = false` in `[workspace.dependencies]` wherever
  possible.
- Adopt only what improves the measurement; each change carries its number
  in the commit.

### Phase 3 — CI and checks

- `dev.sh check` gains a check for broken relative links and `#anchors`.
  Reuse barrierkit's `scripts/check-links.sh` and adapt it.
- Propose a GitHub workflow with `nix develop -c ./dev.sh check` and
  `nix flake check` (audit, §11.10). Only if the runner can build Skia from
  Nix; otherwise say why not.

### Phase 4 — Documentation in English only, clean ADRs

1. **Rule:** change the documentation section of `AGENTS.md` to the
   barrierkit one. Each document exists once, in English, as `name.md`, with
   no `.en.md` and no Spanish copies. Code comments and rustdoc stay in
   English. Visible text stays in `i18n/{en,es}/<crate>.ftl` (it is the
   product, not documentation).
2. **One copy per document:** delete the Spanish `name.md` and rename
   `name.en.md` → `name.md`. Before you delete one, compare the pair. If the
   Spanish one says something the English one lacks, carry it over.
   Translate `guia-menu-global-macos.md`.
3. **English names:**
   - `decisiones/` → `decisions/`;
   - `arquitectura/` → `architecture/`;
   - `configuracion/opciones.md` → `configuration/options.md`;
   - `deuda.md` → `debt.md`;
   - `investigacion.md` → `research.md`;
   - ADR files with an English slug, keeping the number.

   Fix every link: in `docs/`, in `README.md`, in `AGENTS.md`, in code
   comments that cite paths, in `dev.sh`, and in barrierkit (inherited ADR
   stubs and any link to malaysia), in the same push.
4. **Clean ADRs:**
   - a superseded ADR becomes a stub with its context and a link to the one
     that replaces it;
   - an ADR whose subject moved to barrierkit (0011, 0024, 0032–0035, 0039,
     0041, 0046, 0047) keeps its text in one single place. Decide which and
     link from the other;
   - an "accepted" ADR that describes something that no longer exists
     (`yuki-scene`, `yuki-shell`, CelestialKit, Otto) is updated or marked
     superseded, with the reason;
   - the index has a status column that matches each file.
5. **Old names:** `yuki`, `otto`, `celestial`, `neptunia` and `vow-*` stay
   only where they describe history (a superseded ADR) or an external
   reference.
6. **Tracking documents:** fold what is still pending from the 2026-11-03
   audit into `debt.md` or `roadmap.md`, then delete it. `roadmap.md` is the
   only source of status: whatever repeats it elsewhere becomes a link.
7. **README:** a short `README.md` in English. It says what Malaysia is, how
   to build it and where the documentation is.

### Phase 5 — Code audit

A new audit of the current code, not a copy of the 2026-11-03 one. Every
finding is verified against the code ([V]) or marked as a suspicion ([S]).

1. **Dead code:**
   - modules exported with `pub mod` in `lib.rs` hide warnings. Make them
     `mod` or `pub(crate)` and delete what surfaces;
   - options parsed in `config.rs` that nobody reads;
   - `#[allow(dead_code)]` and `#[allow(unused…)]`.
2. **Duplicates** (rule 1): loops, setups, easing, sRGB conversions and
   capture code repeated across crates. Design literals outside `ctdesign`.
   Extend `scripts/check-design-tokens.sh` if it does not catch them.
3. **Files with several reasons to change** (rule 4). Split `session.rs`,
   `input.rs`, `window.rs`, `handlers.rs`, `chrome.rs`, `udev.rs`, `menubar/`
   and `dialogs.rs`. Moves only, one commit per module, no behaviour
   changes.
4. **Tests:** keep only pure logic with risk (rule 8).
5. **Correctness and performance:** anything new compared with the audit
   (leaks, redraws, `unwrap()` on system paths, `unsafe` without
   `// SAFETY:`) gets a row in `debt.md` and, if the fix is small, its own
   `fix(…)` commit.
6. **Stop point:** before deleting a configuration option documented in
   `options.md`, a D-Bus interface or method, or a Wayland global, list what
   goes and why.

### Phase 6 — Close

- Repeat the phase 0 measurement and leave the before/after table in the
  chat.
- Delete `docs/remake-redo.md`.
- Leave a list of what is pending:
  - what curseterm needs to move to the new barrierkit;
  - what dotfiles needs;
  - what could not be run without a session.

## 3. Success criteria

- malaysia builds and passes `./dev.sh check` on barrierkit `main` and
  Freya 0.5.0. No pin points to a commit outside `main`.
- 0 `.en.md` files and 0 Spanish documents. The `i18n/es/*.ftl` files stay.
  0 broken links, checked by `./dev.sh check`.
- The ADR index matches each file's status. No current ADR describes
  something that no longer exists.
- Clean build of the workspace −40 % or more. Incremental build of the
  compositor after touching `input.rs` −25 % or more.
- `cargo-shear` is clean, there is no `#[allow(dead_code)]`, and no file
  passes 1 500 lines.

## 4. How to work

- **Branches:** everything goes straight to `main`, as `AGENTS.md`
  requires, even when the session comes with another branch assigned. Run
  `git pull --ff-only` before starting and before each push. Never
  `--force`, `reset --hard` on pushed commits, or rebase of published
  history. Other sessions push to `main` at the same time, in malaysia and
  in barrierkit.
- **Commits:** conventional commits with the message in Spanish, like the
  history (`build: …`, `docs(adr): …`, `refactor(compositor): …`). New
  dependencies are justified in the commit. Never put the model identifier
  in commits or files.
- **Tracking:** keep the phases and their steps in the task list and check
  them off as you go.
- **Research:** follow rule 10 in its order. First the pinned code
  (`~/.cargo/registry/src`, `~/.cargo/git/checkouts`) and the official
  documentation; then issues and changelogs; forums only as a hint. The chat
  gets the conclusion with its link.
- **Honesty:** say what you could not run. If the dev shell does not build,
  do not call a phase done. Without a session, say that no window was
  opened.
- **At the end of each phase:** 1–3 lines with what changed, the
  measurement and what is left.

Start with step 0: read and summarise. Then phase 0.
