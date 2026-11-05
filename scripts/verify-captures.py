#!/usr/bin/env python3
"""Checks that a drm-vkms capture of pattern_live has its four solid quadrants in their corners.

Usage:
  verify-captures.py <capture.png>
"""

import subprocess
import sys


def decode(path):
    return subprocess.run(
        ["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        capture_output=True,
    ).stdout


def dimensions(path):
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
    path = sys.argv[1]
    raw = decode(path)
    if not raw:
        print(f"FALLA: no se pudo decodificar {path}")
        return 1
    w, h = dimensions(path)
    result = check_pattern(raw, w, h)
    print(f"==> {path}: {'PASA' if result else 'FALLA'}")
    return 0 if result else 1


if __name__ == "__main__":
    sys.exit(main())
