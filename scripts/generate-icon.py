#!/usr/bin/env python3
import math
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "assets"
ICON_SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)
BACKGROUND_TOP = (27, 32, 38)
BACKGROUND_BOTTOM = (14, 17, 21)
BORDER = (57, 64, 72)
ACCENT = (232, 154, 65)
ACCENT_LIGHT = (247, 183, 102)


def clamp(value, low=0.0, high=1.0):
    return max(low, min(high, value))


def coverage(distance):
    return clamp(0.5 - distance)


def rounded_rect_distance(x, y, cx, cy, half, radius):
    qx = abs(x - cx) - (half - radius)
    qy = abs(y - cy) - (half - radius)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    inside = min(max(qx, qy), 0.0)
    return outside + inside - radius


def segment_distance(x, y, x1, y1, x2, y2):
    dx = x2 - x1
    dy = y2 - y1
    length_sq = dx * dx + dy * dy
    if length_sq == 0.0:
        return math.hypot(x - x1, y - y1)
    t = clamp(((x - x1) * dx + (y - y1) * dy) / length_sq)
    return math.hypot(x - (x1 + t * dx), y - (y1 + t * dy))


def over(dst, color, alpha):
    if alpha <= 0.0:
        return dst
    da = dst[3]
    out_a = alpha + da * (1.0 - alpha)
    if out_a <= 0.0:
        return (0.0, 0.0, 0.0, 0.0)
    out_rgb = tuple(
        (color[i] * alpha + dst[i] * da * (1.0 - alpha)) / out_a for i in range(3)
    )
    return (*out_rgb, out_a)


def render(size):
    center = size * 0.5
    half = size * 0.465
    radius = size * 0.215
    border = max(0.7, min(2.0, size * 0.009))
    gap = size * 0.08
    thickness = max(0.65, size * 0.040)
    arm_start = gap + thickness
    arm_end = size * 0.33 - thickness
    dot_radius = max(0.55, size * 0.040)
    pixels = bytearray()

    for y in range(size):
        for x in range(size):
            px = x + 0.5
            py = y + 0.5
            outer = rounded_rect_distance(px, py, center, center, half, radius)
            outer_alpha = coverage(outer)
            color = (0.0, 0.0, 0.0, 0.0)
            if outer_alpha > 0.0:
                color = (BORDER[0], BORDER[1], BORDER[2], outer_alpha)
                inner = rounded_rect_distance(
                    px,
                    py,
                    center,
                    center,
                    half - border,
                    max(0.0, radius - border),
                )
                inner_alpha = coverage(inner)
                if inner_alpha > 0.0:
                    t = py / max(1.0, size)
                    base = tuple(
                        BACKGROUND_TOP[i]
                        + (BACKGROUND_BOTTOM[i] - BACKGROUND_TOP[i]) * t
                        for i in range(3)
                    )
                    color = over(color, base, inner_alpha)

            arm_alpha = 0.0
            arm_alpha = max(
                arm_alpha,
                coverage(segment_distance(px, py, center + arm_start, center, center + arm_end, center) - thickness),
                coverage(segment_distance(px, py, center - arm_start, center, center - arm_end, center) - thickness),
                coverage(segment_distance(px, py, center, center + arm_start, center, center + arm_end) - thickness),
                coverage(segment_distance(px, py, center, center - arm_start, center, center - arm_end) - thickness),
            )
            if arm_alpha > 0.0:
                color = over(color, ACCENT, arm_alpha)

            dot_distance = math.hypot(px - center, py - center) - dot_radius
            dot_alpha = coverage(dot_distance) * outer_alpha
            if dot_alpha > 0.0:
                color = over(color, ACCENT_LIGHT, dot_alpha)

            pixels.extend(
                (
                    int(round(clamp(color[0], 0, 255))),
                    int(round(clamp(color[1], 0, 255))),
                    int(round(clamp(color[2], 0, 255))),
                    int(round(clamp(color[3]) * 255.0)),
                )
            )
    return bytes(pixels)


def png_chunk(kind, data):
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def write_png(path, size):
    rgba = render(size)
    rows = bytearray()
    stride = size * 4
    for y in range(size):
        rows.append(0)
        rows.extend(rgba[y * stride : (y + 1) * stride])
    data = (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(bytes(rows), 9))
        + png_chunk(b"IEND", b"")
    )
    path.write_bytes(data)


def bmp_entry(size):
    rgba = render(size)
    row_bytes = ((size + 31) // 32) * 4
    mask = bytes(row_bytes * size)
    header = struct.pack(
        "<IiiHHIIiiII",
        40,
        size,
        size * 2,
        1,
        32,
        0,
        size * size * 4 + len(mask),
        0,
        0,
        0,
        0,
    )
    pixels = bytearray()
    stride = size * 4
    for y in range(size - 1, -1, -1):
        row = rgba[y * stride : (y + 1) * stride]
        for x in range(size):
            r, g, b, a = row[x * 4 : x * 4 + 4]
            pixels.extend((b, g, r, a))
    return header + bytes(pixels) + mask


def write_ico(path):
    images = [(size, bmp_entry(size)) for size in ICON_SIZES]
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    directory = bytearray()
    for size, data in images:
        directory.extend(
            struct.pack(
                "<BBBBHHII",
                0 if size == 256 else size,
                0 if size == 256 else size,
                0,
                0,
                1,
                32,
                len(data),
                offset,
            )
        )
        offset += len(data)
    path.write_bytes(header + bytes(directory) + b"".join(data for _, data in images))


def main():
    if sys.version_info < (3, 8):
        raise SystemExit("Python 3.8 or newer is required")
    ASSETS.mkdir(parents=True, exist_ok=True)
    write_ico(ASSETS / "icon.ico")
    write_png(ASSETS / "icon.png", 512)
    print(f"Wrote {ASSETS / 'icon.ico'}")
    print(f"Wrote {ASSETS / 'icon.png'}")


if __name__ == "__main__":
    main()
