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
  # `cargo tree -i pkg` now returns 0 with an empty "nothing to print" line when
  # the package is only reachable for non-host targets; check actual output.
  if cargo tree "$@" -i "$package" 2>/dev/null | grep -v '^warning:' | grep -q .; then
    echo "check-client-only-deps: $package must not be in the graph ($*)" >&2
    fail=1
  fi
}

# The compositor's package must not reach the client render stack.
absent freya-engine -p malaysia-compositor
absent freya-skia-bindings -p malaysia-compositor
absent freya-skia-safe -p malaysia-compositor
absent skia-bindings -p malaysia-compositor

# The window backend is the compositor's own (smithay backend_winit, ADR 0009),
# so `winit` is expected under `malaysia-compositor`; it must not reach a client crate.
absent winit -p malaysia-shell
absent tokio --workspace
absent gtk --workspace
absent glutin --workspace
absent tray-icon --workspace

exit "$fail"
