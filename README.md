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
`malaysia-session.target` with the `menubar` and `dock` components
(`programs.malaysia.shellComponents`) and the wallpaper
(`programs.malaysia.wallpaper.enable`). The display manager runs
`malaysia-compositor --drm`, the direct session of Block 8a.

## Build

Everything builds inside the dev shell (ADR 0007):

```sh
nix develop
./dev.sh check            # fmt, lints, links and tests
./dev.sh run              # nested compositor
./dev.sh demo             # nested session with the shell
./dev.sh --help           # every command
```

[`docs/README.md`](docs/README.md#build-and-test) explains the profiles and
the Nix package. Configuration is one TOML `.domain` file per domain under
`~/.config/malaysia/`; the catalog is
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
