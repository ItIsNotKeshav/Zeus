#!/usr/bin/env bash
# Tests for lib.sh, the macOS and Linux sampler's parsers, on captured text.
#   bash scripts/bench/test-lib.sh
set -u
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/lib.sh"

failures=0
expect() {
  if [ "$2" != "$3" ]; then
    echo "FAIL: $1: expected [$3], got [$2]"
    failures=$((failures + 1))
  fi
}

smaps='55d0c0a00000-7ffd8a5f2000 ---p 00000000 00:00 0                      [rollup]
Rss:              215784 kB
Pss:              185483 kB
Pss_Dirty:        101304 kB
Pss_Anon:          93280 kB
Shared_Clean:      51000 kB
Private_Clean:     20000 kB
Private_Dirty:    101304 kB
Swap:                  0 kB'
expect "smaps_rollup" "$(printf '%s\n' "$smaps" | bench_parse_smaps_rollup)" "185483 215784 121304"

status='Name:	zeus
VmPeak:	 1874076 kB
VmHWM:	  560907 kB
VmRSS:	  534951 kB'
expect "status peak" "$(printf '%s\n' "$status" | bench_parse_status_peak)" "560907"
expect "status without peak" "$(printf 'Name:\tzeus\n' | bench_parse_status_peak)" "0"

vmmap='Process:         zeus [4242]
Path:            /Users/someone/Downloads/zeus
Load Address:    0x100000000
Physical footprint:         152.3M
Physical footprint (peak):  1.2G
Idle exit:                  untracked
----

ReadOnly portion of Libraries: Total=1.0G resident=500M(48%)'
expect "vmmap summary" "$(printf '%s\n' "$vmmap" | bench_parse_vmmap_summary)" "155955 1258291"
expect "vmmap in kilobytes" "$(printf 'Physical footprint:         980K\n' | bench_parse_vmmap_summary)" "980 0"

footprint='======================================================================
zeus [4242]: 64-bit    Footprint: 150 MB (16384 bytes per page)
======================================================================

  Dirty      Clean  Reclaimable    Regions    Category
    ---        ---          ---        ---    ---
  80 MB        0 B          0 B         12    IOAccelerator (graphics)'
expect "footprint" "$(printf '%s\n' "$footprint" | bench_parse_footprint)" "153600 0"

expect "renderer glow" "$(printf '7f1 r-xp /usr/lib/x86_64-linux-gnu/libGLX_mesa.so.0.0.0\n' | bench_detect_renderer)" "OpenGL (glow)"
expect "renderer vulkan" "$(printf 'libGLX_mesa.so\nlibvulkan.so.1\n' | bench_detect_renderer)" "wgpu (Vulkan)"
expect "renderer metal" "$(printf '__TEXT /System/Library/Frameworks/Metal.framework/Versions/A/Metal\n' | bench_detect_renderer)" "wgpu (Metal)"
expect "renderer mac gl" "$(printf 'Metal.framework/Metal\nOpenGL.framework/Versions/A/OpenGL\n' | bench_detect_renderer)" "OpenGL (glow)"

marks='{"event":"start","pixels_per_point":2.0,"script":"bench","version":"0.35.0","window":[1280.0,800.0]}
{"event":"phase","phase":"settle","t":0.41}
{"event":"check","name":"scroll 1: reached the oldest message","ok":true,"t":196.54}
{"event":"check","name":"minimized: window reports minimized","ok":false,"t":414.1}
{"event":"frames","frames":3,"interval_ms":{"count":3,"p50":16.0},"over_33ms":0,"phase":"scroll","t":350.02}
{"event":"end","t":772.0}'
expected="start
phase	settle
check	scroll 1: reached the oldest message	true
check	minimized: window reports minimized	false
frames	scroll
end"
expect "marks" "$(printf '%s\n' "$marks" | bench_parse_marks)" "$expected"

samples='settle	tick	2000	100000	120000	1500	-
settle	end	10000	180000	200000	3000	10.0
scroll	end	44000	500000	520000	23400	30.0
settle-again	tick	100000	150000	160000	23900	-
settle-again	end	116000	140000	150000	24380	4.0'
phases="$(printf '%s\n' "$samples" | bench_phases_json)"
case "$phases" in
  *'{"name": "settle", "end": {"t": 10.0, "mb": 184.3, "other_mb": 204.8, "cpu_pct": 18.8, "gpu_pct": 10.0}, "max_mb": 184.3, "cpu_pct": 30.0, "gpu_pct": 10.0, "samples": [{"t": 2.0, "mb": 102.4, "other_mb": 122.9, "cpu_pct": 75.0, "gpu_pct": null}, {"t": 10.0, "mb": 184.3, "other_mb": 204.8, "cpu_pct": 18.8, "gpu_pct": 10.0}]}'*) ;;
  *) echo "FAIL: phases json: $phases"; failures=$((failures + 1)) ;;
esac
case "$phases" in
  *'"name": "scroll", "end": {"t": 44.0, "mb": 512.0, "other_mb": 532.5, "cpu_pct": 60.0, "gpu_pct": 30.0}, "max_mb": 512.0, "cpu_pct": 60.0, "gpu_pct": 30.0,'*) ;;
  *) echo "FAIL: scroll phase: $phases"; failures=$((failures + 1)) ;;
esac
case "$phases" in
  *'"name": "settle-again", "end": {"t": 116.0, "mb": 143.4, "other_mb": 153.6, "cpu_pct": 3.0, "gpu_pct": 4.0}, "max_mb": 153.6, "cpu_pct": 1.4, "gpu_pct": 4.0,'*) ;;
  *) echo "FAIL: settle-again phase: $phases"; failures=$((failures + 1)) ;;
esac
case "$phases" in
  *'{"name": "churn", "end": null, "max_mb": null, "cpu_pct": null, "gpu_pct": null, "samples": []}'*) ;;
  *) echo "FAIL: a phase without readings: $phases"; failures=$((failures + 1)) ;;
esac
expect "peak" "$(printf '%s\n' "$samples" | bench_peak_mb)" "512.0"
expect "settled" "$(printf '%s\n' "$samples" | bench_end_mb settle-again)" "143.4"
expect "missing phase" "$(printf '%s\n' "$samples" | bench_end_mb picker)" "null"
expect "mb" "$(bench_mb 1000)" "1.0"
expect "json string" "$(bench_json_string 'Apple "M2" \ chip')" 'Apple \"M2\" \\ chip'

stat='4242 (zeus (demo) x) S 1 4242 4242 0 -1 4194560 52000 0 12 0 1234 567 0 0 20 0 30 0 100 2000000000 130000'
expect "proc stat cpu" "$(printf '%s\n' "$stat" | bench_parse_proc_stat_cpu)" "1801"
expect "ps time" "$(printf '   0:03.21\n' | bench_parse_ps_time)" "3210"
expect "ps time hours" "$(printf '1:02:03.50\n' | bench_parse_ps_time)" "3723500"
expect "ps time days" "$(printf '1-00:00:01\n' | bench_parse_ps_time)" "86401000"

fdinfo='pos:	0
flags:	02100002
mnt_id:	26
pos:	0
flags:	02100002
drm-driver:	amdgpu
drm-client-id:	42
drm-engine-gfx:	1500000 ns
drm-engine-compute:	900 ns
pos:	0
flags:	02100002
drm-driver:	amdgpu
drm-client-id:	42
drm-engine-gfx:	1500000 ns
pos:	0
drm-driver:	i915
drm-client-id:	7
drm-engine-render:	250000 ns
drm-engine-capacity-render:	1'
expect "drm fdinfo" "$(printf '%s\n' "$fdinfo" | bench_parse_drm_fdinfo)" "1750000"
expect "no drm fdinfo" "$(printf 'pos:\t0\nflags:\t02\n' | bench_parse_drm_fdinfo)" ""

ioreg='+-o AGXAcceleratorG14X  <class AGXAcceleratorG14X, id 0x1000000, registered>
    {
      "PerformanceStatistics" = {"In use system memory"=412844032,"Device Utilization %"=37,"Renderer Utilization %"=35,"Tiler Utilization %"=12}
    }'
expect "ioreg gpu" "$(printf '%s\n' "$ioreg" | bench_parse_ioreg_gpu)" "37"
expect "no ioreg gpu" "$(printf 'nothing\n' | bench_parse_ioreg_gpu)" ""
case "$(bench_now_ms)" in
  [1-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]) ;;
  *) echo "FAIL: now in milliseconds: $(bench_now_ms)"; failures=$((failures + 1)) ;;
esac

# The whole phases array is valid JSON, when python3 is there to say so.
if command -v python3 >/dev/null; then
  printf '%s\n' "$phases" | python3 -c 'import json, sys; json.load(sys.stdin)' \
    || { echo "FAIL: phases json does not parse"; failures=$((failures + 1)); }
fi

if [ "$failures" -gt 0 ]; then
  echo "$failures failed"
  exit 1
fi
echo "lib.sh: all tests passed"
