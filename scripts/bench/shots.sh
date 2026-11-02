#!/usr/bin/env bash
# Captures the same demo pages from one build, for the visual check that goes
# with the benchmark (macOS and Linux; Windows has shots.ps1).
#
#   scripts/bench/shots.sh [BINARY or --binary PATH] [--label NAME] [--out DIR] [--headless] [--scale N]
#
# Writes bench-shots/<label>/{chat,light,pinned,narrow,calls}.png. The clock
# is frozen for screenshots, so two builds on the same machine should match:
#   python3 scripts/bench/imgdiff.py bench-shots/main bench-shots/pr66
set -u

here="$(cd "$(dirname "$0")" && pwd)"
binary="" label="" out="" headless=0 scale=""
while [ $# -gt 0 ]; do
  case "$1" in
    --binary) binary="$2"; shift 2 ;;
    --label) label="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --headless) headless=1; shift ;;
    --scale) scale="$2"; shift 2 ;;
    -*) echo "shots.sh: unknown option $1" >&2; exit 2 ;;
    *) binary="$1"; shift ;;
  esac
done
if [ -z "$binary" ]; then
  for candidate in "$here/../zeus" "$here/zeus"; do
    [ -f "$candidate" ] && binary="$candidate" && break
  done
fi
if [ -z "$binary" ] || [ ! -f "$binary" ]; then
  echo "shots.sh: say which build to use: scripts/bench/shots.sh path/to/zeus" >&2
  exit 2
fi
chmod +x "$binary" 2>/dev/null
[ "$(uname -s)" = Darwin ] && xattr -d com.apple.quarantine "$binary" 2>/dev/null
if ! "$binary" --help 2>/dev/null | grep -q -- "--demo-shot"; then
  echo "shots.sh: $binary is not a demo build (built without --features demo)" >&2
  exit 3
fi
info_file="$(dirname "$binary")/BUILD_INFO.txt"
if [ -z "$label" ] && [ -f "$info_file" ]; then
  label="$(awk -F= '$1 == "label" { sub(/^[^=]*=/, ""); print; exit }' "$info_file")"
fi
[ -z "$label" ] && { echo "shots.sh: name this build, for example --label main" >&2; exit 2; }
[ -z "$out" ] && out="$PWD/bench-shots"
mkdir -p "$out/$label"

extra_pids=""
trap 'for p in $extra_pids; do kill "$p" 2>/dev/null; done' EXIT
if [ "$(uname -s)" = Linux ]; then
  unset WAYLAND_DISPLAY
  if [ "$headless" = 1 ]; then
    number=91
    while [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ]; do number=$((number + 1)); done
    Xvfb ":$number" -screen 0 3000x2000x24 -nolisten tcp >/dev/null 2>&1 &
    extra_pids="$!"
    export DISPLAY=":$number"
    sleep 1
  fi
  [ -n "$scale" ] && export WINIT_X11_SCALE_FACTOR="$scale"
fi

# name, pages, window size in points.
while read -r name pages size; do
  [ -z "$name" ] && continue
  "$binary" --demo-shot "$out/$label/$name.png" --demo-page "$pages" \
    --demo-size "$size" >/dev/null 2>&1
  if [ -f "$out/$label/$name.png" ]; then
    echo "  $name"
  else
    echo "  $name: no picture (see: $binary --demo-shot x.png --demo-page $pages)" >&2
  fi
done <<'EOF'
chat chat 1280x800
light chat,light 1280x800
pinned pinned 1280x800
narrow chat 600x800
calls calls 1280x800
EOF
echo "Wrote $out/$label"
