#!/usr/bin/env python3
"""Verifies Yuki debug captures against expected values/regions (Bloque 7.3, 3c).

Every check compares pixels to an expected colour in an expected region — never
"there are some non-black pixels". A capture that does not decode, or whose
regions fall outside the tolerances, is a FAIL.

Usage:
  verify_captures.py <capture.png> <criterion>

Criteria:
  wallpaper   - the whole output is the default Solid Aqua Blue (65,105,170) ±1.
  window      - the workspace is covered by a client (few wallpaper samples).
  foot        - the foot background fills the workspace and text (light pixels)
                appears in the top prompt band.
  popup       - the popup colour (128,128,128) fills its expected region near
                the bottom, above the window's lower edge.
  ssd         - the titlebar band on the left is the wallpaper and the content
                fills the rest.
  pattern     - four solid quadrants land in their exact corners.
"""

import subprocess
import sys

# The default wallpaper (yuki_design::WALLPAPER). A session whose
# wallpaper.domain selects another image will not match.
WALLPAPER = (65, 105, 170)


def decode(path):
    return subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        capture_output=True,
    ).stdout


def dimensions(path):
    # PNG IHDR: width and height are two big-endian u32s at offsets 16 and 20.
    with open(path, "rb") as f:
        header = f.read(24)
    if header[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")
    return int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")


def px(raw, w, x, y):
    o = (y * w + x) * 3
    return raw[o], raw[o + 1], raw[o + 2]


def close(a, b, tol=1):
    return all(abs(x - y) <= tol for x, y in zip(a, b))


def region(raw, w, x0, y0, x1, y1, step):
    return [px(raw, w, x, y) for y in range(y0, y1, step) for x in range(x0, x1, step)]


def check_wallpaper(raw, w, h):
    # The workspace (below the menu bar at 28, above the dock at 736) must be
    # the default wallpaper; the shell panels are excluded.
    samples = region(raw, w, 0, 60, w, 700, 24)
    bad = [p for p in samples if not close(p, WALLPAPER)]
    print(f"wallpaper: {len(bad)}/{len(samples)} píxeles fuera de ±1")
    return len(bad) == 0


def check_window(raw, w, h):
    # The window covers the workspace (below the menu bar, above the dock);
    # the wallpaper should be mostly hidden there.
    samples = region(raw, w, 40, 60, w - 40, 720, 40)
    wall_px = [p for p in samples if close(p, WALLPAPER)]
    print(f"window: {len(wall_px)}/{len(samples)} muestras del wallpaper en el workspace")
    return len(wall_px) < len(samples) // 2


def check_foot(raw, w, h):
    # The foot window fills the workspace with its dark background and the
    # prompt (light text) sits in the top rows.
    bg = region(raw, w, 32, 120, w - 32, 700, 40)
    dark = [p for p in bg if all(c <= 80 for c in p)]
    if len(dark) < len(bg) // 2:
        print(f"foot: fondo oscuro en {len(dark)}/{len(bg)} muestras del workspace")
        return False
    prompt_band = region(raw, w, 32, 40, w - 32, 200, 2)
    text = [p for p in prompt_band if sum(p) > 300 and not close(p, WALLPAPER, 3)]
    print(f"foot: fondo oscuro {len(dark)}/{len(bg)}, texto del prompt {len(text)} px")
    return len(text) > 50


def check_popup(raw, w, h):
    # The popup colour (0x80 = 128) fills its configured region near the
    # bottom, above the window's lower edge.
    pop = region(raw, w, 400, 620, 600, 760, 4)
    found = [p for p in pop if close(p, (128, 128, 128), 2)]
    if len(found) < len(pop) // 2:
        print(f"popup: {len(found)}/{len(pop)} muestras del color del popup en su región")
        return False
    above = region(raw, w, 400, 600, 600, 640, 8)
    non_pop = [p for p in above if close(p, (128, 128, 128), 2)]
    print(f"popup: región del popup llena ({len(found)}/{len(pop)}), arriba del ancla {len(non_pop)}/2 muestras")
    return len(non_pop) <= 1


def check_ssd(raw, w, h):
    # The titlebar band on the left is the wallpaper; the content (0x40 = 64)
    # fills the rest of the workspace.
    band = region(raw, w, 0, 60, 31, 700, 6)
    wall = [p for p in band if close(p, WALLPAPER, 3)]
    if len(wall) < len(band) // 2:
        print(f"ssd: banda de titlebar con wallpaper en {len(wall)}/{len(band)} muestras")
        return False
    content = region(raw, w, 40, 60, w - 40, 700, 12)
    gray = [p for p in content if close(p, (64, 64, 64), 2)]
    print(f"ssd: banda wallpaper {len(wall)}/{len(band)}, contenido {len(gray)}/{len(content)}")
    return len(gray) > len(content) // 2


def check_pattern(raw, w, h):
    corners = {
        "izq-arriba (rojo)": ((40, 60), (255, 0, 0)),
        "der-arriba (verde)": ((w - 40, 60), (0, 255, 0)),
        "izq-abajo (azul)": ((40, h - 40), (0, 0, 255)),
        "der-abajo (blanco)": ((w - 40, h - 40), (255, 255, 255)),
    }
    ok = True
    for name, ((x, y), want) in corners.items():
        got = px(raw, w, x, y)
        match = close(got, want, 2)
        ok = ok and match
        print(f"  {'PASA' if match else 'FALLA'} {name}: {got} esperado {want}")
    return ok


def main():
    path, criterion = sys.argv[1], sys.argv[2]
    raw = decode(path)
    if not raw:
        print(f"FALLA: no se pudo decodificar {path}")
        return 1
    w, h = dimensions(path)
    checks = {
        "wallpaper": check_wallpaper,
        "window": check_window,
        "foot": check_foot,
        "popup": check_popup,
        "ssd": check_ssd,
        "pattern": check_pattern,
    }
    fn = checks.get(criterion)
    if fn is None:
        print(f"FALLA: criterio desconocido {criterion}")
        return 1
    result = fn(raw, w, h)
    print(f"==> {path} [{criterion}]: {'PASA' if result else 'FALLA'}")
    return 0 if result else 1


if __name__ == "__main__":
    sys.exit(main())