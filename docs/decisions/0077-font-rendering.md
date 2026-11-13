# 0077 — Font rendering like macOS

Status: accepted and implemented (2026-11-12).

## Goal and scope

This ADR fixes how text is rasterized in a Malaysia session: GTK, Qt,
Electron and the `bk` clients (shell, Settings). The compositor's own
titlebar text (cosmic-text and swash) is out of scope: it does not use
FreeType.

Readers: whoever works on the session environment (`spawn.rs`,
`shell_session.rs`) or on how text looks.

## Context

macOS draws text with grayscale antialiasing, no grid fitting and fractional
glyph positions. Its font smoothing also thickens thin stems, so 400-weight
text looks heavier than on Linux.

The toolkits already match the first three points:

- Skia paragraphs (`bk`) use `SkFont::Edging::kAntiAlias`, subpixel
  positioning and `SkFontHinting::kSlight`, which FreeType loads with
  `FT_LOAD_TARGET_LIGHT` (Skia `SkFontHost_FreeType.cpp`).
- GTK 4 in `gtk-font-rendering=automatic` uses `CAIRO_HINT_STYLE_SLIGHT`
  and does not round glyph positions (`gtk_widget_update_pango_context`).
- Qt and Electron follow fontconfig; NixOS defaults to `hintslight` and
  `rgba=none`.

Light hinting runs FreeType's auto-hinter, which can darken stems. FreeType
keeps that off by default; the CFF driver does the same. Both read
`FREETYPE_PROPERTIES` when the library starts
([driver properties](https://freetype.org/freetype2/docs/reference/ft2-properties.html#no-stem-darkening)).

## Decision

1. The compositor exports
   `FREETYPE_PROPERTIES="cff:no-stem-darkening=0 autofitter:no-stem-darkening=0"`
   at startup (`spawn::export_session_env`). A value the user already set
   wins.
2. `shell_session::start` imports it into the systemd user manager, so the
   shell, the portal and D-Bus activated apps get it too.
3. Regular text stays Regular (400); weights are never remapped, so explicit
   Semibold and Bold stay as asked. `share/fontconfig/malaysia.conf`
   includes `/etc/fonts/fonts.conf` and makes `sans-serif` resolve to
   Adwaita Sans. The compositor wrapper sets
   `FONTCONFIG_FILE` to it unless it is already set, and
   `shell_session::start` imports it. Skia asks fontconfig too, so `bk`
   follows. The dev shell includes the same file.

## Consequences

- Text in GTK, Qt, Electron and `bk` looks heavier and closer to macOS.
- FreeType documents darkening for linear blending; with gamma-encoded
  blending it is a bit stronger, which is the look this ADR wants.
- Regular text is Adwaita Sans Regular in every toolkit, like the `ctdesign`
  styles.
- To go back to plain FreeType, set `FREETYPE_PROPERTIES=` (empty) in the
  environment that starts the compositor. To use the system sans, set
  `FONTCONFIG_FILE=/etc/fonts/fonts.conf` there.

## Rejected alternatives

- Fontconfig `embolden`: synthetic bold adds a fixed width at every size,
  far more than macOS.
- Mapping regular to medium or semibold: body text and headings would look
  the same weight.
- A system-wide rule in the NixOS module (`fonts.fontconfig.localConf`): it
  would change every session on the machine, not only Malaysia.
- Raising the weights of the `ctdesign` tokens: it reaches only `bk`, not
  GTK, Qt or Electron.
