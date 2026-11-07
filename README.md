# Malaysia

![Malaysia Compositor](docs/assets/marca/malaysia-banner.jpg)

Wayland desktop for NixOS. Its compositor, *Malaysia Compositor*
(`malaysia-compositor`), is built on Smithay and is developed block by block. The
full documentation lives in [`docs/`](docs/README.md); the status, in
[`docs/roadmap.md`](docs/roadmap.md).

> **Repository note (ADR 0048):** `bk`, `ctdesign`,
> `ctappearance` and `infinity-protocol` live in
> [`github.com/HBlanqueto/barrierkit`](https://github.com/HBlanqueto/barrierkit).
> This repo holds the compositor, the shell, Settings and the portals.

> [!NOTE]
> Vibecoded project, built for personal enjoyment. Its intended use is the daily desktop.

## NixOS

The flake exposes `nixosModules.default`:

```nix
{
  inputs.malaysia.url = "github:HBlanqueto/malaysia";

  outputs = { nixpkgs, malaysia, ... }: {
    nixosConfigurations.mi-pc = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        malaysia.nixosModules.default
        { programs.malaysia.enable = true; }
      ];
    };
  };
}
```

`programs.malaysia.enable` installs the package and the
[CurseTerm](https://github.com/HBlanqueto/curseterm) terminal (the flake's
`curseterm` input), registers the session in the display
manager, creates `/etc/pam.d/malaysia-lock`, enables `hardware.graphics` and starts
`malaysia-session.target` with `menubar`, `dock` and `wallpaper` (configurable with
`programs.malaysia.shellComponents`). The display manager runs
`malaysia-compositor --drm`, the direct session of Block 8a.

## General facts

| Fact | Status |
|---|---|
| Language | Rust (2024 edition) |
| Framework | Smithay, pinned by revision (`023b78b1`, ADR 0028) |
| Backend | Nested Winit for development; direct DRM/KMS accepted on a real panel (8a) |
| Graphics | EGL + OpenGL ES 3; Vulkan only in clients (e.g. wallpaper, vkcube) |
| Shell | Native `malaysia-shell`, one process per component (ADR 0023) |

## Commands

```sh
nix develop
./dev.sh build            # builds the workspace
./dev.sh run              # nested compositor
./dev.sh demo             # shell + CurseTerm + test windows
./dev.sh lab              # shell + CurseTerm + real apps
./dev.sh check            # fmt + clippy + tests
./dev.sh titlebar         # close/minimize/maximize
./dev.sh smoke            # toplevel mapping
./dev.sh drm-vkms         # Direct Session on vkms
nix build .#default       # installable package
```

Each shell component is a process:

```sh
malaysia-wallpaper         # background (Vulkan/wgpu, ADR 0049)
malaysia-shell menubar     # top bar
malaysia-shell dock        # dock
malaysia-shell lock        # lock
malaysia-shell launcher    # launcher
```

## Configuration (ADR 0021)

The agreed interface is the `domain` command, with the syntax of macOS's
`defaults`. It will talk to the compositor over D-Bus (`org.malaysia.Compositor1`); the
binary does not exist yet:

```sh
domain read   <domain> <key>
domain write  <domain> <key> <value> [-bool | -int | -float | -string]
domain delete <domain> <key>
domain rename <domain> <old> <new>
domain find   <word>
domain domains
```

One TOML file per domain, like macOS's `.plist` files:

```text
~/.config/malaysia/                 preferences
├── appearance.domain
├── window.domain
├── dock.domain
├── menubar.domain
├── lock.domain
├── display.domain
├── screenshot.domain
└── effects.domain

~/.local/share/malaysia/            state and data
├── wallpaper.domain            catalog + selection
└── window.domain               active app (written by the compositor)
```

Each file is optional; if it is missing, the defaults apply.
`MALAYSIA_CONFIG_DIR` and `MALAYSIA_DATA_DIR` select other directories.
The full catalog, with types and statuses, is in
[`docs/configuration/options.md`](docs/configuration/options.md).

## Documentation

- [`docs/README.md`](docs/README.md) — index and how to build/test.
- [`docs/roadmap.md`](docs/roadmap.md) — status by block and pending items.
- [`docs/architecture/interface.md`](docs/architecture/interface.md) — visual map.
- [`docs/decisions/`](docs/decisions/) — ADRs.
- [`docs/debt.md`](docs/debt.md) — open technical debt.
- [`docs/research.md`](docs/research.md) — conclusions from macOS,
  GNOME and Otto.

`LICENSE` and `NOTICE` cover the license and the third-party notices.
