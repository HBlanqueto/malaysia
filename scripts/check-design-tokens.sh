#!/bin/sh
# Fails when a crate outside vow-design hardcodes a design value.
#
# Colours are checked in malaysia-compositor and malaysia-shell. Test modules are
# skipped (the awk below stops at #[cfg(test)]).
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

status=0
report() {
    printf 'design-tokens: %s\n' "$1" >&2
    shift
    printf '%s\n' "$@" >&2
    status=1
}

production() {
    for file in $(find "$@" -name '*.rs' | sort); do
        awk '/#\[cfg\(test\)\]/{exit} {print FILENAME ":" FNR ":" $0}' "$file"
    done
}

if matches=$(production crates/malaysia-compositor/src crates/malaysia-shell/src |
    grep -E '0x[0-9a-fA-F]{6,8}|Rgba::(rgb8|rgba8)'); then
    report "colour literals must live in vow-design" "$matches"
fi

if matches=$(production crates/malaysia-shell/src |
    grep -E '\[u8; ?4\]'); then
    report "convert colours with Rgba::to_rgba8, not a local [u8; 4]" "$matches"
fi

if matches=$(production \
    crates/malaysia-shell/src/menubar \
    crates/malaysia-shell/src/dock.rs \
    crates/malaysia-shell/src/lock.rs |
    grep -E ':[0-9]+:[[:space:]]*const [A-Z0-9_]+: (i32|u32|f32) = [0-9]'); then
    report "UI measurements must live in vow-design" "$matches"
fi

exit "$status"
