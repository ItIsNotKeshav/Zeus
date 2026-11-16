//! Zeus's own parts of the chat header and its pinned-message banner.

use super::*;

/// Rounds off the corners of the conversation's `card` over what is painted
/// square beneath a vibrant header or composer, such as messages scrolled
/// under them, where the sidebar's gutter around the card is opaque. The
/// header's own glass and veil keep to the corners themselves.
pub(super) fn card_corners(ui: &egui::Ui, palette: &Palette, card: Rect, radius: f32) {
    let gutter = palette.rail();
    if radius > 0.0 && gutter.is_opaque() {
        ui.painter()
            .add(widgets::card_corners(card, radius, gutter));
    }
}

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
pub(super) const CORNER_STEPS: usize = 8;
