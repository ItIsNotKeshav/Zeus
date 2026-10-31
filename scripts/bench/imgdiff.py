#!/usr/bin/env python3
"""Compares two builds' demo screenshots pixel by pixel.

    python3 scripts/bench/imgdiff.py bench-shots/main bench-shots/pr66
    python3 scripts/bench/imgdiff.py A B --diff-dir bench-shots/diff

For every PNG present in both folders it prints the largest difference of
any colour channel of any pixel (0 is identical, 255 the most), how many
pixels differ at all, and the box around them, as a Markdown table. With
--diff-dir it also writes a picture of each pair with the differing pixels
in red. Exits 1 when any pair differs by more than --tolerance (default 0).
Screenshots from scripts/bench/shots.sh freeze the clock, so the same page of
two builds on the same machine should match exactly. Standard library only.
"""

import argparse
import struct
import sys
import zlib
from pathlib import Path

SIGNATURE = b"\x89PNG\r\n\x1a\n"
CHANNELS = {0: 1, 2: 3, 4: 2, 6: 4}


def read_png(path):
    """Width, height and RGBA bytes of an 8-bit, non-interlaced PNG."""
    data = Path(path).read_bytes()
    if not data.startswith(SIGNATURE):
        raise ValueError(f"{path} is not a PNG")
    offset, idat, header = len(SIGNATURE), [], None
    while offset < len(data):
        length, kind = struct.unpack(">I4s", data[offset : offset + 8])
        body = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if header is None:
        raise ValueError(f"{path} has no header")
    width, height, depth, color, _, _, interlace = header
    if depth != 8 or color not in CHANNELS or interlace:
        raise ValueError(f"{path}: only 8-bit, non-interlaced grey, RGB or RGBA PNGs are read")
    channels = CHANNELS[color]
    raw = zlib.decompress(b"".join(idat))
    stride = width * channels
    rows, previous, position = [], bytearray(stride), 0
    for _ in range(height):
        kind = raw[position]
        line = bytearray(raw[position + 1 : position + 1 + stride])
        position += 1 + stride
        unfilter(kind, line, previous, channels)
        rows.append(line)
        previous = line
    return width, height, to_rgba(b"".join(rows), channels)


def unfilter(kind, line, previous, step):
    if kind == 0:
        return
    for i in range(len(line)):
        left = line[i - step] if i >= step else 0
        up = previous[i]
        if kind == 1:
            line[i] = (line[i] + left) & 0xFF
        elif kind == 2:
            line[i] = (line[i] + up) & 0xFF
        elif kind == 3:
            line[i] = (line[i] + (left + up) // 2) & 0xFF
        elif kind == 4:
            corner = previous[i - step] if i >= step else 0
            line[i] = (line[i] + paeth(left, up, corner)) & 0xFF
        else:
            raise ValueError(f"unknown PNG filter {kind}")


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def to_rgba(pixels, channels):
    if channels == 4:
        return bytes(pixels)
    out = bytearray()
    for i in range(0, len(pixels), channels):
        if channels == 3:
            out += pixels[i : i + 3] + b"\xff"
        elif channels == 2:
            out += bytes([pixels[i]] * 3) + pixels[i + 1 : i + 2]
        else:
            out += bytes([pixels[i]] * 3) + b"\xff"
    return bytes(out)


def write_png(path, width, height, rgba):
    def chunk(kind, body):
        crc = zlib.crc32(kind + body) & 0xFFFFFFFF
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", crc)

    stride = width * 4
    raw = b"".join(b"\x00" + rgba[y * stride : (y + 1) * stride] for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    Path(path).write_bytes(
        SIGNATURE + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    )


def compare(first, second, mark=False):
    """Largest channel difference, differing pixels, total pixels, the box
    around the differences (or None), and, when asked, a picture of the first
    image, dimmed, with the differing pixels in red."""
    width, height, a = first
    other_width, other_height, b = second
    if (width, height) != (other_width, other_height):
        raise ValueError(f"sizes differ: {width}x{height} and {other_width}x{other_height}")
    largest, count, box = 0, 0, None
    stride = width * 4
    differing = []
    for y in range(height):
        row = slice(y * stride, (y + 1) * stride)
        if a[row] == b[row]:
            continue
        for index in range(row.start, row.stop, 4):
            difference = max(abs(a[index + c] - b[index + c]) for c in range(4))
            if not difference:
                continue
            largest = max(largest, difference)
            count += 1
            x = (index - row.start) // 4
            box = (x, y, x, y) if box is None else (
                min(box[0], x), min(box[1], y), max(box[2], x), max(box[3], y)
            )
            if mark:
                differing.append(index)
    marked = None
    if mark:
        dim = bytearray(len(a))
        dim[0::4], dim[1::4], dim[2::4] = (bytes(v // 3 for v in a[c::4]) for c in range(3))
        dim[3::4] = b"\xff" * (len(a) // 4)
        for index in differing:
            dim[index : index + 4] = b"\xff\x00\x00\xff"
        marked = bytes(dim)
    return largest, count, width * height, box, marked


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("first", help="the baseline build's screenshots")
    parser.add_argument("second", help="the other build's screenshots")
    parser.add_argument("--tolerance", type=int, default=0, help="largest difference allowed (0 to 255)")
    parser.add_argument("--diff-dir", help="write pictures marking the differences here")
    args = parser.parse_args(argv)
    first, second = Path(args.first), Path(args.second)
    names = sorted({p.name for p in first.glob("*.png")} & {p.name for p in second.glob("*.png")})
    if not names:
        print("No screenshots with the same name in both folders.", file=sys.stderr)
        return 2
    if args.diff_dir:
        Path(args.diff_dir).mkdir(parents=True, exist_ok=True)
    print("| Page | Largest difference (0 to 255) | Pixels that differ | Where |")
    print("|---|---|---|---|")
    worst = 0
    for name in names:
        try:
            a, b = read_png(first / name), read_png(second / name)
            largest, count, total, box, marked = compare(a, b, mark=bool(args.diff_dir))
        except ValueError as error:
            print(f"| {name} | could not compare: {error} | | |")
            worst = 256
            continue
        worst = max(worst, largest)
        where = "" if box is None else f"x {box[0]} to {box[2]}, y {box[1]} to {box[3]}"
        print(f"| {Path(name).stem} | {largest} | {count} of {total} ({count / total:.3%}) | {where} |")
        if args.diff_dir and count:
            write_png(Path(args.diff_dir) / name, a[0], a[1], marked)
    return 0 if worst <= args.tolerance else 1


if __name__ == "__main__":
    sys.exit(main())
