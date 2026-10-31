#!/usr/bin/env bash
# Runs the Zeus benchmark a few times on macOS or Linux and writes
# one JSON file per run under bench-results/<label>/.
#
#   scripts/bench/run.sh [BINARY] [--label NAME] [--runs N] [--out DIR]
#                        [--headless] [--scale N]
#
# BINARY defaults to the `zeus` beside this folder, as in a bench download.
# The label defaults to the one in BUILD_INFO.txt beside the binary.
# Each run takes about 2 minutes; the default two, with the pause between
# them, about 5.
set -u

here="$(cd "$(dirname "$0")" && pwd)"
binary="" label="" runs=2 out="" pass=()
while [ $# -gt 0 ]; do
  case "$1" in
    --label) label="$2"; shift 2 ;;
    --runs) runs="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --headless) pass+=(--headless); shift ;;
    --scale) pass+=(--scale "$2"); shift 2 ;;
    -h | --help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "run.sh: unknown option $1" >&2; exit 2 ;;
    *) binary="$1"; shift ;;
  esac
done

if [ -z "$binary" ]; then
  for candidate in "$here/../zeus" "$here/zeus"; do
    [ -f "$candidate" ] && binary="$candidate" && break
  done
fi
if [ -z "$binary" ] || [ ! -f "$binary" ]; then
  echo "run.sh: say which build to run: scripts/bench/run.sh path/to/zeus" >&2
  exit 2
fi
binary="$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")"
chmod +x "$binary" 2>/dev/null
# A build downloaded in a browser is quarantined, and macOS would refuse it.
[ "$(uname -s)" = Darwin ] && xattr -d com.apple.quarantine "$binary" 2>/dev/null

if [ -z "$label" ] && [ -f "$(dirname "$binary")/BUILD_INFO.txt" ]; then
  label="$(awk -F= '$1 == "label" { sub(/^[^=]*=/, ""); print; exit }' "$(dirname "$binary")/BUILD_INFO.txt")"
fi
if [ -z "$label" ]; then
  echo "run.sh: name this build, for example --label main" >&2
  exit 2
fi
case "$label" in
  *[!A-Za-z0-9._+-]*) echo "run.sh: use letters, digits and . _ + - in the label" >&2; exit 2 ;;
esac
[ -z "$out" ] && out="$PWD/bench-results"
mkdir -p "$out/$label"

failed=0
for run in $(seq 1 "$runs"); do
  file="$out/$label/run-$(printf '%02d' "$run").json"
  if ! "$here/sample.sh" --binary "$binary" --label "$label" --run "$run" --out "$file" ${pass[@]+"${pass[@]}"}; then
    status=$?
    # A build that cannot run the benchmark fails the same way every time.
    [ "$status" -ge 2 ] && [ "$status" -le 3 ] && [ ! -f "$file" ] && exit "$status"
    failed=$((failed + 1))
  fi
  # Let the machine settle between runs.
  [ "$run" -lt "$runs" ] && sleep 5
done

echo
echo "Done: $runs runs of $label in $out/$label ($failed failed)."
echo "Send back that folder (zip it). To compare builds:"
echo "  python3 scripts/bench/compare.py $out/*"
[ "$failed" -eq 0 ]
