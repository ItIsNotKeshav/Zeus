//! Zeus's own parts of the recording strip.

use super::*;

/// How far the strip starts to the right of its place, in points.
pub(super) const RECORDING_SLIDE: f32 = 12.0;
/// Seconds one pass of the recording light's breath takes.
pub(super) const RECORDING_BREATH: f32 = 1.2;
/// Seconds the level circle takes to follow the voice (the time constant).
pub(super) const RECORDING_LEVEL_TAU: f32 = 0.08;
/// Seconds between two recorded levels.
pub(super) const RECORDING_LEVEL_STEP: f32 = 0.05;

/// The recording light's size and opacity at `time` seconds: a sine loop
/// between 0.85 and 1.0 of its size and 0.6 and 1.0 of its opacity.
pub(super) fn recording_breath(time: f32) -> (f32, f32) {
    let wave = 0.5 + 0.5 * (std::f32::consts::TAU * time / RECORDING_BREATH).sin();
    (0.85 + 0.15 * wave, 0.6 + 0.4 * wave)
}

/// Moves `value` toward `target` over `dt` seconds with an exponential ease
/// of time constant `tau`.
pub(super) fn ease_level(value: f32, target: f32, dt: f32, tau: f32) -> f32 {
    value + (target - value) * (1.0 - (-dt.max(0.0) / tau).exp())
}

/// The strip's motion state: how far it has slid in (0..1), the smoothed
/// loudness (0..1) and the share of the newest bar's slide still to go
/// (1..0). The first frame of a recording starts the entry and taps once.
pub(super) fn recording_motion(ctx: &egui::Context, levels: &[f32]) -> (f32, f32, f32) {
    #[derive(Clone, Copy)]
    struct State {
        started: f64,
        seen: f64,
        loudness: f32,
        count: usize,
        changed: f64,
    }
    let id = egui::Id::new("recording-motion");
    let now = ctx.input(|input| input.time);
    let still = crate::motion::reduced(ctx);
    let previous: Option<State> = ctx.data(|data| data.get_temp(id));
    // A gap of more than half a second means a new recording began.
    let mut state = match previous {
        Some(state) if now - state.seen < 0.5 => state,
        _ => {
            crate::haptics::tap(crate::haptics::Feel::Generic);
            State {
                started: now,
                seen: now,
                loudness: 0.0,
                count: levels.len(),
                changed: now,
            }
        }
    };
    let dt = (now - state.seen) as f32;
    let target = levels.last().map_or(0.0, |level| (level * 4.0).min(1.0));
    state.loudness = if still {
        target
    } else {
        ease_level(state.loudness, target, dt, RECORDING_LEVEL_TAU)
    };
    if levels.len() != state.count {
        state.count = levels.len();
        state.changed = now;
    }
    state.seen = now;
    let entry = if still {
        1.0
    } else {
        theme::ease_out((now - state.started) as f32 / theme::MOTION_PANEL)
    };
    let slide = if still {
        0.0
    } else {
        1.0 - theme::ease_out((now - state.changed) as f32 / RECORDING_LEVEL_STEP)
    };
    ctx.data_mut(|data| data.insert_temp(id, state));
    (entry, state.loudness, slide)
}
