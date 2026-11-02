"""Tests for imgdiff.py.  python3 -m unittest discover -s scripts/bench -p 'test_*.py'"""

import contextlib
import io
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import imgdiff  # noqa: E402


def png(path, width, height, pixels, color=6, filters=(0,)):
    """Writes pixels (bytes per row, already in the colour type's layout)
    with the given row filters applied in turn."""
    channels = imgdiff.CHANNELS[color]
    stride = width * channels
    raw, previous = b"", bytes(stride)
    for y in range(height):
        line = pixels[y * stride : (y + 1) * stride]
        kind = filters[y % len(filters)]
        raw += bytes([kind]) + encode(kind, line, previous, channels)
        previous = line

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, color, 0, 0, 0)
    Path(path).write_bytes(
        imgdiff.SIGNATURE + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
    )


def encode(kind, line, previous, step):
    out = bytearray()
    for i, value in enumerate(line):
        left = line[i - step] if i >= step else 0
        up = previous[i]
        corner = previous[i - step] if i >= step else 0
        predictor = [0, left, up, (left + up) // 2, imgdiff.paeth(left, up, corner)][kind]
        out.append((value - predictor) & 0xFF)
    return bytes(out)


class ImgDiffTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.root = Path(self.folder.name)

    def tearDown(self):
        self.folder.cleanup()

    def test_every_filter_and_colour_type_reads_back(self):
        width, height = 5, 5
        rgba = bytes((x * 37 + y * 11 + c * 5) & 0xFF for y in range(height) for x in range(width) for c in range(4))
        png(self.root / "a.png", width, height, rgba, filters=(0, 1, 2, 3, 4))
        self.assertEqual(imgdiff.read_png(self.root / "a.png"), (width, height, rgba))
        rgb = bytes(value for index, value in enumerate(rgba) if index % 4 != 3)
        png(self.root / "b.png", width, height, rgb, color=2, filters=(4, 3))
        _, _, read = imgdiff.read_png(self.root / "b.png")
        self.assertEqual(read[0:4], rgb[0:3] + b"\xff")
        grey = bytes(range(25))
        png(self.root / "c.png", width, height, grey, color=0, filters=(1,))
        self.assertEqual(imgdiff.read_png(self.root / "c.png")[2][4:8], bytes([1, 1, 1, 255]))

    def test_the_largest_difference_the_count_and_the_box(self):
        width, height = 4, 3
        a = bytearray(b"\x10\x20\x30\xff" * width * height)
        b = bytearray(a)
        b[(1 * width + 2) * 4 + 1] = 0x20 + 9  # x 2, y 1, green
        b[(2 * width + 0) * 4 + 0] = 0x10 + 3  # x 0, y 2, red
        largest, count, total, box, marked = imgdiff.compare((width, height, bytes(a)), (width, height, bytes(b)), mark=True)
        self.assertEqual((largest, count, total, box), (9, 2, 12, (0, 1, 2, 2)))
        self.assertEqual(marked[(1 * width + 2) * 4 : (1 * width + 2) * 4 + 4], b"\xff\x00\x00\xff")
        self.assertEqual(marked[0:4], bytes([0x10 // 3, 0x20 // 3, 0x30 // 3, 255]))
        same = imgdiff.compare((width, height, bytes(a)), (width, height, bytes(a)))
        self.assertEqual(same[:4], (0, 0, 12, None))
        with self.assertRaises(ValueError):
            imgdiff.compare((width, height, bytes(a)), (2, 2, bytes(16)))

    def test_the_command_reports_pages_and_fails_past_the_tolerance(self):
        for name in ("one", "two"):
            (self.root / name).mkdir()
        pixels = bytes([200, 100, 50, 255] * 4)
        png(self.root / "one" / "chat.png", 2, 2, pixels)
        png(self.root / "two" / "chat.png", 2, 2, pixels)
        changed = bytearray(pixels)
        changed[0] = 0
        png(self.root / "one" / "light.png", 2, 2, pixels)
        png(self.root / "two" / "light.png", 2, 2, bytes(changed))
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = imgdiff.main([str(self.root / "one"), str(self.root / "two"), "--diff-dir", str(self.root / "diff")])
        self.assertEqual(code, 1)
        self.assertIn("| chat | 0 | 0 of 4 (0.000%) |  |", out.getvalue())
        self.assertIn("| light | 200 | 1 of 4 (25.000%) | x 0 to 0, y 0 to 0 |", out.getvalue())
        self.assertTrue((self.root / "diff" / "light.png").exists())
        self.assertFalse((self.root / "diff" / "chat.png").exists())
        self.assertEqual(imgdiff.read_png(self.root / "diff" / "light.png")[2][0:4], b"\xff\x00\x00\xff")
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(imgdiff.main([str(self.root / "one"), str(self.root / "two"), "--tolerance", "200"]), 0)


if __name__ == "__main__":
    unittest.main()
