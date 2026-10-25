#!/bin/sh
# Build and run the nested demo from a clean checkout.
#
# POSIX sh. Run it from the repository root (or anywhere: it changes there
# first). It refuses to run outside the dev shell for the same reason dev.sh
# does (ADR 0007).
set -eu

cd "$(dirname "$0")/.."

if [ "${YUKI_DEV_SHELL:-}" != "1" ]; then
    printf 'scripts/demo.sh: not inside the dev shell; run nix develop first (see ADR 0007)\n' >&2
    exit 7
fi

git pull --ff-only
./dev.sh build
exec ./dev.sh demo "$@"
