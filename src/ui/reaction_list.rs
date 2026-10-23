//! Who reacted to a message, and with what: opened from its reaction pill.

use egui::{Align, Layout, Sense, vec2};

use super::widgets;
use crate::app::App;
use crate::i18n::gettext;
use crate::model::{Action, Dialog, Message};
use crate::theme::{self, Icon};

const ROW: f32 = 42.0;
/// Width of the popup's content, and its frame's margin.
const WIDTH: f32 = 270.0;
const MARGIN: i8 = 16;

/// The distinct emoji of `message`'s reactions, most used first.
pub(crate) fn tally(message: &Message) -> Vec<(String, u32)> {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for reaction in &message.reactions {
        match counts
            .iter_mut()
            .find(|(emoji, _)| *emoji == reaction.emoji)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((reaction.emoji.clone(), 1)),
        }
    }
    // Stable: emoji with equal counts keep the order they arrived in.
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
}

pub fn show(app: &mut App, ui: &mut egui::Ui, chat: &str, id: &str) {
    let palette = app.palette;
    let locale = app.locale;
    let message = app
        .conversations
        .get(chat)
        .and_then(|c| c.message(id))
        .cloned();
    let Some(message) = message.filter(|message| !message.reactions.is_empty()) else {
        app.actions.push(Action::CloseDialog);
        return;
    };
    let counts = tally(&message);
    let total = message.reactions.len();
    let state = ui.id().with(("reaction-filter", chat, id));
    let mut filter: Option<String> = ui.ctx().data(|data| data.get_temp(state));
    if filter
        .as_ref()
        .is_some_and(|emoji| !counts.iter().any(|(known, _)| known == emoji))
    {
        filter = None;
    }
    ui.horizontal(|ui| {
        theme::text(
            ui,
            format!(
                "{total} {}",
                gettext(locale, if total == 1 { "reaction" } else { "reactions" })
            ),
            theme::semibold(12.5),
            palette.text,
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if theme::icon_button(
                ui,
                Icon::X,
                16.0,
                palette.secondary,
                palette.text,
                &gettext(locale, "Close"),
            )
            .clicked()
            {
                app.actions.push(Action::CloseDialog);
            }
        });
    });
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        if tab(ui, &palette, state.with("add"), None, "", false).clicked() {
            app.actions.push(Action::CloseDialog);
            app.actions.push(Action::OpenReactionPicker {
                chat: chat.to_owned(),
                message: id.to_owned(),
                beside_menu: false,
            });
        }
        let all = format!("{} {total}", gettext(locale, "All"));
        if tab(
            ui,
            &palette,
            state.with("all"),
            Some(&all),
            "",
            filter.is_none(),
        )
        .clicked()
        {
            filter = None;
        }
        for (emoji, count) in &counts {
            let label = format!("{emoji} {count}");
            let on = filter.as_ref() == Some(emoji);
            if tab(ui, &palette, state.with(emoji), Some(&label), emoji, on).clicked() {
                filter = Some(emoji.clone());
            }
        }
    });
    ui.ctx().data_mut(|data| match &filter {
        Some(emoji) => data.insert_temp(state, Some(emoji.clone())),
        None => data.insert_temp::<Option<String>>(state, None),
    });
    ui.add_space(8.0);
    let height = 5.0 * ROW;
    egui::ScrollArea::vertical()
        .id_salt(("reaction-list", chat, id))
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for reaction in message
                .reactions
                .iter()
                .filter(|r| filter.as_ref().is_none_or(|emoji| *emoji == r.emoji))
            {
                let name = app.display_name_or(&reaction.sender, None);
                let picture = app.avatar(&reaction.sender);
                let height = ROW;
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
                let own = reaction.from_me;
                let hover =
                    ui.ctx()
                        .animate_bool_with_time(response.id, own && response.hovered(), 0.12);
                if hover > 0.0 {
                    ui.painter().rect_filled(
                        rect,
                        theme::RADIUS_CONTROL,
                        palette.surface_hover.gamma_multiply(hover),
                    );
                }
                let avatar = egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 6.0 + 14.0, rect.center().y),
                    vec2(28.0, 28.0),
                );
                widgets::paint_avatar(
                    ui,
                    &palette,
                    avatar,
                    &name,
                    &reaction.sender,
                    picture.as_deref(),
                );
                let name_line = widgets::line(
                    ui,
                    &name,
                    theme::regular(13.0),
                    palette.text,
                    rect.width() - 90.0,
                    1,
                );
                let x = avatar.right() + 10.0;
                if own {
                    let hint = widgets::line(
                        ui,
                        &gettext(locale, "Tap to remove"),
                        theme::regular(11.0),
                        palette.secondary,
                        rect.width() - 90.0,
                        1,
                    );
                    let top = rect.center().y - (name_line.size().y + hint.size().y) / 2.0;
                    name_line.paint(ui, egui::pos2(x, top), palette.text);
                    hint.paint(
                        ui,
                        egui::pos2(x, top + name_line.size().y),
                        palette.secondary,
                    );
                } else {
                    name_line.paint(
                        ui,
                        egui::pos2(x, rect.center().y - name_line.size().y / 2.0),
                        palette.text,
                    );
                }
                let glyph = widgets::line(
                    ui,
                    &reaction.emoji,
                    theme::regular(16.0),
                    palette.text,
                    60.0,
                    1,
                );
                glyph.paint(
                    ui,
                    egui::pos2(
                        rect.right() - 8.0 - glyph.size().x,
                        rect.center().y - glyph.size().y / 2.0,
                    ),
                    palette.text,
                );
                if own {
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        app.actions.push(Action::React {
                            chat: chat.to_owned(),
                            message: id.to_owned(),
                            emoji: String::new(),
                        });
                    }
                }
            }
        });
}

/// A filter chip: the add button (no label), "All", or one emoji. The fill
/// eases between selected and not.
fn tab(
    ui: &mut egui::Ui,
    palette: &theme::Palette,
    id: egui::Id,
    label: Option<&str>,
    _emoji: &str,
    selected: bool,
) -> egui::Response {
    let line =
        label.map(|label| widgets::line(ui, label, theme::semibold(12.5), palette.text, 200.0, 1));
    let width = line.as_ref().map_or(44.0, |line| line.size().x + 20.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    let on = ui.ctx().animate_bool_with_time(id, selected, 0.15);
    let fill = palette
        .surface_hover
        .lerp_to_gamma(palette.accent.gamma_multiply(0.35), on);
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
    match &line {
        Some(line) => line.paint(ui, rect.center() - line.size() / 2.0, palette.text),
        None => {
            let icon = egui::Rect::from_center_size(rect.center(), vec2(20.0, 20.0));
            Icon::Smile
                .image(palette.secondary, 20.0)
                .paint_at(ui, icon);
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Opens the list for `message`.
pub(crate) fn open(chat: &str, message: &str) -> Action {
    Action::ShowDialog(Dialog::Reactions {
        chat: chat.to_owned(),
        message: message.to_owned(),
    })
}

/// The list as a small popup by the pill, in the emoji picker's style. A click
/// outside it, or Escape, closes it.
pub fn popup(app: &mut App, ctx: &egui::Context, chat: &str, id: &str) {
    use egui::{CornerRadius, Frame, Margin, Stroke};
    let palette = app.palette;
    let rows = app
        .conversations
        .get(chat)
        .and_then(|c| c.message(id))
        .map_or(0, |message| message.reactions.len());
    let outer_width = WIDTH + f32::from(MARGIN) * 2.0;
    let outer_height = 92.0 + rows.min(5) as f32 * ROW + f32::from(MARGIN) * 2.0;
    let anchor = ctx.data(|data| {
        data.get_temp::<egui::Rect>(super::conversation::bubble_id(chat, id).with("pill-rect"))
    });
    let pos = super::picker::place_picker(ctx.content_rect(), anchor, outer_width, outer_height);
    let area_id = egui::Id::new("reaction-list");
    let shown = ctx
        .data(|data| data.get_temp::<bool>(area_id.with("shown")))
        .unwrap_or(false);
    let appear = ctx.animate_bool_with_time_and_easing(
        area_id.with("appear"),
        true,
        if crate::motion::screen_reader_attached(ctx) {
            0.0
        } else {
            0.16
        },
        egui::emath::easing::quadratic_out,
    );
    let frost = super::widgets::Frost::reserve(
        ctx,
        egui::LayerId::new(egui::Order::Foreground, area_id),
        theme::RADIUS_DIALOG,
    );
    let area = egui::Area::new(area_id)
        .fixed_pos(pos + vec2(0.0, 6.0 * (1.0 - appear)))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.multiply_opacity(appear);
            Frame::new()
                .fill(palette.menu_glass())
                .stroke(Stroke::new(1.0, palette.outline))
                .corner_radius(CornerRadius::same(theme::RADIUS_DIALOG))
                .inner_margin(Margin::same(MARGIN))
                .shadow(palette.float_shadow())
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    show(app, ui, chat, id);
                })
        });
    frost.place(area.inner.response.rect);
    let clicked_outside = shown
        && ctx.input(|input| input.pointer.any_pressed())
        && ctx
            .input(|input| input.pointer.interact_pos())
            .is_some_and(|at| !area.response.rect.contains(at));
    let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    ctx.data_mut(|data| data.insert_temp(area_id.with("shown"), true));
    if clicked_outside || escape {
        ctx.data_mut(|data| data.insert_temp(area_id.with("shown"), false));
        app.actions.push(Action::CloseDialog);
    }
}
