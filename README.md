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
- [ ] **Block 3 — Input.** Partly runnable: keyboard, pointer and serials work;
      the cursor is not implemented.
- [ ] **Block 4 — Manipulable windows.** Not yet runnable.
- [ ] **Block 5 — Popups.** Not yet runnable.
- [ ] **Block 6 — Clipboard.** Not yet runnable.

The per-block documents live in `docs/blocks/`, decisions in `docs/decisions/`.

## Development

Everything runs inside the dev shell, on the machine's graphical session:

```sh
nix develop
./dev.sh run       # start the compositor nested in a winit window
./dev.sh smoke     # build and run the toplevel mapping smoke test
./dev.sh check     # formatting, clippy and tests
./dev.sh --help    # all commands
```
