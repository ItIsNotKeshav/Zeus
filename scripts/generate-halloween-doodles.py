#!/usr/bin/env python3
"""Builds assets/doodles/halloween.svg and the Halloween icons in assets/icons.

The icons are original line drawings on Lucide's 24 x 24 grid, 2 px round
strokes, made for Zeus (no third-party artwork). The tile reuses the
placements (position, rotation, scale) of the classic tile in
assets/wallpaper.svg, so it tiles at 374 x 666 like the other styles. Needs
no network access: run it from the repository root.
"""
import math, re

def web():
    # A corner web: radials from the top left, three sagging rings.
    cx = cy = 2.0
    angles = [0, 22.5, 45, 67.5, 90]
    pts = lambda r: [(cx + r * math.cos(math.radians(a)), cy + r * math.sin(math.radians(a))) for a in angles]
    d = []
    for x, y in pts(19):
        d.append(f"M{cx:g} {cy:g}L{x:.1f} {y:.1f}")
    for r in (6, 11, 16):
        p = pts(r)
        for (x1, y1), (x2, y2) in zip(p, p[1:]):
            mx, my = (x1 + x2) / 2, (y1 + y2) / 2
            qx, qy = mx + (cx - mx) * 0.22, my + (cy - my) * 0.22
            d.append(f"M{x1:.1f} {y1:.1f}Q{qx:.1f} {qy:.1f} {x2:.1f} {y2:.1f}")
    return f'<path d="{"".join(d)}"/>'

def spider():
    legs = [
        "M10.3 11.6 6.6 8.4 3.4 10.2", "M9.7 13.2 4.8 12.6 2.4 14.6",
        "M9.8 14.8 5.2 17 3.8 20.4", "M10.8 16.2 8.6 20.4 7.6 22",
    ]
    mirror = lambda s: re.sub(r"(\d+(?:\.\d+)?) (\d+(?:\.\d+)?)", lambda m: f"{24 - float(m[1]):g} {m[2]}", s)
    return ('<path d="M12 2v7"/><circle cx="12" cy="10.2" r="1.6"/><ellipse cx="12" cy="15" rx="2.7" ry="3.4"/>'
            + "".join(f'<path d="{l}"/><path d="{mirror(l)}"/>' for l in legs))

ICONS = {
    "pumpkin": '<path d="M12 7.4C9.6 5.6 5.4 6.2 4 9.6c-1.5 3.6.2 8.2 3.6 9.6 1.6.7 3.3.4 4.4-.5 1.1.9 2.8 1.2 4.4.5 3.4-1.4 5.1-6 3.6-9.6C18.6 6.2 14.4 5.6 12 7.4Z"/><path d="M9.4 7c-1.7 3.4-1.7 8.4 0 11.8M14.6 7c1.7 3.4 1.7 8.4 0 11.8"/><path d="M12 7.4c0-1.8.7-3.2 2.4-3.9"/><path d="m8 11.3 1.5 2H6.5Z"/><path d="m16 11.3 1.5 2h-3Z"/><path d="M9 16.2c1.6 1 4.4 1 6 0"/>',
    "ghost": '<path d="M5 20.5V10.2a7 7 0 0 1 14 0v10.3l-2.4-2-2.3 2-2.3-2-2.3 2-2.3-2Z"/><path d="M9.4 10.4h.01M14.6 10.4h.01"/><path d="M10.6 13.4c.8.7 1.9.7 2.8 0"/>',
    "bat": '<path d="M12 8.4c-.9 0-1.5-.9-1.8-2.4-.1 1.2-.3 2.2-1.1 2.8C7.8 7.7 5 7.4 2 9.6c1.2.5 1.9 1.5 1.9 3 1-.8 2.2-.9 3.2-.3.5 1.1 1.5 1.9 3 2.1L12 17.6l2-3.2c1.5-.2 2.5-1 3-2.1 1-.6 2.2-.5 3.2.3 0-1.5.7-2.5 1.9-3C19 7.4 16.2 7.7 14.9 8.8c-.8-.6-1-1.6-1.1-2.8-.3 1.5-.9 2.4-1.8 2.4Z"/>',
    "web": web(),
    "candy": '<g transform="rotate(-40 12 12)"><rect x="7" y="8" width="10" height="8" rx="4"/><path d="M7.4 12 2.6 8.6v6.8L7.4 12Zm9.2 0 4.8-3.4v6.8L16.6 12Z"/><path d="M10.6 8.4v7.2M13.4 8.4v7.2"/></g>',
    "moon": '<path d="M19.6 14.6A8.2 8.2 0 0 1 9.4 4.4a8.2 8.2 0 1 0 10.2 10.2Z"/><path d="M16.5 4.5v3M15 6h3"/>',
    "skull": '<path d="M12 3C7.6 3 4.6 6 4.6 10c0 2.3 1 4 2.4 5v3.5a1.5 1.5 0 0 0 1.5 1.5h7a1.5 1.5 0 0 0 1.5-1.5V15c1.4-1 2.4-2.7 2.4-5 0-4-3-7-7.4-7Z"/><circle cx="9" cy="10.8" r="1.6"/><circle cx="15" cy="10.8" r="1.6"/><path d="m12 13.6-1 1.9h2Z"/><path d="M10 20v-2.4M14 20v-2.4"/>',
    "cauldron": '<path d="M3.6 9.4h16.8"/><path d="M5 9.4c-.6 4.4.4 8.6 3.2 10.1h7.6c2.8-1.5 3.8-5.7 3.2-10.1"/><path d="M8.4 19.5 7.4 22M15.6 19.5l1 2.5"/><circle cx="9.6" cy="6" r="1.3"/><circle cx="14.2" cy="4.6" r="1.9"/><circle cx="14.8" cy="7.7" r=".5"/>',
    "hat": '<path d="M2.8 17.4c0-2.2 4-3.8 9.2-3.8s9.2 1.6 9.2 3.8-4.1 3.6-9.2 3.6-9.2-1.4-9.2-3.6Z"/><path d="M7.4 14.3C8.4 9 10.5 5.4 14.8 2.8c-.5 1.9.1 3.2 1.1 4.1 1 1.6 1.5 4.3 1.3 7.4"/><path d="M7.9 13.2c2.7 1.1 6.6 1.1 9.3 0"/>',
    "candle": '<rect x="8" y="10.5" width="8" height="10.5" rx="1.5"/><path d="M12 10.5V9.2"/><path d="M12 2.8c1.9 2 2.7 3.3 2.7 4.5a2.7 2.7 0 0 1-5.4 0c0-1.2.8-2.5 2.7-4.5Z"/><path d="M10.4 10.5v2.8"/><path d="M6 21h12"/>',
    "spider": spider(),
    "eye": '<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3.6"/><circle cx="12" cy="12" r="1.2"/><path d="M5 6.2 6.4 8M19 6.2 17.6 8"/>',
    "tombstone": '<path d="M6.2 21V10.2a5.8 5.8 0 0 1 11.6 0V21"/><path d="M3 21h18"/><path d="M12 9v6.2M9.4 11.6h5.2"/>',
    "lollipop": '<circle cx="12" cy="9" r="6.2"/><path d="M12 9.2c.9 0 1.5.6 1.5 1.4S12.9 12 12 12s-2.5-.9-2.5-2.6 1.3-2.9 2.8-2.9 3.2 1.3 3.2 2.9"/><path d="M12 15.2V22"/>',
    "sparkle": '<path d="M12 3c.6 4.5 1.5 5.4 6 6-4.5.6-5.4 1.5-6 6-.6-4.5-1.5-5.4-6-6 4.5-.6 5.4-1.5 6-6Z"/><path d="M19 16v4M17 18h4"/>',
    "cat": '<path d="M4.4 3.8 8.6 6.6a9 9 0 0 1 6.8 0l4.2-2.8c.4 1.6.4 3.6.1 5.6a7.6 7.6 0 0 1-7.7 11.6A7.6 7.6 0 0 1 4.3 9.4c-.3-2-.3-3.8.1-5.6Z"/><path d="M9 12h.01M15 12h.01"/><path d="m11 15 1 .9 1-.9Z"/><path d="M2.6 14.6 6 14M21.4 14.6 18 14"/>',
    "potion": '<path d="M9.5 3h5M10.2 3v6.2L4.7 18.5a2 2 0 0 0 1.7 3h11.2a2 2 0 0 0 1.7-3l-5.5-9.3V3"/><path d="M7.4 15.4c1.5-.9 3-.9 4.6 0s3.1.9 4.6 0"/><circle cx="10.4" cy="18.6" r=".5"/><circle cx="14" cy="17.9" r=".5"/>',
    "candycorn": '<path d="M12 3.2c3.2 0 5.7 7.6 7.2 14.6A1.6 1.6 0 0 1 17.6 20H6.4a1.6 1.6 0 0 1-1.6-2.2C6.3 10.8 8.8 3.2 12 3.2Z"/><path d="M8 11.8h8M6.4 16.2h11.2"/>',
}
ICON_FILES = {"pumpkin": "pumpkin", "ghost": "ghost"}

def icon_svg(body):
    return ('<svg\n  xmlns="http://www.w3.org/2000/svg"\n  width="24"\n  height="24"\n  viewBox="0 0 24 24"\n'
            '  fill="none"\n  stroke="#ffffff"\n  stroke-width="2"\n  stroke-linecap="round"\n'
            f'  stroke-linejoin="round"\n>\n  {body}\n</svg>\n')

for name, file in ICON_FILES.items():
    open(f"assets/icons/{file}.svg", "w").write(icon_svg(ICONS[name]))

# Pumpkins, bats and ghosts appear most; the rest fill in between.
ORDER = ["pumpkin", "bat", "ghost", "web", "candy", "moon", "skull", "cauldron", "hat", "candle",
         "spider", "eye", "tombstone", "lollipop", "pumpkin", "sparkle", "cat", "potion", "bat",
         "candycorn", "ghost", "pumpkin", "moon", "spider", "candy", "skull", "web", "hat", "cat", "lollipop"]
classic = open("assets/wallpaper.svg").read()
ids = list(dict.fromkeys(re.findall(r'xlink:href="#(\w+)"', classic)))
body = classic[classic.index("</defs>"):]
defs = "".join(f'<g id="{i}">{ICONS[ORDER[n % len(ORDER)]]}</g>\n' for n, i in enumerate(ids))
head = classic[:classic.index("<defs>")]
head = re.sub(r"<!--.*?-->", "<!-- Halloween doodles drawn for Zeus in the Lucide line style (original artwork). -->", head, flags=re.S)
open("assets/doodles/halloween.svg", "w").write(head + "<defs>\n" + defs + body)
print("halloween", len(ids), "doodles")
