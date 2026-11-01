let
  flake = builtins.getFlake ("path:" + toString ../../../..);
  pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
  patched = flake.packages.x86_64-linux.libadwaita-global-menu;
in pkgs.mkShell {
  packages = with pkgs; [
    weston wayland wayland-scanner pkg-config gcc pixman libxkbcommon libdrm
    (python3.withPackages (p: [ p.pygobject3 ]))
    gobject-introspection gtk4 libadwaita gnome-text-editor gnome-calculator mesa
  ];
  PATCHED = "${patched}";
  shellHook = ''
    export GI_TYPELIB_PATH="${patched}/lib/girepository-1.0:${pkgs.gtk4}/lib/girepository-1.0:$GI_TYPELIB_PATH"
    export __EGL_VENDOR_LIBRARY_DIRS="${pkgs.mesa}/share/glvnd/egl_vendor.d"
    export LIBGL_DRIVERS_PATH="${pkgs.mesa}/lib/dri"
  '';
}
