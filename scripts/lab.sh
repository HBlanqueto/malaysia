#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

if [ "${MALAYSIA_DEV_SHELL:-}" != "1" ]; then
    printf 'scripts/lab.sh: not inside the dev shell; run nix develop first (see ADR 0007)\n' >&2
    exit 7
fi

git pull --ff-only
./dev.sh build
exec ./dev.sh lab "$@"
