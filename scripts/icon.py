#!/usr/bin/env python3
import math
import pathlib
import struct
import sys
import zlib

PATTERN = [
    "..RRR..",
    ".R...R.",
    "R.YYY.R",
    "R.YYY.R",
    "R.YYY.R",
    ".R...R.",
    "..RRR..",
]
RELEASE = {".": (42, 42, 46), "R": (255, 90, 122), "Y": (255, 207, 64)}
DEVELOPMENT = {".": (42, 42, 46), "R": (105, 219, 124), "Y": (56, 217, 169)}
TOP = (22, 22, 22)
BOTTOM = (5, 5, 5)
FIRST = 161.55
STEP = 116.82
RADIUS = 42.25
CLEAR = (0, 0, 0, 0)
COLORS = DEVELOPMENT if len(sys.argv) > 1 else RELEASE


def clamp(value):
    return max(0.0, min(1.0, value))


def mix(a, b, amount):
    return tuple(round(x + (y - x) * amount) for x, y in zip(a, b))


def pixel(x, y, size, margin):
    body = size - 2 * margin
    scale = body / 1024
    u = (x + 0.5 - margin) / scale
    v = (y + 0.5 - margin) / scale
    across, down = abs(u - 512) / 512, abs(v - 512) / 512
    if across >= 1 or down >= 1:
        return CLEAR
    shape = clamp((1 - (across**5 + down**5) ** 0.2) * 512 * scale + 0.5)
    if shape == 0:
        return CLEAR
    color = mix(TOP, BOTTOM, v / 1024)
    column, row = round((u - FIRST) / STEP), round((v - FIRST) / STEP)
    if 0 <= column < 7 and 0 <= row < 7:
        distance = math.hypot(u - FIRST - column * STEP, v - FIRST - row * STEP)
        color = mix(color, COLORS[PATTERN[row][column]], clamp((RADIUS - distance) * scale + 0.5))
    return (*color, round(shape * 255))


def png(size, margin):
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            rows.extend(pixel(x, y, size, margin))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")


def icns():
    entries = [(b"icp4", 16), (b"icp5", 32), (b"icp6", 64), (b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic10", 1024)]
    body = b""
    for kind, size in entries:
        data = png(size, round(size * 100 / 1024))
        body += kind + struct.pack(">I", len(data) + 8) + data
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def ico():
    images = [(size, png(size, 0)) for size in (16, 24, 32, 48, 64, 128, 256)]
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = len(header) + 16 * len(images)
    entries = b""
    data = b""
    for size, image in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(image), offset + len(data))
        data += image
    return header + entries + data


folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent.parent / "assets" / "icon"
(folder / "nucli.png").write_bytes(png(512, 0))
(folder / "nucli.icns").write_bytes(icns())
(folder / "nucli.ico").write_bytes(ico())
