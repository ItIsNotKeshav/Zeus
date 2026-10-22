#!/usr/bin/env python3
"""Frame a screenshot in a MacBook-style laptop for the README.

The laptop is drawn here with Pillow, so there is no third-party artwork or
licence to track. Usage:

    scripts/macbook-mockup.py shot.png out.png [--bg light|dark|#rrggbb]
"""
import argparse
from PIL import Image, ImageDraw, ImageFilter

SS = 2  # draw at 2x and downsample for smooth edges


def frame(shot, bg):
    w, h = shot.size
    bezel = int(w * 0.012)
    lid_r = int(w * 0.022)
    screen_r = int(w * 0.006)
    pad = int(w * 0.09)
    base_h = int(w * 0.022)
    lid_w, lid_h = w + 2 * bezel, h + 2 * bezel
    cw = lid_w + 2 * pad
    ch = lid_h + base_h + 2 * pad
    s = SS
    canvas = Image.new("RGBA", (cw * s, ch * s), bg + (255,))
    lx, ly = pad * s, pad * s

    # soft ground shadow
    sh = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(sh)
    bx0 = lx - int(lid_w * 0.04) * s
    by = ly + (lid_h + base_h) * s
    d.ellipse((bx0, by - 10 * s, bx0 + int(lid_w * 1.08) * s, by + int(base_h * 1.6) * s),
              fill=(0, 0, 0, 110))
    sh = sh.filter(ImageFilter.GaussianBlur(int(w * 0.012) * s))
    canvas.alpha_composite(sh)

    d = ImageDraw.Draw(canvas)
    # lid (aluminium) and black bezel
    d.rounded_rectangle((lx, ly, lx + lid_w * s, ly + lid_h * s), lid_r * s, fill=(201, 203, 208, 255))
    d.rounded_rectangle((lx + 2 * s, ly + 2 * s, lx + (lid_w - 2) * s, ly + (lid_h - 2) * s),
                        (lid_r - 2) * s, fill=(10, 10, 12, 255))
    # base: thin wedge that overhangs the lid slightly
    by0 = ly + lid_h * s
    bx0 = lx - int(lid_w * 0.035) * s
    bx1 = lx + lid_w * s + int(lid_w * 0.035) * s
    d.rounded_rectangle((bx0, by0, bx1, by0 + base_h * s), int(base_h * 0.6) * s,
                        fill=(214, 216, 221, 255))
    d.rectangle((bx0, by0, bx1, by0 + int(base_h * 0.5) * s), fill=(214, 216, 221, 255))
    # thumb notch
    nw = int(lid_w * 0.14)
    nx = lx + (lid_w * s - nw * s) // 2
    d.rounded_rectangle((nx, by0, nx + nw * s, by0 + int(base_h * 0.5) * s), int(base_h * 0.3) * s,
                        fill=(176, 178, 184, 255))
    # hinge shade
    d.rectangle((lx + 4 * s, by0 - 2 * s, lx + (lid_w - 4) * s, by0 + 2 * s), fill=(150, 152, 158, 255))

    # screen, rounded
    big = shot.convert("RGBA").resize((w * s, h * s), Image.LANCZOS)
    mask = Image.new("L", big.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, w * s - 1, h * s - 1), screen_r * s, fill=255)
    canvas.paste(big, (lx + bezel * s, ly + bezel * s), mask)

    # camera dot centred in the top bezel
    cx, cy, r = lx + lid_w * s // 2, ly + bezel * s // 2, max(2, bezel // 5) * s
    d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(40, 42, 48, 255))

    return canvas.resize((cw, ch), Image.LANCZOS)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("shot")
    p.add_argument("out")
    p.add_argument("--bg", default="light", help="light, dark, or #rrggbb")
    a = p.parse_args()
    presets = {"light": (238, 240, 244), "dark": (24, 25, 29)}
    bg = presets.get(a.bg) or tuple(int(a.bg.lstrip("#")[i:i + 2], 16) for i in (0, 2, 4))
    frame(Image.open(a.shot), bg).convert("RGB").save(a.out, optimize=True)


if __name__ == "__main__":
    main()
