"""Tests for compare.py.  python3 -m unittest discover -s scripts/bench -p 'test_*.py'"""

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare  # noqa: E402


def result(label, run, settle, settled, status="complete", p99=20.0, chip="Apple M2", checks=None, gpu=30.0):
    phases = []
    for name in compare.PHASES:
        mb = {"settle": settle, "settle-again": settled}.get(name, settle + 100)
        cpu = 80.0 if name == "scroll" else 5.0
        phases.append(
            {
                "name": name,
                "end": {"t": 1, "mb": mb, "other_mb": mb},
                "max_mb": mb,
                "cpu_pct": cpu,
                "gpu_pct": gpu if name == "scroll" else None,
                "samples": [],
            }
        )
    frames = {
        phase: {
            "interval_ms": {"p50": 16.7, "p99": p99, "max": p99 + 30},
            "work_ms": {"p95": 3.0},
            "fps": 59.0,
            "missed": 4,
            "jitter_ms": 2.5,
        }
        for phase in ("scroll", "churn", "picker")
    }
    return {
        "schema": 2,
        "label": label,
        "run": run,
        "status": status,
        "commit": f"{label}-commit",
        "os": "macOS 15.1 (arm64)",
        "hardware": {"chip": chip, "ram_gb": 16.0},
        "renderer": "OpenGL (glow)",
        "app": {"pixels_per_point": 2.0},
        "metric": "physical_footprint",
        "gpu_metric": "whole_gpu",
        "phases": phases,
        "peak_mb": settle + 200,
        "os_peak_mb": settle + 210,
        "settled_mb": settled,
        "frames": frames,
        "checks": checks or [{"name": "scroll 1: reached the oldest message", "ok": True}],
    }


class CompareTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.root = Path(self.folder.name)

    def tearDown(self):
        self.folder.cleanup()

    def write(self, data):
        folder = self.root / data["label"]
        folder.mkdir(exist_ok=True)
        (folder / f"run-{data['run']:02d}.json").write_text(json.dumps(data), encoding="utf-8")

    def run_main(self, *args):
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            code = compare.main([str(arg) for arg in args])
        return code, out.getvalue()

    def test_spread_is_the_median_and_the_range(self):
        self.assertEqual(compare.spread([3.0, 1.0, 2.0, 10.0, 4.0]), (3.0, 1.0, 10.0))
        self.assertEqual(compare.spread([1.0, 2.0]), (1.5, 1.0, 2.0))
        self.assertIsNone(compare.spread([]))

    def test_change_is_against_the_baseline(self):
        self.assertEqual(compare.change(200.0, 150.0), "-50.0, -25%")
        self.assertEqual(compare.change(100.0, 110.0), "+10.0, +10%")
        self.assertEqual(compare.change(0.0, 5.0), "+5.0")
        self.assertEqual(compare.change(None, 5.0), "")

    def test_a_table_of_two_builds_with_changes_against_the_first(self):
        for run, (settle, settled) in enumerate([(200, 180), (210, 190), (190, 170)], 1):
            self.write(result("main", run, settle, settled))
        for run, (settle, settled) in enumerate([(150, 100), (160, 110), (140, 90)], 1):
            self.write(result("pr66", run, settle, settled, p99=18.0, gpu=20.0))
        code, out = self.run_main(self.root / "main", self.root / "pr66")
        self.assertEqual(code, 0)
        self.assertIn("| Memory at phase end, physical footprint (MB) | main (3 runs) | pr66 (3 runs) |", out)
        self.assertIn("| Settle (10 s) | 200.0 (190.0 to 210.0) | 150.0 (140.0 to 160.0) **-50.0, -25%** |", out)
        self.assertIn("| Idle again (40 s) | 180.0 (170.0 to 190.0) | 100.0 (90.0 to 110.0) **-80.0, -44%** |", out)
        self.assertIn(
            "| Scrolling: frame time, 99th percentile (ms) | 20.0 (20.0 to 20.0) | 18.0 (18.0 to 18.0) **-2.0, -10%** |",
            out,
        )
        self.assertIn("| Scrolling: frames a second (higher is better) | 59.0 (59.0 to 59.0) |", out)
        self.assertIn("| Switching chats: missed frames | 4 (4 to 4) | 4 (4 to 4) **+0, +0%** |", out)
        self.assertIn("| Sticker picker: slowest frame (ms) | 50.0 (50.0 to 50.0) | 48.0 (48.0 to 48.0) **-2.0, -4%** |", out)
        self.assertIn("| CPU: Scroll the photo chat | 80.0 (80.0 to 80.0) | 80.0 (80.0 to 80.0) **+0.0, +0%** |", out)
        self.assertIn("| GPU: Scroll the photo chat | 30.0 (30.0 to 30.0) | 20.0 (20.0 to 20.0) **-10.0, -33%** |", out)
        self.assertNotIn("GPU: Settle", out)
        self.assertIn("whole GPU's utilisation", out)
        self.assertLess(out.index("Smoothness"), out.index("CPU and GPU"))
        self.assertLess(out.index("CPU and GPU"), out.index("Memory at phase end"))
        self.assertIn("- main: main-commit; macOS 15.1 (arm64), Apple M2, 16 GB, renderer OpenGL (glow), scale 2", out)
        self.assertNotIn(">", out.split("- main")[0])

    def test_the_parent_folder_finds_every_build_and_the_baseline_can_be_chosen(self):
        self.write(result("main", 1, 200, 180))
        self.write(result("pr66", 1, 150, 100))
        code, out = self.run_main("--baseline", "pr66", self.root)
        self.assertEqual(code, 0)
        self.assertIn("| pr66 (1 run) | main (1 run) |", out)
        self.assertIn("200.0 (200.0 to 200.0) **+50.0, +33%**", out)

    def test_failed_runs_are_left_out_and_said(self):
        self.write(result("main", 1, 200, 180))
        self.write(result("main", 2, 999, 999, status="failed"))
        self.write(result("pr66", 1, 150, 100, checks=[{"name": "minimized: window reports minimized", "ok": False}]))
        code, out = self.run_main(self.root)
        self.assertEqual(code, 0)
        self.assertIn("main (1 run)", out)
        self.assertNotIn("999", out)
        self.assertIn("> main: 1 of 2 runs did not finish and are left out.", out)
        self.assertIn("> pr66: some runs failed checks: minimized: window reports minimized.", out)

    def test_different_machines_are_flagged(self):
        self.write(result("main", 1, 200, 180))
        self.write(result("pr66", 1, 150, 100, chip="Apple M1"))
        _, out = self.run_main(self.root)
        self.assertIn("not all measured on the same machine", out)

    def test_no_results_or_an_unknown_baseline_fail(self):
        self.assertEqual(self.run_main(self.root)[0], 1)
        self.write(result("main", 1, 200, 180))
        self.assertEqual(self.run_main("--baseline", "nope", self.root)[0], 1)
        (self.root / "main" / "notes.json").write_text('{"other": true}', encoding="utf-8")
        (self.root / "main" / "broken.json").write_text("{", encoding="utf-8")
        self.assertEqual(self.run_main(self.root)[0], 0)

    def test_windows_results_with_a_byte_order_mark_load(self):
        folder = self.root / "win"
        folder.mkdir()
        data = result("win", 1, 120, 100)
        data["metric"] = "private_bytes"
        (folder / "run-01.json").write_bytes(b"\xef\xbb\xbf" + json.dumps(data).encode())
        code, out = self.run_main(folder)
        self.assertEqual(code, 0)
        self.assertIn("Memory at phase end, private bytes (MB)", out)

    def test_first_version_results_still_compare(self):
        data = result("old", 1, 200, 180)
        data["schema"] = 1
        data["frames"] = {"interval_ms": {"p50": 16.7, "p99": 25.0, "max": 40.0}, "work_ms": {"p95": 3.0}}
        for phase in data["phases"]:
            del phase["cpu_pct"], phase["gpu_pct"]
        self.write(data)
        code, out = self.run_main(self.root)
        self.assertEqual(code, 0)
        self.assertIn("| Scrolling: frame time, 99th percentile (ms) | 25.0 (25.0 to 25.0) |", out)
        self.assertNotIn("CPU and GPU", out)
        self.assertNotIn("Switching chats", out)


if __name__ == "__main__":
    unittest.main()
