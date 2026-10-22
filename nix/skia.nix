# Skia precompilado (skia-safe/skia-bindings 0.93) en su propia derivación, para
# que el build de Yuki lo reutilice en vez de reconstruirlo en cada cambio.
#
# Copia adaptada del patrón de Otto (MIT, Riccardo Canalicchio): compila el
# crate mínimo de nix/skia-prebuild con las features que pide laye-rs v1.18.2
# (`all-linux` + defaults) y empaqueta lo que skia-bindings deja en
# `$OUT_DIR/skia` como un .tar.gz. Los consumidores lo usan con
# `SKIA_BINARIES_URL = file://…`; el build.rs de skia-bindings solo entonces
# baja «binarios» en vez de compilar Skia.
{
  lib,
  pkgs,
  rustPlatform,
  clangStdenv,
  fetchFromGitHub,
  fetchgit,
  linkFarm,
  runCommand,
  gn,
  ninja,
  python3,
  pkg-config,
  removeReferencesTo,
}:

let
  deps = import ./skia-deps.nix { inherit pkgs; };

  # Árbol de Skia que espera skia-bindings 0.93 (tag y externals: ver
  # skia-bindings/Cargo.toml#package.metadata y skia/DEPS).
  skia-source =
    let
      repo = fetchFromGitHub {
        owner = "rust-skia";
        repo = "skia";
        tag = "m145-0.92.0";
        hash = "sha256-9N780AwheKBJRcZC4l/uWFNq+oOyoNp4M6dJAVVAFeo=";
      };
      externals = linkFarm "skia-externals" (
        lib.mapAttrsToList
          (name: value: { inherit name; path = fetchgit value; })
          (lib.importJSON ./skia-externals.json)
      );
    in
    runCommand "skia-source" { } ''
      cp -R ${repo} $out
      chmod -R +w $out
      ln -s ${externals} $out/third_party/externals
    '';
in
rustPlatform.buildRustPackage.override { stdenv = clangStdenv; } {
  pname = "skia-binaries";
  version = "0.93.1";

  src = ./skia-prebuild;
  cargoLock.lockFile = ./skia-prebuild/Cargo.lock;

  env = {
    SKIA_SOURCE_DIR = skia-source;
    SKIA_GN_COMMAND = "${gn}/bin/gn";
    SKIA_NINJA_COMMAND = "${ninja}/bin/ninja";
    FORCE_SKIA_BUILD = "1";
  };

  nativeBuildInputs = [
    pkg-config
    python3
    removeReferencesTo
  ];

  buildInputs = [
    rustPlatform.bindgenHook
  ]
  ++ deps;

  installPhase = ''
    runHook preInstall

    skia_out=$(find . -type d -path '*/release/build/skia-bindings-*/out/skia' | head -n1)
    if [ -z "$skia_out" ]; then
      echo "skia-binaries: no encuentro el OUT_DIR/skia de skia-bindings" >&2
      exit 1
    fi

    # Skia graba en las librerías la ruta de su árbol de fuentes; quítala para
    # no arrastrar todo Skia en el cierre.
    find "$skia_out" -type f -exec remove-references-to -t ${skia-source} {} +

    mkdir -p $out/skia-binaries
    cp "$skia_out"/*.a $out/skia-binaries/
    cp "$skia_out"/bindings.rs $out/skia-binaries/
    if [ -f "$skia_out/icudtl.dat" ]; then
      cp "$skia_out/icudtl.dat" $out/skia-binaries/
    fi
    tar -czf $out/skia-binaries.tar.gz -C $out skia-binaries
    rm -rf $out/skia-binaries

    # Features con las que se compiló, para que el preBuild del paquete
    # compruebe que las de laye-rs no cambiaron.
    cargo tree --offline -e features -i skia-bindings --prefix none \
      | sed -n 's/^skia-bindings feature "\([^"]*\)".*/\1/p' | sort -u > $out/features.txt

    runHook postInstall
  '';

  doCheck = false;

  disallowedReferences = [ skia-source ];

  meta = {
    description = "Prebuilt Skia for skia-safe/skia-bindings 0.93";
    license = lib.licenses.bsd3;
    platforms = [ "x86_64-linux" "aarch64-linux" ];
  };
}
