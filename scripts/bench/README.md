# Zeus benchmark

This checks how smoothly Zeus runs (frame rate, missed frames, jitter while
scrolling, switching chats and opening the sticker picker) and how much
memory, CPU and GPU it uses, while it plays the same scripted session every
time. A run takes about 3 minutes; the default two runs of a build take
about 5. It exists so that builds can be compared with numbers.

It only ever runs the built-in demo: made-up chats in a fresh temporary
folder, no WhatsApp connection, no account, no real messages or contacts.
The scripts refuse any build that cannot run the demo benchmark. The result
files hold numbers and a description of the machine (model, chip, cores,
memory, system version), nothing personal.

## What one run does

The app opens a 1280 by 800 window on a chat with 240 photos and a few
animated stickers, then, on a fixed timetable (about 3 minutes):

| Phase | What happens |
|---|---|
| settle | 10 seconds after the window appears |
| scroll | scrolls the photo chat to the top and back down, twice, fast |
| churn | switches through 20 other chats, one every 0.6 seconds |
| picker | opens the sticker picker and closes it, 5 times |
| search | types three queries into the chat list search, scrolls each list of results down and back, and clears it (the sample data has no archive, so results are found in memory; time to first results is from the last key to the frame showing them) |
| minimized | minimizes the window for 10 seconds, then restores it |
| settle-again | 40 seconds idle (long enough for a build that lets go of pictures after 30 idle seconds) |

Between steps the app gets no input at all, so idle really is idle.

The app times its own frames while it scrolls, switches chats and opens the
picker: frames a second, how long frames took (median, 99th percentile,
slowest), jitter (how unevenly they arrive), and missed frames (frames that
came a whole display refresh or more too late, so the screen showed the
previous one again).

From outside the app, every 2 seconds and at the end of each phase:

- Memory. macOS: physical footprint (`vmmap -summary`), what Activity
  Monitor shows as Memory; graphics memory counts too. Windows: private
  bytes (Task Manager's Commit size) and the working set. Linux: PSS from
  `/proc/<pid>/smaps_rollup`.
- CPU: the time the app ran on the CPU over each phase, where 100% is one
  core fully busy.
- GPU. Windows: the app's own 3D engine use, as in Task Manager's GPU
  column. macOS: the whole GPU's utilisation (`ioreg`), which includes other
  apps, so close them. Linux: the app's 3D engine time from the driver
  (amdgpu, Intel), or nothing under software rendering or NVIDIA.

The window must stay visible and uncovered, so leave the computer alone
during a run. Each run writes one JSON file.

## For a friend with a Mac (no Rust needed)

You will get a file named like `zeus-bench-pr66-aarch64-apple-darwin.tar.gz`
(an Apple silicon Mac is needed). For each file you get:

1. Double-click the file in Finder. It unpacks into a folder.
2. Open Terminal, type `cd ` (with a space), drag that folder into the
   Terminal window, and press Return.
3. Run:

   ```sh
   bash bench/run.sh
   ```

4. Leave the Mac alone for about 5 minutes: plugged in, lid open, nothing
   else running, and do not move the Zeus Demo window or cover it. The
   window opens and closes twice; that is expected. The script keeps
   the Mac awake while it runs.
5. When it says Done, you will find a `bench-results` folder inside the
   folder from step 1. Zip it (right-click, Compress) and send the zip back.

If macOS says the app cannot be opened, open System Settings, Privacy and
Security, click Open Anyway for zeus, and run the command again.

If you were also asked for screenshots, run `bash bench/shots.sh` in the
same folder and send back the `bench-shots` folder too.

Please run every build on the same Mac, one after another, with the same
display and the same apps closed.

## For the maintainer

### Making the builds

Each build is current `main` plus exactly one change, never stacked. The
easy way for every platform is the **Zeus benchmark build** workflow
(`.github/workflows/zeus-bench.yml`) in the Actions tab: Run workflow, fill
in the fields, and download the artifacts when it finishes (about 20 to 40
minutes). It merges pull requests into `main` on GitHub's machine only and
pushes nothing.

| Build | label | prs | fork |
|---|---|---|---|
| Baseline | `main` | (empty) | (empty) |
| Frame pacing, one-pass glass, photo sizes | `pr59` | `59` | (empty) |
| Let go of pictures when idle or minimized | `pr66` | `66` | (empty) |
| Metal on macOS | `pr63-65` | `65` | (empty) |
| pratyakshm's fork, as a reference | `pratyakshm` | (empty) | `pratyakshm/Zeus@main` |
| Everything chosen, combined | `combined` | see below | (empty) |

Notes:

- #65 sits on top of #63, so merging #65 brings both. They point egui at
  RathnamS089's fork of egui, which AGENTS.md does not allow; the benchmark
  build works anyway, but that part has to change before either can be
  merged.
- The fork build is the fork's own branch with the benchmark's commits added
  on top. If they do not apply cleanly the workflow says so, and the fork has
  to get them by hand.
- Combined comes last. #59 and #66 both change the image budget in
  `src/image_cache.rs`, so they do not merge into each other on their own.
  Once you know which changes win, put them together (for example in a draft
  pull request with the conflict resolved) and give that number as `prs`.
- Pick `macos` for a Mac friend, `windows` for yourself, or both.
- The macOS download is a `.tar.gz` inside the artifact's zip; send the
  `.tar.gz` itself. It holds `zeus`, `BUILD_INFO.txt` (what was built) and
  the `bench` scripts.

On your Windows PC you can also build locally. Check out what you want to
measure without creating a branch (for example `git checkout --detach
origin/main`, then `git merge --no-ff --no-edit FETCH_HEAD` after `git fetch
origin pull/66/head`), then, from PowerShell or cmd with the sandbox off and
never from Git Bash:

```bat
packaging\windows\build-zeus.cmd bench
```

That leaves `dist\bench\zeus.exe`, a benchmark build (no installer). Go back
with `git checkout main` afterwards; a normal `build-zeus.cmd` rebuilds the
release binary without the demo.

### Running on Windows

```powershell
powershell -ExecutionPolicy Bypass -File scripts\bench\run.ps1 -Binary dist\bench\zeus.exe -Label pr66
```

With a downloaded build, unzip it, open PowerShell in its folder and run
`powershell -ExecutionPolicy Bypass -File bench\run.ps1`: it finds the
build and its label by itself. Results go to `bench-results\<label>\`.
Keep the PC plugged in and leave it alone while it runs.

### Running on macOS or Linux from a checkout

```sh
cargo build --release --locked --features demo
scripts/bench/run.sh target/release/zeus --label main
```

On Linux, `--headless` draws on a virtual screen (it needs `Xvfb`, and a
window manager such as `openbox` to minimize), and `--scale 2` draws at twice
the size like a Retina screen. Under Xvfb the graphics are drawn in software,
so textures count as process memory, as on a Mac, but frame times say little
about real smoothness. Wayland sessions run the app through XWayland, since
Wayland cannot restore a minimized window.

### Comparing

```sh
python3 scripts/bench/compare.py bench-results/main bench-results/pr66 bench-results/pr59
python3 scripts/bench/compare.py bench-results            # every build in the folder
```

The first build is the baseline unless `--baseline` names another. It prints
Markdown tables to paste into a pull request: per phase, the median of the
runs with the lowest and highest in brackets, and the change of the median
against the baseline. There are three tables: smoothness, CPU and GPU per
phase, and memory per phase. It warns when builds were measured on different machines, when runs failed, and
when the scenario's checks (scrolled all the way, window minimized) did not
pass. Results are review evidence: they go in pull requests, never in the
repository (`bench-results/` and `bench-shots/` are ignored by git).

### Visual check

A memory saving that changes how the app looks does not count. Capture the
same pages from each build and compare them:

```sh
scripts/bench/shots.sh path/to/zeus --label main          # macOS, Linux
powershell -ExecutionPolicy Bypass -File scripts\bench\shots.ps1 -Binary dist\bench\zeus.exe -Label main
python3 scripts/bench/imgdiff.py bench-shots/main bench-shots/pr66 --diff-dir bench-shots/diff
```

It captures the chat, light, pinned, narrow (600 points wide) and calls
pages. Screenshots stop the clock at a fixed moment, so the same page from
two builds on the same machine should match exactly. The table shows the
largest difference of any pixel (0 means identical), how many pixels differ
and where; `--diff-dir` writes pictures with the differences in red.

## Being fair

- Measure every build on the same machine, at the same display scale, one
  after another, plugged in, with nothing else running.
- Two runs per build (the default). If the lowest and highest of a number
  are far apart, add runs with `--runs 5` (`-Runs 5` on Windows); for a quick
  look, `--runs 1` takes 2 minutes.
- Frame times depend on the display's refresh rate; compare builds on the
  same display.
- Every build is `main` plus one change. The demo uses the plain Dark look
  for every build and date.
- Benchmark builds include the demo. They behave like release builds apart
  from small demo details (for example, emoji pictures are drawn during the
  frame instead of on a background thread).

## Files

| File | What it is |
|---|---|
| `run.sh`, `run.ps1` | runs a build N times (default 2) |
| `sample.sh`, `sample.ps1` | one run: launches the build, measures, writes the JSON |
| `lib.sh`, `lib.ps1` | shared parsing and helpers |
| `compare.py` | turns result folders into Markdown tables |
| `shots.sh`, `shots.ps1`, `imgdiff.py` | the visual check |
| `test-lib.sh`, `test_*.py` | tests for the parsers and the comparison |

The scenario itself is `src/demo/tour/bench.rs`, played through the demo tour
(`--demo-tour --demo-tour-script bench --demo-tour-marks FILE`).
