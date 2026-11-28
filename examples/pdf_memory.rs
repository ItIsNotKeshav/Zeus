//! Measures the PDF viewer's memory and frame times without a window, for
//! pull requests that touch it.
//!
//! ```sh
//! cargo run --profile fast --features demo --example pdf_memory
//! cargo run --profile fast --features demo --example pdf_memory -- --json after.json --baseline before.json
//! ```
//!
//! It plays the PDF benchmark's scenario (`--demo-tour-script pdf`, about 80
//! seconds) through the real interface, headless, at 60 frames a second and
//! a 2x display: a synthetic 40-page report with a picture on every page is
//! opened from its card in the demo chat, read from the first page to the
//! last, zoomed to 2x and to the 5x limit and scrolled, searched, left open,
//! and closed. Nothing touches a linked account: the demo runs in a fresh
//! temporary folder.
//!
//! Each phase reports, at its end and at its highest:
//!
//! - textures: the bytes of every texture egui holds, which a window keeps
//!   on the GPU (in the app's memory on a Mac, where the GPU shares it);
//! - process: the process's own memory (physical footprint on macOS, PSS on
//!   Linux), which here holds no textures, since nothing is drawn;
//!
//! and, for the phases that move, the interface's work per frame: median,
//! 99th percentile, slowest, and how many frames took longer than a 60 Hz
//! display's 16.7 ms. Pages render on threads of their own, so those numbers
//! are the frames' own cost, not the time a page takes to appear.
//!
//! `--json PATH` saves the numbers; `--baseline PATH` adds the change
//! against numbers saved before. The table is Markdown, to paste into a
//! pull request. Run both builds on the same machine, one after another.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use zeus::demo::tour::{Script, Tour};

/// The scenario's phases, in order.
const PHASES: [&str; 7] = ["settle", "open", "read", "zoom", "search", "idle", "closed"];
/// The phases whose frame times are shown.
const TIMED: [&str; 3] = ["read", "zoom", "search"];
const FRAME: Duration = Duration::from_micros(16_667);
const MIB: f64 = 1024.0 * 1024.0;

#[derive(Default)]
struct Phase {
    textures_end: usize,
    textures_peak: usize,
    process_end: u64,
    process_peak: u64,
    frames: Vec<f32>,
}

fn main() {
    let mut json_out = None;
    let mut baseline = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json_out = args.next(),
            "--baseline" => baseline = args.next(),
            "-h" | "--help" => {
                println!("pdf_memory [--json PATH] [--baseline PATH]");
                return;
            }
            other => {
                eprintln!("pdf_memory: unknown option {other}");
                std::process::exit(2);
            }
        }
    }
    let baseline: Option<Value> = baseline.map(|path| {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read {path}: {error}"));
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"))
    });

    let root = tempfile::Builder::new()
        .prefix("zeus-pdf-memory-")
        .tempdir()
        .expect("a temporary folder");
    let (mut app, _events) = zeus::app::App::headless(
        zeus::paths::AppDirs::under(root.path()),
        zeus::settings::Settings::default(),
    );
    zeus::demo::populate(&mut app);
    Script::Pdf.prepare(&mut app);
    let ctx = egui::Context::default();
    app.attach(&ctx);
    let mut tour = Tour::scripted(Script::Pdf, None, None, None).with_marks(None);

    // Textures egui holds: what a window would keep on the GPU.
    let mut textures: HashMap<egui::TextureId, usize> = HashMap::new();
    let mut phases: HashMap<&'static str, Phase> = HashMap::new();
    let started = Instant::now();
    let deadline = Script::Pdf.duration() + Duration::from_secs(30);
    let mut next = Instant::now();
    eprintln!(
        "Playing the PDF scenario, about {} seconds.",
        Script::Pdf.duration().as_secs()
    );
    while !tour.finished() && started.elapsed() < deadline {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            max_texture_side: Some(16_384),
            time: Some(started.elapsed().as_secs_f64()),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(2.0);
        let frame = Instant::now();
        tour.input(&mut app, &ctx, &mut input);
        let output = ctx.run_ui(input, |ui| {
            app.background_frame(&ctx);
            app.frame_ui(ui);
            tour.observe(&mut app, &ctx);
        });
        tour.drive(&mut app, &ctx);
        let work = frame.elapsed().as_secs_f32() * 1000.0;
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                if delta.pos.is_none() {
                    let [width, height] = delta.image.size();
                    textures.insert(*id, width * height * 4);
                }
            }
        }
        for id in &output.textures_delta.free {
            textures.remove(id);
        }
        drop(output);
        if let Some(name) = tour.phase() {
            let held: usize = textures.values().sum();
            let process = process_bytes().unwrap_or(0);
            let phase = phases.entry(name).or_default();
            phase.textures_end = held;
            phase.textures_peak = phase.textures_peak.max(held);
            phase.process_end = process;
            phase.process_peak = phase.process_peak.max(process);
            if TIMED.contains(&name) {
                phase.frames.push(work);
            }
        }
        next += FRAME;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    }
    if tour.failed() {
        eprintln!("The scenario stopped: something it looked for was not on screen.");
        std::process::exit(1);
    }
    for (name, ok) in tour.checks() {
        if !ok {
            eprintln!("check failed: {name}");
        }
    }

    let result = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "process_metric": PROCESS_METRIC,
        "phases": PHASES.iter().map(|name| {
            let phase = phases.remove(name).unwrap_or_default();
            let mut sorted = phase.frames.clone();
            sorted.sort_by(f32::total_cmp);
            let rank = |percent: f32| {
                if sorted.is_empty() {
                    return Value::Null;
                }
                let index = ((percent / 100.0) * sorted.len() as f32).ceil() as usize;
                json!(round(sorted[index.clamp(1, sorted.len()) - 1]))
            };
            json!({
                "name": name,
                "textures_mb": round(phase.textures_end as f64 / MIB),
                "textures_peak_mb": round(phase.textures_peak as f64 / MIB),
                "process_mb": round(phase.process_end as f64 / MIB),
                "process_peak_mb": round(phase.process_peak as f64 / MIB),
                "frames": sorted.len(),
                "frame_p50_ms": rank(50.0),
                "frame_p99_ms": rank(99.0),
                "frame_max_ms": sorted.last().map(|max| round(*max)),
                "frames_over_16ms": sorted.iter().filter(|ms| **ms > 16.7).count(),
            })
        }).collect::<Vec<_>>(),
    });
    if let Some(path) = json_out {
        std::fs::write(&path, format!("{result:#}\n"))
            .unwrap_or_else(|error| panic!("could not write {path}: {error}"));
        eprintln!("Wrote {path}");
    }
    print!("{}", table(&result, baseline.as_ref()));
}

fn round<T: Into<f64>>(value: T) -> f64 {
    (value.into() * 10.0).round() / 10.0
}

/// The results as Markdown tables, with the change against `baseline`.
fn table(result: &Value, baseline: Option<&Value>) -> String {
    let phase = |result: &Value, name: &str| -> Option<Value> {
        result["phases"]
            .as_array()?
            .iter()
            .find(|phase| phase["name"] == name)
            .cloned()
    };
    let cell = |name: &str, field: &str| -> String {
        let Some(value) = phase(result, name).and_then(|phase| phase[field].as_f64()) else {
            return "n/a".into();
        };
        let before = baseline
            .and_then(|baseline| phase(baseline, name))
            .and_then(|phase| phase[field].as_f64());
        match before {
            Some(before) if before > 0.0 => format!(
                "{value:.1} ({:+.1}, {:+.0}%)",
                value - before,
                (value - before) / before * 100.0
            ),
            Some(before) => format!("{value:.1} ({:+.1})", value - before),
            None => format!("{value:.1}"),
        }
    };
    let mut out = String::new();
    out.push_str(&format!(
        "| Phase | Textures at end (MB) | Textures, highest (MB) | Process at end, {0} (MB) | Process, highest (MB) |\n|---|---|---|---|---|\n",
        result["process_metric"].as_str().unwrap_or("memory")
    ));
    for name in PHASES {
        out.push_str(&format!(
            "| {name} | {} | {} | {} | {} |\n",
            cell(name, "textures_mb"),
            cell(name, "textures_peak_mb"),
            cell(name, "process_mb"),
            cell(name, "process_peak_mb"),
        ));
    }
    out.push_str(
        "\n| Phase | Frame work, median (ms) | 99th percentile (ms) | Slowest (ms) | Frames over 16.7 ms |\n|---|---|---|---|---|\n",
    );
    for name in TIMED {
        out.push_str(&format!(
            "| {name} | {} | {} | {} | {} |\n",
            cell(name, "frame_p50_ms"),
            cell(name, "frame_p99_ms"),
            cell(name, "frame_max_ms"),
            cell(name, "frames_over_16ms"),
        ));
    }
    if baseline.is_some() {
        out.push_str("\nIn brackets: the change against the baseline. Lower is better.\n");
    }
    out
}

#[cfg(target_os = "macos")]
const PROCESS_METRIC: &str = "physical footprint";
#[cfg(target_os = "linux")]
const PROCESS_METRIC: &str = "PSS";
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const PROCESS_METRIC: &str = "not measured here";

/// The process's own memory: what Activity Monitor calls Memory on macOS,
/// PSS on Linux.
#[cfg(target_os = "macos")]
fn process_bytes() -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: `info` is a rusage_info_v4, which RUSAGE_INFO_V4 fills, for
    // this process.
    let status = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as i32,
            libc::RUSAGE_INFO_V4,
            info.as_mut_ptr().cast(),
        )
    };
    // SAFETY: filled by the successful call above.
    (status == 0).then(|| unsafe { info.assume_init() }.ri_phys_footprint)
}

#[cfg(target_os = "linux")]
fn process_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/smaps_rollup").ok()?;
    let line = text.lines().find(|line| line.starts_with("Pss:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_bytes() -> Option<u64> {
    None
}
