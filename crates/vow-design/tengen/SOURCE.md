# Origen de esta copia

- **Procedencia:** `dotfiles/kits/neptunia` (repositorio de dotfiles del autor).
- **Commit de origen:** `e367fd1` (`chore: limpia los rodeos del reordenamiento y ajusta el fileset`).
- **Fecha de la copia:** 2026-10-22.

Esta carpeta es una **copia real** dentro de malaysia, no un enlace ni una
dependencia del build. Se han quitado los archivos que malaysia no usa
(`vscode.nix`, `filecolors.nix`, `palette.nix`, `tengen.nix` y
`generated/kmscon-palette`); la paleta de terminal y el tema de VS Code no
forman parte del escritorio.

## Regla

Esta copia es la fuente para malaysia: los cambios de diseño se hacen aquí
(`tokens.json`, `components.json`) y, si se quiere, se devuelven a dotfiles a
mano. Ni Cargo ni Nix leen `~/dotfiles` ni ningún otro repositorio.

## Regenerar `generated/`

```sh
cd crates/vow-design/tengen
nix eval --raw -f . slint > generated/tengen.slint
nix eval --raw -f . css   > generated/tengen.css
nix eval --raw -f . qml   > generated/Tengen.qml
```

`./dev.sh check` comprueba que esos tres archivos coinciden con `default.nix`.
El script `tools/sync-tengen.sh <ruta>` trae cambios desde una copia externa
a propósito y actualiza este archivo; no se ejecuta en el build.

26 oct 2026: tokens de la barra lateral en isla añadidos aquí, siguiendo el
sistema de diseño Tengen.

28 oct 2026: renombrado a Tengen (ADR 0046).
