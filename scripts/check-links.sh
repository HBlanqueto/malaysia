#!/bin/sh
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

anchors() {
    awk '/^[[:space:]]*```/ { fenced = !fenced; next }
        !fenced && /^#+ / {
            sub(/^#+ /, "")
            slug = tolower($0)
            gsub(/[^a-z0-9 _-]/, "", slug)
            gsub(/ /, "-", slug)
            print slug
        }' "$1"
}

broken=$(git ls-files '*.md' | while read -r file; do
    awk '/^[[:space:]]*```/ { fenced = !fenced; next }
        !fenced {
            line = $0
            while (match(line, /\]\([^)[:space:]]+\)/)) {
                print FILENAME "\t" substr(line, RSTART + 2, RLENGTH - 3)
                line = substr(line, RSTART + RLENGTH)
            }
        }' "$file"
done | while IFS="$(printf '\t')" read -r file target; do
    case $target in
        *://* | mailto:*) continue ;;
    esac
    path=${target%%#*}
    if [ -n "$path" ]; then
        path="$(dirname -- "$file")/$path"
        [ -e "$path" ] || { printf '%s: %s\n' "$file" "$target"; continue; }
    else
        path=$file
    fi
    case $target in
        *#*) ;;
        *) continue ;;
    esac
    case $path in
        *.md) anchors "$path" | grep -qx -- "${target#*#}" || printf '%s: %s\n' "$file" "$target" ;;
    esac
done)

if [ -n "$broken" ]; then
    printf 'check-links: broken relative link\n%s\n' "$broken" >&2
    exit 1
fi
