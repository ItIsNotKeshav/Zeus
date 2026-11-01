//! The benchmark in Settings > About. It plays the same two-minute scenario
//! as `scripts/bench` (`--demo-tour --demo-tour-script bench`) in a second
//! Zeus window, a demo with built-in sample chats in a throwaway folder, and
//! measures that process from outside as the scripts do: memory, CPU and GPU
//! every 2 seconds and at each phase's end, with the frame times the window
//! writes itself.
//!
//! Privacy: the second window never opens this account's archive, keys or
//! connection, and the results hold only the hardware summary in
//! [`machine`] and the numbers. Nothing leaves the computer unless the user
//! saves the file or opens the prefilled GitHub issue and submits it there.

pub mod machine;
pub mod probe;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub use machine::Hardware;

/// The scenario's phases in order, as the window's marks name them.
pub const PHASES: [&str; 6] = [
    "settle",
    "scroll",
    "churn",
    "picker",
    "minimized",
    "settle-again",
];
/// The phases whose frame times the window records.
pub const TIMED: [&str; 3] = ["scroll", "churn", "picker"];
/// How long the scenario takes from its first phase to its end.
pub const LENGTH: Duration = Duration::from_secs(117);
/// Seconds between readings inside a phase.
const INTERVAL: Duration = Duration::from_secs(2);
/// A run that has not ended by then has failed.
const TIMEOUT: Duration = Duration::from_secs(300);
/// The window closes itself a few seconds after its last mark.
const CLOSE_WAIT: Duration = Duration::from_secs(30);
/// The window's size, as the scripts launch it.
const WINDOW: &str = "1280x800";

/// What a phase is called on screen.
pub fn phase_label(phase: &str) -> &'static str {
    match phase {
        "settle" => "Letting the window settle",
        "scroll" => "Scrolling a chat full of photos",
        "churn" => "Switching between chats",
        "picker" => "Opening the sticker picker",
        "minimized" => "Minimized",
        "settle-again" => "Idle",
        _ => "Starting",
    }
}

/// Where a run is.
#[derive(Clone, Debug)]
pub enum Stage {
    /// The window is starting, or `phase` is under way since `since`.
    Running {
        phase: Option<String>,
        since: Instant,
        /// When the first phase began.
        began: Option<Instant>,
    },
    Done(Arc<Report>),
    Failed(String),
    /// Stopped from Settings before it ended.
    Stopped,
}

#[derive(Debug)]
struct Shared {
    stage: Stage,
    /// The outcome of the last save.
    saved: Option<Result<PathBuf, String>>,
}

/// A benchmark run, under way or finished. Dropping it stops the window.
pub struct Benchmark {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
}

impl Benchmark {
    /// Starts the benchmark window and its measurement. `wake` is called
    /// whenever the stage changes.
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            stage: Stage::Running {
                phase: None,
                since: Instant::now(),
                began: None,
            },
            saved: None,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_shared = shared.clone();
        let thread_stop = stop.clone();
        let spawned = std::thread::Builder::new()
            .name("benchmark".into())
            .spawn(move || {
                let outcome = run(&thread_shared, &thread_stop, &wake);
                let stage = match outcome {
                    Ok(report) => Stage::Done(Arc::new(report)),
                    Err(_) if thread_stop.load(Ordering::Relaxed) => Stage::Stopped,
                    Err(reason) => Stage::Failed(reason),
                };
                lock(&thread_shared).stage = stage;
                wake();
            });
        if let Err(error) = spawned {
            lock(&shared).stage = Stage::Failed(format!("Could not start the benchmark: {error}"));
        }
        Self { shared, stop }
    }

    /// A run that already ended with `report`, for the demo and tests.
    pub fn finished(report: Report) -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                stage: Stage::Done(Arc::new(report)),
                saved: None,
            })),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn stage(&self) -> Stage {
        lock(&self.shared).stage.clone()
    }

    pub fn running(&self) -> bool {
        matches!(self.stage(), Stage::Running { .. })
    }

    /// Closes the benchmark window; the run ends as stopped.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn saved(&self) -> Option<Result<PathBuf, String>> {
        lock(&self.shared).saved.clone()
    }

    /// Asks where to save the finished results and writes them there as
    /// JSON that `scripts/bench/compare.py` reads.
    pub fn save(&self, wake: impl Fn() + Send + 'static) {
        let Stage::Done(report) = self.stage() else {
            return;
        };
        let shared = self.shared.clone();
        let _ = std::thread::Builder::new()
            .name("benchmark-save".into())
            .spawn(move || {
                let Some(path) = rfd::FileDialog::new()
                    .set_title("Save benchmark results")
                    .set_file_name(report.file_name())
                    .add_filter("JSON", &["json"])
                    .save_file()
                else {
                    return;
                };
                let outcome = report
                    .to_json()
                    .and_then(|json| std::fs::write(&path, json).map_err(|error| error.to_string()))
                    .map(|()| path);
                lock(&shared).saved = Some(outcome);
                wake();
            });
    }
}

impl Drop for Benchmark {
    fn drop(&mut self) {
        self.stop();
    }
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A child process killed when dropped, and its marks folder removed.
struct Window {
    child: Child,
    folder: PathBuf,
}

impl Drop for Window {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn run(shared: &Mutex<Shared>, stop: &AtomicBool, wake: &dyn Fn()) -> Result<Report, String> {
    let program = std::env::current_exe()
        .map_err(|error| format!("Could not find Zeus's own program: {error}"))?;
    let folder = std::env::temp_dir().join(format!(
        "zeus-benchmark-{}-{}",
        std::process::id(),
        jiff::Timestamp::now().as_millisecond()
    ));
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Could not make a folder for the benchmark: {error}"))?;
    let marks_path = folder.join("marks.jsonl");
    std::fs::File::create(&marks_path)
        .map_err(|error| format!("Could not make a folder for the benchmark: {error}"))?;
    let child = Command::new(&program)
        .args([
            "--demo-tour",
            "--demo-tour-script",
            "bench",
            "--demo-size",
            WINDOW,
        ])
        .arg("--demo-tour-marks")
        .arg(&marks_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(format!("Could not open the benchmark window: {error}"));
        }
    };
    let mut window = Window { child, folder };
    let started = Instant::now();
    let started_at = jiff::Timestamp::now();
    let mut probe = probe::Probe::default();
    let mut recorder = Recorder::default();
    let mut marks = MarkReader::new(marks_path);
    let mut phase: Option<String> = None;
    let mut began = None;
    let mut last_tick = Instant::now();
    let mut outcome: Option<Result<(), String>> = None;
    while outcome.is_none() {
        if stop.load(Ordering::Relaxed) {
            return Err("Stopped.".into());
        }
        if started.elapsed() > TIMEOUT {
            return Err("The benchmark took too long and was stopped.".into());
        }
        for mark in marks.read_new() {
            match mark["event"].as_str().unwrap_or_default() {
                "start" => recorder.app = mark,
                "phase" => {
                    if let Some(previous) = &phase {
                        recorder.take(&mut probe, &window.child, started, previous, true);
                    }
                    let name = mark["phase"].as_str().unwrap_or_default().to_owned();
                    let now = Instant::now();
                    began.get_or_insert(now);
                    phase = Some(name.clone());
                    last_tick = now;
                    lock(shared).stage = Stage::Running {
                        phase: Some(name),
                        since: now,
                        began,
                    };
                    wake();
                }
                "check" => recorder.checks.push(mark),
                "frames" => {
                    let name = mark["phase"].as_str().unwrap_or_default().to_owned();
                    recorder.frames.insert(name, mark);
                }
                "end" => {
                    if let Some(previous) = phase.take() {
                        recorder.take(&mut probe, &window.child, started, &previous, true);
                    }
                    outcome = Some(Ok(()));
                }
                "failed" => {
                    outcome = Some(Err(
                        "The benchmark window could not find something it needed on screen.".into(),
                    ));
                }
                _ => {}
            }
        }
        if outcome.is_some() {
            break;
        }
        if let Ok(Some(_)) = window.child.try_wait() {
            return Err("The benchmark window closed before the test ended.".into());
        }
        if let Some(name) = &phase
            && last_tick.elapsed() >= INTERVAL
        {
            last_tick = Instant::now();
            recorder.take(&mut probe, &window.child, started, name, false);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    outcome.unwrap_or(Ok(()))?;
    // The window closes itself shortly after its last mark.
    let closing = Instant::now();
    while closing.elapsed() < CLOSE_WAIT && !stop.load(Ordering::Relaxed) {
        if let Ok(Some(_)) = window.child.try_wait() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    drop(window);
    let (os, hardware) = machine::describe();
    Ok(recorder.finish(os, hardware, started_at))
}

/// Reads the marks file's new complete lines.
struct MarkReader {
    path: PathBuf,
    offset: u64,
    partial: String,
}

impl MarkReader {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            partial: String::new(),
        }
    }

    fn read_new(&mut self) -> Vec<Value> {
        let mut text = String::new();
        if let Ok(mut file) = std::fs::File::open(&self.path) {
            use std::io::Seek;
            if file.seek(std::io::SeekFrom::Start(self.offset)).is_ok()
                && let Ok(read) = file.read_to_string(&mut text)
            {
                self.offset += read as u64;
            }
        }
        parse_lines(&mut self.partial, &text)
    }
}

/// Appends `text` to what was left of the last line and returns the
/// complete lines' JSON, keeping an unfinished last line for next time.
fn parse_lines(partial: &mut String, text: &str) -> Vec<Value> {
    partial.push_str(text);
    let Some(end) = partial.rfind('\n') else {
        return Vec::new();
    };
    let complete: String = partial.drain(..=end).collect();
    complete
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// One reading, as the results file keeps it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Seconds since the window was launched.
    pub t: f64,
    pub mb: f64,
    pub other_mb: f64,
    pub cpu_pct: Option<f64>,
    pub gpu_pct: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,
    pub end: Option<Point>,
    pub max_mb: Option<f64>,
    /// CPU time over the phase's length, in percent (100 is one core busy).
    pub cpu_pct: Option<f64>,
    /// The mean of the phase's GPU readings.
    pub gpu_pct: Option<f64>,
    pub samples: Vec<Point>,
}

/// A finished run, in the scripts' results format (schema 2), so a saved
/// file goes straight into `scripts/bench/compare.py`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub label: String,
    pub run: u32,
    pub status: String,
    pub reason: String,
    pub commit: String,
    pub changes: String,
    pub started: String,
    pub os: String,
    pub hardware: Hardware,
    pub renderer: String,
    /// The window's start mark: version, scale and window size.
    pub app: Value,
    pub metric: String,
    pub metric_other: String,
    pub gpu_metric: String,
    pub phases: Vec<Phase>,
    pub peak_mb: Option<f64>,
    pub os_peak_mb: Option<f64>,
    pub os_peak_metric: String,
    pub settled_mb: Option<f64>,
    /// The timed phases' frame times, by phase.
    pub frames: serde_json::Map<String, Value>,
    pub checks: Vec<Value>,
    /// Where it was run from: Settings, not the scripts.
    pub source: String,
}

/// Collects readings and marks into a [`Report`], the way
/// `bench_phases_json` in scripts/bench/lib.sh does.
#[derive(Default)]
struct Recorder {
    /// Phase, whether it ended it, time since launch, reading.
    readings: Vec<(String, bool, Duration, probe::Reading)>,
    app: Value,
    frames: serde_json::Map<String, Value>,
    checks: Vec<Value>,
}

fn mb(bytes: u64) -> f64 {
    (bytes as f64 / 1e5).round() / 10.0
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn percent(cpu_ms: f64, ms: f64) -> Option<f64> {
    (ms > 0.0).then(|| round1(cpu_ms / ms * 100.0))
}

impl Recorder {
    /// Reads the window now and records it under `phase`.
    fn take(
        &mut self,
        probe: &mut probe::Probe,
        child: &Child,
        started: Instant,
        phase: &str,
        end: bool,
    ) {
        let at = started.elapsed();
        if let Some(reading) = probe.read(child) {
            self.record(phase, end, at, reading);
        }
    }

    fn record(&mut self, phase: &str, end: bool, at: Duration, reading: probe::Reading) {
        self.readings.push((phase.to_owned(), end, at, reading));
    }

    fn finish(self, os: String, mut hardware: Hardware, started: jiff::Timestamp) -> Report {
        hardware.gpu = self.app["gpu"].as_str().unwrap_or("unknown").to_owned();
        let mut phases: Vec<Phase> = PHASES
            .iter()
            .map(|name| Phase {
                name: (*name).to_owned(),
                ..Phase::default()
            })
            .collect();
        let (mut last_ms, mut last_cpu, mut end_ms, mut end_cpu) = (0.0, 0.0, 0.0, 0.0);
        let mut gpu_sums: Vec<(f64, usize)> = vec![(0.0, 0); PHASES.len()];
        let mut peak = 0;
        let mut os_peak = 0;
        for (name, end, at, reading) in &self.readings {
            let ms = at.as_secs_f64() * 1000.0;
            let point = Point {
                t: round1(at.as_secs_f64()),
                mb: mb(reading.memory),
                other_mb: mb(reading.other),
                cpu_pct: percent(reading.cpu_ms - last_cpu, ms - last_ms),
                gpu_pct: reading.gpu,
            };
            (last_ms, last_cpu) = (ms, reading.cpu_ms);
            peak = peak.max(reading.memory);
            os_peak = os_peak.max(reading.peak);
            let Some(index) = PHASES.iter().position(|phase| phase == name) else {
                continue;
            };
            let phase = &mut phases[index];
            phase.max_mb = Some(phase.max_mb.map_or(point.mb, |max: f64| max.max(point.mb)));
            if let Some(gpu) = point.gpu_pct {
                gpu_sums[index].0 += gpu;
                gpu_sums[index].1 += 1;
            }
            if *end {
                phase.end = Some(point.clone());
                phase.cpu_pct = percent(reading.cpu_ms - end_cpu, ms - end_ms);
                (end_ms, end_cpu) = (ms, reading.cpu_ms);
            }
            phase.samples.push(point);
        }
        for (phase, (sum, count)) in phases.iter_mut().zip(gpu_sums) {
            phase.gpu_pct = (count > 0).then(|| round1(sum / count as f64));
        }
        let settled_mb = phases
            .iter()
            .find(|phase| phase.name == "settle-again")
            .and_then(|phase| phase.end.as_ref())
            .map(|end| end.mb);
        let version = env!("CARGO_PKG_VERSION");
        Report {
            schema: 2,
            label: format!("Zeus {version}"),
            run: 1,
            status: "complete".into(),
            reason: String::new(),
            commit: String::new(),
            changes: String::new(),
            started: started
                .round(jiff::Unit::Second)
                .unwrap_or(started)
                .to_string(),
            os,
            hardware,
            renderer: "OpenGL (glow)".into(),
            app: self.app,
            metric: probe::METRIC.into(),
            metric_other: probe::METRIC_OTHER.into(),
            gpu_metric: probe::GPU_METRIC.into(),
            phases,
            peak_mb: (peak > 0).then(|| mb(peak)),
            os_peak_mb: (os_peak > 0).then(|| mb(os_peak)),
            os_peak_metric: probe::PEAK_METRIC.into(),
            settled_mb,
            frames: self.frames,
            checks: self.checks,
            source: "settings".into(),
        }
    }
}

/// How one timed phase went, for the results card.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Smoothness {
    pub fps: Option<f64>,
    pub median_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub missed: Option<u64>,
    pub slowest_ms: Option<f64>,
}

impl Report {
    pub fn smoothness(&self, phase: &str) -> Smoothness {
        let Some(frames) = self.frames.get(phase) else {
            return Smoothness::default();
        };
        let interval = &frames["interval_ms"];
        Smoothness {
            fps: frames["fps"].as_f64(),
            median_ms: interval["p50"].as_f64(),
            p99_ms: interval["p99"].as_f64(),
            jitter_ms: frames["jitter_ms"].as_f64(),
            missed: frames["missed"].as_u64(),
            slowest_ms: interval["max"].as_f64(),
        }
    }

    pub fn phase(&self, name: &str) -> Option<&Phase> {
        self.phases.iter().find(|phase| phase.name == name)
    }

    /// Whether every check the window made passed.
    pub fn checks_passed(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check["ok"] == Value::Bool(true))
    }

    /// The window's display scale.
    pub fn scale(&self) -> Option<f64> {
        self.app["pixels_per_point"].as_f64()
    }

    /// The one line naming the computer.
    pub fn machine_line(&self) -> String {
        let hardware = &self.hardware;
        let mut parts = vec![self.label.clone(), self.os.clone()];
        if !hardware.chip.is_empty() {
            parts.push(hardware.chip.clone());
        }
        if hardware.cores > 0 {
            parts.push(format!("{} cores", hardware.cores));
        }
        if hardware.ram_gb > 0.0 {
            parts.push(format!("{:.1} GB", hardware.ram_gb));
        }
        if !hardware.gpu.is_empty() && hardware.gpu != "unknown" {
            parts.push(format!("GPU {}", hardware.gpu));
        }
        if let Some(scale) = self.scale() {
            parts.push(format!("scale {scale}"));
        }
        parts.join(" · ")
    }

    pub fn file_name(&self) -> String {
        let date = self.started.get(..10).unwrap_or("results");
        format!("zeus-benchmark-{}-{date}.json", env!("CARGO_PKG_VERSION"))
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|error| error.to_string())
    }

    /// The results as Markdown: exactly what the GitHub issue receives.
    pub fn markdown(&self) -> String {
        let value = value_of;
        let mut out = String::new();
        out.push_str(&self.machine_line());
        out.push_str("\n\n| Smoothness | Scrolling | Switching chats | Sticker picker |\n|---|---|---|---|\n");
        let rows: [(&str, fn(&Smoothness) -> String); 6] = [
            ("Frames a second", |s| value_of(s.fps)),
            ("Frame time, median (ms)", |s| value_of(s.median_ms)),
            ("Frame time, 99th percentile (ms)", |s| value_of(s.p99_ms)),
            ("Jitter (ms)", |s| value_of(s.jitter_ms)),
            ("Missed frames", |s| {
                s.missed.map_or("–".into(), |n| n.to_string())
            }),
            ("Slowest frame (ms)", |s| value_of(s.slowest_ms)),
        ];
        let smooth: Vec<Smoothness> = TIMED.iter().map(|phase| self.smoothness(phase)).collect();
        for (label, cell) in rows {
            out.push_str(&format!(
                "| {label} | {} | {} | {} |\n",
                cell(&smooth[0]),
                cell(&smooth[1]),
                cell(&smooth[2])
            ));
        }
        out.push_str(&format!(
            "\n| Phase | Memory at end (MB, {}) | CPU % | GPU % |\n|---|---|---|---|\n",
            self.metric
        ));
        for phase in &self.phases {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                phase_label(&phase.name),
                value(phase.end.as_ref().map(|end| end.mb)),
                value(phase.cpu_pct),
                value(phase.gpu_pct),
            ));
        }
        out.push_str(&format!(
            "\nPeak memory {} MB. CPU 100% = one core busy. GPU: {}. Checks: {}.\n",
            value(self.peak_mb),
            if self.gpu_metric == "whole_gpu" {
                "the whole GPU"
            } else {
                "this window's 3D engine"
            },
            if self.checks_passed() {
                "all passed"
            } else {
                "some failed"
            },
        ));
        out
    }

    /// A GitHub "new issue" link with the results filled in. Opening it
    /// sends nothing: the user reads the issue on GitHub and submits it there.
    pub fn issue_url(&self) -> String {
        let repository = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/');
        let title = format!("Benchmark: {} on {}", self.label, self.os);
        format!(
            "{repository}/issues/new?template=benchmark.yml&labels=benchmark&title={}&results={}",
            encode(&title),
            encode(&self.markdown())
        )
    }
}

#[cfg(any(test, feature = "demo"))]
impl Report {
    /// Made-up results from a made-up computer, for the demo's screenshots
    /// and tests.
    pub fn sample() -> Self {
        use serde_json::json;
        let reading = |memory_mb: u64, cpu_ms: f64, gpu: Option<f64>| probe::Reading {
            memory: memory_mb * 1_000_000,
            other: memory_mb * 1_200_000,
            peak: memory_mb * 1_300_000,
            cpu_ms,
            gpu,
        };
        let mut recorder = Recorder {
            app: json!({"event": "start", "pixels_per_point": 2.0, "gpu": "Test GPU"}),
            ..Recorder::default()
        };
        // settle: 0 to 10 s at half a core, then scroll to 20 s at more than one.
        let readings = [
            ("settle", false, 2, 100, 1000.0, None),
            ("settle", true, 10, 120, 5000.0, Some(10.0)),
            ("scroll", false, 12, 300, 7000.0, Some(40.0)),
            ("scroll", true, 20, 250, 15000.0, Some(60.0)),
        ];
        for (phase, end, seconds, memory, cpu, gpu) in readings {
            recorder.record(
                phase,
                end,
                Duration::from_secs(seconds),
                reading(memory, cpu, gpu),
            );
        }
        recorder.frames.insert(
            "scroll".into(),
            json!({"fps": 59.9, "missed": 2, "jitter_ms": 1.25,
                   "interval_ms": {"p50": 16.7, "p99": 33.4, "max": 50.0}}),
        );
        recorder
            .checks
            .push(json!({"event": "check", "name": "top", "ok": true}));
        let hardware = Hardware {
            chip: "Test CPU".into(),
            cores: 8,
            ram_gb: 16.0,
            ..Hardware::default()
        };
        recorder.finish("TestOS 1".into(), hardware, jiff::Timestamp::UNIX_EPOCH)
    }
}

fn value_of(value: Option<f64>) -> String {
    value.map_or("–".to_owned(), |value| format!("{value:.1}"))
}

/// Percent-encodes a query value: everything but unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Whether this build carries the benchmark's scenario.
pub const AVAILABLE: bool = cfg!(feature = "demo");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_become_phases_as_the_scripts_count_them() {
        let report = Report::sample();
        let settle = report.phase("settle").unwrap();
        assert_eq!(settle.end.as_ref().unwrap().mb, 120.0);
        assert_eq!(settle.max_mb, Some(120.0));
        // 5000 ms of CPU over 10 s.
        assert_eq!(settle.cpu_pct, Some(50.0));
        assert_eq!(settle.gpu_pct, Some(10.0));
        let scroll = report.phase("scroll").unwrap();
        assert_eq!(scroll.max_mb, Some(300.0));
        assert_eq!(scroll.cpu_pct, Some(100.0));
        assert_eq!(scroll.gpu_pct, Some(50.0));
        assert_eq!(scroll.samples.len(), 2);
        assert_eq!(report.peak_mb, Some(300.0));
        assert_eq!(report.os_peak_mb, Some(390.0));
        assert!(report.phase("minimized").unwrap().end.is_none());
        assert_eq!(report.hardware.gpu, "Test GPU");
        assert!(report.checks_passed());
        let smooth = report.smoothness("scroll");
        assert_eq!(smooth.fps, Some(59.9));
        assert_eq!(smooth.missed, Some(2));
        assert_eq!(report.smoothness("churn"), Smoothness::default());
    }

    #[test]
    fn the_saved_file_is_the_scripts_schema_and_reads_back() {
        let report = Report::sample();
        let json = report.to_json().unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["schema"], 2);
        assert_eq!(value["status"], "complete");
        assert_eq!(value["phases"].as_array().unwrap().len(), PHASES.len());
        assert_eq!(value["frames"]["scroll"]["fps"], 59.9);
        let back: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
        assert_eq!(
            report.file_name(),
            format!(
                "zeus-benchmark-{}-1970-01-01.json",
                env!("CARGO_PKG_VERSION")
            )
        );
    }

    #[test]
    fn the_issue_carries_only_the_summary() {
        let report = Report::sample();
        let markdown = report.markdown();
        assert!(
            markdown.contains("TestOS 1 · Test CPU · 8 cores · 16.0 GB · GPU Test GPU · scale 2")
        );
        assert!(markdown.contains("| Frames a second | 59.9 | – | – |"));
        assert!(markdown.contains("| Scrolling a chat full of photos | 250.0 | 100.0 | 50.0 |"));
        let url = report.issue_url();
        assert!(url.starts_with(env!("CARGO_PKG_REPOSITORY")));
        assert!(url.contains(
            "/issues/new?template=benchmark.yml&labels=benchmark&title=Benchmark%3A%20Zeus"
        ));
        assert!(!url[url.find('?').unwrap()..].contains(' '));
        // Short enough for a browser's address bar and GitHub.
        assert!(url.len() < 6000, "{}", url.len());
    }

    #[test]
    fn marks_are_read_a_whole_line_at_a_time() {
        let mut partial = String::new();
        assert!(parse_lines(&mut partial, "{\"event\":\"pha").is_empty());
        let lines = parse_lines(&mut partial, "se\"}\n{\"event\":\"end\"}\n{\"ev");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["event"], "end");
        assert_eq!(partial, "{\"ev");
        assert_eq!(encode("a b/ü"), "a%20b%2F%C3%BC");
    }

    #[cfg(feature = "demo")]
    #[test]
    fn the_phases_and_length_match_the_scenario() {
        assert_eq!(PHASES, crate::demo::tour::BENCH_PHASES);
        assert_eq!(LENGTH, crate::demo::tour::BENCH_LENGTH);
    }
}
