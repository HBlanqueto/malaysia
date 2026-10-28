#!/bin/sh
# Bring a Tengen folder into malaysia on purpose. Never run from the build or CI.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
dest="$root/crates/vow-design/tengen"

if [ "$#" -ne 1 ]; then
    printf 'usage: %s <tengen-directory>\n' "$0" >&2
    exit 2
fi

src=$1
if [ ! -d "$src" ]; then
    printf 'sync-tengen: %s is not a directory\n' "$src" >&2
    exit 2
fi
src=$(CDPATH= cd -- "$src" && pwd)

printf 'sync-tengen: diff %s -> %s\n' "$src" "$dest" >&2
diff -ru --exclude .git "$dest" "$src" || true

printf 'sync-tengen: copying\n' >&2
mkdir -p "$dest"
cp -a "$src"/. "$dest"/

origin_commit=$(git -C "$src" rev-parse --short HEAD 2>/dev/null || printf 'desconocido')
copy_date=$(date -u +%Y-%m-%d)
cat > "$dest/SOURCE.md" <<SOURCE
# Origen de esta copia

- **Procedencia:** \`$src\`.
- **Commit de origen:** \`$origin_commit\`.
- **Fecha de la copia:** $copy_date.

Esta carpeta es una **copia real** dentro de malaysia, no un enlace ni una
dependencia del build.

## Regla

Esta copia es la fuente para malaysia: los cambios de diseño se hacen aquí
(\`tokens.json\`, \`components.json\`) y, si se quiere, se devuelven a dotfiles a
mano. Ni Cargo ni Nix leen \`~/dotfiles\` ni ningún otro repositorio.

## Regenerar \`generated/\`

\`\`\`sh
cd crates/vow-design/tengen
nix eval --raw -f . slint > generated/tengen.slint
nix eval --raw -f . css   > generated/tengen.css
nix eval --raw -f . qml   > generated/Tengen.qml
\`\`\`

\`./dev.sh check\` comprueba que esos tres archivos coinciden con \`default.nix\`.
El script \`tools/sync-tengen.sh <ruta>\` trae cambios desde una copia externa
a propósito y actualiza este archivo; no se ejecuta en el build.
SOURCE

printf 'sync-tengen: copied; review the diff and regenerate generated/\n' >&2
