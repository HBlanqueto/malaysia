#!/usr/bin/env python3
"""Builds the lock screen clock fonts (ADR 0076): digits 0-9 of 14 numeral systems plus the colon,
in four styles, solid and outline, from Noto Sans and Baloo 2 (SIL OFL 1.1).

Usage:
  pip install fonttools skia-pathops pillow
  scripts/clock-fonts.py [cache dir]
"""
import math
import statistics
import sys
import urllib.request
from pathlib import Path

import pathops
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.basePen import BasePen
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from PIL import Image, ImageChops, ImageDraw

SRC = Path(sys.argv[1] if len(sys.argv) > 1 else Path.home() / ".cache" / "malaysia-clock-fonts")
OUT = Path(__file__).resolve().parent.parent / "crates" / "malaysia-shell" / "fonts"

UPM = 1000
DIGIT_HEIGHT = 700
DIGIT_CENTER = 350
SIDE_BEARING = 60
COLON_BEARING = 40
ITALIC_ANGLE = 11
ROUND_RADIUS = 30
OUTLINE_WIDTH = 26
MATRIX_ROWS = 13
MATRIX_SAMPLES = 8
MATRIX_THRESHOLD = 0.42
DOT_RADIUS = 0.42
RING_OUTER, RING_INNER = 0.405, 0.275
VERSION = "1.000"

SCRIPTS = [
    ("latn", 0x0030, "Noto Sans", "Baloo 2"),
    ("arab", 0x0660, "Noto Sans Arabic", "Baloo Bhaijaan 2"),
    ("beng", 0x09E6, "Noto Sans Bengali", "Baloo Da 2"),
    ("mymr", 0x1040, "Noto Sans Myanmar", None),
    ("knda", 0x0CE6, "Noto Sans Kannada", "Baloo Tamma 2"),
    ("deva", 0x0966, "Noto Sans Devanagari", "Baloo 2"),
    ("guru", 0x0A66, "Noto Sans Gurmukhi", "Baloo Paaji 2"),
    ("gujr", 0x0AE6, "Noto Sans Gujarati", "Baloo Bhai 2"),
    ("khmr", 0x17E0, "Noto Sans Khmer", None),
    ("mlym", 0x0D66, "Noto Sans Malayalam", "Baloo Chettan 2"),
    ("mtei", 0xABF0, "Noto Sans Meetei Mayek", None),
    ("olck", 0x1C50, "Noto Sans Ol Chiki", None),
    ("orya", 0x0B66, "Noto Sans Oriya", "Baloo Bhaina 2"),
    ("telu", 0x0C66, "Noto Sans Telugu", "Baloo Tammudu 2"),
]
LATIN_NAMES = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"]

_fonts = {}


def download(family, weight, target):
    css_url = f"https://fonts.googleapis.com/css2?family={family.replace(' ', '+')}:wght@{weight}"
    css = urllib.request.urlopen(css_url).read().decode()
    font_url = css.split("url(")[1].split(")")[0]
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(urllib.request.urlopen(font_url).read())


def source(family, weight):
    key = f"{family.replace(' ', '')}-{weight}"
    if key not in _fonts:
        path = SRC / f"{key}.ttf"
        if not path.exists():
            download(family, weight, path)
        font = TTFont(path)
        _fonts[key] = (font.getGlyphSet(), font.getBestCmap())
    return _fonts[key]


def outline(family, weight, codepoint, matrix=(1, 0, 0, 1, 0, 0)):
    glyphs, cmap = source(family, weight)
    if codepoint not in cmap:
        raise SystemExit(f"{family}-{weight} has no U+{codepoint:04X}")
    recording = DecomposingRecordingPen(glyphs)
    glyphs[cmap[codepoint]].draw(recording)
    path = pathops.Path()
    recording.replay(TransformPen(path.getPen(), matrix))
    path.simplify(fix_winding=True)
    return path


def union(a, b):
    return pathops.op(a, b, pathops.PathOp.UNION, fix_winding=True)


def difference(a, b):
    return pathops.op(a, b, pathops.PathOp.DIFFERENCE, fix_winding=True)


def stroked(path, width):
    ring = pathops.Path(path)
    ring.stroke(width, pathops.LineCap.ROUND_CAP, pathops.LineJoin.ROUND_JOIN, 4)
    ring.convertConicsToQuads()
    return ring


def grown(path, radius):
    return union(path, stroked(path, 2 * radius))


def normalizer(family, weight, zero):
    heights, centers = [], []
    for d in range(10):
        _, ymin, _, ymax = outline(family, weight, zero + d).bounds
        heights.append(ymax - ymin)
        centers.append((ymax + ymin) / 2)
    scale = DIGIT_HEIGHT / statistics.median(heights)
    return scale, DIGIT_CENTER - statistics.median(centers) * scale


def placed(path, advance):
    xmin, _, xmax, _ = path.bounds
    shift = (advance - (xmax - xmin)) / 2 - xmin
    moved = pathops.Path()
    path.draw(TransformPen(moved.getPen(), (1, 0, 0, 1, shift, 0)))
    return moved


def skewed(path):
    slant = math.tan(math.radians(ITALIC_ANGLE))
    out = pathops.Path()
    path.draw(TransformPen(out.getPen(), (1, 0, slant, 1, -slant * DIGIT_CENTER, 0)))
    return out


def circle(cx, cy, r):
    k = 0.5523 * r
    path = pathops.Path()
    pen = path.getPen()
    pen.moveTo((cx + r, cy))
    pen.curveTo((cx + r, cy + k), (cx + k, cy + r), (cx, cy + r))
    pen.curveTo((cx - k, cy + r), (cx - r, cy + k), (cx - r, cy))
    pen.curveTo((cx - r, cy - k), (cx - k, cy - r), (cx, cy - r))
    pen.curveTo((cx + k, cy - r), (cx + r, cy - k), (cx + r, cy))
    pen.closePath()
    return path


class FlattenPen(BasePen):
    def __init__(self):
        super().__init__(None)
        self.contours = []

    def _moveTo(self, p):
        self.contours.append([p])

    def _lineTo(self, p):
        self.contours[-1].append(p)

    def _curveToOne(self, p1, p2, p3):
        p0 = self._getCurrentPoint()
        for i in range(1, 13):
            t = i / 12
            u = 1 - t
            self.contours[-1].append(tuple(u**3 * a + 3 * u * u * t * b + 3 * u * t * t * c + t**3 * d for a, b, c, d in zip(p0, p1, p2, p3)))

    def _qCurveToOne(self, p1, p2):
        p0 = self._getCurrentPoint()
        for i in range(1, 13):
            t = i / 12
            u = 1 - t
            self.contours[-1].append(tuple(u * u * a + 2 * u * t * b + t * t * c for a, b, c in zip(p0, p1, p2)))


CELL = DIGIT_HEIGHT / MATRIX_ROWS
GRID_BOTTOM = DIGIT_CENTER - (MATRIX_ROWS / 2 + 1) * CELL
GRID_ROWS = MATRIX_ROWS + 2


def lit_cells(path):
    xmin, _, xmax, _ = path.bounds
    cols = math.ceil((xmax - xmin) / CELL) + 1
    left = (xmin + xmax) / 2 - cols * CELL / 2
    px = CELL / MATRIX_SAMPLES
    size = (cols * MATRIX_SAMPLES, GRID_ROWS * MATRIX_SAMPLES)
    flat = FlattenPen()
    path.draw(flat)
    mask = Image.new("1", size, 0)
    for contour in flat.contours:
        layer = Image.new("1", size, 0)
        ImageDraw.Draw(layer).polygon([((x - left) / px, (GRID_BOTTOM + GRID_ROWS * CELL - y) / px) for x, y in contour], fill=1)
        mask = ImageChops.logical_xor(mask, layer)
    gray = mask.convert("L")
    cells = []
    for row in range(GRID_ROWS):
        for col in range(cols):
            box = gray.crop((col * MATRIX_SAMPLES, row * MATRIX_SAMPLES, (col + 1) * MATRIX_SAMPLES, (row + 1) * MATRIX_SAMPLES))
            if sum(box.tobytes()) / (MATRIX_SAMPLES * MATRIX_SAMPLES * 255) > MATRIX_THRESHOLD:
                cells.append((col, GRID_ROWS - 1 - row))
    return cells, cols


def dots(cells, cols, advance_cols, ring):
    offset = (advance_cols - cols) / 2
    path = pathops.Path()
    for col, row in cells:
        cx, cy = (col + offset + 0.5) * CELL, GRID_BOTTOM + (row + 0.5) * CELL
        dot = difference(circle(cx, cy, RING_OUTER * CELL), circle(cx, cy, RING_INNER * CELL)) if ring else circle(cx, cy, DOT_RADIUS * CELL)
        path = union(path, dot)
    return path


def style_shapes(style):
    shapes = {}
    for script, zero, noto, baloo in SCRIPTS:
        if style == "Rounded" and baloo:
            family, weight, radius = baloo, 600, 0
        elif style == "Rounded":
            family, weight, radius = noto, 500, ROUND_RADIUS
        else:
            family, weight, radius = noto, 700 if style == "Dots" else 600, 0
        scale, rise = normalizer(family, weight, zero)
        for d in range(10):
            path = outline(family, weight, zero + d, (scale, 0, 0, scale, 0, rise))
            shapes[glyph_name(zero + d)] = (zero + d, grown(path, radius) if radius else path, script)
    colon_family, colon_weight = {"Rounded": ("Baloo 2", 600), "Dots": ("Noto Sans", 700)}.get(style, ("Noto Sans", 600))
    latin_scale, _ = normalizer(colon_family, colon_weight, 0x30)
    colon = outline(colon_family, colon_weight, 0x3A, (latin_scale, 0, 0, latin_scale, 0, 0))
    _, ymin, _, ymax = colon.bounds
    colon = outline(colon_family, colon_weight, 0x3A, (latin_scale, 0, 0, latin_scale, 0, DIGIT_CENTER - (ymin + ymax) / 2))
    shapes["colon"] = (0x3A, colon, None)
    return shapes


def glyph_name(codepoint):
    return LATIN_NAMES[codepoint - 0x30] if 0x30 <= codepoint <= 0x39 else f"uni{codepoint:04X}"


def build_glyphs(style, hollow):
    shapes = style_shapes(style)
    glyphs = {}
    if style == "Dots":
        rasters = {name: lit_cells(path) for name, (_, path, _) in shapes.items()}
        for script, *_ in SCRIPTS:
            names = [n for n, (_, _, s) in shapes.items() if s == script]
            advance_cols = max(rasters[n][1] for n in names) + 1
            for n in names:
                glyphs[n] = (shapes[n][0], dots(*rasters[n], advance_cols, hollow), advance_cols * CELL)
        cells, cols = rasters["colon"]
        glyphs["colon"] = (0x3A, dots(cells, cols, cols + 1, hollow), (cols + 1) * CELL)
        return glyphs
    for script, *_ in SCRIPTS:
        names = [n for n, (_, _, s) in shapes.items() if s == script]
        widest = max(shapes[n][1].bounds[2] - shapes[n][1].bounds[0] for n in names)
        for n in names:
            glyphs[n] = (shapes[n][0], shapes[n][1], widest + 2 * SIDE_BEARING)
    colon = shapes["colon"][1]
    glyphs["colon"] = (0x3A, colon, colon.bounds[2] - colon.bounds[0] + 2 * COLON_BEARING)
    finished = {}
    for name, (codepoint, path, advance) in glyphs.items():
        path = placed(path, advance)
        if style == "Italic":
            path = skewed(path)
        if hollow:
            path = difference(grown(path, OUTLINE_WIDTH), path)
        finished[name] = (codepoint, path, advance)
    return finished


def write_font(style, hollow):
    glyphs = build_glyphs(style, hollow)
    family = "Malaysia Clock Outline" if hollow else "Malaysia Clock"
    ps_family = family.replace(" ", "")
    order = [".notdef", "space"] + list(glyphs)
    fb = FontBuilder(UPM, isTTF=False)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({0x20: "space", **{cp: name for name, (cp, _, _) in glyphs.items()}})
    charstrings, metrics = {}, {}
    for name in [".notdef", "space"]:
        pen = T2CharStringPen(500, None)
        charstrings[name] = pen.getCharString()
        metrics[name] = (500, 0)
    for name, (_, path, advance) in glyphs.items():
        pen = T2CharStringPen(round(advance), None)
        path.draw(pen)
        charstrings[name] = pen.getCharString()
        metrics[name] = (round(advance), round(path.bounds[0]) if path.bounds else 0)
    legacy_family = f"{family} {style}"
    legacy_style = "Italic" if style == "Italic" else "Regular"
    ps_name = f"{ps_family}-{style}"
    fb.setupCFF(ps_name, {"FullName": f"{family} {style}"}, charstrings, {})
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=900, descent=-200)
    fb.setupNameTable({
        "copyright": "Copyright 2022 The Noto Project Authors (https://github.com/notofonts). Copyright 2019 The Baloo 2 Project Authors (https://github.com/EkType/Baloo2). Modified 2026 for Malaysia.",
        "familyName": legacy_family,
        "styleName": legacy_style,
        "uniqueFontIdentifier": f"{VERSION};{ps_name}",
        "fullName": f"{family} {style}",
        "version": f"Version {VERSION}",
        "psName": ps_name,
        "licenseDescription": "This Font Software is licensed under the SIL Open Font License, Version 1.1.",
        "licenseInfoURL": "https://openfontlicense.org",
    })
    fb.setupOS2(
        sTypoAscender=900, sTypoDescender=-200, sTypoLineGap=0, usWinAscent=1000, usWinDescent=300,
        sCapHeight=DIGIT_HEIGHT, sxHeight=0, achVendID="NONE",
        fsSelection=(0x01 if style == "Italic" else 0x40 if style == "Regular" else 0) | 0x80,
        usWeightClass=400, version=4,
    )
    fb.setupPost(italicAngle=-ITALIC_ANGLE if style == "Italic" else 0, isFixedPitch=0)
    if style == "Italic":
        fb.font["head"].macStyle = 0x02
    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / f"{ps_name}.otf"
    fb.save(target)
    return target


if __name__ == "__main__":
    for hollow in (False, True):
        for style in ("Regular", "Italic", "Rounded", "Dots"):
            print(write_font(style, hollow))
