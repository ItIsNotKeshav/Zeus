//! The benchmark's marks: one JSON object a line, written as they happen, so
//! a sampler outside the app knows when each phase ends. They carry phase
//! names, times on the tour's clock, checks on what the screen showed, and the
//! timed phases' frame times; never message text, names or numbers.

use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

/// How long the benchmark keeps its window after the last mark.
pub(super) const LINGER: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(super) struct Marks {
    file: Option<std::fs::File>,
    /// Every line, also kept for tests.
    pub(super) lines: Vec<Value>,
    started: bool,
    /// The phase under way.
    phase: Option<&'static str>,
    /// The phases whose frames are timed.
    timed: &'static [&'static str],
    last_frame: Option<Instant>,
    frame_began: Option<Instant>,
    /// Milliseconds between the starts of consecutive frames, when the
    /// first asked for the second at once.
    intervals: Vec<f32>,
    /// Milliseconds from a frame's input to the end of its interface pass.
    work: Vec<f32>,
    /// Milliseconds of CPU eframe spent on a frame: the interface pass,
    /// tessellation and painting, without the wait to present it.
    cost: Vec<f32>,
    /// The graphics chip, as its OpenGL driver names it.
    gpu: Option<String>,
    /// Milliseconds from the last key of a search query to the frame that
    /// first showed its results, one per query.
    results: Vec<f32>,
}

impl Marks {
    pub(super) fn new(path: Option<PathBuf>, timed: &'static [&'static str]) -> Self {
        let file = path.and_then(|path| match std::fs::File::create(&path) {
            Ok(file) => Some(file),
            Err(error) => {
                log::error!("could not write the tour marks: {error}");
                None
            }
        });
        Self {
            file,
            timed,
            ..Self::default()
        }
    }

    fn write(&mut self, line: Value) {
        if let Some(file) = &mut self.file {
            // A line at a time, flushed, so a reader never sees half of one.
            let _ = writeln!(file, "{line}").and_then(|()| file.flush());
        }
        self.lines.push(line);
    }

    /// Names the graphics chip in the start mark.
    pub(super) fn set_gpu(&mut self, name: String) {
        self.gpu = Some(name);
    }

    pub(super) fn knows_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    pub(super) fn started(&self) -> bool {
        self.started
    }

    pub(super) fn start(
        &mut self,
        script: super::Script,
        pixels_per_point: f32,
        size: Option<egui::Vec2>,
    ) {
        self.started = true;
        let script = super::Script::NAMES
            .into_iter()
            .find(|name| super::Script::from_name(name) == Some(script))
            .unwrap_or_default();
        self.write(json!({
            "event": "start",
            "script": script,
            "version": env!("CARGO_PKG_VERSION"),
            "pixels_per_point": pixels_per_point,
            "window": size.map(|size| [size.x.round(), size.y.round()]),
            "gpu": self.gpu,
        }));
    }

    /// Ends the phase under way and begins `name`.
    pub(super) fn phase(&mut self, at: f32, name: &'static str) {
        self.close(at);
        self.phase = Some(name);
        self.write(json!({ "event": "phase", "phase": name, "t": seconds(at) }));
    }

    pub(super) fn end(&mut self, at: f32) {
        self.close(at);
        self.phase = None;
        self.write(json!({ "event": "end", "t": seconds(at) }));
    }

    pub(super) fn failed(&mut self, at: f32, step: usize) {
        self.write(json!({ "event": "failed", "step": step, "t": seconds(at) }));
    }

    pub(super) fn check(&mut self, at: f32, name: &str, ok: bool) {
        self.write(json!({ "event": "check", "name": name, "ok": ok, "t": seconds(at) }));
    }

    /// Writes the frame times of the phase that ends, if it was timed.
    fn close(&mut self, at: f32) {
        if !self.timing() {
            return;
        }
        let phase = self.phase.unwrap_or_default();
        let line = json!({
            "event": "frames",
            "phase": phase,
            "t": seconds(at),
            "frames": self.intervals.len(),
            "interval_ms": summary(&self.intervals),
            "work_ms": summary(&self.work),
            "cpu_ms": summary(&self.cost),
            "fps": fps(&self.intervals),
            "missed": missed(&self.intervals),
            "jitter_ms": jitter(&self.intervals),
            "over_33ms": self.intervals.iter().filter(|&&ms| ms > 33.4).count(),
            "first_results_ms": median(&self.results),
        });
        self.write(line);
        self.intervals.clear();
        self.work.clear();
        self.cost.clear();
        self.results.clear();
        self.last_frame = None;
        self.frame_began = None;
    }

    /// Whether this frame's times are recorded.
    pub(super) fn timing(&self) -> bool {
        self.phase.is_some_and(|phase| self.timed.contains(&phase))
    }

    pub(super) fn phase_name(&self) -> Option<&'static str> {
        self.phase
    }

    /// A frame begins. `continuous` says the frame before asked for this one
    /// at once; after a pause between gestures there is no interval to time.
    pub(super) fn frame_began(&mut self, now: Instant, continuous: bool) {
        if !self.timing() {
            return;
        }
        if let Some(last) = self.last_frame.filter(|_| continuous) {
            self.intervals.push(milliseconds(now - last));
        }
        self.last_frame = Some(now);
        self.frame_began = Some(now);
    }

    /// A search query's results first showed `milliseconds` after its last key.
    pub(super) fn results_shown(&mut self, milliseconds: f32) {
        if self.timing() {
            self.results.push(milliseconds);
        }
    }

    pub(super) fn frame_cost(&mut self, seconds: Option<f32>) {
        if let Some(seconds) = seconds.filter(|_| self.timing()) {
            self.cost.push(seconds * 1000.0);
        }
    }

    pub(super) fn frame_done(&mut self, now: Instant) {
        if let Some(began) = self.frame_began.take() {
            self.work.push(milliseconds(now - began));
        }
    }
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// The middle of the times, or null for none.
fn median(times: &[f32]) -> Value {
    if times.is_empty() {
        return Value::Null;
    }
    let mut sorted = times.to_vec();
    sorted.sort_by(f32::total_cmp);
    json!(round(sorted[sorted.len() / 2]))
}

/// Frames a second, from the mean interval.
fn fps(intervals: &[f32]) -> Value {
    if intervals.is_empty() {
        return Value::Null;
    }
    let mean = intervals.iter().sum::<f32>() / intervals.len() as f32;
    json!(round(1000.0 / mean.max(0.01)))
}

/// Frames the display showed twice because the next one was late: an
/// interval of about twice the usual one missed one, three times two, and so
/// on. The usual interval is the median, the display's own rate when the app
/// keeps up with it.
pub(super) fn missed(intervals: &[f32]) -> usize {
    let mut sorted = intervals.to_vec();
    sorted.sort_by(f32::total_cmp);
    let Some(&usual) = sorted.get(sorted.len().saturating_sub(1) / 2) else {
        return 0;
    };
    if usual <= 0.0 {
        return 0;
    }
    intervals
        .iter()
        .map(|&ms| ((ms / usual).round() as usize).saturating_sub(1))
        .sum()
}

/// How unevenly frames arrive: the standard deviation of the intervals.
fn jitter(intervals: &[f32]) -> Value {
    if intervals.len() < 2 {
        return Value::Null;
    }
    let mean = intervals.iter().sum::<f32>() / intervals.len() as f32;
    let variance = intervals
        .iter()
        .map(|&ms| (ms - mean) * (ms - mean))
        .sum::<f32>()
        / intervals.len() as f32;
    json!(round(variance.sqrt()))
}

fn seconds(at: f32) -> f64 {
    (f64::from(at) * 100.0).round() / 100.0
}

fn milliseconds(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

/// Count, mean, median, 95th and 99th percentile and maximum, rounded to
/// hundredths of a millisecond. Percentiles are nearest-rank.
pub(super) fn summary(values: &[f32]) -> Value {
    if values.is_empty() {
        return json!({ "count": 0 });
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let rank = |percent: f32| {
        let index = ((percent / 100.0) * sorted.len() as f32).ceil() as usize;
        sorted[index.clamp(1, sorted.len()) - 1]
    };
    let mean = sorted.iter().sum::<f32>() / sorted.len() as f32;
    json!({
        "count": sorted.len(),
        "mean": round(mean),
        "p50": round(rank(50.0)),
        "p95": round(rank(95.0)),
        "p99": round(rank(99.0)),
        "max": round(sorted[sorted.len() - 1]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_takes_nearest_rank_percentiles() {
        let values: Vec<f32> = (1..=100).map(|value| value as f32).collect();
        let summary = summary(&values);
        assert_eq!(summary["count"], 100);
        assert_eq!(summary["p50"], 50.0);
        assert_eq!(summary["p95"], 95.0);
        assert_eq!(summary["p99"], 99.0);
        assert_eq!(summary["max"], 100.0);
        assert_eq!(summary["mean"], 50.5);
        assert_eq!(super::summary(&[])["count"], 0);
        assert_eq!(super::summary(&[7.0])["p99"], 7.0);
    }

    #[test]
    fn missed_frames_count_the_intervals_that_spanned_several() {
        assert_eq!(missed(&[16.7, 16.6, 16.8, 33.3, 16.7, 50.1, 16.7]), 3);
        assert_eq!(missed(&[16.7; 10]), 0);
        assert_eq!(missed(&[]), 0);
        assert_eq!(jitter(&[10.0, 20.0]), json!(5.0));
        assert_eq!(fps(&[20.0, 20.0]), json!(50.0));
        assert_eq!(fps(&[]), Value::Null);
    }

    #[test]
    fn only_the_timed_phases_record_frames_and_their_ends_write_them() {
        let mut marks = Marks::new(None, &["scroll"]);
        let t0 = Instant::now();
        marks.phase(0.0, "settle");
        marks.frame_began(t0, true);
        marks.frame_done(t0 + Duration::from_millis(3));
        assert!(marks.intervals.is_empty() && marks.work.is_empty());
        marks.phase(1.0, "scroll");
        for frame in 0..4u64 {
            let at = t0 + Duration::from_millis(16 * frame);
            marks.frame_began(at, true);
            marks.frame_done(at + Duration::from_millis(2));
        }
        // After a pause the next frame times its work but no interval.
        let later = t0 + Duration::from_millis(900);
        marks.frame_began(later, false);
        marks.frame_done(later + Duration::from_millis(2));
        marks.phase(2.0, "churn");
        marks.end(3.0);
        let events: Vec<_> = marks
            .lines
            .iter()
            .map(|line| line["event"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(events, ["phase", "phase", "frames", "phase", "end"]);
        let frames = &marks.lines[2];
        assert_eq!(frames["phase"], "scroll");
        assert_eq!(frames["frames"], 3);
        assert_eq!(frames["interval_ms"]["p50"], 16.0);
        assert_eq!(frames["work_ms"]["count"], 5);
        assert_eq!(frames["over_33ms"], 0);
        assert_eq!(frames["missed"], 0);
        assert_eq!(frames["fps"], 62.5);
    }
}
