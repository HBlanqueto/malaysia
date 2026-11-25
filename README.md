# Malaysia

Malaysia is a Wayland compositor written in Rust on
[Smithay](https://github.com/Smithay/smithay).

It is being rebuilt from its foundation, one dependency at a time. Today it
draws the windows of Wayland clients that use `xdg_shell` and shared-memory
buffers, and gives them a keyboard and a pointer. It runs in two ways:

- **Directly on a display.** Started from a TTY by a normal user, it takes its
  session from libseat (logind or seatd), finds the GPU through udev, drives
  the display through DRM/KMS and reads the keyboard and the mouse through
  libinput.
- **Nested.** Started inside a Wayland or X11 session, it is a window of that
  desktop.

[ROADMAP.md](ROADMAP.md) says which step is in progress, what is missing and
what has not been verified on real hardware.

## Build and run

The compositor needs Linux. The development shell comes from Nix:

```sh
nix develop
cargo run                      # nested inside a session, direct from a TTY
cargo run -- /dev/dri/card0    # directly on that DRM device, in any case
```

The compositor logs the backend it chose and the name of its Wayland socket.
Clients reach it through that name:

```sh
WAYLAND_DISPLAY=wayland-2 foot
```

Directly on a display, Ctrl+Alt+Backspace quits and Ctrl+Alt+F1 to F12 switch
to another VT.

`RUST_LOG` sets the log level per module, for example
`RUST_LOG=smithay=debug`.

`nix build` builds the compositor as a package. `nix build .#checks.<system>.vm`
boots a NixOS machine, logs a normal user in on a TTY, runs the compositor
directly on the virtual GPU, types into a terminal, moves the mouse and checks
the cursor, switches to another VT and back, types again and quits.

On a Mac, `vm.sh` runs the same commands in a Linux box that has a virtual
display and a headless desktop:

```sh
./vm.sh up
./vm.sh run nix develop -c cargo run -- /dev/dri/card0
./vm.sh shot screen.png
./vm.sh test shots             # the NixOS machine check
```

## License

MIT. See [LICENSE](LICENSE).
