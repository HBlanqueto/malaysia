# Malaysia

Malaysia is a Wayland compositor written in Rust on
[Smithay](https://github.com/Smithay/smithay).

It is being rebuilt from its foundation, one dependency at a time. Today it
draws the windows of Wayland clients that use `xdg_shell` and shared-memory
buffers, either directly on a display through DRM/KMS or nested as a window in
another desktop. Nested, it takes keyboard and pointer input: a terminal such
as `foot` works. Directly on a display it has no input yet. [ROADMAP.md](ROADMAP.md) says
which step is in progress and what comes next.

## Build and run

The compositor needs Linux. The development shell comes from Nix:

```sh
nix develop
cargo run                      # nested, inside a Wayland or X11 session
cargo run -- /dev/dri/card0    # directly on that DRM device
```

The compositor logs the backend it chose and the name of its Wayland socket.
Clients reach it through that name:

```sh
WAYLAND_DISPLAY=wayland-2 foot
```

`RUST_LOG` sets the log level per module, for example
`RUST_LOG=smithay=debug`.

`nix build` builds the compositor as a package. `nix build .#checks.<system>.vm`
boots a NixOS machine, runs the compositor directly on its virtual GPU and
checks that a terminal is drawn.

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
