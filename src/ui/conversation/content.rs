//! Zeus's own parts of a message's content.

use super::*;

/// How round the corners are where two tiles of an album meet.
pub(super) const ALBUM_SEAM_RADIUS: f32 = 4.0;
/// The size of the button that downloads a whole album.
pub(super) const ALBUM_BUTTON: f32 = 56.0;

/// The one button over an album that still has members to fetch: it downloads
/// them all, and eases away once none is left.
pub(super) fn album_download_button(
    ui: &mut egui::Ui,
    view: &View<'_>,
    messages: &[Message],
    tiles: &[(Rect, String)],
    actions: &mut Vec<Action>,
) {
    let Some(grid) = tiles
        .iter()
        .map(|(rect, _)| *rect)
        .reduce(|all, rect| all.union(rect))
    else {
        return;
    };
    let missing: Vec<&Message> = messages
        .iter()
        .filter(|message| {
            message
                .content
                .media()
                .is_some_and(|media| media.path.is_none())
        })
        .collect();
    let downloading = !missing.is_empty()
        && missing.iter().all(|message| {
            message
                .content
                .media()
                .is_some_and(|media| matches!(media.state, MediaState::Downloading))
        });
    let id = bubble_id(&view.chat.id, &messages[0].id).with("album-download");
    let appear = theme::animate(ui.ctx(), id, !missing.is_empty(), theme::MOTION_QUICK);
    if appear <= 0.0 {
        return;
    }
    let button = Rect::from_center_size(grid.center(), Vec2::splat(ALBUM_BUTTON));
    let response = ui.interact(button, id, Sense::click());
    let label = if downloading {
        "Downloading…"
    } else {
        "Download all"
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            !downloading,
            crate::i18n::gettext(view.locale, label),
        )
    });
    theme::reveal_focus(&response);
    let hover = theme::animate(
        ui.ctx(),
        id.with("hover"),
        response.hovered() && !downloading,
        theme::MOTION_QUICK,
    );
    let scale = (0.7 + 0.3 * appear) * (1.0 + 0.06 * hover);
    let disc = Rect::from_center_size(button.center(), button.size() * scale);
    let painter = ui.painter();
    let alpha = |value: f32| (value * appear).round() as u8;
    // A tint over the pictures it floats on, not a blur: it is drawn among
    // the messages, and a frost of its own there would cost a frame two
    // more trips out of the window's framebuffer (crmne/zapfast#55).
    let tint = 170.0 * crate::backdrop::tinted(0.5);
    painter.circle_filled(
        disc.center(),
        disc.width() / 2.0,
        Color32::from_black_alpha(alpha(tint + 40.0 * hover)),
    );
    painter.circle_stroke(
        disc.center(),
        disc.width() / 2.0 - 0.5,
        Stroke::new(1.0, Color32::from_white_alpha(alpha(46.0))),
    );
    let white = Color32::from_white_alpha(alpha(255.0));
    if downloading {
        theme::paint_spinner(ui, disc, 26.0 * scale, white);
    } else {
        theme::paint_icon(ui, Icon::Download, disc, 26.0 * scale, white);
    }
    theme::focus_outline(ui, response.id, disc, disc.width() / 2.0);
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        && !downloading
    {
        for message in missing {
            if let Some(media) = message.content.media()
                && !matches!(media.state, MediaState::Downloading)
            {
                actions.push(Action::Download {
                    manual: true,
                    card: None,
                    chat: message.chat.clone(),
                    message: message.id.clone(),
                });
            }
        }
    }
}
