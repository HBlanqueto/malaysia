{
  stdenvNoCC,
  src,
  adwaita-icon-theme,
  gtk3,
  hicolor-icon-theme,
  meson,
  ninja,
}:
stdenvNoCC.mkDerivation {
  pname = "malaysia-icon-theme";
  version = "0.1.0";
  inherit src;

  nativeBuildInputs = [ meson ninja gtk3 ];

  # index.theme: Inherits=Adwaita,hicolor
  propagatedUserEnvPkgs = [ adwaita-icon-theme hicolor-icon-theme ];

  dontDropIconThemeCache = true;
}
