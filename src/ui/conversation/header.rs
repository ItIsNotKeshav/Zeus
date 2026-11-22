//! Zeus's own parts of the chat header and its pinned-message banner.

use super::*;

/// Room around the floating pin pill, beside it and above and below it.
pub(super) const PIN_FLOAT_GAP: f32 = 12.0;
/// Whether the chat has a pin the line under the header would show.
pub(super) fn has_live_pins(app: &App, chat: &Chat) -> bool {
    let now = crate::util::now();
    app.chat_pins
        .get(&chat.id)
        .is_some_and(|rows| rows.iter().any(|row| row.expires_at > now))
}

/// Rows down each of the veil's rounded top corners.
pub(super) const CORNER_STEPS: usize = crate::ui::rounded_clip::ARC_STEPS;
