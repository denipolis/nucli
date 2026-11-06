#!/usr/bin/env python3
import pathlib
import struct
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
COLORS = {".": (42, 42, 46, 255), "R": (255, 90, 122, 255), "Y": (255, 207, 64, 255)}
BACKGROUND = (17, 17, 17, 255)
CLEAR = (0, 0, 0, 0)


def pixel(x, y, size, margin):
    body = size - 2 * margin
    if not (margin <= x < size - margin and margin <= y < size - margin):
        return CLEAR
    cell = body / 9
    column = (x - margin) / cell - 1
    row = (y - margin) / cell - 1
    if 0 <= column < 7 and 0 <= row < 7 and column % 1 < 0.86 and row % 1 < 0.86:
        return COLORS[PATTERN[int(row)][int(column)]]
    return BACKGROUND


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


folder = pathlib.Path(__file__).resolve().parent.parent / "assets" / "icon"
(folder / "nucli.png").write_bytes(png(512, 0))
(folder / "nucli.icns").write_bytes(icns())
(folder / "nucli.ico").write_bytes(ico())
