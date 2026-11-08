//! The chat's springy overscroll, as on Android: a touchpad swipe that runs
//! on past the newest message or the oldest one pulls the rows beyond their
//! place against a growing resistance, and when the fingers lift (and the
//! touchpad's glide dies down) they spring back with a slight bounce.
//!
//! Only precise scrolling stretches the list: a touchpad or a touch screen
//! reports its distance in points, a mouse wheel in lines and never
//! stretches. The stretch is drawn by shifting the rows, as the scroll
//! springs do, so it adds no layout work and the offset itself stays inside
//! the content.

use crate::motion::Spring;
use egui::Rect;

/// The release: a shade over a third of a second, bouncing a little, in
/// step with the app's other springs.
pub(crate) const SPRING: Spring = Spring::new(0.45, 0.2);

/// The furthest the rows stretch, in points.
pub(crate) const MAX: f32 = 96.0;

/// How much of a swipe the first stretch takes; it falls to nothing at
/// [`MAX`].
const PULL: f32 = 0.55;

/// How long after the last precise wheel event a swipe counts as live,
/// for the frames between events.
const LIVE: f64 = 0.35;

/// Nearer rest than this, and slower, the stretch is over.
const REST_POSITION: f32 = 0.25;
const REST_SPEED: f32 = 4.0;

/// Whether a wheel event comes from a touchpad (or a free-spinning wheel)
/// rather than a notched wheel. Linux and macOS report a touchpad in points.
/// Windows reports everything in lines, a touchpad as fractions of a notch
/// (a notch is a whole number of lines), so a fractional step counts too.
pub(crate) fn is_precise(unit: egui::MouseWheelUnit, delta: f32) -> bool {
    match unit {
        egui::MouseWheelUnit::Point => true,
        egui::MouseWheelUnit::Line => {
            let part = delta.abs().fract();
            delta != 0.0 && part > 0.01 && part < 0.99
        }
        egui::MouseWheelUnit::Page => false,
    }
}

/// The wheel input over `list` this frame. A wheel notch is not precise, so
/// it never stretches the list.
pub(crate) fn read_wheel(input: &egui::InputState, list: Rect) -> Wheel {
    if !input
        .pointer
        .hover_pos()
        .is_some_and(|pos| list.contains(pos))
    {
        return Wheel::default();
    }
    Wheel {
        precise: input.raw.events.iter().any(|event| {
            matches!(event, egui::Event::MouseWheel { unit, delta, .. } if is_precise(*unit, delta.y))
        }),
        delta: input.smooth_scroll_delta.y,
    }
}

/// Which ends of the list the view is at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edges {
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    /// The view at offset `offset` of `content` points, `view` tall.
    pub fn of(offset: f32, view: f32, content: f32) -> Self {
        Self {
            top: offset <= 0.5,
            bottom: offset + view >= content - 1.0,
        }
    }
}

/// What the reader's scrolling brought this frame.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Wheel {
    /// A precise (touchpad or touch) event arrived this frame.
    pub precise: bool,
    /// The frame's smoothed scroll, positive when the content moves down.
    pub delta: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Overscroll {
    /// How far below its place the rows are drawn; negative above it.
    position: f32,
    velocity: f32,
    time: Option<f64>,
    live_until: f64,
    edges: Edges,
}

impl Overscroll {
    fn id(chat: &str) -> egui::Id {
        egui::Id::new(("overscroll", chat))
    }

    pub fn load(ctx: &egui::Context, chat: &str) -> Self {
        ctx.data(|data| data.get_temp::<Self>(Self::id(chat)))
            .unwrap_or_default()
    }

    pub fn store(self, ctx: &egui::Context, chat: &str) {
        ctx.data_mut(|data| {
            if self.position == 0.0 && self.velocity == 0.0 && self.edges == Edges::default() {
                data.remove::<Self>(Self::id(chat));
            } else {
                data.insert_temp(Self::id(chat), self);
            }
        });
    }

    /// The shift to add to every row.
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Whether the rows are stretched or still moving.
    pub fn moving(&self) -> bool {
        self.position.abs() > REST_POSITION || self.velocity.abs() > REST_SPEED
    }

    /// Lets the rows go to their places at once.
    pub fn clear(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
    }

    /// Records where the view ended this frame.
    pub fn set_edges(&mut self, edges: Edges) {
        self.edges = edges;
    }

    /// Brings the stretch to `now`. A pass egui redoes has the same time
    /// and changes nothing.
    pub fn advance(&mut self, now: f64, wheel: Wheel) {
        let dt = match self.time {
            Some(then) if now <= then => return,
            Some(then) => (now - then).min(0.1),
            None => 0.0,
        };
        self.time = Some(now);
        if wheel.precise {
            self.live_until = now + LIVE;
        }
        let outward =
            (wheel.delta > 0.0 && self.edges.top) || (wheel.delta < 0.0 && self.edges.bottom);
        let stretched_same_way = self.position * wheel.delta > 0.0;
        if self.live_until > now && wheel.delta != 0.0 && (outward || stretched_same_way) {
            let before = self.position;
            let room = (1.0 - self.position.abs() / MAX).max(0.0);
            self.position = (self.position + wheel.delta * PULL * room * room).clamp(-MAX, MAX);
            self.velocity = if dt > 0.0 {
                ((self.position - before) / dt as f32).clamp(-4000.0, 4000.0)
            } else {
                0.0
            };
            return;
        }
        (self.position, self.velocity) = SPRING.advance(self.position, self.velocity, dt);
        if !self.moving() {
            self.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top() -> Overscroll {
        let mut over = Overscroll::default();
        over.set_edges(Edges {
            top: true,
            bottom: false,
        });
        over
    }

    fn swipe(delta: f32) -> Wheel {
        Wheel {
            precise: true,
            delta,
        }
    }

    #[test]
    fn a_swipe_past_the_top_stretches_with_growing_resistance() {
        let mut over = top();
        let mut last = 0.0;
        let mut steps = Vec::new();
        for frame in 0..30 {
            over.advance(frame as f64 / 60.0, swipe(20.0));
            steps.push(over.position() - last);
            last = over.position();
        }
        assert!(over.position() > 0.0 && over.position() <= MAX);
        assert!(
            steps[2] > steps[25],
            "each step gives less than the one before"
        );
    }

    #[test]
    fn it_springs_back_and_comes_to_rest() {
        let mut over = top();
        for frame in 0..10 {
            over.advance(frame as f64 / 60.0, swipe(20.0));
        }
        assert!(over.position() > 10.0);
        // The glide ends: no more events, past the live window.
        for frame in 10..240 {
            over.advance(frame as f64 / 60.0, Wheel::default());
        }
        assert_eq!(over.position(), 0.0);
        assert!(!over.moving());
    }

    #[test]
    fn a_mouse_wheel_never_stretches() {
        let mut over = top();
        for frame in 0..10 {
            over.advance(
                frame as f64 / 60.0,
                Wheel {
                    precise: false,
                    delta: 20.0,
                },
            );
        }
        assert_eq!(over.position(), 0.0);
    }

    #[test]
    fn it_stretches_up_at_the_bottom_and_not_in_the_middle() {
        let mut over = Overscroll::default();
        over.set_edges(Edges::of(500.0, 400.0, 900.0));
        for frame in 0..10 {
            over.advance(frame as f64 / 60.0, swipe(-20.0));
        }
        assert!(over.position() < 0.0);
        let mut mid = Overscroll::default();
        mid.set_edges(Edges::of(100.0, 400.0, 900.0));
        for frame in 0..10 {
            mid.advance(frame as f64 / 60.0, swipe(-20.0));
        }
        assert_eq!(mid.position(), 0.0);
    }

    #[test]
    fn windows_touchpad_lines_are_told_from_wheel_notches() {
        use egui::MouseWheelUnit::{Line, Page, Point};
        assert!(is_precise(Point, 3.0));
        assert!(is_precise(Line, 0.2));
        assert!(is_precise(Line, -1.4));
        assert!(!is_precise(Line, 1.0));
        assert!(!is_precise(Line, -3.0));
        assert!(!is_precise(Page, 0.5));
    }

    #[test]
    fn a_redone_pass_changes_nothing() {
        let mut over = top();
        over.advance(1.0, swipe(20.0));
        let once = over.position();
        over.advance(1.0, swipe(20.0));
        assert_eq!(over.position(), once);
    }
}
