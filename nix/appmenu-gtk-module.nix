{
  lib,
  stdenv,
  appmenu-glib-translator,
  glib,
  gtk3,
  meson,
  ninja,
  pkg-config,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "appmenu-gtk-module";
  inherit (appmenu-glib-translator) version src;

  sourceRoot = "source/subprojects/appmenu-gtk-module";

  patches = [ ./patches/appmenu-gtk-module-wayland-realize.patch ];

  postPatch = ''
    substituteInPlace src/gtk-3.0/meson.build \
      --replace-fail "gtk3.get_variable(pkgconfig:'libdir')" "get_option('prefix'), get_option('libdir')"
  '';

  nativeBuildInputs = [
    glib
    meson
    ninja
    pkg-config
  ];

  buildInputs = [ gtk3 ];

  mesonFlags = [ "-Dgtk=3" ];

  # The module calls g_settings_new, which aborts GTK 3 apps without a compiled schema.
  postFixup = ''
    glib-compile-schemas "$out/share/gsettings-schemas/$name/glib-2.0/schemas"
  '';

  meta = {
    description = "GTK 3 module that exports GtkMenuBar menus for global menus";
    homepage = "https://gitlab.com/vala-panel-project/vala-panel-appmenu/-/tree/${finalAttrs.version}/subprojects/appmenu-gtk-module";
    license = lib.licenses.lgpl3Plus;
    platforms = lib.platforms.linux;
  };
})
