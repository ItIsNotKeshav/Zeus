//! The chat's rubber band, as on a phone: a touchpad swipe that runs on
//! past the newest message or the oldest one pulls the rows with the
//! finger, resisting more the further they go, and holds there while the
//! finger stays down. When the finger lifts they snap back. The glide or
//! momentum that follows the lift does not keep pulling.
//!
//! Only precise scrolling stretches the list: a touchpad or a touch screen
//! reports its distance in points, a mouse wheel in lines and never
//! stretches. The stretch is drawn by shifting the rows, as the scroll
//! springs do, so it adds no layout work and the offset itself stays inside
//! the content.

use crate::motion::Spring;
use egui::Rect;

/// How quickly the rows return once the finger lifts. Short, with a little
/// give, so the band snaps home instead of easing for a third of a second.
pub(crate) const SPRING: Spring = Spring::new(0.22, 0.12);

/// The furthest the rows stretch, in points. The band approaches this and
/// does not reach it.
pub(crate) const MAX: f32 = 56.0;

/// How freely the first point of a pull moves. It falls off as the rows
/// near [`MAX`].
const PULL: f32 = 0.55;

/// How long a platform that never says the finger lifted may pause before
/// the band lets go. A frame or two between events is not a lift.
const HOLD: f64 = 0.15;

/// A press this soon after a lift is the momentum phase starting, not a
/// new finger.
const COAST_START: f64 = 0.12;

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
    let mut precise = false;
    let mut began = false;
    let mut lifted = false;
    for event in &input.raw.events {
        let egui::Event::MouseWheel {
            unit, delta, phase, ..
        } = event
        else {
            continue;
        };
        if is_precise(*unit, delta.y) {
            precise = true;
        }
        match phase {
            egui::TouchPhase::Start => {
                // A start after the finger lifted, in the same frame, is the
                // momentum phase beginning.
                if !lifted {
                    began = true;
                }
            }
            egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                lifted = true;
                began = false;
            }
            egui::TouchPhase::Move => {}
        }
    }
    Wheel {
        precise,
        delta: input.smooth_scroll_delta.y,
        began,
        lifted,
    }
}

/// Where a pull of `raw` points lands, resisting more as it nears [`MAX`].
fn rubber(raw: f32) -> f32 {
    let x = raw.abs();
    if x == 0.0 {
        return 0.0;
    }
    let band = (1.0 - 1.0 / (x * PULL / MAX + 1.0)) * MAX;
    band.copysign(raw)
}

/// The pull that [`rubber`] would draw at `position`, so a finger that
/// lands again mid-snap continues from what is on screen.
fn inverse_rubber(position: f32) -> f32 {
    let y = position.abs().min(MAX * 0.98);
    if y < 0.01 {
        return 0.0;
    }
    let raw = MAX / PULL * (1.0 / (1.0 - y / MAX) - 1.0);
    raw.copysign(position)
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
    /// A finger pressed this frame, and not the momentum that follows a lift.
    pub began: bool,
    /// The finger lifted this frame.
    pub lifted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Overscroll {
    /// How far below its place the rows are drawn; negative above it.
    position: f32,
    velocity: f32,
    /// The finger's travel past the edge, before the band resists it.
    raw: f32,
    time: Option<f64>,
    /// The finger is down and the band follows it. Frames with no delta hold.
    tracking: bool,
    /// The platform said the finger went down, so a pause is not a lift.
    held: bool,
    /// The finger has lifted. Later precise deltas are momentum.
    coasting: bool,
    /// When the gesture last moved, for a platform that never says it lifted.
    moved_at: f64,
    /// A press before this time, after a lift, is momentum beginning.
    ignore_begin_until: f64,
    /// Momentum counts as finished once this time passes with no more of it.
    coast_until: f64,
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
            if self.position == 0.0
                && self.velocity == 0.0
                && !self.coasting
                && !self.held
                && self.edges == Edges::default()
            {
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
        self.raw = 0.0;
        self.tracking = false;
        self.held = false;
    }

    /// Records where the view ended this frame.
    pub fn set_edges(&mut self, edges: Edges) {
        self.edges = edges;
    }

    fn release(&mut self, now: f64) {
        self.tracking = false;
        self.held = false;
        self.coasting = true;
        self.ignore_begin_until = now + COAST_START;
        self.coast_until = now + HOLD;
        self.raw = 0.0;
        self.velocity = 0.0;
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
        if self.coasting {
            if wheel.precise {
                self.coast_until = now + HOLD;
            } else if now >= self.coast_until {
                self.coasting = false;
            }
        }
        if wheel.began && !wheel.lifted && now >= self.ignore_begin_until {
            self.coasting = false;
            self.held = true;
            self.tracking = true;
            self.raw = inverse_rubber(self.position);
        } else if wheel.precise && !self.coasting && !self.held && !self.tracking {
            self.tracking = true;
        }
        let outward =
            (wheel.delta > 0.0 && self.edges.top) || (wheel.delta < 0.0 && self.edges.bottom);
        let follow = self.tracking && !self.coasting && wheel.precise && wheel.delta != 0.0;
        if follow && (outward || self.raw != 0.0) {
            let before = self.position;
            let before_raw = self.raw;
            self.raw += wheel.delta;
            if before_raw != 0.0 && before_raw.signum() != self.raw.signum() {
                self.raw = 0.0;
            }
            self.raw = self.raw.clamp(-2000.0, 2000.0);
            self.position = rubber(self.raw);
            self.moved_at = now;
            self.velocity = if dt > 0.0 {
                ((self.position - before) / dt as f32).clamp(-4000.0, 4000.0)
            } else {
                0.0
            };
            if wheel.lifted {
                self.release(now);
            }
            return;
        }
        if wheel.lifted {
            self.release(now);
        } else if self.held || (self.tracking && now - self.moved_at < HOLD) {
            return;
        }
        // The first frame away from the finger drops the pull's speed, so
        // the band returns from where it is. Later frames keep the spring's
        // own speed.
        if self.tracking {
            self.velocity = 0.0;
        }
        self.tracking = false;
        self.held = false;
        self.raw = 0.0;
        if self.position == 0.0 && self.velocity == 0.0 {
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
            ..Wheel::default()
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
    fn a_quiet_frame_holds_the_band() {
        let mut over = top();
        over.advance(0.0, swipe(40.0));
        let held = over.position();
        assert!(held > 0.0);
        over.advance(0.05, Wheel::default());
        assert_eq!(over.position(), held);
    }

    #[test]
    fn a_lift_snaps_back_and_momentum_does_not_keep_stretching() {
        let mut over = top();
        over.advance(0.0, swipe(80.0));
        let stretched = over.position();
        over.advance(
            0.05,
            Wheel {
                lifted: true,
                ..Wheel::default()
            },
        );
        let after_lift = over.position();
        assert!(after_lift < stretched);
        over.advance(0.07, swipe(80.0));
        assert!(
            over.position() <= after_lift,
            "momentum must not pull the band out again"
        );
    }

    #[test]
    fn it_springs_back_and_comes_to_rest() {
        let mut over = top();
        for frame in 0..10 {
            over.advance(frame as f64 / 60.0, swipe(20.0));
        }
        assert!(over.position() > 10.0);
        // The finger has paused: no more events, past the hold.
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
                    ..Wheel::default()
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
