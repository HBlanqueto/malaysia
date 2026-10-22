#!/bin/sh
# Fails when a crate outside yuki-design hardcodes a design value.
#
# Colours are checked in yuki, yuki-shell and celestial-kit. Test modules are
# skipped (the awk below stops at #[cfg(test)]); buffer formats ([u8; 4]) are
# allowed in the compositor and only forbidden in the shell/toolkit, where the
# single conversion helper must be Rgba::to_rgba8.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
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

if matches=$(production crates/yuki/src crates/yuki-shell/src crates/celestial-kit/src |
    grep -E '0x[0-9a-fA-F]{6,8}|Rgba::(rgb8|rgba8)'); then
    report "colour literals must live in yuki-design" "$matches"
fi

if matches=$(production crates/yuki-shell/src crates/celestial-kit/src |
    grep -E '\[u8; ?4\]'); then
    report "convert colours with Rgba::to_rgba8, not a local [u8; 4]" "$matches"
fi

ui_files="crates/yuki-shell/src/menubar.rs
crates/yuki-shell/src/dock.rs
crates/yuki-shell/src/lock.rs
crates/celestial-kit/src/password.rs
crates/celestial-kit/src/text.rs"
if matches=$(production $ui_files |
    grep -E ':[0-9]+:[[:space:]]*const [A-Z0-9_]+: (i32|u32|f32) = [0-9]'); then
    report "UI measurements must live in yuki-design" "$matches"
fi

exit "$status"
