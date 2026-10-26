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
    nix eval --raw -f crates/vow-design/neptunia "$attr" > "$tmp/$attr"
    if ! diff -u "$file" "$tmp/$attr"; then
        printf 'neptunia-generated: %s is stale; regenerate it\n' "$file" >&2
        exit 1
    fi
}

check slint crates/vow-design/neptunia/generated/neptunia.slint
check css crates/vow-design/neptunia/generated/neptunia.css
check qml crates/vow-design/neptunia/generated/Neptunia.qml
