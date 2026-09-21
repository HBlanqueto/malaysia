# neptune

A Wayland compositor built on Smithay, developed block by block. Each block is
a working increment: the checkbox is **can run**, meaning the command after it
starts that block's behaviour and the check proves it.

## Blocks

The canonical block list and each block's acceptance test live in
[`docs/blocks/roadmap.md`](docs/blocks/roadmap.md). The checkbox is **can run**:
the command after it starts that block's behaviour.

- [x] **Block 0 — Scaffolding.** `./dev.sh run`
- [x] **Block 1 — Getting a client to connect.** `./dev.sh run`; `wayland-info`
      lists every global.
- [x] **Block 2 — A window on screen.** `./dev.sh run`, checked by
      `./dev.sh smoke`.
- [x] **Block 3 — Input.** `./dev.sh run`, checked by `./dev.sh smoke`; typing
      and the rendered cursor are hand-checked.
- [x] **Block 4 — Manipulable windows.** `./dev.sh block4` opens and closes 50
      windows and asserts the scene empties; drag feel and resize visuals are
      hand-checked.
- [ ] **Block 5 — Popups.** Not yet runnable.
- [ ] **Block 6 — Clipboard.** Not yet runnable.

The per-block documents live in `docs/blocks/`, decisions in `docs/decisions/`.

## Development

Building and testing run inside the dev shell, on the machine's graphical
session (`run`, `stop`, `restart`, `status` and `log` also work from a plain
shell — ADR 0007):

```sh
nix develop
./dev.sh run       # start the compositor nested in a winit window
./dev.sh smoke     # build and run the toplevel mapping smoke test
./dev.sh check     # formatting, clippy and tests
./dev.sh --help    # all commands
```
