#!/usr/bin/env python3
"""Builds assets/doodles/<style>.svg from Lucide icons (ISC, https://lucide.dev).

Every tile reuses the placements (position, rotation, scale) of the classic
tile in assets/wallpaper.svg, swapping its icons for a themed set, so all the
styles tile at 374 x 666 the same way. Needs network access to fetch icons.
"""
import re, sys, urllib.request

RAW = "https://raw.githubusercontent.com/lucide-icons/lucide/main/icons/{}.svg"
THEMES = {
    "nature": "tree-pine leaf flower flower-2 sprout cloud-sun sun moon cloud-rain rainbow mountain waves bird bug fish snowflake wind droplets shell squirrel rabbit cat dog turtle tent trees clover feather palmtree",
    "space": "rocket star moon sun orbit satellite satellite-dish telescope sparkles sparkle atom zap earth cloud-moon meteor-off ufo planet comet radar compass globe crown gem flame",
    "food": "coffee pizza cake cake-slice cookie ice-cream-cone ice-cream-bowl apple banana cherry grape citrus carrot croissant egg-fried beer wine martini soup sandwich salad popcorn candy lollipop utensils chef-hat milk drumstick",
    "travel": "plane car bus train-front bike ship sailboat tent map map-pin compass luggage camera backpack umbrella tickets hotel mountain-snow palmtree anchor fuel signpost ferris-wheel castle landmark",
    "play": "music guitar piano headphones mic gamepad-2 dices puzzle trophy medal drama clapperboard film popcorn palette brush paintbrush pen-tool dumbbell volleyball swords drum radio tv bike joystick ghost smile party-popper",
}
cache = {}

def icon(name):
    if name not in cache:
        try:
            svg = urllib.request.urlopen(RAW.format(name), timeout=20).read().decode()
            cache[name] = re.sub(r"\s+", " ", svg[svg.index(">") + 1 : svg.rindex("</svg>")]).strip()
        except Exception:
            print("skip", name, file=sys.stderr)
            cache[name] = None
    return cache[name]

classic = open("assets/wallpaper.svg").read()
ids = list(dict.fromkeys(re.findall(r'xlink:href="#(\w+)"', classic)))
body = classic[classic.index("</defs>") :]
for style, names in THEMES.items():
    icons = [i for i in map(icon, names.split()) if i]
    if len(icons) < 12:
        sys.exit(f"{style}: too few icons")
    defs = "".join(f'<g id="{i}">{icons[n % len(icons)]}</g>\n' for n, i in enumerate(ids))
    head = classic[: classic.index("<defs>")].replace(
        "Doodles built from Lucide icons", f"{style.title()} doodles built from Lucide icons")
    open(f"assets/doodles/{style}.svg", "w").write(head + "<defs>\n" + defs + body)
    print(style, len(icons), "icons")
