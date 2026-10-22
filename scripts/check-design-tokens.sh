#!/bin/sh
# Fails when the shell or the toolkit hardcodes a design value instead of
# reading it from yuki-design. Algorithm constants (sampling, PAM, pixel
# formats) are allowed; colours and UI measurements are not.
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

if matches=$(grep -rnE --include='*.rs' '0x[0-9a-fA-F]{6,8}' \
    crates/yuki-shell/src crates/celestial-kit/src); then
    report "hex colour literals must live in yuki-design" "$matches"
fi

if matches=$(grep -rnE --include='*.rs' 'Rgba::(rgb8|rgba8)' \
    crates/yuki-shell/src crates/celestial-kit/src); then
    report "Rgba constructors must live in yuki-design" "$matches"
fi

if matches=$(grep -rnE --include='*.rs' '\[u8; ?4\]' \
    crates/yuki-shell/src crates/celestial-kit/src); then
    report "convert colours with Rgba::to_rgba8, not a local [u8; 4]" "$matches"
fi

ui_files="crates/yuki-shell/src/menubar.rs
crates/yuki-shell/src/dock.rs
crates/yuki-shell/src/lock.rs
crates/celestial-kit/src/password.rs
crates/celestial-kit/src/text.rs"
if matches=$(grep -nE '^[[:space:]]*const [A-Z0-9_]+: (i32|u32|f32) = [0-9]' $ui_files); then
    report "UI measurements must live in yuki-design" "$matches"
fi

exit "$status"
