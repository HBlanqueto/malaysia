# Native libraries Skia needs while building and linking. Kept separate from the
# compositor's deps so nix/skia.nix can be reused without pulling GStreamer or
# input libraries.
{ pkgs }:
with pkgs;
[
  brotli
  expat
  fontconfig
  freetype
  icu
  libdrm
  libglvnd
  libjpeg
  libpng
  libwebp
  vulkan-headers
  zlib
]
