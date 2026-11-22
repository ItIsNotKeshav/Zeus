#!/usr/bin/env python3
"""Draws Zeus's app icons: the default mark and every tile the App icon
picker offers, each at two levels of detail.

    python3 scripts/app-icons.py            # the SVGs only, no dependencies
    python3 scripts/app-icons.py --png      # also the PNG and ICO sets
                                            # (pip install resvg-py pillow)

Writes:

- `packaging/icons/zeus.svg` and `assets/app-icons/<name>.svg`: the full
  artwork, shown from 40 pixels up (the picker, the window, the Dock).
- `packaging/icons/zeus-small.svg` and `assets/app-icons/small/<name>.svg`:
  the same tile with a bigger, heavier mark and no fine detail, for the
  tray and the taskbar (16 to 32 pixels), where shading and stars only blur.
- With `--png`: `packaging/icons/zeus-1024.png` (the full tile),
  `packaging/macos/zeus-1024.png` (the tile on Apple's 1024 grid, for the
  .icns), `packaging/macos/icon-dock.png` (the Dock tile Cargo runs show) and
  `packaging/windows/zeus.ico` (16 to 256, the small mark below 40).

Every icon shares one mark, one tile shape and one lighting: a light from
above that brightens the tile's top rim and casts the mark's shadow down.
Only the material changes between them.
"""

import math
import pathlib
import sys

ROOT = pathlib.Path(__import__("os").environ.get("ICON_ROOT") or pathlib.Path(__file__).resolve().parent.parent)

# The mark: a Z whose diagonal breaks into a lightning bolt, as one outline
# on the 128 grid. The small one is heavier and fills more of the tile.
MARK = [
    (35, 31), (35, 43.5), (68, 43.5), (47.15, 66), (57.15, 66), (40, 84.5),
    (35, 84.5), (35, 97), (93, 97), (93, 84.5), (60, 84.5), (80.85, 62),
    (70.85, 62), (88, 43.5), (93, 43.5), (93, 31),
]
MARK_SMALL = [
    (29, 27), (29, 43), (69, 43), (44.36, 66), (54.36, 66), (34, 85),
    (29, 85), (29, 101), (99, 101), (99, 85), (59, 85), (83.64, 62),
    (73.64, 62), (94, 43), (99, 43), (99, 27),
]


def rounded(points, radius):
    """A closed path through `points` with every corner eased by `radius`."""
    count = len(points)
    parts = []
    for index in range(count):
        before, here, after = (
            points[index - 1],
            points[index],
            points[(index + 1) % count],
        )

        def toward(target):
            dx, dy = target[0] - here[0], target[1] - here[1]
            length = math.hypot(dx, dy)
            step = min(radius, length / 2)
            return here[0] + dx / length * step, here[1] + dy / length * step

        start, end = toward(before), toward(after)
        parts.append(
            f"{'M' if index == 0 else 'L'}{start[0]:.2f} {start[1]:.2f}"
            f"Q{here[0]:.2f} {here[1]:.2f} {end[0]:.2f} {end[1]:.2f}"
        )
    return "".join(parts) + "Z"


def squircle(left=4.0, size=120.0, exponent=5.0, steps=96):
    """Apple's continuous-corner tile: a superellipse, not a rounded rect."""
    centre, half = left + size / 2, size / 2
    points = []
    for step in range(steps):
        angle = 2 * math.pi * step / steps
        cos, sin = math.cos(angle), math.sin(angle)
        x = centre + half * math.copysign(abs(cos) ** (2 / exponent), cos)
        y = centre + half * math.copysign(abs(sin) ** (2 / exponent), sin)
        points.append(f"{x:.2f} {y:.2f}")
    return "M" + "L".join(points) + "Z"


TILE = squircle()
GLYPH = rounded(MARK, 1.6)
GLYPH_SMALL = rounded(MARK_SMALL, 2.0)


def stops(*colors):
    """`(offset, colour[, opacity])` tuples as gradient stops."""
    out = []
    for stop in colors:
        offset, color = stop[0], stop[1]
        opacity = f' stop-opacity="{stop[2]}"' if len(stop) > 2 else ""
        out.append(f'<stop offset="{offset}" stop-color="{color}"{opacity}/>')
    return "".join(out)


def linear(id_, colors, x1=0.5, y1=0.0, x2=0.5, y2=1.0):
    return (
        f'<linearGradient id="{id_}" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}">'
        f"{stops(*colors)}</linearGradient>"
    )


def radial(id_, colors, cx=0.5, cy=0.5, r=0.5):
    return (
        f'<radialGradient id="{id_}" cx="{cx}" cy="{cy}" r="{r}">'
        f"{stops(*colors)}</radialGradient>"
    )


def stars(points):
    return "".join(
        f'<circle cx="{x}" cy="{y}" r="{r}" fill="#fff" fill-opacity="{o}"/>'
        for x, y, r, o in points
    )


# Each icon: its tile, the mark's colour, the shadow the mark casts, and any
# scenery behind the mark (`extras`, dropped in the small version unless
# `keep_extras`).
ICONS = {
    "zeus": dict(
        tile=[(0, "#34353B"), (0.55, "#141416"), (1, "#050506")],
        mark=[(0, "#FFFFFF"), (1, "#DADCE3")],
        # Flat white when small: the macOS menu bar template cuts the mark
        # out of the tile by how white it is (`util::tray_template_rgba`).
        small_mark=[(0, "#FFFFFF"), (1, "#FFFFFF")],
        shadow=("#000000", 0.7),
        rim=0.34,
    ),
    "lagoon": dict(
        tile=[(0, "#6EE0FF"), (0.5, "#25AEF5"), (1, "#1477E0")],
        mark=[(0, "#FFFFFF"), (1, "#E3F4FF")],
        shadow=("#0A4E9A", 0.55),
    ),
    "meadow": dict(
        tile=[(0, "#5EEAD4"), (0.5, "#14B8A6"), (1, "#0A7F78")],
        mark=[(0, "#FFFFFF"), (1, "#DDFBF5")],
        shadow=("#06524C", 0.55),
    ),
    "orchid": dict(
        tile=[(0, "#C08BFF"), (0.5, "#9B5CF6"), (1, "#D9469E")],
        tile_angle=(0.15, 0, 0.85, 1),
        mark=[(0, "#FFFFFF"), (1, "#F6E6FF")],
        shadow=("#4C1873", 0.55),
    ),
    "sunset": dict(
        tile=[(0, "#FFD36B"), (0.45, "#FF8A48"), (1, "#E2416F")],
        mark=[(0, "#FFFFFF"), (1, "#FFEDE3")],
        shadow=("#8E2440", 0.5),
        extras=lambda: (
            '<circle cx="64" cy="122" r="38" fill="#FFF0C4" fill-opacity="0.26"/>'
            '<circle cx="64" cy="122" r="26" fill="#FFF0C4" fill-opacity="0.22"/>',
            "",
        ),
    ),
    "ocean": dict(
        tile=[(0, "#3A8BF5"), (0.55, "#1747B8"), (1, "#0A1E66")],
        mark=[(0, "#FFFFFF"), (1, "#D8E6FF")],
        shadow=("#020B33", 0.6),
        extras=lambda: (
            '<path d="M0 92 C22 82 40 100 64 92 S104 82 128 90 V128 H0Z"'
            ' fill="#5EB4FF" fill-opacity="0.18"/>'
            '<path d="M0 104 C24 96 42 112 66 104 S106 96 128 104 V128 H0Z"'
            ' fill="#7FD0FF" fill-opacity="0.16"/>',
            "",
        ),
    ),
    "mint": dict(
        tile=[(0, "#F4FFFA"), (1, "#BDEFD9")],
        mark=[(0, "#1CC796"), (1, "#0B8466")],
        shadow=("#0B8466", 0.35),
        lip=0.4,
        rim=0.9,
        edge=("#000000", 0.06),
    ),
    "graphite": dict(
        tile=[(0, "#6B7079"), (0.5, "#3B3F46"), (1, "#1F2226")],
        mark=[(0, "#FFFFFF"), (0.4, "#E2E5EA"), (0.75, "#B4B9C2"), (1, "#D9DCE2")],
        shadow=("#000000", 0.6),
        extras=lambda: (
            '<path d="M-10 40 L60 -10 L80 -10 L10 40Z" fill="#fff" fill-opacity="0.05"/>'
            '<path d="M-10 70 L90 -10 L98 -10 L-2 70Z" fill="#fff" fill-opacity="0.04"/>',
            "",
        ),
    ),
    "midnight": dict(
        tile=[(0, "#2A3A86"), (0.6, "#121A48"), (1, "#070A1E")],
        mark=[(0, "#FFFFFF"), (1, "#C9D5FF")],
        shadow=("#000000", 0.6),
        halo=("#8FA8FF", 0.45),
        extras=lambda: (
            '<path d="M104 14 A12 12 0 1 0 114 34 A9.5 9.5 0 1 1 104 14Z"'
            ' fill="#FFF4D6" fill-opacity="0.9"/>'
            + stars([(20, 22, 1.1, 0.9), (32, 108, 0.9, 0.6), (16, 66, 0.8, 0.6),
                     (70, 15, 0.8, 0.7), (108, 72, 0.8, 0.6), (98, 110, 1.0, 0.7),
                     (52, 112, 0.7, 0.5), (86, 22, 0.6, 0.5)]),
            "",
        ),
    ),
    "paper": dict(
        tile=[(0, "#FFFFFF"), (1, "#E7E9EE")],
        mark=[(0, "#2A2B30"), (1, "#0B0B0D")],
        shadow=("#000000", 0.22),
        lip=0.22,
        rim=1.0,
        edge=("#000000", 0.1),
    ),
    "glow": dict(
        tile=[(0, "#1E1340"), (0.6, "#0C0820"), (1, "#05030C")],
        mark=[(0, "#F2EBFF"), (0.5, "#C4A8FF"), (1, "#7DE3FF")],
        shadow=None,
        halo=("#9B6BFF", 0.95),
        halo_wide=("#5BD6FF", 0.5),
        rim=0.22,
    ),
    "longshadow": dict(
        tile=[(0, "#FFB45A"), (0.55, "#FF7A3A"), (1, "#EE4D2A")],
        mark=[(0, "#FFFFFF"), (1, "#FFF0E6")],
        shadow=("#9B2A0E", 0.35),
        long_shadow=True,
        keep_extras=True,
    ),
    "starfield": dict(
        tile=[(0, "#2F1E78"), (0.6, "#120A36"), (1, "#05030F")],
        mark=[(0, "#FFFFFF"), (1, "#E6DEFF")],
        shadow=("#000000", 0.55),
        extras=lambda: (
            '<rect width="128" height="128" fill="url(#neb1)"/>'
            '<rect width="128" height="128" fill="url(#neb2)"/>'
            + stars([(18, 20, 1.5, 0.95), (30, 42, 0.8, 0.7), (24, 94, 1.1, 0.8),
                     (44, 110, 0.7, 0.6), (104, 18, 1.7, 0.95), (112, 54, 0.8, 0.7),
                     (98, 86, 1.0, 0.7), (108, 106, 1.4, 0.9), (60, 13, 0.8, 0.7),
                     (13, 62, 0.7, 0.6), (80, 112, 0.7, 0.6)]),
            radial("neb1", [(0, "#E04BB0", 0.55), (1, "#E04BB0", 0)], cx=0.85, cy=0.15, r=0.6)
            + radial("neb2", [(0, "#2AA8F0", 0.5), (1, "#2AA8F0", 0)], cx=0.1, cy=0.92, r=0.6),
        ),
        keep_extras="nebula",
    ),
    "green": dict(
        tile=[(0, "#52EE8A"), (0.5, "#24D366"), (1, "#12A84E")],
        mark=[(0, "#FFFFFF"), (1, "#E4FBEC")],
        shadow=("#07612C", 0.5),
    ),
}


def long_shadow_path(points, length=80, step=1.0):
    """The mark swept down and to the right, as the union of its outline's
    edges dragged along the diagonal: each edge becomes a parallelogram."""
    parts = []
    count = len(points)
    for index in range(count):
        a, b = points[index], points[(index + 1) % count]
        a2 = (a[0] + length, a[1] + length)
        b2 = (b[0] + length, b[1] + length)
        quad = [a, b, b2, a2]
        # One winding for every piece, so the non-zero fill unites them.
        area = sum(
            p[0] * q[1] - q[0] * p[1] for p, q in zip(quad, quad[1:] + quad[:1])
        )
        if area < 0:
            quad.reverse()
        parts.append("M" + "L".join(f"{x:g} {y:g}" for x, y in quad) + "Z")
    # The mark itself, in the same winding, fills the gap the edges leave.
    return "".join(parts)


def icon_svg(name, spec, small):
    glyph = GLYPH_SMALL if small else GLYPH
    points = MARK_SMALL if small else MARK
    defs = []
    body = []
    angle = spec.get("tile_angle", (0.3, 0, 0.7, 1))
    defs.append(linear("tile", spec["tile"], *angle))
    defs.append(f'<clipPath id="clip"><path d="{TILE}"/></clipPath>')
    mark = spec.get("small_mark", spec["mark"]) if small else spec["mark"]
    defs.append(linear("mark", mark))
    body.append(f'<path d="{TILE}" fill="url(#tile)"/>')

    scenery = []
    extras = spec.get("extras")
    keep = spec.get("keep_extras")
    if extras and (not small or keep):
        shapes, more_defs = extras()
        if small and keep == "nebula":
            shapes = shapes.split("<circle", 1)[0]
        scenery.append(shapes)
        defs.append(more_defs)
    if spec.get("long_shadow"):
        defs.append(
            linear(
                "long",
                [(0, spec["shadow"][0], 0.42), (1, spec["shadow"][0], 0)],
                0.25, 0.25, 0.85, 0.85,
            )
        )
        scenery.append(
            f'<path d="{long_shadow_path(points)}" fill="url(#long)"/>'
        )
    # The light from above: a soft sheen over the top of the tile.
    if not small:
        defs.append(
            radial("sheen", [(0, "#FFFFFF", 0.22), (1, "#FFFFFF", 0)], cx=0.5, cy=0.0, r=0.75)
        )
        scenery.append('<rect width="128" height="70" fill="url(#sheen)"/>')
    if scenery:
        body.append(f'<g clip-path="url(#clip)">{"".join(scenery)}</g>')

    # The mark's glow (Glow, Midnight), then its cast shadow.
    halo = spec.get("halo")
    if halo:
        deviation = 2.0 if small else 4.0
        defs.append(
            f'<filter id="halo" x="-30%" y="-30%" width="160%" height="160%">'
            f'<feGaussianBlur stdDeviation="{deviation}"/></filter>'
        )
        body.append(
            f'<path d="{glyph}" fill="{halo[0]}" fill-opacity="{halo[1]}" filter="url(#halo)"/>'
        )
        wide = spec.get("halo_wide")
        if wide and not small:
            defs.append(
                '<filter id="wide" x="-50%" y="-50%" width="200%" height="200%">'
                '<feGaussianBlur stdDeviation="10"/></filter>'
            )
            body.append(
                f'<path d="{glyph}" fill="{wide[0]}" fill-opacity="{wide[1]}" filter="url(#wide)"/>'
            )
    shadow = spec.get("shadow")
    if shadow and not small:
        # A soft shadow without SVG filters: Qt's tray renderer rejects
        # them in the shipped icon (see `util.rs`). Each layer sits lower,
        # spreads wider and fades, which reads as a blur.
        layers = "".join(
            f'<path d="{glyph}" fill="{shadow[0]}" stroke="{shadow[0]}"'
            f' stroke-width="{spread}" stroke-linejoin="round"'
            f' opacity="{shadow[1] * weight:.3f}" transform="translate(0 {drop})"/>'
            for drop, spread, weight in [
                (1.0, 0.5, 0.30), (2.0, 2.0, 0.22), (3.0, 3.6, 0.15),
                (4.0, 5.4, 0.10), (5.0, 7.4, 0.06),
            ]
        )
        body.append(f"<g>{layers}</g>")
    elif shadow and small:
        body.append(
            f'<path d="{glyph}" fill="{shadow[0]}" fill-opacity="{shadow[1] * 0.6:.2f}"'
            f' transform="translate(0 2)"/>'
        )

    body.append(f'<path d="{glyph}" fill="url(#mark)"/>')
    if not small:
        # The mark's glass edge: a bright lip along its top, clipped inside.
        defs.append(f'<clipPath id="markclip"><path d="{glyph}"/></clipPath>')
        defs.append(
            linear("lip", [(0, "#FFFFFF", spec.get("lip", 0.9)), (0.18, "#FFFFFF", 0), (0.82, "#000000", 0), (1, "#000000", 0.14)])
        )
        body.append(
            f'<path d="{glyph}" fill="none" stroke="url(#lip)" stroke-width="2.4"'
            f' clip-path="url(#markclip)"/>'
        )
        # The tile's rim: lit along the top, a faint return along the bottom.
        rim = spec.get("rim", 0.3)
        defs.append(
            linear(
                "rim",
                [(0, "#FFFFFF", rim), (0.35, "#FFFFFF", rim * 0.15),
                 (0.7, "#FFFFFF", 0), (1, "#FFFFFF", rim * 0.35)],
            )
        )
        body.append(
            f'<path d="{TILE}" fill="none" stroke="url(#rim)" stroke-width="2.4"'
            f' clip-path="url(#clip)"/>'
        )
    edge = spec.get("edge")
    if edge:
        # Light tiles need an outline to hold their shape on a light desktop.
        body.append(
            f'<path d="{TILE}" fill="none" stroke="{edge[0]}" stroke-opacity="{edge[1]}"'
            f' stroke-width="{2 if small else 1.4}" clip-path="url(#clip)"/>'
        )

    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128">\n'
        f"<defs>{''.join(defs)}</defs>\n" + "\n".join(body) + "\n</svg>\n"
    )


def paths(name):
    if name == "zeus":
        return ROOT / "packaging/icons/zeus.svg", ROOT / "packaging/icons/zeus-small.svg"
    return ROOT / f"assets/app-icons/{name}.svg", ROOT / f"assets/app-icons/small/{name}.svg"


def render(svg, size):
    import io

    import resvg_py
    from PIL import Image

    png = bytes(resvg_py.svg_to_bytes(svg_string=svg, width=size, height=size))
    return Image.open(io.BytesIO(png)).convert("RGBA")


def write_pngs():
    from PIL import Image, ImageFilter

    full = icon_svg("zeus", ICONS["zeus"], small=False)
    small = icon_svg("zeus", ICONS["zeus"], small=True)
    render(full, 1024).save(ROOT / "packaging/icons/zeus-1024.png", optimize=True)

    # Apple's grid: an 824 tile centred on 1024, over a soft drop shadow.
    def on_grid(canvas, tile):
        art = render(full, tile)
        out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
        shadow = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
        offset = (canvas - tile) // 2
        alpha = art.split()[3].point(lambda value: int(value * 0.45))
        black = Image.new("RGBA", art.size, (0, 0, 0, 255))
        black.putalpha(alpha)
        shadow.paste(black, (offset, offset + canvas // 80), black)
        shadow = shadow.filter(ImageFilter.GaussianBlur(canvas / 70))
        out.alpha_composite(shadow)
        out.alpha_composite(art, (offset, offset))
        return out

    on_grid(1024, 824).save(ROOT / "packaging/macos/zeus-1024.png", optimize=True)
    on_grid(512, 412).save(ROOT / "packaging/macos/icon-dock.png", optimize=True)

    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [render(small if size < 40 else full, size) for size in sizes]
    images[-1].save(
        ROOT / "packaging/windows/zeus.ico",
        format="ICO",
        sizes=[(size, size) for size in sizes],
        append_images=images[:-1],
    )


def main():
    (ROOT / "assets/app-icons/small").mkdir(parents=True, exist_ok=True)
    for name, spec in ICONS.items():
        full, small = paths(name)
        full.write_text(icon_svg(name, spec, small=False))
        small.write_text(icon_svg(name, spec, small=True))
    if "--png" in sys.argv:
        write_pngs()


if __name__ == "__main__":
    main()
