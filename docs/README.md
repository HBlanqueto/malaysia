# Malaysia documentation

Index. The status lives in [`roadmap.md`](roadmap.md); the why, in the ADRs;
the open findings, in [`debt.md`](debt.md).

## Map

```
docs/
├── README.md                  this index
├── roadmap.md                 status by block and «Now»
├── debt.md                    open technical debt
├── glossary.md                project names (BarrierKit, Grade, Infinity)
├── hardware.md                GPU/EGL baseline of the machine
├── research.md                macOS, GNOME and Otto conclusions
├── macos-global-menu-guide.md how the macOS global menu behaves
├── macos-desktop-architecture.md macOS processes, preferences and APIs against Malaysia
├── remake-redo.md             tracking of the running Remake-Redo project
├── prompts/remake-redo.md     the Remake-Redo brief
├── architecture/              what the UI and the shell look like today
│   ├── interface.md           full visual map
│   ├── process-model.md       processes and split
│   ├── windows.md             titlebar, frame and decoration
│   ├── wallpaper.md           malaysia-wallpaper (wgpu/Vulkan)
│   ├── menubar.md             top bar and menus
│   ├── dock.md                dock
│   ├── lock.md                lock screen
│   ├── captures.md            screenshots
│   └── malaysia-and-shell.svg diagram of the split
├── decisions/                 ADRs and their index
├── assets/brand/              banner (README) and icon, SVG sources and exports
└── configuration/
    └── options.md             canonical options catalog
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

Day-to-day work uses the `dev` profile: our code unoptimized with line
tables; dependencies at `opt-level = 0` without debug info, except the ones
that cost at run time (Smithay, the `freya-*` crates, `torin`, `ragnarok`,
`cosmic-text`, `swash` and the image codecs), which keep `opt-level = 2`. The
list is the `[profile.dev.package.*]` tables of the workspace `Cargo.toml`. The Nix package uses `dist` (fat
LTO, `codegen-units = 1`) and splits `buildDepsOnly` from `buildPackage` with crane.
Skia arrives prebuilt from barrierkit's `skia-binaries` package through
`SKIA_BINARIES_URL`.
