#!/usr/bin/env python3
"""Lays out the doodle tiles: assets/wallpaper.svg (classic) and
assets/doodles/<style>.svg, all 374 x 666, from Lucide icons (ISC,
https://lucide.dev).

Every style shares one packed layout, as dense as WhatsApp's own canvas:
icons of mixed sizes placed largest first so smaller ones fill the gaps,
then small marks (dots, rings, plus signs, sparkles) in the holes left. The
tile wraps, so an item crossing an edge is placed again a tile away, and
every item is a `<use>` line, so the app's thinning (`wallpaper::thinned`)
can drop any of them.

Each style keeps the icons already in its file. `--fetch` fetches the
themed sets named in `THEMES` again instead (needs network access).
"""
import math, random, re, sys, urllib.request

W, H = 374, 666
LINE = 1.15  # stroke width as drawn, whatever an icon's scale
SEED = 20261118

RAW = "https://raw.githubusercontent.com/lucide-icons/lucide/main/icons/{}.svg"
THEMES = {
    "nature": "tree-pine leaf flower flower-2 sprout cloud-sun sun moon cloud-rain rainbow mountain waves bird bug fish snowflake wind droplets shell squirrel rabbit cat dog turtle tent trees clover feather palmtree",
    "space": "rocket star moon sun orbit satellite satellite-dish telescope sparkles sparkle atom zap earth cloud-moon meteor-off ufo planet comet radar compass globe crown gem flame",
    "food": "coffee pizza cake cake-slice cookie ice-cream-cone ice-cream-bowl apple banana cherry grape citrus carrot croissant egg-fried beer wine martini soup sandwich salad popcorn candy lollipop utensils chef-hat milk drumstick",
    "travel": "plane car bus train-front bike ship sailboat tent map map-pin compass luggage camera backpack umbrella tickets hotel mountain-snow palmtree anchor fuel signpost ferris-wheel castle landmark",
    "play": "music guitar piano headphones mic gamepad-2 dices puzzle trophy medal drama clapperboard film popcorn palette brush paintbrush pen-tool dumbbell volleyball swords drum radio tv bike joystick ghost smile party-popper",
}

# Small marks for the holes, in the icons' 24-unit box around (12, 12).
MARKS = {
    "m0": '<circle cx="12" cy="12" r="0.9"/>',
    "m1": '<circle cx="12" cy="12" r="3"/>',
    "m2": '<path d="M12 8.6v6.8M8.6 12h6.8"/>',
    "m3": '<path d="M12 8.5c.5 2.4 1.1 3 3.5 3.5-2.4.5-3 1.1-3.5 3.5-.5-2.4-1.1-3-3.5-3.5 2.4-.5 3-1.1 3.5-3.5z"/>',
    "m4": '<path d="M9.6 9.6l4.8 4.8M14.4 9.6l-4.8 4.8"/>',
}
# How far each mark reaches from its centre at scale 1, for spacing.
MARK_REACH = {"m0": 1.6, "m1": 3.6, "m2": 4.0, "m3": 4.0, "m4": 3.8}


def fetch(name, cache={}):
    if name not in cache:
        try:
            svg = urllib.request.urlopen(RAW.format(name), timeout=20).read().decode()
            cache[name] = re.sub(r"\s+", " ", svg[svg.index(">") + 1 : svg.rindex("</svg>")]).strip()
        except Exception:
            print("skip", name, file=sys.stderr)
            cache[name] = None
    return cache[name]


def defs_of(svg):
    """The icon definitions of a tile, in order, as (id, inner markup)."""
    block = svg[svg.index("<defs>") + 6 : svg.index("</defs>")]
    return [(m[0], m[1]) for m in re.findall(r'<g id="(i\d+)">(.*?)</g>\n', block)]


def gap(a, b):
    dx, dy = abs(a[0] - b[0]), abs(a[1] - b[1])
    return math.hypot(min(dx, W - dx), min(dy, H - dy))


def fits(items, x, y, r, space):
    return all(gap((x, y), (ix, iy)) >= r + ir + space for ix, iy, ir, *_ in items)


def layout():
    """Where things go: icon slots (href None) and marks, as
    (x, y, reach, href, turn, scale)."""
    rng = random.Random(SEED)
    items = []
    # Icons, largest first: each try is a little smaller than the last, so
    # the large ones spread out and the small ones settle between them.
    tries = 40000
    for n in range(tries):
        top = 1.85 - 0.75 * n / tries
        scale = round(rng.uniform(top - 0.15, top), 2)
        reach = 10.0 * scale  # most of an icon sits within 10 of its 12
        x, y = rng.uniform(0, W), rng.uniform(0, H)
        if fits(items, x, y, reach, 4.0):
            items.append((x, y, reach, None, round(rng.uniform(-30, 30), 2), scale))
    # Marks in the holes left.
    names = ["m0", "m1", "m1", "m2", "m3", "m4"]
    for _ in range(30000):
        name = rng.choice(names)
        scale = round(rng.uniform(0.85, 1.25), 2)
        reach = MARK_REACH[name] * scale
        x, y = rng.uniform(0, W), rng.uniform(0, H)
        if fits(items, x, y, reach, 7.0):
            items.append((x, y, reach, name, round(rng.uniform(-30, 30), 2), scale))
    return items


def assign(slots, defs):
    """Fills the icon slots with `defs`, each time the picture used least so
    far among those not drawn near it, so a picture never sits beside itself
    (a themed set repeats pictures under several ids)."""
    rng = random.Random(SEED + 1)
    pictures = {}
    for k, inner in defs:
        pictures.setdefault(inner, []).append(k)
    uses = {inner: 0 for inner in pictures}
    items, placed = [], []  # placed: (x, y, picture)
    for x, y, reach, href, turn, scale in slots:
        if href is None:
            near = {p for px, py, p in placed if gap((x, y), (px, py)) < 110}
            free = [p for p in pictures if p not in near] or list(pictures)
            least = min(uses[p] for p in free)
            picture = rng.choice([p for p in free if uses[p] == least])
            uses[picture] += 1
            href = pictures[picture][uses[picture] % len(pictures[picture])]
            placed.append((x, y, picture))
        items.append((x, y, reach, href, turn, scale))
    # Each item's key in the app's thinning is its href, turn and size; make
    # them unique so no two items read as copies of one.
    seen = set()
    for i, (x, y, reach, href, turn, scale) in enumerate(items):
        while (href, turn, scale) in seen:
            turn = round(turn + 0.01, 2)
        seen.add((href, turn, scale))
        items[i] = (x, y, reach, href, turn, scale)
    return items


def body(items):
    lines = []
    for x, y, reach, href, turn, scale in items:
        width = round(LINE / scale, 2)
        for dx in (0, -W, W):
            for dy in (0, -H, H):
                px, py = x + dx, y + dy
                if (dx or dy) and not (-reach * 1.2 < px < W + reach * 1.2 and -reach * 1.2 < py < H + reach * 1.2):
                    continue
                lines.append(
                    f'<use xlink:href="#{href}" stroke-width="{width}" '
                    f'transform="translate({round(px, 2)} {round(py, 2)}) rotate({turn}) '
                    f'scale({scale}) translate(-12 -12)"/>'
                )
    return "\n".join(lines)


def write(path, head, defs, items):
    marks = "".join(f'<g id="{k}">{v}</g>\n' for k, v in MARKS.items())
    icons = "".join(f'<g id="{k}">{v}</g>\n' for k, v in defs)
    svg = (
        head
        + "<defs>\n" + icons + marks + "</defs>\n"
        + f'<g stroke="currentColor" stroke-width="{LINE}" stroke-linecap="round" stroke-linejoin="round">\n'
        + body(items)
        + "\n</g></svg>\n"
    )
    open(path, "w").write(svg)


classic = open("assets/wallpaper.svg").read()
classic_defs = defs_of(classic)
slots = layout()
icons = sum(1 for slot in slots if slot[3] is None)
print(f"{icons} icons, {len(slots) - icons} marks")

write("assets/wallpaper.svg", classic[: classic.index("<defs>")], classic_defs, assign(slots, classic_defs))
for style, names in THEMES.items():
    path = f"assets/doodles/{style}.svg"
    old = open(path).read()
    defs = defs_of(old)
    if "--fetch" in sys.argv:
        fetched = [i for i in map(fetch, names.split()) if i]
        if len(fetched) < 12:
            sys.exit(f"{style}: too few icons")
        defs = [(k, fetched[n % len(fetched)]) for n, (k, _) in enumerate(classic_defs)]
    if {k for k, _ in defs} != {k for k, _ in classic_defs}:
        sys.exit(f"{style}: its icons are not the classic tile's ids")
    write(path, old[: old.index("<defs>")], defs, assign(slots, defs))
    print(style, "written")
