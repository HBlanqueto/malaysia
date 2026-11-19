#!/usr/bin/env python3
"""Checks that a drm-vkms capture of pattern_live has its four solid quadrants in their corners.

Usage:
  verify-captures.py <capture.png>
"""

import struct
import sys
import zlib

CHANNELS = {2: 3, 6: 4}


def paeth(left, up, upper_left):
    estimate = left + up - upper_left
    distances = (abs(estimate - left), abs(estimate - up), abs(estimate - upper_left))
    return (left, up, upper_left)[distances.index(min(distances))]


def unfilter(raw, width, height, channels):
    stride = width * channels
    prior = bytearray(stride)
    rgb = bytearray()
    for y in range(height):
        start = y * (stride + 1)
        kind = raw[start]
        line = bytearray(raw[start + 1:start + 1 + stride])
        for i in range(stride):
            left = line[i - channels] if i >= channels else 0
            up = prior[i]
            upper_left = prior[i - channels] if i >= channels else 0
            if kind == 1:
                line[i] = (line[i] + left) & 255
            elif kind == 2:
                line[i] = (line[i] + up) & 255
            elif kind == 3:
                line[i] = (line[i] + (left + up) // 2) & 255
            elif kind == 4:
                line[i] = (line[i] + paeth(left, up, upper_left)) & 255
        for x in range(width):
            rgb += line[x * channels:x * channels + 3]
        prior = line
    return bytes(rgb)


def decode(path):
    with open(path, "rb") as f:
        data = f.read()
    header = None
    compressed = []
    position = 8
    while position + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[position:position + 8])
        body = data[position + 8:position + 8 + length]
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            compressed.append(body)
        position += 12 + length
    if header is None:
        return b""
    width, height, depth, color_type, _, _, interlace = header
    channels = CHANNELS.get(color_type)
    if depth != 8 or channels is None or interlace:
        return b""
    return unfilter(zlib.decompress(b"".join(compressed)), width, height, channels)


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
