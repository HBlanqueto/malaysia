# neptune

A Wayland compositor built on Smithay, developed block by block. Each block is
a working increment: the checkbox is **can run**, meaning the command after it
starts that block's behaviour and the check proves it.

## Blocks

- [x] **Block 0 — Foundations.** It can run: `./dev.sh run` starts, logs and
      exits on a signal. Check: `./dev.sh check`.
- [x] **Block 1 — Nested Wayland session.** It can run: `./dev.sh run` opens a
      nested window that real clients connect to and map in. Check:
      `./dev.sh smoke`. In progress: typing, pointer response and legible
      client output are still hand-checked.
- [ ] **Block 2 — Direct DRM/KMS session.** Not yet runnable. Replaces the winit
      backend with DRM/KMS and the window tracker with `neptune-scene` (dmabuf,
      GBM, EGL).

Further blocks are added here as they are planned. The per-block documents live
in `docs/blocks/`, decisions in `docs/decisions/`.

## Development

Everything runs inside the dev shell, on the machine's graphical session:

```sh
nix develop
./dev.sh run       # start the compositor nested in a winit window
./dev.sh smoke     # build and run the toplevel mapping smoke test
./dev.sh check     # formatting, clippy and tests
./dev.sh --help    # all commands
```
