#!/bin/sh
# Fail when generated/ differs from what default.nix produces.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

check() {
    attr=$1
    file=$2
    nix eval --raw -f crates/vow-design/tengen "$attr" > "$tmp/$attr"
    if ! diff -u "$file" "$tmp/$attr"; then
        printf 'tengen-generated: %s is stale; regenerate it\n' "$file" >&2
        exit 1
    fi
}

check slint crates/vow-design/tengen/generated/tengen.slint
check css crates/vow-design/tengen/generated/tengen.css
check qml crates/vow-design/tengen/generated/Tengen.qml
