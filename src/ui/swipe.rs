//! Two-finger trackpad swipes: right on a chat in the list opens it, left on
//! a message in the chat starts a reply to it. The row follows the fingers
//! while they are down and the swipe acts only once they let go; short of the
//! threshold it eases back. Only touchpad scrolling counts (a mouse wheel
//! reports whole lines), and a gesture that starts upward or downward stays a
//! vertical scroll.

use egui::{self, Context, Id, Pos2};

/// How far the fingers travel, in points, before a release acts.
pub const THRESHOLD: f32 = 110.0;
/// A pause this long between scroll events is the fingers lifting. Windows
/// says nothing about touches, and its touchpad keeps sending a fading glide
/// after the lift, which belongs to the gesture.
const PAUSE: f64 = 0.3;
/// The most a row follows the fingers.
const MAX_PULL: f32 = 56.0;
/// Points per line of a Windows touchpad's fractional line deltas. Smaller
/// than a scroll's, so a swipe takes a deliberate stroke.
const LINE_POINTS: f32 = 30.0;

#[derive(Clone, Copy, Default)]
struct Gesture {
    dx: f32,
    last: f64,
    start: Option<Pos2>,
    /// Whether the gesture is horizontal; decided by its first movement.
    horizontal: Option<bool>,
    /// The fingers have let go: this frame is the one that acts.
    released: bool,
}

fn id() -> Id {
    Id::new("trackpad-swipe")
}

/// A wheel event's travel in points if a touchpad made it. macOS and Linux
/// touchpads report points; Windows precision touchpads report lines, but
/// fractions of one, where a mouse wheel's notch is a whole line.
fn touchpad_points(unit: egui::MouseWheelUnit, delta: egui::Vec2) -> Option<egui::Vec2> {
    match unit {
        egui::MouseWheelUnit::Point => Some(delta),
        egui::MouseWheelUnit::Line if delta.x.fract() != 0.0 || delta.y.fract() != 0.0 => {
            Some(delta * LINE_POINTS)
        }
        _ => None,
    }
}

/// Reads this frame's touchpad scrolling. Call once a frame, from the shell.
pub fn update(ctx: &Context) {
    let (deltas, time, pointer, modifiers) = ctx.input(|input| {
        let touch: egui::Vec2 = input
            .events
            .iter()
            .filter_map(|event| match event {
                egui::Event::MouseWheel { unit, delta, .. } => touchpad_points(*unit, *delta),
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
    if gesture.released {
        gesture = Gesture::default();
    } else if gesture.dx != 0.0 && time - gesture.last > PAUSE {
        gesture.released = true;
    }
    if deltas != egui::Vec2::ZERO && !modifiers.any() {
        gesture.last = time;
        gesture.start = gesture.start.or(pointer);
        let horizontal = *gesture
            .horizontal
            .get_or_insert(deltas.x.abs() > deltas.y.abs() * 1.5);
        if horizontal {
            gesture.dx += deltas.x;
        }
    }
    if gesture.dx != 0.0 {
        // Come back to notice the pause that ends it.
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(PAUSE / 2.0));
    }
    ctx.data_mut(|data| data.insert_temp(id(), gesture));
}

fn gesture(ctx: &Context) -> Gesture {
    ctx.data(|data| data.get_temp::<Gesture>(id()))
        .unwrap_or_default()
}

fn started_in(gesture: &Gesture, rect: egui::Rect) -> bool {
    gesture
        .start
        .is_some_and(|start| start.y >= rect.top() && start.y <= rect.bottom())
}

/// How far, 0 to 1, the fingers have pulled the row at `rect` in `direction`
/// (+1 right, -1 left), and whether they let go past the threshold this
/// frame. A gesture belongs to the row it started over.
pub fn pull_of(ctx: &Context, rect: egui::Rect, direction: f32) -> (f32, bool) {
    let gesture = gesture(ctx);
    let travel = gesture.dx * direction;
    if !started_in(&gesture, rect) || travel <= 0.0 {
        return (0.0, false);
    }
    let pulled = (travel / THRESHOLD).min(1.0);
    (pulled, gesture.released && pulled >= 1.0)
}

/// Follows the fingers exactly, and eases back to rest once they let go.
pub fn ease(ctx: &Context, id: Id, pulled: f32) -> f32 {
    if pulled > 0.0 {
        ctx.animate_value_with_time(id, pulled, 0.0)
    } else {
        let time = if crate::motion::screen_reader_attached(ctx) {
            0.0
        } else {
            0.18
        };
        ctx.animate_value_with_time(id, 0.0, time)
    }
}

/// Where a row's shapes begin, for [`shift_since`].
pub fn mark(ui: &egui::Ui) -> egui::layers::ShapeIdx {
    ui.ctx().graphics(|graphics| {
        graphics
            .get(ui.layer_id())
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    })
}

/// Slides everything drawn in this layer since `start` sideways.
pub fn shift_since(ui: &egui::Ui, start: egui::layers::ShapeIdx, by: f32) {
    let layer = ui.layer_id();
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in start.0..list.next_idx().0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                clipped.shape.translate(egui::vec2(by, 0.0));
            });
        }
    });
}

/// A rubber band: follows the fingers at first, then resists.
pub fn pull(progress: f32) -> f32 {
    MAX_PULL * (1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(2))
}
