#!/usr/bin/env bash
# One run of the Zeus benchmark on macOS or Linux: launches a demo build on
# the two-minute benchmark scenario, measures its memory, CPU and GPU use from
# outside every 2 seconds and at the end of each phase, and writes one JSON
# file with those and the app's own frame times.
#
#   scripts/bench/sample.sh --binary PATH --out FILE.json [--label NAME]
#                           [--run N] [--headless] [--scale N]
#
# Usually started by run.sh, which runs it several times. The app runs the
# synthetic demo only: offline sample chats in a fresh temporary folder,
# never a linked account, archive or contacts.
set -u

here="$(cd "$(dirname "$0")" && pwd)"
. "$here/lib.sh"

binary="" out="" label="" run=1 headless=0 scale=""
while [ $# -gt 0 ]; do
  case "$1" in
    --binary) binary="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --label) label="$2"; shift 2 ;;
    --run) run="$2"; shift 2 ;;
    --headless) headless=1; shift ;;
    --scale) scale="$2"; shift 2 ;;
    *) echo "sample.sh: unknown option $1" >&2; exit 2 ;;
  esac
done
if [ -z "$binary" ] || [ -z "$out" ]; then
  echo "usage: sample.sh --binary PATH --out FILE.json [--label NAME] [--run N] [--headless] [--scale N]" >&2
  exit 2
fi
if [ ! -f "$binary" ]; then
  echo "sample.sh: no build at $binary" >&2
  exit 2
fi

os="$(uname -s)"
case "$os" in
  Darwin | Linux) ;;
  *) echo "sample.sh: macOS and Linux only; on Windows use run.ps1" >&2; exit 2 ;;
esac

# Only a demo build can run the scenario; anything else is refused before it
# starts, so the benchmark can never open a linked account.
if ! "$binary" --help 2>/dev/null | grep -q -- "--demo-tour-marks"; then
  echo "sample.sh: $binary is not a benchmark build (built without --features demo," >&2
  echo "or from a commit before the benchmark). Nothing was run." >&2
  exit 3
fi

# What was built, from the BUILD_INFO.txt that bench builds carry.
info_file="$(dirname "$binary")/BUILD_INFO.txt"
info() {
  [ -f "$info_file" ] && awk -F= -v key="$1" '$1 == key { sub(/^[^=]*=/, ""); print; exit }' "$info_file"
}
[ -z "$label" ] && label="$(info label)"
if [ -z "$label" ]; then
  echo "sample.sh: name this build with --label (no BUILD_INFO.txt beside it)" >&2
  exit 2
fi
commit="$(info commit)"
if [ -z "$commit" ]; then
  # A local build: the checkout's commit, when the build sits inside it.
  repo="$(git -C "$here" rev-parse --show-toplevel 2>/dev/null || true)"
  case "$(cd "$(dirname "$binary")" && pwd)" in
    "$repo"/*)
      commit="$(git -C "$repo" rev-parse HEAD)"
      git -C "$repo" diff --quiet HEAD -- src Cargo.toml Cargo.lock || commit="$commit+changes"
      ;;
    *) commit="unknown" ;;
  esac
fi
changes="$(info changes)"

work="$(mktemp -d "${TMPDIR:-/tmp}/zeus-bench.XXXXXX")"
marks="$work/marks.jsonl"
log="$work/app.log"
samples="$work/samples.tsv"
: >"$samples"
: >"$marks"
extra_pids=""
cleanup() {
  [ -n "${pid:-}" ] && kill "$pid" 2>/dev/null
  for extra in $extra_pids; do kill "$extra" 2>/dev/null; done
  rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# Linux: on X11 the window can be minimized and restored (Wayland has no
# restore), so XWayland it is; --headless draws on a virtual screen instead.
display_note=""
if [ "$os" = Linux ]; then
  unset WAYLAND_DISPLAY
  if [ "$headless" = 1 ]; then
    number=91
    while [ -e "/tmp/.X11-unix/X$number" ] || [ -e "/tmp/.X$number-lock" ]; do number=$((number + 1)); done
    Xvfb ":$number" -screen 0 3000x2000x24 -nolisten tcp >/dev/null 2>&1 &
    extra_pids="$extra_pids $!"
    export DISPLAY=":$number"
    sleep 1
    # A window manager, so the window can be minimized.
    for wm in openbox fluxbox twm; do
      if command -v "$wm" >/dev/null; then
        "$wm" >/dev/null 2>&1 &
        extra_pids="$extra_pids $!"
        sleep 1
        break
      fi
    done
    display_note="Xvfb (software rendering)"
  elif [ -z "${DISPLAY:-}" ]; then
    echo "sample.sh: no X display; use --headless (needs Xvfb)" >&2
    exit 2
  fi
  [ -n "$scale" ] && export WINIT_X11_SCALE_FACTOR="$scale"
fi

started=$(date +%s)
started_ms=$(bench_now_ms)
"$binary" --demo-tour --demo-tour-script bench --demo-size 1280x800 \
  --demo-tour-marks "$marks" >"$log" 2>&1 &
pid=$!
# Keep the Mac's display and the machine awake for the run.
if [ "$os" = Darwin ] && command -v caffeinate >/dev/null; then
  caffeinate -d -i -w "$pid" &
fi

[ "$os" = Linux ] && ticks="$(getconf CLK_TCK 2>/dev/null || echo 100)"

# Prints the main reading, another one and the operating system's own peak,
# in KiB; the CPU time used so far, in milliseconds; and the GPU reading:
# nanoseconds of 3D engine time so far on Linux, percent on macOS, or -.
measure() {
  if [ "$os" = Linux ]; then
    local readings peak cpu gpu
    readings="$(bench_parse_smaps_rollup </proc/"$pid"/smaps_rollup 2>/dev/null)" || return 1
    peak="$(bench_parse_status_peak </proc/"$pid"/status 2>/dev/null)"
    cpu="$(bench_parse_proc_stat_cpu </proc/"$pid"/stat 2>/dev/null)"
    gpu="$(cat /proc/"$pid"/fdinfo/* 2>/dev/null | bench_parse_drm_fdinfo)"
    set -- $readings
    echo "$1 $2 ${peak:-0} $(( ${cpu:-0} * 1000 / ticks )) ${gpu:--}"
  else
    local readings rss cpu gpu
    readings="$(vmmap -summary "$pid" 2>/dev/null | bench_parse_vmmap_summary)"
    case "$readings" in
      "0 "*|"") readings="$(footprint -p "$pid" 2>/dev/null | bench_parse_footprint)" ;;
    esac
    rss="$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ')"
    cpu="$(ps -o time= -p "$pid" 2>/dev/null | bench_parse_ps_time)"
    gpu="$(ioreg -r -d 1 -w 0 -c IOAccelerator 2>/dev/null | bench_parse_ioreg_gpu)"
    set -- $readings
    [ "${1:-0}" -gt 0 ] || return 1
    echo "$1 ${rss:-0} ${2:-0} ${cpu:-0} ${gpu:--}"
  fi
}

os_peak=0 last_gpu_ns="" last_gpu_ms=0
# Records a reading: phase, kind (tick or end).
record() {
  local now_ms readings gpu
  now_ms=$(($(bench_now_ms) - started_ms))
  readings="$(measure)" || return
  set -- "$1" "$2" $readings
  gpu="$7"
  if [ "$os" = Linux ] && [ "$gpu" != "-" ]; then
    # Engine time over the time since the last reading, in percent.
    if [ -n "$last_gpu_ns" ] && [ "$now_ms" -gt "$last_gpu_ms" ]; then
      gpu="$(awk -v ns="$(($7 - last_gpu_ns))" -v ms="$((now_ms - last_gpu_ms))" 'BEGIN { printf "%.1f", ns / 1e4 / ms }')"
    else
      gpu="-"
    fi
    last_gpu_ns="$7" last_gpu_ms="$now_ms"
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$now_ms" "$3" "$4" "$6" "$gpu" >>"$samples"
  [ "$5" -gt "$os_peak" ] && os_peak="$5"
  printf '  %-13s %-4s %5.1fs  %7s MB  CPU %s ms\n' "$1" "$2" "$(awk -v ms="$now_ms" 'BEGIN { print ms / 1000 }')" "$(bench_mb "$3")" "$6"
}

phase="" seen=0 last_tick=0 status="running" reason="" checks="" frames="" app_start=""
echo "Run $run of $label: about 2 minutes. Leave the window alone until it closes."
while :; do
  if ! kill -0 "$pid" 2>/dev/null; then
    [ "$status" = running ] && { status="failed"; reason="the app quit before the scenario ended"; }
    break
  fi
  now=$(date +%s)
  if [ $((now - started)) -gt "$BENCH_TIMEOUT" ]; then
    status="failed"; reason="timed out after ${BENCH_TIMEOUT}s"
    break
  fi
  total=$(wc -l <"$marks" | tr -d ' ')
  if [ "$total" -gt "$seen" ]; then
    lines="$(sed -n "$((seen + 1)),${total}p" "$marks")"
    seen=$total
    while IFS= read -r line; do
      [ -z "$line" ] && continue
      parsed="$(printf '%s\n' "$line" | bench_parse_marks)"
      event="$(printf '%s' "$parsed" | cut -f1)"
      case "$event" in
        start) app_start="$line" ;;
        phase)
          [ -n "$phase" ] && record "$phase" end
          phase="$(printf '%s' "$parsed" | cut -f2)"
          last_tick=$(bench_now_ms)
          ;;
        check)
          checks="$checks${checks:+, }$line"
          [ "$(printf '%s' "$parsed" | cut -f3)" = true ] || echo "  check failed: $(printf '%s' "$parsed" | cut -f2)"
          ;;
        frames) frames="$frames${frames:+, }\"$(printf '%s' "$parsed" | cut -f2)\": $line" ;;
        end)
          [ -n "$phase" ] && record "$phase" end
          phase=""
          status="complete"
          ;;
        failed)
          status="failed"; reason="the scenario could not find something on screen; see the log"
          ;;
      esac
    done <<EOF
$lines
EOF
    [ "$status" != running ] && break
  fi
  if [ -n "$phase" ] && [ $(($(bench_now_ms) - last_tick)) -ge $((BENCH_INTERVAL * 1000)) ]; then
    last_tick=$(bench_now_ms)
    record "$phase" tick
  fi
  sleep 0.5
done

# Which renderer drew it, from the libraries it loaded; and then let it go.
renderer="unknown"
if kill -0 "$pid" 2>/dev/null; then
  if [ "$os" = Linux ]; then
    renderer="$(bench_detect_renderer </proc/"$pid"/maps)"
  else
    renderer="$(vmmap "$pid" 2>/dev/null | bench_detect_renderer)"
  fi
  # The app closes itself a few seconds after its last mark.
  for _ in $(seq 1 30); do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
  kill "$pid" 2>/dev/null
fi
wait "$pid" 2>/dev/null
pid=""

if [ "$status" != complete ]; then
  echo "Run $run failed: $reason. The app's last lines:" >&2
  tail -n 15 "$log" >&2
fi

# The machine.
if [ "$os" = Linux ]; then
  os_name="$(. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME") (Linux $(uname -r))"
  chip="$(awk -F': ' '/^model name/ { print $2; exit }' /proc/cpuinfo)"
  ram_kib="$(awk '/^MemTotal:/ { print $2 }' /proc/meminfo)"
  ram_gb="$(awk -v k="$ram_kib" 'BEGIN { printf "%.1f", k * 1024 / 1e9 }')"
  model="$(cat /sys/class/dmi/id/product_name 2>/dev/null || echo unknown)"
  gpu="$(command -v lspci >/dev/null && lspci 2>/dev/null | awk -F': ' '/VGA|3D/ { print $2; exit }')"
  [ -n "$display_note" ] && gpu="$display_note"
  metric="pss" other="rss" peak_metric="peak_rss"
  cores="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 0)"
  gpu_metric="process_3d_engine"
else
  os_name="macOS $(sw_vers -productVersion) ($(uname -m))"
  chip="$(sysctl -n machdep.cpu.brand_string 2>/dev/null)"
  ram_gb="$(awk -v b="$(sysctl -n hw.memsize)" 'BEGIN { printf "%.1f", b / 1e9 }')"
  model="$(sysctl -n hw.model 2>/dev/null)"
  gpu="$chip"
  metric="physical_footprint" other="rss" peak_metric="peak_physical_footprint"
  cores="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 0)"
  gpu_metric="whole_gpu"
fi

mkdir -p "$(dirname "$out")"
{
  printf '{\n'
  printf '  "schema": 2,\n'
  printf '  "label": "%s",\n' "$(bench_json_string "$label")"
  printf '  "run": %s,\n' "$run"
  printf '  "status": "%s",\n' "$status"
  printf '  "reason": "%s",\n' "$(bench_json_string "$reason")"
  printf '  "commit": "%s",\n' "$(bench_json_string "$commit")"
  printf '  "changes": "%s",\n' "$(bench_json_string "$changes")"
  printf '  "started": "%s",\n' "$(date -u -r "$started" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d "@$started" +%Y-%m-%dT%H:%M:%SZ)"
  printf '  "os": "%s",\n' "$(bench_json_string "$os_name")"
  printf '  "hardware": {"model": "%s", "chip": "%s", "cores": %s, "ram_gb": %s, "gpu": "%s"},\n' \
    "$(bench_json_string "$model")" "$(bench_json_string "$chip")" "${cores:-0}" "${ram_gb:-0}" "$(bench_json_string "${gpu:-unknown}")"
  printf '  "renderer": "%s",\n' "$(bench_json_string "$renderer")"
  printf '  "app": %s,\n' "${app_start:-null}"
  printf '  "metric": "%s",\n' "$metric"
  printf '  "metric_other": "%s",\n' "$other"
  printf '  "gpu_metric": "%s",\n' "$gpu_metric"
  printf '  "phases": %s,\n' "$(bench_phases_json <"$samples")"
  printf '  "peak_mb": %s,\n' "$(bench_peak_mb <"$samples")"
  printf '  "os_peak_mb": %s,\n' "$(bench_mb "$os_peak")"
  printf '  "os_peak_metric": "%s",\n' "$peak_metric"
  printf '  "settled_mb": %s,\n' "$(bench_end_mb settle-again <"$samples")"
  printf '  "frames": {%s},\n' "$frames"
  printf '  "checks": [%s]\n' "$checks"
  printf '}\n'
} >"$out"
echo "Wrote $out"
[ "$status" = complete ]
