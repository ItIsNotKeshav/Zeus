//! The PDF viewer's memory benchmark: a report sent in a chat, opened from
//! its card, read from the first page to the last, zoomed deep and scrolled,
//! searched, left open, and closed, on a fixed timetable of about 80
//! seconds. `scripts/bench/run.sh --script pdf` launches it and measures the
//! process from outside, as it does the main benchmark.
//!
//! Every phase but the first starts from what the one before left, so the
//! memory at a phase's end says what the viewer holds there: `open` its
//! first pages, `read` the pages it keeps while scrolling, `zoom` the deepest
//! zoom's pages, `search` the text it reads, `idle` what an open document
//! settles to, and `closed` what is left once it has gone.

use super::{Cue, Gesture, PdfState, Target};
use crate::{app::App, model::Content};
use egui::{Key as K, Modifiers};
use std::time::Duration;

/// The document's message in the first sample chat.
pub(super) const MESSAGE: &str = "bench-pdf";
const FILE_NAME: &str = "Workshop report.pdf";
/// Pages in the document. At the fit zoom of a 1280 by 800 window a page is
/// about 1,180 points tall, so the read covers some 47,000 points.
pub(super) const PAGES: usize = 40;
/// Searched for once the document has been read; it is on every page.
pub(super) const QUERY: &str = "pump";

/// Phases in order, each ended by the next. `scripts/bench` names them the same.
pub const PHASES: [&str; 7] = ["settle", "open", "read", "zoom", "search", "idle", "closed"];
/// The phases whose frame times are recorded.
pub(super) const TIMED: [&str; 3] = ["read", "zoom", "search"];

/// The chat with the document's card, its preview rendered.
const SETTLE: f32 = 6.0;
/// From the click on the card until its first pages are up.
const OPEN: f32 = 6.0;
const READ_START: f32 = SETTLE + OPEN;
/// Seconds the pages scroll down, from the first to the last.
const READ: f32 = 14.0;
/// Points a second the read moves: the whole document in `READ`.
const READ_SPEED: f32 = 3_400.0;
/// Seconds after the read for the last pages to render.
const READ_REST: f32 = 2.0;
const ZOOM_START: f32 = READ_START + READ + READ_REST;
/// Zoom steps in, twice (to about 2x, then to the 5x limit), each followed
/// by a pause to render and a scroll at the zoom; then back to fit.
const ZOOM_STEPS: [usize; 2] = [4, 5];
const ZOOM_PAUSE: f32 = 1.5;
const ZOOM_SCROLL: f32 = 4.0;
/// Points a second a zoomed scroll moves.
const ZOOM_SPEED: f32 = 1_200.0;
const ZOOM_ROUND: f32 = 2.5 + ZOOM_PAUSE + ZOOM_SCROLL + 0.5;
const ZOOM_END: f32 = ZOOM_START + ZOOM_STEPS.len() as f32 * ZOOM_ROUND + 2.0;
/// Matches stepped through, one a second.
const SEARCH_STEPS: usize = 4;
const SEARCH_END: f32 = ZOOM_END + 2.0 + SEARCH_STEPS as f32 + 2.0;
/// Long enough for the render thread to hand back what it freed.
const IDLE: f32 = 10.0;
const CLOSE: f32 = SEARCH_END + IDLE;
/// Long enough for the closing card to land and its pages to go.
const CLOSED: f32 = 12.0;

/// Length of the benchmark, excluding its start delay.
pub const DURATION: Duration = Duration::from_secs((CLOSE + CLOSED) as u64);

/// Seconds between the wheel's events, a frame at 60 a second.
const WHEEL_STEP: f32 = 0.016;
/// Over the pages, inside the viewer's card in a 1280 by 800 window.
const POINT: (f32, f32) = (640.0, 460.0);

/// Points a second, in the unit the tour's wheel events carry: only Linux
/// scales a touchpad's points up, so there they carry that much less.
fn wheel_speed(speed: f32) -> f32 {
    if cfg!(target_os = "linux") {
        speed / fastframe_scroll::TOUCHPAD_SCALE
    } else {
        speed
    }
}

/// The first sample chat with the benchmark's document as its newest
/// message, in the Dark theme, at the chat's end.
pub fn prepare(app: &mut App) {
    super::common_setup(app);
    let chat = app.open_chat.clone().expect("the tour opens a chat");
    let bytes = crate::pdf::bench_document(PAGES);
    let dir = app.dirs.media_cache_dir().join("bench");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(FILE_NAME);
    if let Err(error) = std::fs::write(&path, &bytes) {
        log::error!("pdf benchmark: could not write its document: {error}");
        return;
    }
    let mut media = super::super::media("application/pdf", bytes.len() as u64, None, None);
    media.path = Some(path);
    let row = super::super::message(
        &chat,
        MESSAGE,
        false,
        crate::util::now(),
        Content::Document {
            media,
            file_name: FILE_NAME.into(),
            caption: None,
            pages: Some(PAGES as u32),
        },
    );
    if let Some(conversation) = app.conversations.get_mut(&chat) {
        conversation.messages.push(row);
    }
    app.scroll_to_bottom = true;
    app.video.silence();
}

/// Wheel cues over the pages, `speed` points a second (negative scrolls
/// down, towards the end), from `from` for `seconds`.
fn wheel(add: &mut impl FnMut(f32, Gesture), from: f32, seconds: f32, speed: f32) {
    let steps = (seconds / WHEEL_STEP) as usize;
    let delta = wheel_speed(speed) * WHEEL_STEP;
    for step in 0..steps {
        add(from + step as f32 * WHEEL_STEP, Gesture::Wheel(delta));
    }
}

pub(super) fn script() -> Vec<Cue> {
    use Gesture::*;
    use Target::*;
    let mut cues = Vec::new();
    let mut add = |at: f32, gesture| cues.push(Cue { at, gesture });
    let left = egui::PointerButton::Primary;
    let zoom_in = || Key(K::Equals, super::command(), "Ctrl + = · Zoom in");
    add(0.0, Phase(PHASES[0]));
    add(
        SETTLE - 1.0,
        Expect(Bubble(MESSAGE), "settle: the document's card is shown"),
    );
    add(SETTLE, Phase(PHASES[1]));
    add(SETTLE + 0.1, Move(Bubble(MESSAGE)));
    add(SETTLE + 0.5, Click(left));
    add(
        READ_START - 0.5,
        Expect(Pdf(PdfState::Rendered(0)), "open: the first page is drawn"),
    );
    add(READ_START, Phase(PHASES[2]));
    add(READ_START + 0.05, Move(Point(POINT.0, POINT.1)));
    wheel(&mut add, READ_START + 0.4, READ, -READ_SPEED);
    add(
        ZOOM_START - 0.2,
        Expect(
            Pdf(PdfState::Rendered(PAGES - 1)),
            "read: the last page is drawn",
        ),
    );
    add(ZOOM_START, Phase(PHASES[3]));
    let mut at = ZOOM_START;
    for steps in ZOOM_STEPS {
        for step in 0..steps {
            add(at + 0.2 + step as f32 * 0.4, zoom_in());
        }
        let scroll = at + 2.5 + ZOOM_PAUSE;
        // Back up the document, the way the read came.
        wheel(&mut add, scroll, ZOOM_SCROLL, ZOOM_SPEED);
        at += ZOOM_ROUND;
    }
    add(at, Key(K::Num0, super::command(), "Ctrl + 0 · Fit"));
    add(ZOOM_END, Phase(PHASES[4]));
    add(
        ZOOM_END + 0.1,
        Key(K::F, super::command(), "Ctrl + F · Find in document"),
    );
    for (index, character) in QUERY.chars().enumerate() {
        add(ZOOM_END + 0.5 + index as f32 * 0.12, Text(character));
    }
    for step in 0..SEARCH_STEPS {
        add(
            ZOOM_END + 2.0 + step as f32,
            Key(K::Enter, Modifiers::NONE, "Enter · Next match"),
        );
    }
    add(
        SEARCH_END - 1.5,
        Expect(Pdf(PdfState::Hits), "search: the matches are found"),
    );
    add(
        SEARCH_END - 1.0,
        Key(K::Escape, Modifiers::NONE, "Esc · Close the search"),
    );
    add(SEARCH_END, Phase(PHASES[5]));
    add(CLOSE, Phase(PHASES[6]));
    add(
        CLOSE + 0.1,
        Key(K::Escape, Modifiers::NONE, "Esc · Close the document"),
    );
    add(
        DURATION.as_secs_f32() - 1.0,
        Expect(Pdf(PdfState::Closed), "closed: the viewer has gone"),
    );
    add(DURATION.as_secs_f32(), End);
    cues.sort_by(|a, b| a.at.total_cmp(&b.at));
    cues
}
