//! Who reacted to a message, and with what: opened from its reaction pill.

use egui::{Align, Layout, Sense, vec2};

use super::conversation::{own_reaction, reaction_choice};
use super::widgets;
use crate::app::App;
use crate::i18n::gettext;
use crate::model::{Action, Dialog, Message};
use crate::theme::{self, Icon};

const ROW: f32 = 42.0;
/// People shown before the list scrolls.
const MAX_ROWS: usize = 3;
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
    let own = own_reaction(&message).map(str::to_owned);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        let tabs = ui.id().with(("reaction-tab", chat, id));
        if tab(ui, &palette, tabs.with("add"), None, false).clicked() {
            app.actions.push(Action::CloseDialog);
            app.actions.push(Action::OpenReactionPicker {
                chat: chat.to_owned(),
                message: id.to_owned(),
                beside_menu: false,
            });
        }
        // Tapping an emoji adds our own with it, or takes ours back.
        for (emoji, count) in &counts {
            let mine = own.as_deref() == Some(emoji.as_str());
            if tab(ui, &palette, tabs.with(emoji), Some((emoji, *count)), mine).clicked() {
                app.actions.push(Action::React {
                    chat: chat.to_owned(),
                    message: id.to_owned(),
                    emoji: reaction_choice(own.as_deref(), emoji),
                });
            }
        }
    });
    ui.add_space(8.0);
    let height = list_height(ui.ctx(), message.reactions.len());
    egui::ScrollArea::vertical()
        .id_salt(("reaction-list", chat, id))
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for reaction in &message.reactions {
                let name = app.display_name_or(&reaction.sender, None);
                let picture = app.avatar(&reaction.sender);
                let height = ROW;
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
                let own = reaction.from_me;
                let hover = ui.ctx().animate_bool_with_time_and_easing(
                    response.id,
                    own && response.hovered(),
                    theme::MOTION_QUICK,
                    theme::ease_out,
                );
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

/// A chip: the add button (no label), or one emoji with its count, which is
/// tinted when it is ours. The fill eases between the two. Emoji and count are
/// laid out apart, each centred on the chip, with the digits in a fixed-width
/// slot so one and two digit counts line up across chips.
fn tab(
    ui: &mut egui::Ui,
    palette: &theme::Palette,
    id: egui::Id,
    label: Option<(&str, u32)>,
    selected: bool,
) -> egui::Response {
    const GAP: f32 = 6.0;
    let parts = label.map(|(emoji, count)| {
        (
            widgets::line(ui, emoji, theme::regular(14.0), palette.text, 60.0, 1),
            widgets::line(
                ui,
                &count.to_string(),
                theme::semibold(12.5),
                palette.text,
                60.0,
                1,
            ),
        )
    });
    let digit = widgets::line(ui, "0", theme::semibold(12.5), palette.text, 20.0, 1)
        .size()
        .x;
    let width = parts.as_ref().map_or(44.0, |(emoji, count)| {
        let digits = count.size().x.max(digit);
        emoji.size().x + GAP + digits + 24.0
    });
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    let on = theme::animate(ui.ctx(), id, selected, theme::MOTION_QUICK);
    let fill = palette
        .surface_hover
        .lerp_to_gamma(palette.accent.gamma_multiply(0.35), on);
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
    match &parts {
        Some((emoji, count)) => {
            let digits = count.size().x.max(digit);
            let left = rect.center().x - (emoji.size().x + GAP + digits) / 2.0;
            emoji.paint(
                ui,
                egui::pos2(left, rect.center().y - emoji.size().y / 2.0),
                palette.text,
            );
            // Right-aligned in its slot, so the digits share a right edge.
            count.paint(
                ui,
                egui::pos2(
                    left + emoji.size().x + GAP + digits - count.size().x,
                    rect.center().y - count.size().y / 2.0,
                ),
                palette.text,
            );
        }
        None => {
            let icon = egui::Rect::from_center_size(rect.center(), vec2(20.0, 20.0));
            Icon::Smile
                .image(palette.secondary, 20.0)
                .paint_at(ui, icon);
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Height of the people list: up to three rows, then it scrolls. It eases
/// when reactions come or go.
fn list_height(ctx: &egui::Context, rows: usize) -> f32 {
    let target = rows.clamp(1, MAX_ROWS) as f32 * ROW;
    let time = if crate::motion::screen_reader_attached(ctx) {
        0.0
    } else {
        theme::MOTION_QUICK
    };
    ctx.animate_value_with_time(egui::Id::new("reaction-list-height"), target, time)
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
    let outer_height = 92.0 + list_height(ctx, rows) + f32::from(MARGIN) * 2.0;
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
            theme::MOTION_QUICK
        },
        theme::ease_out,
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
    super::closing::keep(
        ctx,
        egui::LayerId::new(egui::Order::Foreground, area_id),
        area.inner.response.rect.center_top(),
        theme::MOTION_QUICK,
        appear,
    );
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
