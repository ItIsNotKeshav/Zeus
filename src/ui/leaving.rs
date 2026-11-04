//! A message deleted from the open chat leaves with motion instead of
//! vanishing: its row closes the gap it held while the bubble fades and
//! shrinks toward its tail corner.
//!
//! The app records a [`Leaving`] when it removes one message from the open
//! chat (`Conversation::leaving`); the transcript draws it after the message
//! that was above it until [`theme::MOTION_PANEL`] has passed. A leaving row
//! only paints: it is disabled, never reaches the copy rows, and its actions
//! are dropped.

use std::time::{Duration, Instant};

use egui::emath::TSTransform;
use egui::{Rect, UiBuilder, pos2};

use crate::model::Message;
use crate::theme;

/// More messages than this removed together leave at once, with no motion.
pub(crate) const BULK: usize = 5;

/// Removals this close together count as one batch for [`BULK`].
pub(crate) const BATCH: Duration = Duration::from_millis(100);

/// How far a leaving bubble shrinks, about its tail corner.
const SCALE: f32 = theme::CLOSE_SCALE;

/// A message on its way out of the open chat.
#[derive(Clone, Debug)]
pub(crate) struct Leaving {
    pub message: Message,
    /// When it was removed.
    pub since: Instant,
    /// The id of the message that was just above it, or `None` when it was
    /// the first loaded one.
    pub after: Option<String>,
}

/// How long a message takes to leave.
pub(crate) fn duration() -> Duration {
    Duration::from_secs_f32(theme::MOTION_PANEL)
}

/// Records `message` as leaving, then [`settle`]s the list.
pub(crate) fn record(
    leaving: &mut Vec<Leaving>,
    message: Message,
    after: Option<String>,
    now: Instant,
) {
    leaving.push(Leaving {
        message,
        since: now,
        after,
    });
    settle(leaving, now);
}

/// Drops the messages that have finished leaving, and every message of a
/// batch of more than [`BULK`] removed together, which go at once.
pub(crate) fn settle(leaving: &mut Vec<Leaving>, now: Instant) {
    let duration = duration();
    leaving.retain(|ghost| now.saturating_duration_since(ghost.since) < duration);
    let batch = leaving
        .iter()
        .filter(|ghost| now.saturating_duration_since(ghost.since) < BATCH)
        .count();
    if batch > BULK {
        leaving.retain(|ghost| now.saturating_duration_since(ghost.since) >= BATCH);
    }
}

/// How far `ghost` has gone, from 0 when removed to 1 when gone.
pub(crate) fn progress(ghost: &Leaving, now: Instant) -> f32 {
    (now.saturating_duration_since(ghost.since).as_secs_f32() / theme::MOTION_PANEL).clamp(0.0, 1.0)
}

/// Draws one leaving row at the cursor, `t` of the way out. The space it
/// takes closes from its full height to nothing, and `paint` draws the
/// bubble fading out and shrinking toward its tail corner (bottom right for
/// our own, bottom left for others). Nothing in it answers the pointer.
pub(crate) fn show(
    ui: &mut egui::Ui,
    id: egui::Id,
    t: f32,
    from_me: bool,
    paint: impl FnOnce(&mut egui::Ui),
) {
    let eased = theme::ease_out(t);
    let top = ui.cursor().top();
    let left = ui.max_rect().left();
    let right = ui.max_rect().right();
    // The full height is known from the first frame it was drawn in.
    let full: Option<f32> = ui.ctx().data(|data| data.get_temp(id));
    let shown = full.map(|full| full * (1.0 - eased));
    let layer = ui.layer_id();
    let start = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(id)
            .max_rect(Rect::from_min_max(
                pos2(left, top),
                pos2(right, top + 100_000.0),
            ))
            .layout(*ui.layout()),
    );
    child.disable();
    // Disabling dims; the fade sets the opacity outright instead.
    child.set_opacity(ui.opacity() * (1.0 - eased));
    let clip = Rect::from_min_max(
        pos2(left, top),
        pos2(right, top + shown.unwrap_or(100_000.0)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(clip));
    paint(&mut child);
    let measured = (child.cursor().top() - top).max(0.0);
    let full = full.unwrap_or(measured);
    if shown.is_none() {
        ui.ctx().data_mut(|data| data.insert_temp(id, full));
    }
    let painted = child.min_rect();
    let end = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    let scale = 1.0 - (1.0 - SCALE) * eased;
    if end.0 > start.0 && scale < 1.0 {
        let pivot = if from_me {
            painted.right_bottom()
        } else {
            painted.left_bottom()
        };
        let transform = TSTransform::new(pivot.to_vec2() * (1.0 - scale), scale);
        ui.ctx().graphics_mut(|graphics| {
            graphics.entry(layer).transform_range(start, end, transform);
        });
    }
    ui.add_space(full * (1.0 - eased));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str) -> Message {
        Message {
            id: id.to_owned(),
            chat: "chat".into(),
            sender: "123@s.whatsapp.net".into(),
            sender_name: None,
            from_me: false,
            timestamp: 0,
            content: crate::model::Content::text("gone"),
            status: crate::model::Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            history_order: None,
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    }

    #[test]
    fn a_ghost_expires_after_the_motion() {
        let start = Instant::now();
        let mut leaving = Vec::new();
        record(&mut leaving, message("a"), None, start);
        assert_eq!(leaving.len(), 1);
        settle(&mut leaving, start + duration() / 2);
        assert_eq!(leaving.len(), 1);
        assert!((progress(&leaving[0], start + duration() / 2) - 0.5).abs() < 0.01);
        settle(&mut leaving, start + duration());
        assert!(leaving.is_empty());
    }

    #[test]
    fn a_bulk_removal_leaves_at_once() {
        let start = Instant::now();
        let mut leaving = Vec::new();
        for index in 0..BULK {
            record(&mut leaving, message(&index.to_string()), None, start);
        }
        assert_eq!(leaving.len(), BULK);
        record(&mut leaving, message("last"), None, start);
        assert!(leaving.is_empty());
    }

    #[test]
    fn separate_removals_are_not_a_bulk_one() {
        let start = Instant::now();
        let mut leaving = Vec::new();
        for index in 0..BULK {
            record(&mut leaving, message(&index.to_string()), None, start);
        }
        let later = start + BATCH;
        record(&mut leaving, message("later"), None, later);
        assert_eq!(leaving.len(), BULK + 1);
    }
}
