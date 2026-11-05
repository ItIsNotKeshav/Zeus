//! The open chat's bubbles hang on springs while the reader scrolls, as in
//! iOS Messages (Apple's "Advanced Techniques with UIKit Dynamics", WWDC
//! 2013): each row is tied to its laid-out place by a spring (UIKit's
//! attachment of length 0, damping 0.8 at 1 Hz), and every step of a scroll
//! pushes it back against the scroll by a share that grows with its distance
//! from the pointer. Rows under the pointer follow the scroll exactly, rows
//! far from it lag and catch up, so the list ripples as it moves.
//!
//! Only the reader's own scrolling pushes: a wheel or touchpad turn, its
//! glide, a gesture carried over from beside the list, and the scroll bar.
//! A jump, a chat opening, following the newest message and keyboard scrolls
//! move the list without it. The transcript lays each row out where its
//! spring has it, so clicks land on what is drawn.

use std::collections::HashMap;

use crate::motion::Spring;

/// UIKit's attachment as the WWDC sample sets it: damping 0.8 at 1 Hz, a
/// response of one second at a bounce of 0.2.
pub(crate) const SPRING: Spring = Spring::new(1.0, 0.2);

/// A row this far from the pointer lags by a whole scroll step; nearer ones
/// by their share of it. The sample divides by 1500 phone points; a desktop
/// point is about a third larger on the glass, so the same reach on screen
/// takes fewer of them.
pub(crate) const SPAN: f32 = 1200.0;

/// The furthest a row lags behind its place, about a line of text, so a
/// fast scroll ripples rather than smears.
pub(crate) const MAX_LAG: f32 = 20.0;

/// Nearer its place than this, and slower, a row counts as at rest.
const REST_POSITION: f32 = 0.25;
const REST_SPEED: f32 = 4.0;

/// A row the transcript laid out, in content coordinates (from the top of
/// the transcript, unaffected by the scroll offset), where it lies without
/// its spring: the top of its slot and the bottom of what it drew.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Laid {
    pub id: String,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
struct Row {
    laid: Laid,
    /// How far below its place the row is drawn.
    position: f32,
    /// How fast it moves, in points a second.
    velocity: f32,
}

/// The springs of the rows near the view, in layout order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Springs {
    rows: Vec<Row>,
    /// When they were last brought up to date, in egui input time.
    time: Option<f64>,
    /// Where the pointer was, in content coordinates, when the last push
    /// came: the row there stays put while the others keep in order.
    anchor: f32,
    /// A scroll this frame made, pushed when the next frame, which draws the
    /// list at its new offset, brings the springs up to date.
    pending: Option<f32>,
}

impl Springs {
    fn id(chat: &str) -> egui::Id {
        egui::Id::new(("scroll-springs", chat))
    }

    pub fn load(ctx: &egui::Context, chat: &str) -> Self {
        ctx.data(|data| data.get_temp::<Self>(Self::id(chat)))
            .unwrap_or_default()
    }

    pub fn store(self, ctx: &egui::Context, chat: &str) {
        ctx.data_mut(|data| {
            if self.rows.is_empty() && self.pending.is_none() {
                data.remove::<Self>(Self::id(chat));
            } else {
                data.insert_temp(Self::id(chat), self);
            }
        });
    }

    /// Lets every row go to its place at once.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Brings the springs to `now`: the scroll the last frame made pushes the
    /// rows, then they pull toward their places for the time between. A pass
    /// egui redoes has the same time and changes nothing.
    pub fn advance(&mut self, now: f64) {
        let dt = match self.time {
            Some(then) if now <= then => return,
            Some(then) => now - then,
            // At rest until the frame before: only its scroll moves them.
            None => 0.0,
        };
        if let Some(delta) = self.pending.take() {
            self.push(delta);
        }
        for row in &mut self.rows {
            (row.position, row.velocity) = SPRING.advance(row.position, row.velocity, dt);
        }
        self.keep_order();
        if !self.moving() {
            for row in &mut self.rows {
                row.position = 0.0;
                row.velocity = 0.0;
            }
        }
        self.time = Some(now);
    }

    /// Records a scroll by the reader: the offset grew by `delta` with the
    /// pointer at `anchor` (content coordinates). It pushes the rows when
    /// the next frame draws them at the new offset.
    pub fn scrolled(&mut self, delta: f32, anchor: f32) {
        if delta == 0.0 || !delta.is_finite() || !anchor.is_finite() {
            return;
        }
        self.anchor = anchor;
        self.pending = Some(self.pending.unwrap_or(0.0) + delta);
    }

    /// Ends a frame: while the rows move, or a scroll waits to push them, the
    /// rows this frame laid out near the view take over from the last
    /// frame's (see [`Self::laid_out`]); at rest nothing is kept.
    pub fn finish(&mut self, laid: Vec<Laid>) {
        if self.moving() {
            self.laid_out(laid);
        } else {
            self.clear();
        }
    }

    /// The rows this frame laid out near the view take over from the last
    /// frame's: a row that stays keeps its spring, one coming into reach
    /// starts at its place, and one gone out of reach is forgotten.
    pub fn laid_out(&mut self, laid: Vec<Laid>) {
        let mut old: HashMap<String, (f32, f32)> = self
            .rows
            .drain(..)
            .map(|row| (row.laid.id, (row.position, row.velocity)))
            .collect();
        self.rows = laid
            .into_iter()
            .map(|laid| {
                let (position, velocity) = old.remove(&laid.id).unwrap_or((0.0, 0.0));
                Row {
                    laid,
                    position,
                    velocity,
                }
            })
            .collect();
    }

    /// How far below its place the row `id` is drawn.
    #[cfg(test)]
    pub fn shift(&self, id: &str) -> f32 {
        self.rows
            .iter()
            .find(|row| row.laid.id == id)
            .map_or(0.0, |row| row.position)
    }

    /// Each row's shift by id, for a frame's layout.
    pub fn shifts(&self) -> HashMap<String, f32> {
        self.rows
            .iter()
            .filter(|row| row.position != 0.0)
            .map(|row| (row.laid.id.clone(), row.position))
            .collect()
    }

    /// Whether any row is still on its way, or a push waits.
    pub fn moving(&self) -> bool {
        self.pending.is_some()
            || self
                .rows
                .iter()
                .any(|row| row.position.abs() > REST_POSITION || row.velocity.abs() > REST_SPEED)
    }

    /// Pushes every row against a scroll of `delta` by its share: its
    /// distance from the anchor over [`SPAN`], at most the whole step.
    fn push(&mut self, delta: f32) {
        for row in &mut self.rows {
            row.position = (row.position + delta * resistance(&row.laid, self.anchor))
                .clamp(-MAX_LAG, MAX_LAG);
        }
        self.keep_order();
    }

    /// Keeps each row clear of its neighbour, from the row under the anchor
    /// outward: a row may close the gap to the next one nearer the anchor,
    /// never cross into it, so the order never changes. A row held back
    /// moves on with its neighbour.
    fn keep_order(&mut self) {
        let Some(anchor) = self.anchor_row() else {
            return;
        };
        for index in anchor + 1..self.rows.len() {
            let (nearer, row) = (&self.rows[index - 1], &self.rows[index]);
            let gap = (row.laid.top - nearer.laid.bottom).max(0.0);
            let lowest = nearer.position - gap;
            if row.position < lowest {
                let velocity = nearer.velocity;
                let row = &mut self.rows[index];
                row.position = lowest;
                row.velocity = velocity;
            }
        }
        for index in (0..anchor).rev() {
            let (row, nearer) = (&self.rows[index], &self.rows[index + 1]);
            let gap = (nearer.laid.top - row.laid.bottom).max(0.0);
            let highest = nearer.position + gap;
            if row.position > highest {
                let velocity = nearer.velocity;
                let row = &mut self.rows[index];
                row.position = highest;
                row.velocity = velocity;
            }
        }
    }

    /// The row under the anchor, or the one nearest it.
    fn anchor_row(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| (row.laid.top..row.laid.bottom).contains(&self.anchor))
            .or_else(|| {
                self.rows
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let distance = |row: &Row| (centre(&row.laid) - self.anchor).abs();
                        distance(a).total_cmp(&distance(b))
                    })
                    .map(|(index, _)| index)
            })
    }
}

fn centre(laid: &Laid) -> f32 {
    (laid.top + laid.bottom) / 2.0
}

/// The share of a scroll step a row lags by: none under the pointer,
/// growing with distance, at most the whole step.
pub(crate) fn resistance(laid: &Laid, anchor: f32) -> f32 {
    ((centre(laid) - anchor).abs() / SPAN).min(1.0)
}

/// What a frame knows about why the list may not spring.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Calm {
    /// A screen reader reads the window.
    pub screen_reader: bool,
    /// The window is minimised or in the background.
    pub resting: bool,
    /// The list is laid out in full to reach a message or the unread
    /// divider, or glides to a quoted one.
    pub jumping: bool,
    /// A button held in the list, a sweep over messages or a text
    /// selection, a sideways swipe, or a message carried off.
    pub holding: bool,
}

impl Calm {
    /// Whether rows may hang on their springs this frame. When not, they
    /// all go to their places at once.
    pub fn allows_springs(self) -> bool {
        !(self.screen_reader || self.resting || self.jumping || self.holding || held_still())
    }
}

#[cfg(test)]
thread_local! {
    /// Whether a test on this thread holds every row at its place.
    static HELD_STILL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether a test holds the rows still on this thread.
#[cfg(test)]
fn held_still() -> bool {
    HELD_STILL.with(std::cell::Cell::get)
}

/// Rows are never held still outside tests.
#[cfg(not(test))]
fn held_still() -> bool {
    false
}

/// Holds every row at its laid-out place on this test thread until the
/// guard drops, for tests that measure the layout itself rather than the
/// springs: a row painted where its spring has it would read as moved.
#[cfg(test)]
#[must_use]
pub(crate) fn hold_still() -> HeldStill {
    HeldStill(HELD_STILL.with(|cell| cell.replace(true)))
}

/// Puts back what [`hold_still`] replaced.
#[cfg(test)]
pub(crate) struct HeldStill(bool);

#[cfg(test)]
impl Drop for HeldStill {
    fn drop(&mut self) {
        HELD_STILL.with(|cell| cell.set(self.0));
    }
}

/// What moved the list's offset this frame.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Moved {
    /// The reader turned the wheel or touchpad, glided, scrolled over a
    /// neighbouring pane, or held the scroll bar.
    pub by_reader: bool,
    /// The app moved it too: to the newest message, a keyboard page, an
    /// edge scroll while selecting, a jump, or a chat's first showing.
    pub by_app: bool,
}

impl Moved {
    /// The part of `delta`, the offset's change this frame, that pushes the
    /// rows: all of it when only the reader moved the list, none otherwise.
    pub fn push(self, delta: f32) -> f32 {
        if self.by_reader && !self.by_app {
            delta
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows `height` tall, `gap` apart, from the top of the transcript.
    fn column(count: usize, height: f32, gap: f32) -> Vec<Laid> {
        (0..count)
            .map(|index| {
                let top = index as f32 * (height + gap);
                Laid {
                    id: format!("m{index}"),
                    top,
                    bottom: top + height,
                }
            })
            .collect()
    }

    fn springs(laid: Vec<Laid>) -> Springs {
        let mut springs = Springs::default();
        springs.laid_out(laid);
        springs.advance(0.0);
        springs
    }

    fn positions(springs: &Springs) -> Vec<f32> {
        springs.rows.iter().map(|row| row.position).collect()
    }

    #[test]
    fn rows_lag_more_the_further_they_are_from_the_pointer() {
        let laid = column(20, 40.0, 3.0);
        // The pointer on the middle of row 10.
        let anchor = centre(&laid[10]);
        let mut springs = springs(laid);
        springs.scrolled(4.0, anchor);
        springs.advance(1e-9);
        let shift = |index: usize| springs.shift(&format!("m{index}"));
        // The row under the pointer follows the scroll exactly.
        assert!(shift(10).abs() < 1e-4, "{}", shift(10));
        // Further rows lag more, on both sides, against the scroll.
        for index in 11..19 {
            assert!(shift(index + 1) > shift(index), "{:?}", positions(&springs));
        }
        for index in (1..10).rev() {
            assert!(shift(index - 1) > shift(index), "{:?}", positions(&springs));
        }
        // By distance over the span: 5 rows of 43 points.
        assert!((shift(15) - 4.0 * 5.0 * 43.0 / SPAN).abs() < 1e-3);
        // A scroll the other way pulls them the other way.
        let mut back = Springs::default();
        back.laid_out(column(20, 40.0, 3.0));
        back.advance(0.0);
        back.scrolled(-4.0, anchor);
        back.advance(1e-9);
        assert!(back.shift("m19") < 0.0 && back.shift("m0") < 0.0);
    }

    #[test]
    fn resistance_grows_with_distance_and_stops_at_the_whole_step() {
        let row = |top: f32| Laid {
            id: String::new(),
            top,
            bottom: top + 20.0,
        };
        assert_eq!(resistance(&row(-10.0), 0.0), 0.0);
        let near = resistance(&row(90.0), 0.0);
        let far = resistance(&row(590.0), 0.0);
        assert!(near > 0.0 && far > near);
        assert!((far - 600.0 / SPAN).abs() < 1e-6);
        assert_eq!(resistance(&row(50_000.0), 0.0), 1.0);
        assert_eq!(resistance(&row(-50_000.0), 0.0), 1.0);
    }

    #[test]
    fn a_fast_scroll_lags_no_more_than_a_line() {
        let laid = column(30, 40.0, 3.0);
        let mut springs = springs(laid);
        for frame in 1..=30 {
            springs.scrolled(120.0, 0.0);
            springs.advance(f64::from(frame) / 60.0);
        }
        assert!(
            positions(&springs)
                .iter()
                .all(|position| position.abs() <= MAX_LAG),
            "{:?}",
            positions(&springs)
        );
        assert!(springs.shift("m29") > MAX_LAG * 0.9);
    }

    #[test]
    fn the_springs_settle_on_the_places_and_stop_asking_for_frames() {
        let laid = column(20, 40.0, 3.0);
        let mut springs = springs(laid);
        for frame in 1..=10 {
            springs.scrolled(15.0, 0.0);
            springs.advance(f64::from(frame) / 120.0);
        }
        assert!(springs.moving());
        let mut time = 10.0 / 120.0;
        let mut frames = 0;
        while springs.moving() {
            time += 1.0 / 120.0;
            springs.advance(time);
            frames += 1;
            assert!(frames < 400, "still moving: {:?}", positions(&springs));
        }
        // About a second and a half at 1 Hz with damping 0.8.
        assert!(frames > 60, "settled after {frames} frames");
        assert!(positions(&springs).iter().all(|position| *position == 0.0));
        // Nothing pushes again until the reader scrolls.
        springs.advance(time + 1.0);
        assert!(!springs.moving());
    }

    #[test]
    fn rows_never_cross_their_neighbours() {
        // Tight rows, a jerky scroll back and forth, the pointer moving.
        let laid = column(25, 18.0, 2.0);
        let mut springs = springs(laid.clone());
        let mut time = 0.0;
        for step in 0..240 {
            let delta = if (step / 20) % 2 == 0 { 90.0 } else { -140.0 };
            springs.scrolled(delta, (step % 25) as f32 * 20.0);
            time += 1.0 / 60.0;
            springs.advance(time);
            for pair in springs.rows.windows(2) {
                let above = pair[0].laid.bottom + pair[0].position;
                let below = pair[1].laid.top + pair[1].position;
                assert!(
                    below >= above - 1e-3,
                    "step {step}: {} overlaps {}: {:?}",
                    pair[1].laid.id,
                    pair[0].laid.id,
                    positions(&springs)
                );
            }
        }
    }

    #[test]
    fn a_row_coming_into_reach_starts_at_its_place_and_one_leaving_is_forgotten() {
        let mut springs = springs(column(5, 40.0, 3.0));
        springs.scrolled(10.0, 0.0);
        springs.advance(0.01);
        let kept = springs.shift("m4");
        assert!(kept > 0.0);
        let mut next = column(6, 40.0, 3.0);
        next.remove(0);
        springs.laid_out(next);
        assert_eq!(springs.shift("m0"), 0.0);
        assert_eq!(springs.shift("m5"), 0.0);
        assert_eq!(springs.shift("m4"), kept);
    }

    #[test]
    fn a_redone_pass_does_not_push_or_pull_twice() {
        let mut springs = springs(column(10, 40.0, 3.0));
        springs.scrolled(10.0, 0.0);
        springs.advance(0.01);
        let once = positions(&springs);
        springs.advance(0.01);
        assert_eq!(positions(&springs), once);
    }

    #[test]
    fn only_the_reader_s_own_scrolling_pushes() {
        let reader = Moved {
            by_reader: true,
            by_app: false,
        };
        assert_eq!(reader.push(12.0), 12.0);
        // Following the newest message, a jump, a keyboard page.
        let app = Moved {
            by_reader: false,
            by_app: true,
        };
        assert_eq!(app.push(12.0), 0.0);
        // A turn of the wheel while the app scrolls too, as when a sent
        // message pins the list to its end.
        let both = Moved {
            by_reader: true,
            by_app: true,
        };
        assert_eq!(both.push(12.0), 0.0);
        assert_eq!(Moved::default().push(12.0), 0.0);
    }

    #[test]
    fn a_screen_reader_or_a_resting_window_holds_the_rows_still() {
        assert!(Calm::default().allows_springs());
        for calm in [
            Calm {
                screen_reader: true,
                ..Calm::default()
            },
            Calm {
                resting: true,
                ..Calm::default()
            },
            Calm {
                jumping: true,
                ..Calm::default()
            },
            Calm {
                holding: true,
                ..Calm::default()
            },
        ] {
            assert!(!calm.allows_springs(), "{calm:?}");
        }
    }
}
