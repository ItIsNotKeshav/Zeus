//! Two-finger trackpad swipes across the chat: right slides the open chat
//! away, left on a message starts a reply to it. Only touchpad scrolling
//! counts (a mouse wheel reports lines), and a gesture that starts upward
//! or downward stays a vertical scroll.

use egui::{self, Context, Id, Pos2};

/// How far the fingers travel, in points, before the gesture acts.
pub const THRESHOLD: f32 = 110.0;
/// A pause this long between scroll events ends a gesture.
const PAUSE: f64 = 0.18;
/// The most a message follows the fingers.
const MAX_PULL: f32 = 56.0;

#[derive(Clone, Copy, Default)]
struct Gesture {
    dx: f32,
    last: f64,
    start: Option<Pos2>,
    /// Whether the gesture is horizontal; decided by its first movement.
    horizontal: Option<bool>,
    done: bool,
}

fn id() -> Id {
    Id::new("trackpad-swipe")
}

/// Reads this frame's touchpad scrolling. Call once a frame, from the chat.
pub fn update(ctx: &Context, hovered: bool) {
    let (deltas, time, pointer, modifiers) = ctx.input(|input| {
        let touch: egui::Vec2 = input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta,
                    ..
                } => Some(*delta),
                _ => None,
            })
            .fold(egui::Vec2::ZERO, |sum, delta| sum + delta);
        (
            touch,
            input.time,
            input.pointer.hover_pos(),
            input.modifiers,
        )
    });
    let mut gesture = ctx
        .data(|data| data.get_temp::<Gesture>(id()))
        .unwrap_or_default();
    if time - gesture.last > PAUSE {
        gesture = Gesture::default();
    }
    if deltas != egui::Vec2::ZERO && hovered && !modifiers.any() {
        gesture.last = time;
        gesture.start = gesture.start.or(pointer);
        let horizontal = *gesture
            .horizontal
            .get_or_insert(deltas.x.abs() > deltas.y.abs() * 1.5);
        if horizontal && !gesture.done {
            gesture.dx += deltas.x;
        }
    }
    if gesture.dx != 0.0 {
        // Keep drawing until the pause ends it.
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(id(), gesture));
}

/// Fingers moving right past the threshold: the chat should slide away.
/// Reports once per gesture.
pub fn back(ctx: &Context) -> bool {
    let mut gesture = ctx
        .data(|data| data.get_temp::<Gesture>(id()))
        .unwrap_or_default();
    if gesture.dx >= THRESHOLD && !gesture.done {
        gesture.done = true;
        ctx.data_mut(|data| data.insert_temp(id(), gesture));
        return true;
    }
    false
}

/// How far, 0 to 1, the fingers have pulled the message at `rect` leftward,
/// and whether that crossed the threshold this frame (once per gesture).
pub fn reply(ctx: &Context, rect: egui::Rect) -> (f32, bool) {
    let mut gesture = ctx
        .data(|data| data.get_temp::<Gesture>(id()))
        .unwrap_or_default();
    let mine = gesture
        .start
        .is_some_and(|start| start.y >= rect.top() && start.y <= rect.bottom());
    let pulled = if mine && gesture.dx < 0.0 {
        (-gesture.dx / THRESHOLD).min(1.0)
    } else {
        0.0
    };
    let fired = pulled >= 1.0 && !gesture.done;
    if fired {
        gesture.done = true;
        ctx.data_mut(|data| data.insert_temp(id(), gesture));
    }
    (pulled, fired)
}

/// A rubber band: follows the fingers at first, then resists.
pub fn pull(progress: f32) -> f32 {
    MAX_PULL * (1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(2))
}
