#!/usr/bin/env bash
# Client-only dependency guard (ADR 0034).
#
# The compositor must not link Freya, Skia, winit, GTK, glutin, tokio or the
# tray icon stack; those belong to the shell and only the shell. The checks run
# against the resolved workspace graph, so they cost no compile.
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

# absent PACKAGE [extra cargo-tree args...] - fail if PACKAGE is reachable.
absent() {
  local package=$1
  shift
  if cargo tree "$@" -i "$package" >/dev/null 2>&1; then
    echo "check-client-only-deps: $package must not be in the graph ($*)" >&2
    fail=1
  fi
}

# The compositor's package must not reach the client render stack.
absent freya-engine -p yuki
absent freya-skia-bindings -p yuki
absent freya-skia-safe -p yuki
absent skia-bindings -p yuki

# The window backend is the compositor's own (smithay backend_winit, ADR 0009),
# so `winit` is expected under `yuki`; it must not reach a client crate.
absent winit -p celestial-kit
absent winit -p yuki-shell
absent tokio --workspace
absent gtk --workspace
absent glutin --workspace
absent tray-icon --workspace

# The clipboard must go through wl_data_device. freya-clipboard pulls
# copypasta (and thus x11rb); report it instead of hiding it. The alternative
# is the kit's own wl_data_device clipboard, which is deferred until a control
# needs it.
if cargo tree --workspace -i x11rb >/dev/null 2>&1; then
  echo "check-client-only-deps: WARNING: x11rb reached the graph via freya-clipboard; use wl_data_device instead" >&2
fi
if cargo tree --workspace -i copypasta >/dev/null 2>&1; then
  echo "check-client-only-deps: WARNING: copypasta reached the graph via freya-clipboard; the kit's clipboard is empty for now" >&2
fi

# Duplicate versions in the kit that are avoidable are build noise and bloat.
if cargo tree -d -p celestial-kit 2>/dev/null | grep -q '^[a-z]'; then
  echo "check-client-only-deps: duplicated crates in celestial-kit:" >&2
  cargo tree -d -p celestial-kit >&2 || true
fi

exit "$fail"
