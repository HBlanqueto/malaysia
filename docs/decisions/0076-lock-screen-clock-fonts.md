# 0076 — Lock screen clock fonts

Status: accepted and implemented (2026-11-11).

## Goal and scope

This ADR fixes which font draws the time on the lock screen and how it is
built. It starts at the digits the user sees and ends at the font files that
`malaysia-shell lock` loads. The date line, the layout and the rise animation
stay as [ADR 0074](0074-lock-screen-and-lock-transition.md) describes them.
Readers: whoever works on `lock.rs`, `clock_face.rs` or the fonts.

## Decision

1. The clock has its own fonts, **Malaysia Clock** and **Malaysia Clock
   Outline**, in four styles: Regular, Italic, Rounded and Dots.
2. Each file holds the digits 0–9 of 14 numeral systems at their real Unicode
   code points, plus the colon and the space: `latn`, `arab`, `beng`, `mymr`,
   `knda`, `deva`, `guru`, `gujr`, `khmr`, `mlym`, `mtei`, `olck`, `orya` and
   `telu` (CLDR numbering system names).
3. The digits of every system share one height (700 units) and one center
   line, so a change of system does not move the clock.
4. Digits are tabular within each system: the time does not shift when a
   minute changes.
5. The colon is centered on the digits.
6. Outline is a ring around the solid shape; its inside is empty, so the
   wallpaper shows through.
7. Three options of `lock.domain` choose the face:
   `clock_style`, `clock_fill` and `clock_numerals`
   ([options](../configuration/options.md#lock--session-lock)).

## How the fonts are built

`scripts/clock-fonts.py` writes the eight `.otf` files to
`crates/malaysia-shell/fonts/`.

1. It downloads Noto Sans (one family per script) and Baloo 2 (Latin,
   Arabic and the Indic families) from Google Fonts. Both are SIL OFL 1.1
   without reserved names.
2. It scales and moves the digits of each system to the shared height and
   center line.
3. Regular takes Noto Sans 600. Italic slants Regular by 11°.
4. Rounded takes Baloo 2 600 where it covers the system. Myanmar, Khmer,
   Meetei Mayek and Ol Chiki have no Baloo, so their Noto Sans 500 digits
   grow by 30 units with round joins.
5. Dots samples Noto Sans 700 on a grid of 13 rows and draws a circle per lit
   cell, like an LED alarm clock.
6. Outline grows each shape by 26 units and subtracts the shape.
7. It removes overlaps with `skia-pathops` and saves CFF fonts that pass the
   OpenType Sanitizer.

## How the lock uses them

1. `ClockFace::load` reads `lock.domain` when the lock starts.
2. `ClockFace::register` passes the chosen file, embedded in the binary, to
   `bk::register_font`.
3. The clock text uses only that family and writes the time with the chosen
   numerals.

## Consequences

- Each font name is unique per file, so one registered face matches it by
  name alone.
- The fonts derive from Noto and Baloo; Italic is a slant, not a redrawn
  italic. Stroke weight varies a little between systems.
- In systems with wide digits, such as Malayalam and Kannada, the tabular
  width leaves visible gaps around narrow digits.
- Google Fonts serves the latest source versions, so a rebuild can change
  the outlines slightly.
- The license and the copyright of the sources ship in
  `crates/malaysia-shell/fonts/OFL.txt`.
