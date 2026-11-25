# Malaysia

Malaysia is a Wayland compositor written in Rust on
[Smithay](https://github.com/Smithay/smithay).

It is being rebuilt from its foundation, one dependency at a time. Today it
opens a display directly through DRM/KMS and fills it with one color. It does
not show windows or take input yet. [ROADMAP.md](ROADMAP.md) says which step
is in progress and what comes next.

## Build and run

The compositor needs Linux. The development shell comes from Nix:

```sh
nix develop
cargo run -- /dev/dri/card0
```

The argument is the DRM device to drive. The compositor logs the mode it chose
and the name of its Wayland socket, and stops with Ctrl+C. `RUST_LOG` sets the
log level per module, for example `RUST_LOG=smithay=debug`.

On a Mac, `vm.sh` runs the same commands in a Linux box with a virtual
display:

```sh
./vm.sh up
./vm.sh run nix develop -c cargo run -- /dev/dri/card0
./vm.sh shot screen.png
```

## License

MIT. See [LICENSE](LICENSE).
