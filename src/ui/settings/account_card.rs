//! The account card at the top of Settings' category list: our picture, name
//! and number, which open the account switcher.

use egui::{Align2, Rect, Sense, pos2, vec2};

use crate::app::App;
use crate::i18n::gettext;
use crate::theme::{self, Icon};
use crate::ui::widgets;

/// How tall the card is.
const CARD_HEIGHT: f32 = 76.0;
/// Our picture's size on it.
const CARD_AVATAR: f32 = 60.0;
/// The room between the card's edge and its picture.
const CARD_PAD: f32 = 8.0;

/// The card: our picture, then our name over our number, then a chevron.
/// Without a name the number stands alone; without either it reads
/// "Profile". A click opens the switcher the tab bar's Profile tab opens.
/// Spans the width the categories below it do.
pub(super) fn show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let me = app.me.clone().unwrap_or_default();
    let name = app
        .me_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    let phone = app.phone();
    let picture = app.avatar(&me);
    let profile = gettext(app.locale, "Profile").into_owned();
    let (title, subtitle) = match (name, phone) {
        (Some(name), Some(phone)) => (name, Some(phone)),
        (Some(name), None) => (name, None),
        (None, Some(phone)) => (phone, None),
        (None, None) => (profile, None),
    };
    let width = (ui.available_width() - 2.0 * widgets::ROW_CARD_X).max(0.0);
    let (outer, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), CARD_HEIGHT), Sense::hover());
    let rect = Rect::from_min_size(
        pos2(outer.left() + widgets::ROW_CARD_X, outer.top()),
        vec2(width, CARD_HEIGHT),
    );
    let response = ui.interact(rect, ui.id().with("settings-account-card"), Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, 12.0);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), title.as_str())
    });
    if ui.is_rect_visible(rect) {
        let hover = super::super::chats::row_hover_amount(ui, response.id, response.hovered());
        if hover > 0.0 {
            ui.painter()
                .rect_filled(rect, 12.0, palette.surface_hover.gamma_multiply(hover));
        }
        let avatar = Rect::from_min_size(
            pos2(rect.left() + CARD_PAD, rect.center().y - CARD_AVATAR / 2.0),
            vec2(CARD_AVATAR, CARD_AVATAR),
        );
        widgets::paint_avatar(ui, &palette, avatar, &title, &me, picture.as_deref());
        let chevron = Rect::from_center_size(
            pos2(rect.right() - CARD_PAD - 10.0, rect.center().y),
            vec2(20.0, 20.0),
        );
        theme::paint_icon(ui, Icon::ChevronRight, chevron, 18.0, palette.secondary);
        let left = avatar.right() + 12.0;
        let text = Rect::from_min_max(
            pos2(left, rect.top()),
            pos2((chevron.left() - 6.0).max(left), rect.bottom()),
        );
        let painter = ui.painter_at(text);
        let font = theme::semibold(theme::TEXT_SECTION);
        match subtitle {
            Some(subtitle) => {
                painter.text(
                    pos2(left, rect.center().y - 2.0),
                    Align2::LEFT_BOTTOM,
                    &title,
                    font,
                    palette.text,
                );
                painter.text(
                    pos2(left, rect.center().y + 2.0),
                    Align2::LEFT_TOP,
                    subtitle,
                    theme::medium(theme::TEXT_CAPTION),
                    palette.secondary,
                );
            }
            None => {
                painter.text(
                    pos2(left, rect.center().y),
                    Align2::LEFT_CENTER,
                    &title,
                    font,
                    palette.text,
                );
            }
        }
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.account_menu = true;
    }
}
