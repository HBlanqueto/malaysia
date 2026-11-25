# Malaysia

Malaysia is a Wayland compositor written in Rust on
[Smithay](https://github.com/Smithay/smithay).

It is being rebuilt from its foundation, one dependency at a time. Today it
draws the windows of Wayland clients that use `xdg_shell` and shared-memory
buffers, and gives them a keyboard and a pointer. It runs in two ways:

- **Directly on a display.** Started from a TTY by a normal user, it takes its
  session from libseat, finds the GPU through udev, drives the display through
  DRM/KMS and reads the keyboard and the mouse through libinput.
- **Nested.** Started inside a Wayland session, it is a window of that desktop.

[ROADMAP.md](ROADMAP.md) says which step is in progress, what is missing and
what has not been verified. Nothing has run on real hardware yet.

## Build

The compositor needs Linux. The development shell comes from Nix:

```sh
nix develop
cargo build
```

`nix build` builds it as a package.

## Run

```sh
cargo run                      # nested inside a session, direct from a TTY
cargo run -- /dev/dri/card0    # directly on that DRM device, in any case
```

The compositor logs the backend it chose and the name of its Wayland socket,
normally `wayland-1`. It starts no program: clients reach it through that
name.

```sh
WAYLAND_DISPLAY=wayland-1 foot
```

Directly on a display, Ctrl+Alt+Backspace quits and Ctrl+Alt+F1 to F12 switch
to another VT. `RUST_LOG` sets the log level per module, for example
`RUST_LOG=smithay=debug`.

### First run on a real machine

Directly on a display the screen is in graphics mode, so the log cannot be
read there and a second terminal is not at hand. This starts a terminal three
seconds after the compositor, keeps the log in a file, and ends the compositor
after two minutes in case the keyboard does not answer:

```sh
(sleep 3; WAYLAND_DISPLAY=wayland-1 foot) &
timeout 120 malaysia-compositor 2>~/malaysia.log
```

What it needs:

- A logind session or seatd. No group membership is needed with logind.
- The active VT: start it from the TTY that is on the screen.
- polkit, for Ctrl+Alt+F1 to F12 under logind. Without it the switch is
  refused and logged.
- `XKB_DEFAULT_LAYOUT`, for a keyboard that is not US, for example
  `XKB_DEFAULT_LAYOUT=es`.

Clients draw with software rendering for now.

## Check

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo test
nix build .#checks.<system>.vm
```

The last one boots a NixOS machine, logs a normal user in on a TTY, runs the
compositor directly on the virtual GPU, types into a terminal, moves the mouse
and checks the cursor, switches to another VT and back, types again and quits.

On a Mac, `vm.sh` runs all of this in a Linux box that has a virtual display
and a headless desktop:

```sh
./vm.sh up
./vm.sh run nix develop -c cargo run -- /dev/dri/card0
./vm.sh shot screen.png        # capture the virtual display
./vm.sh test shots             # the NixOS machine check, with screenshots
```

## License

MIT. See [LICENSE](LICENSE).
