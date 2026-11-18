# Doodle tiles

`nature.svg`, `space.svg`, `food.svg`, `travel.svg` and `play.svg` (and the
classic `../wallpaper.svg`) are 374 x 666 tiles drawn from [Lucide](https://lucide.dev)
icons, licensed ISC, Copyright (c) Lucide Icons and Contributors. The licence text
is in `../icons/LICENSE.txt`.

Regenerate them with `python3 scripts/generate-doodles.py`, which lays out one
packed placement for every style (icons largest first, small marks in the
holes) and keeps each tile's icons; `--fetch` fetches the `THEMES` icons again.
