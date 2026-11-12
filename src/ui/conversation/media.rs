//! Zeus's own parts of pictures and attachments in a bubble.

use super::*;

/// The widest a picture or video grows: in a Nothing bubble that also holds
/// a quote, a label or a caption it reaches into the text padding on both
/// sides, as the quote above it does, so the two line up.
pub(super) fn picture_limit(palette: &Palette, message: &Message) -> f32 {
    if palette.bubbles == theme::BubbleStyle::Nothing && !compact_picture_message(message) {
        PICTURE_WIDTH + f32::from(2 * (NOTHING_TEXT_MARGIN.left - CARD_RING))
    } else {
        PICTURE_WIDTH
    }
}
/// The dimming over a picture while it downloads, by message, so it can lift
/// once the file (a new path) is there.
pub(super) fn download_veil_id(message: &Message) -> egui::Id {
    egui::Id::new(("download-veil", &message.chat, &message.id))
}

/// What a picture had over it while it downloaded, the dimming and the
/// spinner, at `strength` as it lifts.
pub(super) fn paint_download_veil(ui: &egui::Ui, rect: Rect, radius: f32, strength: f32) {
    if strength <= 0.0 || !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().rect_filled(
        rect,
        radius,
        Color32::from_black_alpha(60).gamma_multiply(strength),
    );
    let disc = Rect::from_center_size(rect.center(), Vec2::splat(44.0));
    ui.painter().circle_filled(
        disc.center(),
        22.0,
        Color32::from_black_alpha(120).gamma_multiply(strength),
    );
    theme::paint_spinner(ui, disc, 22.0, Color32::WHITE.gamma_multiply(strength));
}

/// How wide the round icon of a file card is.
pub(super) const ATTACHMENT_DISC: f32 = 36.0;
/// The least height of a file card's contents: its icon.
pub(super) const ATTACHMENT_ROW: f32 = ATTACHMENT_DISC;
/// The space between a file card's icon, its text and its action.
pub(super) const ATTACHMENT_GAP: f32 = 12.0;
/// The space between a file's name and the line under it.
pub(super) const ATTACHMENT_LINE_GAP: f32 = 2.0;
/// How wide a file card's open or download action is.
pub(super) const ATTACHMENT_ACTION: f32 = 20.0;

/// The room around a file card inside its bubble, and inside the card around
/// its contents. A Nothing bubble holds the card with the even rim it gives a
/// photo, and the card keeps the room a text bubble gives its text, so a file
/// lines up with the messages around it.
pub(super) fn attachment_insets(palette: &Palette) -> (Vec2, Vec2) {
    if palette.bubbles == theme::BubbleStyle::Nothing {
        (
            vec2(NOTHING_ATTACHMENT_RIM.into(), NOTHING_ATTACHMENT_RIM.into()),
            vec2(
                f32::from(NOTHING_TEXT_MARGIN.left) - f32::from(NOTHING_ATTACHMENT_RIM),
                f32::from(NOTHING_TEXT_MARGIN.top),
            ),
        )
    } else {
        // WhatsApp's bubble margin, and an even room inside the card.
        (vec2(10.0, 6.0), vec2(10.0, 10.0))
    }
}

/// How round a file card's corners are: inside a Nothing bubble, its
/// corners less the rim, so the two curves run together.
pub(super) fn attachment_radius(palette: &Palette) -> f32 {
    if palette.bubbles == theme::BubbleStyle::Nothing {
        f32::from(widgets::NOTHING_BUBBLE_RADIUS) - f32::from(NOTHING_ATTACHMENT_RIM)
    } else {
        8.0
    }
}

/// The even rim of a Nothing bubble around a file card, as around a photo.
pub(super) const NOTHING_ATTACHMENT_RIM: i8 = 6;
