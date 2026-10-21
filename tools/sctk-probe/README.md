# sctk-probe

Dev-only Wayland clients for driving Yuki while developing. This package is
**not** part of the compositor: it is a workspace member excluded from the
default build (`default-members` in the root `Cargo.toml`). A plain
`cargo build`/`cargo test` at the repository root will not build it; name the
package to build it on purpose.

The clients here connect to whatever `WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR` point
at, so they can be run against Yuki or the host session without any change.
`smoke` and `block4` are different: they launch Yuki itself and drive clients
against it.

Everything here runs inside the dev shell (`nix develop`). The simplest way to
run the smoke test is the project script, which builds both the compositor and
this harness and propagates the result:

```sh
./dev.sh smoke
```

The sections below are the underlying `cargo` commands, for when you want to run
one binary directly.

## `parent_probe`

Demonstrates that decoration insets are a function of a window's *current*
state, not of the state it had at map time. It maps two toplevels and toggles a
parent on the child while both are tracked:

- initial (no parent) — decorated, configured one left-titlebar narrower;
- `set_parent` — the child becomes a dialog, so it is reconfigured wider;
- unset — the child returns to exactly its initial size.

The probe checks the three configure sizes and exits non-zero if the cycle is
not reproduced, so it is a pass/fail check rather than output to read by eye.

Run it against a running compositor. While Yuki runs under `./dev.sh run`,
the compositor inherits the caller's `XDG_RUNTIME_DIR` and auto-names its socket
there (Smithay picks the first free `wayland-N`, `wayland-1` when the host
already holds `wayland-0`); `yuki-dev` holds the pid and the log, not the
socket. Read the name the compositor logged rather than assuming one:

```sh
runtime="${XDG_RUNTIME_DIR:?the compositor inherits this for its socket}"
# The compositor logs the name it bound, inside ANSI colour codes.
sock=$(sed 's/\x1b\[[0-9;]*m//g' "$runtime/yuki-dev/yuki.log" |
    sed -n 's/.*socket=\(wayland-[0-9]*\).*/\1/p' | tail -n 1)
XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="$sock" \
    cargo run -p sctk-probe --bin parent_probe
```

## `smoke`

An assertion-based smoke test for the toplevel mapping sequence. It launches the
compositor, starts a real client (`foot` by default) with `WAYLAND_DEBUG=1`,
captures the client's trace, and checks, in order: `xdg_toplevel.configure`
before the first non-null attach; a matching `ack_configure` serial before that
attach; a non-null buffered commit that the compositor reports as mapped;
`wl_keyboard.enter` on the mapped surface; and completion of the *specific*
callback id requested by `wl_surface.frame`. A bare `wl_callback.done` is not
accepted, so a `wl_display.sync` round-trip cannot be mistaken for a frame.

It exits non-zero if any step is missing or out of order.

```sh
./dev.sh smoke          # builds yuki and the harness, then runs it
cargo run -p sctk-probe --bin smoke   # run it directly, if already built
```

Environment overrides: `YUKI_BIN` (path to the compositor binary) and
`YUKI_SMOKE_CLIENT` (the client to drive).

## `block4`

The Block 4 acceptance harness. It launches the compositor and three batches of
`window_stress`, each opening and destroying fifty decorated toplevels. It
asserts the compositor's live-object counts (clients, `wl_surface`s, tracked
toplevels) return to the startup baseline after every batch, and that resident
memory does not grow from batch 2 to batch 3 by more than 1 MiB (the first batch
is warm-up). Both are asserted; the three measurements are printed.

It then runs three resize scenarios, each with a fresh compositor, a fresh
`resize_client` and `YUKI_TEST_INPUT=1`: the compositor presses a corner or
an edge of the mapped window and its scripted motions drive the grab. The
harness asserts the final configured size — corner growth with no client
maximum, the same growth clamped to a client maximum, and a shrunk width held
by a client minimum:

```
block4: resize grow PASS (initial 1280x800, final 1328x832, expected 1328x832)
block4: resize maximum PASS (initial 1280x800, final 1290x810, expected 1290x810)
block4: resize minimum PASS (initial 1280x800, final 1270x800, expected 1270x800)
```

```sh
./dev.sh block4                        # builds yuki and the probes, then runs it
cargo run -p sctk-probe --bin block4   # run it directly, if already built
```

Environment overrides: `YUKI_BIN`, `YUKI_STRESS_BIN` and
`YUKI_RESIZE_BIN`.

## `window_stress`

The client `block4` drives. It opens fifty decorated toplevels with
shared-memory buffers, waits for a configure on each, then closes every one and
exits. On its own it is only useful to watch a compositor's teardown:

```sh
cargo run -p sctk-probe --bin window_stress
```

## `resize_client`

The client `block4` drives for the resize assertions. It maps a
client-decorated toplevel, applies a bound from `YUKI_RESIZE_MIN` or
`YUKI_RESIZE_MAX` (an `x,y` offset from the initial configure, in surface
coordinates, before it maps), and prints every configure it receives, then the
size once the stream settles:

```
resize_client: configure width=1280 height=800 resizing=false
resize_client: configure width=1328 height=832 resizing=true
resize_client: final width=1328 height=832
```

It is client-decorated so its configured size is its window size, with no
decoration reconfigure racing the first configure. It exits non-zero if it is
never configured or if no resize settles.

## `decoration_probe`

The Phase 6 acceptance harness for the decoration policy (ADR 0019). It launches
one Yuki and, for each real application, runs it inside that compositor with
`WAYLAND_DEBUG=1`, captures the client's trace and asserts what it negotiated:
the decoration protocol it bound (xdg-decoration, the KDE one, or none), the
mode Yuki configured, and whether the surface was inset by the titlebar. The
inset is read from the configure width against the output width, so it holds
even when the first configure already carries it (GTK4 asks before its toplevel
exists). Configures after a client-initiated resize or state change are ignored,
because a host pointer can click into a window and resize it.

Applications are located through `YUKI_APP_<NAME>` variables; one with no
binary is reported `UNVERIFIED` rather than skipped silently, and an application
that exits before mapping a window is reported the same way. Whether two
titlebars are *visible* needs a pixel comparison, so it stays manual.

```sh
./dev.sh decoration                        # builds yuki and the probes, then runs it
cargo run -p sctk-probe --bin decoration_probe   # run it directly, if already built
```

Environment overrides: `YUKI_BIN`, `YUKI_DECORATION_CLIENT` (the
mode-change client, found next to the probe by default), `YUKI_APP_NAUTILUS`,
`YUKI_APP_GTK3_DEMO`, `YUKI_APP_GTK3_WIDGET_FACTORY`,
`YUKI_APP_GTK3_PLAIN`, `YUKI_APP_GTK4_DEMO`,
`YUKI_APP_GTK4_WIDGET_FACTORY`, `YUKI_APP_GTK4_PLAIN`, `YUKI_APP_QT6`
(with `YUKI_APP_QT6_QML_IMPORT_PATH`), `YUKI_APP_QT5` and
`YUKI_APP_ONLYOFFICE`.

`gtk3_plain.c` and `gtk4_plain.c` under `gtk/` are minimal plain-window clients
for the server-side branch, because the GTK demos all use a headerbar. Build
them against the same GTK the probe runs against:

```sh
cc gtk/gtk3_plain.c -o /tmp/gtk3_plain $(pkg-config --cflags --libs gtk+-3.0)
cc gtk/gtk4_plain.c -o /tmp/gtk4_plain $(pkg-config --cflags --libs gtk4-wayland)
YUKI_APP_GTK3_PLAIN=/tmp/gtk3_plain YUKI_APP_GTK4_PLAIN=/tmp/gtk4_plain \
    ./dev.sh decoration
```

## `decoration_client`

The client `decoration_probe` runs for the one policy rule real applications
cannot be made to exercise: a mode change after mapping (ADR 0019, rule 4). It
maps a window asking for server-side decorations, waits for the compositor's
server-side configure (one titlebar narrower), asks for client-side on the first
frame, and asserts the compositor then configures it one titlebar wider. It
exits non-zero if the second configure never arrives or the sizes are wrong.

## `move_probe`

Exercises the client-initiated `xdg_toplevel.move`/`resize` path and the
surface-local coordinates a client sees for pointer motion. It runs three cases,
each with a fresh compositor and `YUKI_TEST_INPUT=1` (the compositor presses
once, holds the button, follows the press with motions and releases it, which a
probe cannot make itself):

- `move_client` repeats a GTK-shaped gesture: press, three motions past a drag
  threshold, a second window that takes focus, then `move` and `resize` with the
  original press serial while the button is held. The harness asserts both were
  accepted, that the window moved by the injected pointer delta, and that the
  released and fabricated serials were rejected for both requests.
- `hover_client` asserts the surface-local coordinates of a buttonless hover:
  the scripted drag moves the window to a non-zero origin and the release ends
  the grab, then the compositor hovers the pointer over the global points in
  `YUKI_TEST_HOVER` and the client must read each at the position it holds
  over the surface. A compositor that hands Smithay the surface-local point
  where it expects the surface origin makes every client read the window origin,
  which fails here.
- Nautilus, a real GTK4 + libadwaita client, gets a scripted headerbar drag; the
  harness asserts its window moved by the same delta. If Nautilus is not
  installed, or never maps a window, the case is reported UNVERIFIED rather than
  failed.

```sh
./dev.sh move                          # builds yuki and the probes, then runs it
cargo run -p sctk-probe --bin move_probe   # run it directly, if already built
```

Environment overrides: `YUKI_BIN`, `YUKI_MOVE_BIN`, `YUKI_HOVER_BIN`
and `YUKI_APP_NAUTILUS`.

## `pointer_lock`

Maps a client-decorated toplevel and, on the first `wl_pointer.enter` (the
compositor's scripted input hovers the pointer over the window), asks
`zwp_pointer_constraints_v1` for a persistent lock with no region. It passes
when the compositor sends `locked`, which it only does after activating the
constraint for the surface the pointer is over. Times out if the global is
advertised but never activated.

## `relative_pointer`

Maps a toplevel, binds `zwp_relative_pointer_manager_v1`, creates a relative
pointer for the seat's pointer and waits for a `relative_motion` with a non-zero
delta from the scripted motions. Times out if the compositor never forwards
motion to relative-pointer clients.

Both are driven by one command, each against its own compositor because the
scripted press is injected once and stays held:

```sh
./dev.sh pointers
```

## `screencopy`

Captures the output through `ext-image-copy-capture-v1`. It creates a source
for the first `wl_output`, opens a session, and on the compositor's constraints
allocates a matching SHM buffer, attaches it to a frame and captures. It passes
on the frame's `Ready` event and additionally checks the centre pixel is the
compositor's clear colour (not blank), which catches a wrong channel order or an
empty copy. It also reports whether the session offered DMA-BUF constraints.

```sh
./dev.sh screencopy
```

## `move_client`

The client `move_probe` drives: it maps a toplevel, waits for the scripted press
and three motions, maps a second window so the compositor raises and focuses it
in between, then sends `move` and `resize` with the original press serial. After
the compositor's release it sends both again with that serial and both with a
fabricated one. It exits non-zero if no resizing configure arrives.

## `hover_client`

The client `move_probe` drives for the coordinate assertion. It maps a
client-decorated window, so its surface origin is exactly its window origin, and
sends the `move` and `resize` the scripted gesture answers: the move makes the
compositor's scripted drag move the window to a non-zero origin, the resize
makes it release the button. It then asserts that the `wl_pointer.enter` and
`wl_pointer.motion` it receives for the buttonless hover carry the surface-local
positions in `YUKI_EXPECT_HOVER` (an `x,y` list separated by `;`), and exits
non-zero on a mismatch or if the hover never arrives.
