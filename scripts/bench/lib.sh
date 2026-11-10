# Shared pieces of the benchmark's macOS and Linux sampler. Sourced by
# sample.sh and test-lib.sh; plain bash 3.2 and POSIX awk, so a stock Mac runs
# it without anything installed.

# The scenario's phases, in order, as the app's marks name them.
BENCH_PHASES="settle scroll churn picker search minimized settle-again"
# Seconds between readings, in every phase.
BENCH_INTERVAL=2
# The scenario runs about 2 minutes; give up on a run well after that.
BENCH_TIMEOUT=300

# Milliseconds since 1970: GNU date has %N, a stock Mac has perl.
bench_now_ms() {
  case "$(date +%3N 2>/dev/null)" in
    [0-9][0-9][0-9]) date +%s%3N ;;
    *) perl -MTime::HiRes=time -e 'printf "%d\n", time * 1000' ;;
  esac
}

# Reads /proc/<pid>/smaps_rollup on stdin. Prints PSS, RSS and private bytes
# (clean plus dirty), in KiB.
bench_parse_smaps_rollup() {
  awk '
    $1 == "Pss:" { pss = $2 }
    $1 == "Rss:" { rss = $2 }
    $1 == "Private_Clean:" { private += $2 }
    $1 == "Private_Dirty:" { private += $2 }
    END { printf "%d %d %d\n", pss, rss, private }
  '
}

# Reads /proc/<pid>/status on stdin. Prints the peak resident size, in KiB.
bench_parse_status_peak() {
  awk '$1 == "VmHWM:" { print $2 + 0; found = 1 } END { if (!found) print 0 }'
}

# Converts sizes such as 152.3M, 980K, 1.2G or 512 (bytes), as vmmap and
# footprint print them, to KiB.
bench_awk_size='
  function kib(text,   number, unit) {
    number = text
    gsub(/[^0-9.]/, "", number)
    unit = toupper(text)
    gsub(/[0-9. ]/, "", unit)
    if (unit ~ /^G/) return number * 1024 * 1024
    if (unit ~ /^M/) return number * 1024
    if (unit ~ /^K/) return number + 0
    return number / 1024
  }
'

# Reads `vmmap -summary <pid>` on stdin. Prints the physical footprint (what
# Activity Monitor calls Memory) and its peak, in KiB.
bench_parse_vmmap_summary() {
  awk "$bench_awk_size"'
    /^Physical footprint \(peak\):/ { peak = kib($4); next }
    /^Physical footprint:/ { now = kib($3) }
    END { printf "%d %d\n", now, peak }
  '
}

# Reads `footprint -p <pid>` on stdin, the fallback when vmmap is missing.
# Prints the footprint and its peak (0 when not shown), in KiB.
bench_parse_footprint() {
  awk "$bench_awk_size"'
    {
      for (i = 1; i < NF; i++) {
        if ($i == "Footprint:" && !now) now = kib($(i + 1) $(i + 2))
        if ($i == "Peak" && $(i + 1) == "Footprint:" && !peak) peak = kib($(i + 2) $(i + 3))
      }
    }
    END { printf "%d %d\n", now, peak }
  '
}

# Reads /proc/<pid>/stat on stdin. Prints the CPU time the process used, in
# clock ticks (user plus system).
bench_parse_proc_stat_cpu() {
  awk '{ sub(/^.*\) /, ""); split($0, field, " "); print field[12] + field[13] }'
}

# Reads `ps -o time= -p <pid>` on stdin ([[days-]hours:]minutes:seconds).
# Prints the CPU time in milliseconds.
bench_parse_ps_time() {
  awk '
    {
      text = $1
      days = 0
      if (index(text, "-")) { days = substr(text, 1, index(text, "-") - 1); text = substr(text, index(text, "-") + 1) }
      count = split(text, part, ":")
      seconds = 0
      for (i = 1; i <= count; i++) seconds = seconds * 60 + part[i]
      printf "%d\n", (days * 86400 + seconds) * 1000
    }
  '
}

# Reads every /proc/<pid>/fdinfo/* of the process on stdin, one after the
# other. Prints the nanoseconds the GPU's 3D engine spent on this process
# (amdgpu, Intel and other drivers that report drm-engine), or nothing.
bench_parse_drm_fdinfo() {
  awk '
    function flush() {
      if (client != "" && !(client in seen)) { seen[client] = 1; total += busy; found = 1 }
      client = ""; busy = 0
    }
    $1 == "pos:" { flush() }
    $1 == "drm-client-id:" { client = $2 }
    $1 == "drm-engine-render:" || $1 == "drm-engine-gfx:" { busy += $2 }
    END { flush(); if (found) printf "%.0f\n", total }
  '
}

# Reads `ioreg -r -d 1 -w 0 -c IOAccelerator` on stdin (macOS). Prints the
# GPU's utilisation in percent, the whole GPU, or nothing.
bench_parse_ioreg_gpu() {
  awk '
    match($0, /"Device Utilization %"=[0-9]+/) {
      value = substr($0, RSTART, RLENGTH); sub(/.*=/, "", value)
      if (!found || value + 0 > best) best = value + 0
      found = 1
    }
    END { if (found) print best }
  '
}

# Reads `/proc/<pid>/maps` (Linux) or `vmmap <pid>` (macOS) on stdin and
# names the renderer from the graphics libraries the process loaded.
bench_detect_renderer() {
  awk '
    /libvulkan/ { vulkan = 1 }
    /libGLX|libEGL|libGL\.so|OpenGL\.framework/ { gl = 1 }
    /Metal\.framework/ { metal = 1 }
    END {
      if (vulkan) print "wgpu (Vulkan)"
      else if (gl) print "OpenGL (glow)"
      else if (metal) print "wgpu (Metal)"
      else print "unknown"
    }
  '
}

# Reads the app's marks on stdin. Prints one line per mark, tab-separated:
# the event, then the phase (phase, frames), the check name and ok (check), or
# nothing.
bench_parse_marks() {
  awk '
    function field(name,   rest, value) {
      if (!match($0, "\"" name "\":")) return ""
      rest = substr($0, RSTART + RLENGTH)
      if (substr(rest, 1, 1) == "\"") {
        rest = substr(rest, 2)
        value = substr(rest, 1, index(rest, "\"") - 1)
      } else {
        match(rest, /^[^,}]*/)
        value = substr(rest, 1, RLENGTH)
      }
      return value
    }
    {
      event = field("event")
      if (event == "phase") print event "\t" field("phase")
      else if (event == "check") print event "\t" field("name") "\t" field("ok")
      else if (event == "frames") print event "\t" field("phase")
      else if (event != "") print event
    }
  '
}

# KiB to MB (10^6 bytes), one decimal, as Activity Monitor and Task Manager
# count.
bench_mb() {
  awk -v kib="$1" 'BEGIN { printf "%.1f", kib * 1024 / 1000000 }'
}

# Escapes text for a JSON string.
bench_json_string() {
  printf '%s' "$1" | awk '
    BEGIN { ORS = "" }
    {
      gsub(/\\/, "\\\\"); gsub(/"/, "\\\""); gsub(/\t/, " ")
      if (NR > 1) print "\\n"
      print
    }
  '
}

# Reads the samples file on stdin: tab-separated lines of phase, kind (tick
# or end), milliseconds since launch, main KiB, other KiB, CPU milliseconds
# used since launch, and GPU percent (- when unknown). Prints the "phases"
# JSON array, the phases in BENCH_PHASES order. A phase's CPU percent is the
# time the process ran on the CPU over the phase's length (100 is one core
# busy); its GPU percent is the mean of its readings.
bench_phases_json() {
  awk -F '\t' -v order="$BENCH_PHASES" '
    function mb(kib) { return sprintf("%.1f", kib * 1024 / 1000000) }
    function percent(cpu, ms) { return ms > 0 ? sprintf("%.1f", cpu / ms * 100) : "null" }
    BEGIN { count = split(order, names, " ") }
    {
      phase = $1
      cpu = percent($6 - last_cpu, $3 - last_t)
      last_cpu = $6; last_t = $3
      gpu = ($7 == "-" || $7 == "") ? "null" : $7
      sample = "{\"t\": " sprintf("%.1f", $3 / 1000) ", \"mb\": " mb($4) ", \"other_mb\": " mb($5) ", \"cpu_pct\": " cpu ", \"gpu_pct\": " gpu "}"
      samples[phase] = samples[phase] (samples[phase] == "" ? "" : ", ") sample
      if (!(phase in max) || $4 > max[phase]) max[phase] = $4
      if (gpu != "null") { gpu_sum[phase] += $7; gpu_count[phase]++ }
      if ($2 == "end") {
        end[phase] = sample
        phase_cpu[phase] = percent($6 - end_cpu, $3 - end_t)
        end_cpu = $6; end_t = $3
      }
    }
    END {
      printf "["
      for (i = 1; i <= count; i++) {
        name = names[i]
        printf "%s\n    {\"name\": \"%s\", ", (i > 1 ? "," : ""), name
        printf "\"end\": %s, ", (name in end ? end[name] : "null")
        printf "\"max_mb\": %s, ", (name in max ? mb(max[name]) : "null")
        printf "\"cpu_pct\": %s, ", (name in phase_cpu ? phase_cpu[name] : "null")
        printf "\"gpu_pct\": %s, ", (gpu_count[name] ? sprintf("%.1f", gpu_sum[name] / gpu_count[name]) : "null")
        printf "\"samples\": [%s]}", samples[name]
      }
      printf "\n  ]"
    }
  '
}

# Reads the samples file on stdin. Prints the highest main reading, in MB.
bench_peak_mb() {
  awk -F '\t' '$4 > peak { peak = $4 } END { printf "%.1f", peak * 1024 / 1000000 }'
}

# Reads the samples file on stdin. Prints the main reading at the end of
# `phase`, in MB, or null.
bench_end_mb() {
  awk -F '\t' -v phase="$1" '
    $1 == phase && $2 == "end" { value = sprintf("%.1f", $4 * 1024 / 1000000) }
    END { print (value == "" ? "null" : value) }
  '
}
