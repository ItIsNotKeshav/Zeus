#!/usr/bin/env python3
"""Compares Zeus benchmark results and prints Markdown tables.

    python3 scripts/bench/compare.py bench-results/main bench-results/pr66 ...
    python3 scripts/bench/compare.py bench-results            # every build in it
    python3 scripts/bench/compare.py --baseline main bench-results

Each folder holds one build's run-*.json files (or folders that do). For each
build it prints smoothness (frame rate, missed frames, jitter), CPU and GPU
use, and memory: the median of the runs and their spread (lowest to highest),
and the change of the median against the baseline build, the first one named
unless --baseline says otherwise. Paste the output into a pull request;
results do not go in the repository. Standard library only.
"""

import argparse
import json
import statistics
import sys
from pathlib import Path

PHASES = ["settle", "scroll", "churn", "picker", "search", "minimized", "settle-again"]
PHASE_NAMES = {
    "settle": "Settle (10 s)",
    "scroll": "Scroll the photo chat",
    "churn": "Switch through 20 chats",
    "picker": "Sticker picker",
    "search": "Search",
    "minimized": "Minimized (10 s)",
    "settle-again": "Idle again (40 s)",
}
# The phases whose frames the app times, and what they are called in the
# smoothness table.
TIMED = {"scroll": "Scrolling", "churn": "Switching chats", "picker": "Sticker picker",
         "search": "Search"}
METRIC_NAMES = {
    "physical_footprint": "physical footprint",
    "pss": "PSS",
    "private_bytes": "private bytes",
}
OTHER_NAMES = {"rss": "RSS", "working_set": "working set"}


def load_runs(paths):
    """Every result file under the paths, grouped by label in first-seen order."""
    builds = {}
    for path in paths:
        path = Path(path)
        files = [path] if path.is_file() else sorted(path.rglob("*.json"))
        for file in files:
            try:
                run = json.loads(file.read_text(encoding="utf-8-sig"))
            except (OSError, ValueError) as error:
                print(f"skipping {file}: {error}", file=sys.stderr)
                continue
            if not isinstance(run, dict) or run.get("schema") not in (1, 2) or "label" not in run:
                continue
            if run.get("schema") == 1:
                # The first version timed the scroll only.
                run["frames"] = {"scroll": run["frames"]} if run.get("frames") else {}
            run["_file"] = str(file)
            builds.setdefault(run["label"], []).append(run)
    return builds


def complete(runs):
    return [run for run in runs if run.get("status") == "complete"]


def phase_end(run, phase, field="mb"):
    for entry in run.get("phases") or []:
        if entry.get("name") == phase and entry.get("end"):
            return entry["end"].get(field)
    return None


def phase_value(run, phase, field):
    for entry in run.get("phases") or []:
        if entry.get("name") == phase:
            return entry.get(field)
    return None


def frame_stat(run, phase, group, stat=None):
    """A number from the frames the app timed in `phase`: a group such as
    interval_ms with a stat such as p95, or a plain one such as fps."""
    frames = (run.get("frames") or {}).get(phase) or {}
    value = frames.get(group)
    return value.get(stat) if stat and isinstance(value, dict) else value


def values(runs, pick):
    found = [pick(run) for run in runs]
    return [value for value in found if isinstance(value, (int, float))]


def spread(numbers):
    """Median, lowest and highest, or None for no numbers."""
    if not numbers:
        return None
    return statistics.median(numbers), min(numbers), max(numbers)


def change(base, value, digits=1):
    """The difference and the percentage against base, as text."""
    if base is None or value is None:
        return ""
    delta = value - base
    percent = f", {delta / base * 100:+.0f}%" if base else ""
    return f"{delta:+.{digits}f}{percent}"


def cell(summary, base=None, digits=1):
    if summary is None:
        return "n/a"
    median, low, high = summary
    text = f"{median:.{digits}f} ({low:.{digits}f} to {high:.{digits}f})"
    if base is not None:
        diff = change(base[0], median, digits)
        if diff:
            text += f" **{diff}**"
    return text


def measured(builds, order, baseline, measures):
    """Rows of a table: a name, then a cell per build. Each measure is a
    name, a function from a run to a number, and the digits to show."""
    table = []
    for name, pick, digits in measures:
        base = spread(values(complete(builds[baseline]), pick))
        cells = [spread(values(complete(builds[label]), pick)) for label in order]
        if all(cell_ is None for cell_ in cells):
            continue
        line = [name]
        for label, summary in zip(order, cells):
            line.append(cell(summary, None if label == baseline else base, digits))
        table.append(line)
    return table


def rows(builds, order, baseline):
    """The memory table's rows: a label and a cell per build."""
    measures = [(PHASE_NAMES[phase], lambda run, p=phase: phase_end(run, p), 1) for phase in PHASES]
    measures += [
        ("Peak (highest reading)", lambda run: run.get("peak_mb"), 1),
        ("Peak (system's own count)", lambda run: run.get("os_peak_mb"), 1),
    ]
    return measured(builds, order, baseline, measures)


def frame_rows(builds, order, baseline):
    measures = []
    for phase, name in TIMED.items():
        pick = lambda group, stat=None, p=phase: lambda run: frame_stat(run, p, group, stat)  # noqa: E731
        if phase == "scroll":
            measures += [
                (f"{name}: frames a second (higher is better)", pick("fps"), 1),
                (f"{name}: frame time, median (ms)", pick("interval_ms", "p50"), 1),
                (f"{name}: frame time, 99th percentile (ms)", pick("interval_ms", "p99"), 1),
                (f"{name}: jitter (ms)", pick("jitter_ms"), 1),
            ]
        measures += [
            (f"{name}: missed frames", pick("missed"), 0),
            (f"{name}: slowest frame (ms)", pick("interval_ms", "max"), 1),
            (f"{name}: interface work, 95th percentile (ms)", pick("work_ms", "p95"), 1),
        ]
    return measured(builds, order, baseline, measures)


def use_rows(builds, order, baseline):
    """CPU and GPU use per phase."""
    measures = []
    for kind in ("cpu", "gpu"):
        for phase in PHASES:
            pick = lambda run, p=phase, k=kind: phase_value(run, p, k + "_pct")  # noqa: E731
            measures.append((f"{kind.upper()}: {PHASE_NAMES[phase]}", pick, 1))
    return measured(builds, order, baseline, measures)


def gpu_note(builds, order):
    metrics = {run.get("gpu_metric") for label in order for run in complete(builds[label])}
    if "whole_gpu" in metrics:
        return "GPU on macOS is the whole GPU's utilisation, other apps included; close them first."
    if any(phase_value(run, "scroll", "gpu_pct") is not None for label in order for run in complete(builds[label])):
        return "GPU is this process's 3D engine time."
    return "No GPU readings here (software rendering, or a driver that does not report them)."


def markdown(header, table):
    lines = ["| " + " | ".join(header) + " |", "|" + "---|" * len(header)]
    lines += ["| " + " | ".join(line) + " |" for line in table]
    return "\n".join(lines)


def describe(runs):
    """What the runs were measured on, from the first complete one."""
    run = (complete(runs) or runs)[0]
    hardware = run.get("hardware") or {}
    app = run.get("app") or {}
    parts = [run.get("os", "unknown OS"), hardware.get("chip") or "unknown chip"]
    if hardware.get("ram_gb"):
        parts.append(f"{hardware['ram_gb']:g} GB")
    parts.append(f"renderer {run.get('renderer', 'unknown')}")
    if app.get("pixels_per_point"):
        parts.append(f"scale {app['pixels_per_point']:g}")
    return ", ".join(parts)


def warnings(builds, order):
    notes = []
    machines = {label: machine_key(builds[label]) for label in order}
    if len(set(machines.values())) > 1:
        notes.append(
            "The builds were not all measured on the same machine, system and "
            "scale, so their numbers are not comparable."
        )
    for label in order:
        runs = builds[label]
        failed = len(runs) - len(complete(runs))
        if failed:
            notes.append(f"{label}: {failed} of {len(runs)} runs did not finish and are left out.")
        missed = sorted(
            {
                check.get("name", "?")
                for run in complete(runs)
                for check in run.get("checks") or []
                if check.get("ok") is not True
            }
        )
        if missed:
            notes.append(f"{label}: some runs failed checks: {'; '.join(missed)}.")
        commits = sorted({run.get("commit", "unknown") for run in runs})
        if len(commits) > 1:
            notes.append(f"{label}: runs come from different commits: {', '.join(commits)}.")
    return notes


def machine_key(runs):
    run = (complete(runs) or runs)[0]
    hardware = run.get("hardware") or {}
    app = run.get("app") or {}
    return (
        run.get("os"),
        hardware.get("chip"),
        hardware.get("ram_gb"),
        run.get("metric"),
        app.get("pixels_per_point"),
    )


def report(builds, order, baseline):
    first = complete(builds[baseline]) or builds[baseline]
    metric = METRIC_NAMES.get(first[0].get("metric"), first[0].get("metric", "memory"))
    out = []
    header = []
    for label in order:
        count = len(complete(builds[label]))
        header.append(f"{label} ({count} run{'s' if count != 1 else ''})")
    smooth = frame_rows(builds, order, baseline)
    if smooth:
        out.append(markdown(["Smoothness"] + header, smooth))
        out.append("")
    use = use_rows(builds, order, baseline)
    if use:
        out.append(markdown(["CPU and GPU, % (100 = one core busy)"] + header, use))
        out.append("")
        out.append(gpu_note(builds, order))
        out.append("")
    out.append(markdown(["Memory at phase end, " + metric + " (MB)"] + header, rows(builds, order, baseline)))
    out.append("")
    out.append(
        f"Each cell: median of the runs (lowest to highest). **Bold**: the median's "
        f"change against {baseline}; lower is better unless the row says otherwise."
    )
    out.append("")
    for label in order:
        runs = builds[label]
        commits = ", ".join(sorted({run.get("commit", "unknown")[:12] for run in runs}))
        changes = next((run.get("changes") for run in runs if run.get("changes")), "")
        detail = f" ({changes})" if changes else ""
        out.append(f"- {label}: {commits}{detail}; {describe(runs)}")
    notes = warnings(builds, order)
    if notes:
        out.append("")
        out.extend(f"> {note}" for note in notes)
    return "\n".join(out)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("folders", nargs="+", help="result folders or files")
    parser.add_argument("--baseline", help="the build to compare against (default: the first)")
    args = parser.parse_args(argv)
    builds = load_runs(args.folders)
    if not builds:
        print("No benchmark results found.", file=sys.stderr)
        return 1
    order = list(builds)
    baseline = args.baseline or order[0]
    if baseline not in builds:
        print(f"No results for {baseline}; there are {', '.join(order)}.", file=sys.stderr)
        return 1
    order.remove(baseline)
    order.insert(0, baseline)
    if not complete(builds[baseline]):
        print(f"No finished runs of {baseline} to compare against.", file=sys.stderr)
        return 1
    print(report(builds, order, baseline))
    return 0


if __name__ == "__main__":
    sys.exit(main())
