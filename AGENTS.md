# AGENTS.md — malaysia

## Mission

Research and build Wayland compositors. malaysia is a Wayland compositor in
Rust on [Smithay](https://github.com/smithay/smithay). GNOME and KDE Plasma
are the reference desktops: before you design or change a behavior, find out
how both solve it, then build it on Smithay.

See `README.md`, `docs/architecture.md`, `docs/decisions.md` and
`docs/configuration/options.md`.

> **Repository:** `bk`, `ctdesign`, `ctappearance` and `infinity-protocol`
> live in [`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit).
> Edit barrierkit on its `main` and bump the `rev` in `Cargo.toml`.

## Role

You act as a senior software engineer with a full degree in Systems,
Computer Science and Software Development. Your field is Wayland compositors
and the human environments built on them (desktops and shells), toolkits,
dependencies and libraries, and Unix and Unix-like systems.

Starting sources by area. Those of each pinned dependency are in rule 10 of
«Minimal code».

- **Base:** [Smithay](https://github.com/smithay/smithay) at the rev pinned in
  `Cargo.toml`: its code, its `anvil` example and its
  [API documentation](https://smithay.github.io/smithay/).
- **Reference desktops:** GNOME
  ([mutter](https://gitlab.gnome.org/GNOME/mutter),
  [gnome-shell](https://gitlab.gnome.org/GNOME/gnome-shell)) and KDE Plasma
  ([KWin](https://invent.kde.org/plasma/kwin),
  [plasma-workspace](https://invent.kde.org/plasma/plasma-workspace)).
- **Wayland:** [Wayland documentation](https://wayland.freedesktop.org/docs/html/),
  [The Wayland Book](https://wayland-book.com/) and the protocols on
  [wayland.app](https://wayland.app/protocols/).
- **Human environments:** [GNOME HIG](https://developer.gnome.org/hig/),
  [KDE HIG](https://develop.kde.org/hig/),
  [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/),
  [WCAG 2.2](https://www.w3.org/TR/WCAG22/) and the
  [freedesktop.org specifications](https://specifications.freedesktop.org/).
- **Unix and Unix-like:** [POSIX.1-2024](https://pubs.opengroup.org/onlinepubs/9799919799/),
  the [Linux man-pages](https://man7.org/linux/man-pages/) and
  [The Art of Unix Programming](http://www.catb.org/esr/writings/taoup/html/).
- **Dependencies and libraries:** [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/),
  [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
  and the [Rustonomicon](https://doc.rust-lang.org/nomicon/) for `unsafe`.

## Research, then execution

Each task has two phases, and each phase has its subagent in `.claude/agents/`.

1. **Research: `researcher`.** It reads how mutter and KWin solve the problem,
   what the protocol requires and what Smithay offers at the pinned rev. It
   changes no file. It returns one recommendation with its sources, its reason
   and its risks.
2. **Execution: `executor`.** It makes the smallest complete change that
   applies the recommendation, checks it with `./dev.sh check` and reports what
   changed and what is left.

`scout` does quick searches in this repo for either phase. Skip the research
phase only when the cause and the API are already clear from the code.

## Branches and push (permanent)

- Every change goes **straight to `the-culling-branch`**. Do not commit to
  `main`, do not create other branches, local or remote, and do not open pull
  requests unless the user asks. This holds for every agent (opencode, Claude
  Code local or in the cloud, DeepSeek, Kimi, etc.).
- `git pull --ff-only` before starting and before each push. Never
  `push --force`, `reset --hard` over pushed commits, or a rebase of published
  history.
- `Cargo.lock` is not pushed when generated with `kit-local`; if it shows up
  modified without a dependency change, `git checkout Cargo.lock`.
- The history before 2026-10-28 lives in the tag
  `archivo/historia-hasta-2026-10-26`.

## Commits

Group the work by change and session: one commit per session, not split into
many redundant commits. The whole message is in English. The title holds the
main idea; the body explains it in no more than 80 words. If the session brings
changes of several types, the body is a list by type:

- Feature: …
- Fix: …
- Bug: …

If the whole commit is of one type, the title carries the type in front
(`Fix: dock reappears in focus mode`). No trailer: no `Co-Authored-By`, no
`Claude-Session`, no links to the session.

## Design policy

Applies to decorations, chrome, shadows, blur, borders, themes and shell.

0. **Native shell:** `malaysia-shell`, one binary with one process per
   component (`menubar`, `dock`, `lock`, `launcher`, `force-quit`), each a
   Wayland client built with `bk`. The background is another binary,
   `malaysia-wallpaper`. No external shell (Quickshell removed). It reads
   `.domain`, `~/.local/share/malaysia/` and the `ctdesign` tokens.
1. **GTK/Libadwaita:** they keep their headerbar and look; they get no
   titlebar or controls from the compositor.
2. **Qt and other apps:** they get the compositor's unified chrome
   (server-side titlebar and controls).
3. **Effects for ALL windows**, CSD or SSD: macOS-style inner border, shadow
   (on CSD too) and blur/vibrancy. The compositor composes the blur and covers
   the region the app requests with `ext-background-effect-v1`.
4. **Effects are compositor-side**, not per app (a mechanism like
   `otto-surface-style`). The inside of the app is its own; what surrounds it
   belongs to the compositor.
5. **Design values in `ctdesign`:** radii, shadows, blur, colors and margins.
   No design literals in `crates/malaysia-compositor/src`.
6. **Appearance:** dark/light and accent are published through
   `org.freedesktop.appearance` (xdg-desktop-portal). The compositor reads
   them from its store.
7. **The shell** uses the same language and tokens.
8. **Skia only in clients.** The shell and the own apps (`bk`) may use it. The
   compositor **never** uses Skia or links `freya-*`: it stays on GLES,
   `grade-graphics` and `cosmic-text`. `bk` is built on the Freya core crates
   `=0.5.3` (`freya-core`, `freya-components` without default features,
   `freya-edit`, `freya-animation`, `freya-engine`, `freya-clipboard`, `torin`
   and `ragnarok`), **never** `freya` or `freya-winit`, and uses `ctdesign`.
   Grade Animation is a module of the kit on `freya-animation`, with its own
   spring. Infinity is the private app↔compositor protocol; the compositor
   implements it (`infinity.rs`). `otto-kit`, `otto-foundations`, `otto-dbus`,
   `otto-surface-style` and `laye-rs` are not used.

## Quick references

- **What it is, install and build:** `README.md`.
- **Architecture:** `docs/architecture.md`: processes and binaries, who draws
  and who composes, how they talk and the invariants.
- **Decisions:** `docs/decisions.md`: the why of each decision in force, with
  its ADR number. The toolkit's are in barrierkit's `docs/decisions.md`.
- **Configuration:** `docs/configuration/options.md` is the canonical catalog.
  One TOML `.domain` per domain: preferences in `~/.config/malaysia/`,
  state/`wallpaper` in `~/.local/share/malaysia/`. Override with
  `MALAYSIA_CONFIG_DIR`/`MALAYSIA_DATA_DIR`. The options are typed in
  `crates/malaysia-compositor/src/config.rs`.

## Build and test

```sh
nix develop            # required (sets MALAYSIA_DEV_SHELL)
./dev.sh check         # fmt + clippy + tests
./dev.sh run           # nested compositor
./dev.sh demo          # shell + test windows
./dev.sh titlebar      # close/minimize/maximize
./dev.sh smoke         # toplevel mapping
# probes: move, pointers, screencopy, presentation, frames, activation
```

Do not build outside the dev shell: `build.rs` and `dev.sh` refuse.
NixOS: `nixosModules.default` installs the package, the session, PAM and
`malaysia-session.target`.

## Environment rule

Change nothing outside the repo and `$XDG_RUNTIME_DIR/malaysia`: sockets,
session files or services.

- **Sockets:** only the `wayland-N` served by `malaysia-compositor`, by exact
  name. Never `wayland-0` or another process's sockets.
- **Services:** do not restart mutter, gnome-shell, kwin, plasmashell, the
  session or other processes. If something on the host fails, stop and report
  it.
- **Files:** only captures and dumps in `$XDG_RUNTIME_DIR/malaysia` and `/tmp`.

## Third parties

malaysia is MIT. mutter, gnome-shell, KWin and plasma-workspace are GPL: read
them to learn the behavior and the design, never copy their code. If you copy
code from a third party with a compatible license, update `NOTICE` in the same
commit. Otto is MIT; `laye-rs` has no license and is not used.

## Documentation and comments (top priority)

Do not write documentation (ADR, `docs/`, README, reports or notes) unless the
user asks for it explicitly; when asked, in English. Do not write comments in
the code: Rust (no `///`, `//!` or `// SAFETY:` either), GLSL/WGSL, shell, Nix,
TOML or Fluent. Names explain the code; the why goes in the commit message.

## Minimal code (hard rules, ponytail style)

They are followed to the letter. The shortest diff and the shortest answer win.

1. **Reuse before you write.** Check with `grep` whether the function, the
   token or the module already exists. Each value and each rule lives in one
   place; never copy.
2. **Nothing «just in case».** Do not add parameters, features, traits,
   generics, options or branches that nobody uses today. Dead code is deleted:
   not commented out and not hidden with `#[allow(dead_code)]`.
3. **The most direct way.** No wrappers and no single-use abstractions.
   Extract on the third repetition, not before.
4. **One function, one task; one module, one reason to change.** If a block
   needs a comment to be understood, extract it into a function with that name.
5. **Names replace comments:** `elapsed_days`, not `d`. Booleans shaped as a
   question: `focused`, `zoom_supported`.
6. **No comments.** The why of a workaround goes in the commit message, with
   the link.
7. **Minimal diff.** Touch only what the change asks for: do not reformat or
   rename in passing. The tools set the format and the lints.
8. **Tests only for risky pure logic**, and the name is the specification. If
   you delete code, delete its tests too.
9. **Save tokens.** Read ranges, not whole files. Do not paste in the chat the
   code that is already in the diff. Close with 1–3 lines: what changed and
   what is left.
10. **Research before you assume.** If the cause or the API is not clear,
    look it up online, in this order:
    1. the code and the official documentation of the dependency **at the
       pinned version**: Smithay at the rev of `Cargo.toml`, wayland-protocols
       on wayland.app, the freedesktop and xdg-desktop-portal specifications,
       Mesa and libinput;
    2. how the reference desktops do it: mutter and KWin first, then the
       Smithay compositors niri and cosmic-comp, and Otto for design;
    3. the upstream project's issues, merge requests, changelogs and
       discussions;
    4. forums such as Discourse, Stack Overflow or Reddit, only as a hint and
       checked against 1, 2 or 3.

    Do not adopt a solution you cannot back with a source. If the solution is
    a workaround, put the link in the commit. The chat gets the conclusion
    with the link, not the research.
11. **Fixed budget.** On the hot path (what runs on each frame or input event)
    no memory is allocated that could be reused: no `Vec`, `String`,
    `format!`, `Box` or `clone` of collections per frame. What depends on the
    configuration is computed when it changes, not on each frame.
12. **Data before abstractions.** `const` tables instead of `if` chains;
    `enum` instead of `Box<dyn Trait>` when the set is closed; a concrete type
    instead of a generic with one instance.
13. **Each dependency pays for its build.** `default-features = false` and
    only the features in use. A crate that gives one function is replaced by
    own code if that builds less.
14. **It is measured.** A performance change is compared with the probes of
    `./dev.sh` (`frames`, `presentation`) or with `cargo build --timings`; if
    it does not improve, it is reverted.

### How it applies in malaysia

```rust
// ✗ spawning a client by hand (it inherits the wrapper's private Mesa/EGL)
Command::new(app).spawn()?;
// ✓ the only place that spawns processes
spawn::spawn(spawn::command(app, &config), &config)?;
```

```rust
// ✗ design literal in crates/malaysia-compositor/src
const FADE: Duration = Duration::from_millis(250);
// ✓ ctdesign token
const DURATION: Duration = Duration::from_millis(ctdesign::DURATION_BASE);
```

```rust
// ✓ the name of the test is the specification (malaysia-scene/geometry.rs)
#[test]
fn contains_includes_the_top_left_and_excludes_the_bottom_right() { … }
```

- One module per topic (`cursor.rs`, `lock.rs`, `spaces.rs`…). If a file
  passes about 1500 lines, the next new function goes in its own module.
- A new option is added in `config.rs` and nowhere else.
- The visible text of the shell and of Settings goes in
  `i18n/{en,es}/<crate>.ftl` and is read with `tr!`; never a literal in the
  code. Guide: barrierkit's `docs/barrierkit/17-languages.md`.
- Symbols are SVG, never PNG, with `currentColor` or drawn with barrierkit's
  `TemplateRenderingMode::Template`: black in light and white in dark. App
  icons, photos and illustrations keep their colors.
