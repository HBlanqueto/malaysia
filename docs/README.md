# Malaysia documentation

Index. The status lives in [`roadmap.md`](roadmap.md); the why, in the ADRs;
the open findings, in [`debt.md`](debt.md).

## Map

```
docs/
├── README.md            this index
├── roadmap.md           status by block and «Now»
├── debt.md             open technical debt
├── glossary.md          project names (BarrierKit, Grade, Infinity)
├── hardware.md          GPU/EGL baseline of the machine
├── research.md     macOS/GNOME/Otto conclusions
├── audit-2026-11-03.md  full audit (dead code, performance, niri)
├── architecture/        what the UI and the shell look like TODAY
│   ├── interface.md          full visual map
│   ├── process-model.md  processes and split
│   ├── windows.md          titlebar, frame and decoration
│   ├── wallpaper.md         malaysia-wallpaper (wgpu/Vulkan)
│   ├── menubar.md           top bar and menus
│   ├── dock.md              dock
│   ├── lock.md              lock screen
│   ├── captures.md          screenshots
│   └── malaysia-and-shell.svg diagram of the split
├── decisions/          ADRs
├── configuration/
    └── options.md      canonical options catalog
```

## Build and test

The dev shell is mandatory for cargo (ADR 0007):

```sh
nix develop
./dev.sh build            # debug build of the default members
./dev.sh run              # nested compositor
./dev.sh demo             # nested session with shell, CurseTerm and test windows
./dev.sh lab              # nested session with CurseTerm and real apps
./dev.sh check            # fmt + clippy + tests
./dev.sh titlebar         # close/minimize/maximize
./dev.sh smoke            # toplevel mapping
./dev.sh drm-vkms         # Direct Session on vkms
nix build .#default       # installable package
```

Day-to-day work uses the `dev` profile: our code unoptimized, dependencies at
`opt-level = 2` and `debug = "line-tables-only"`. The Nix package uses `dist` (fat
LTO, `codegen-units = 1`) and splits `buildDepsOnly` from `buildPackage` with crane.
Skia arrives prebuilt from `nix/skia.nix` through `SKIA_BINARIES_URL`.


## Status

Blocks 0–15 are done or accepted, except the pending items the roadmap
details; 16a (Vulkan background) is in progress. Right now the following are missing: focus mode,
window capture with frame, the rest of the global menu (14b-2/14e/14f-2), the
12a portal (`Screenshot`/`ScreenCast`), the 12b services, 8b/8c and the NixOS
options for polkit/keyring/AT-SPI. For details, dates and test commands,
read [`roadmap.md`](roadmap.md).
