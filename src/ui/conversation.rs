//! The open chat: its header, the messages, and the composer.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers,
    Rect, Sense, Stroke, Vec2, pos2, vec2,
};

use super::widgets::FrostedPopup;
use crate::animation;
use crate::app::{App, Conversation, JumpHighlight, KeyScroll, RowHeight};
use crate::geo::Spot;
use crate::markup;
use crate::model::{
    Action, Chat, ChatId, Content, Delivery, Dialog, LinkPreview, Media, MediaState, Message,
    PickerTab, Scroll,
};
use crate::theme::{self, Icon, Palette};
use crate::wallpaper;

use super::focus::{self, Stop, TabStop};
use super::widgets;

/// Group-message avatar size.
const SENDER_AVATAR: f32 = 28.0;
const BODY_SIZE: f32 = widgets::MESSAGE_TEXT_SIZE;
/// Extra space above the first message of a run from one side. With the
/// list's 3 points between rows, runs stand 8 points apart and a run's
/// bubbles 3, Messages' own spacing on the Mac
/// (`CKUIBehaviorMac.balloonNonContiguousSpace` and
/// `balloonContiguousSpace`).
const RUN_GAP: f32 = 5.0;
/// How much of the transcript's width a bubble may take in the Messages
/// style (`CKUIBehaviorMac.balloonMaxWidthPercent`), and in WhatsApp's.
const MESSAGES_WIDTH_SHARE: f32 = 0.65;
const WHATSAPP_WIDTH_SHARE: f32 = 0.72;
/// The transcript's side margin in the Messages style: Messages' 11 points
/// (`CKUIBehaviorMac.minTranscriptMarginInsets`) plus the 5 points beside
/// each balloon's body that its tail curls into, at ZapFast's text size.
const MESSAGES_SIDE_MARGIN: i8 = 17;
/// The transcript's side margin in WhatsApp's style.
const WHATSAPP_SIDE_MARGIN: i8 = 18;
/// Room around a Messages bubble's contents: Messages' 12 points beside its
/// text and 7 above and below (`CKUIBehaviorMac.textBalloonPillContentInsets`)
/// at ZapFast's text size, 13.4 and 7.8, rounded.
const MESSAGES_BUBBLE_MARGIN: Margin = Margin {
    left: 13,
    right: 13,
    top: 8,
    bottom: 8,
};
/// The side of a selecting row's check box, and the column that holds it.
const SELECT_BOX: f32 = 18.0;
const SELECT_GUTTER: f32 = SELECT_BOX + 14.0;
/// Footer label on an outgoing message that failed to send.
const NOT_SENT: &str = "Not sent";
const NOT_SENT_HINT: &str =
    "This message could not be sent, and Zeus will not retry it. Send it again yourself.";

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let Some(chat) = app.current_chat().cloned() else {
        empty(app, ui);
        return;
    };
    let radius = if super::narrow(ui.ctx()) {
        0.0
    } else {
        f32::from(super::chats::CARD_RADIUS)
    };
    wallpaper::paint(ui, &app.wallpaper(), radius);
    let card = ui.max_rect();
    // While a message is edited in its bubble, the rest of the chat recedes
    // under a scrim on a layer of its own. Asked before the messages, so the
    // row they lift and the scrim agree on the frame.
    paint_edit_focus(ui.ctx(), &app.palette, &chat.id, card, radius);
    // On a vibrant window the messages scroll on under the header, as in
    // Messages: the header goes on a layer above them.
    if app.palette.vibrant() {
        let layer = egui::LayerId::new(egui::Order::Middle, ui.id().with("chat-header"));
        let bar = ui
            .scope_builder(egui::UiBuilder::new().layer_id(layer), |ui| {
                // Beneath the header: the messages under it, frosted, and a
                // veil of the chat's colour that keeps the title readable.
                let glass = ui.painter().add(egui::Shape::Noop);
                let veil = ui.painter().add(egui::Shape::Noop);
                header(app, ui, &chat);
                let bar = ui.min_rect();
                // Strongest at the top, both ease off from within the bar to
                // a little below it, so where they start never shows.
                let frosted = bar.with_max_y(bar.bottom() + HEADER_TAIL);
                let ease = HEADER_TAIL + bar.height() * HEADER_EASE;
                ui.painter()
                    .set(glass, super::glass::sheet_shape(frosted, ease, radius));
                ui.painter().set(
                    veil,
                    header_veil(
                        app.palette.vibrant_header(app.palette.chat),
                        frosted,
                        ease,
                        radius,
                    ),
                );
                // Over messages scrolled up under the header's corners.
                card_corners(ui, &app.palette, card, radius);
                bar
            })
            .inner;
        // The pinned line hangs under the header on a layer of its own, over
        // frosted messages that scroll under it, and they start below it.
        let pins = has_live_pins(app, &chat);
        let below_pins = if pins {
            // A pill floating clear of the panel's edges, as the header's own
            // buttons do.
            let rect = Rect::from_min_size(
                pos2(
                    bar.left() + PIN_FLOAT_GAP,
                    bar.bottom() + PIN_FLOAT_GAP / 2.0,
                ),
                vec2(bar.width() - PIN_FLOAT_GAP * 2.0, PIN_ROW_HEIGHT),
            );
            let layer = egui::LayerId::new(egui::Order::Middle, ui.id().with("chat-pins"));
            ui.scope_builder(
                egui::UiBuilder::new().layer_id(layer).max_rect(rect),
                |ui| {
                    let glass = ui.painter().add(egui::Shape::Noop);
                    pin_banner(app, ui, &chat, true);
                    ui.painter().set(
                        glass,
                        super::glass::sheet_rounded(rect, PIN_ROW_HEIGHT / 2.0),
                    );
                },
            );
            rect.bottom() + PIN_FLOAT_GAP / 2.0
        } else {
            bar.bottom()
        };
        let bar = bar.with_max_y(below_pins);
        // The composer too floats on its own layer, over frosted messages
        // that scroll on under it, as under the header.
        let area = ui.available_rect_before_wrap();
        let layer = egui::LayerId::new(egui::Order::Middle, ui.id().with("chat-composer"));
        let foot = ui
            // Laid out from the bottom: a scope's own area spans from where
            // its layout starts to its last widget, and on this layer above
            // the messages it would take every click over them.
            .scope_builder(
                egui::UiBuilder::new()
                    .layer_id(layer)
                    .layout(Layout::bottom_up(Align::Min)),
                |ui| {
                    composer(app, ui, &chat);
                    // From a little above the field, or the reply, edit or voice
                    // strip over it, to the bottom, as wide as the chat. Nothing
                    // is blurred under it: the wallpaper and the theme's tint
                    // show through the translucent field.
                    let pill = ui
                        .ctx()
                        .data(|data| data.get_temp::<Rect>(composer_pill_id()))
                        .unwrap_or(area);
                    let top = pill.top().min(ui.min_rect().top());
                    let foot = Rect::from_x_y_ranges(
                        area.x_range(),
                        (top - 12.0).max(area.top())..=area.bottom(),
                    );
                    card_corners(ui, &app.palette, card, radius);
                    foot
                },
            )
            .inner;
        let list = area.with_min_y(bar.top());
        // A strip opening over the field slides the messages up with it, so
        // the last one is never left behind it.
        let over = ui.ctx().animate_value_with_time(
            ui.id().with("composer-over"),
            foot.height(),
            theme::MOTION_QUICK,
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
            messages(app, ui, &chat, bar.height(), over);
        });
        // A vibrant window is flat, as Messages is: no shadows over the chat.
        return;
    }
    let header = header(app, ui, &chat);
    pin_banner(app, ui, &chat, false);
    composer(app, ui, &chat);
    messages(app, ui, &chat, 0.0, 0.0);
    // Over the messages, which scroll under the header.
    widgets::paint_shadow_below(
        ui,
        &app.palette,
        header.left(),
        header.right(),
        header.bottom(),
    );
}

/// Rounds off the corners of the conversation's `card` over what is painted
/// square beneath a vibrant header or composer, such as messages scrolled
/// under them, where the sidebar's gutter around the card is opaque. The
/// header's own glass and veil keep to the corners themselves.
fn card_corners(ui: &egui::Ui, palette: &Palette, card: Rect, radius: f32) {
    let gutter = palette.rail();
    if radius > 0.0 && gutter.is_opaque() {
        ui.painter()
            .add(widgets::card_corners(card, radius, gutter));
    }
}

/// Height of the pinned-message line under the header. It is one line
/// whatever the number of pins, as on WhatsApp: it shows one pin at a time.
const PIN_ROW_HEIGHT: f32 = 44.0;
/// Gap between the banner edge and the pin count bar, and the icon's own box.
const PIN_ROW_INSET: f32 = 12.0;
const PIN_ICON: f32 = 16.0;
/// Room around the floating pin pill, beside it and above and below it.
const PIN_FLOAT_GAP: f32 = 12.0;
/// The bar at the line's left edge: one segment per pin, the shown one lit.
const PIN_BAR_WIDTH: f32 = 3.0;
const PIN_BAR_GAP: f32 = 2.0;

/// Which pin the line shows: the one it showed last, or the newest pin when
/// that one is gone. `rows` is newest first.
fn shown_pin(rows: &[crate::archive::Pinned], cursor: Option<&str>) -> usize {
    cursor
        .and_then(|id| rows.iter().position(|row| row.id == id))
        .unwrap_or(0)
}

/// The pin a click leaves on the line: the next older one, back to the
/// newest after the oldest, so repeated clicks walk every pin.
fn next_pin(rows: usize, shown: usize) -> usize {
    if rows == 0 { 0 } else { (shown + 1) % rows }
}

/// Where each segment of the pin count bar goes, top to bottom, for `count`
/// pins in a bar `height` tall. The newest pin is the bottom segment, as on
/// WhatsApp.
fn pin_bar_segments(top: f32, height: f32, count: usize) -> Vec<(f32, f32)> {
    let count = count.max(1);
    let gaps = PIN_BAR_GAP * (count - 1) as f32;
    let segment = ((height - gaps) / count as f32).max(1.0);
    (0..count)
        .map(|index| {
            let start = top + (segment + PIN_BAR_GAP) * index as f32;
            (start, start + segment)
        })
        .collect()
}

/// The chat's pinned messages under its header, one line like WhatsApp's: it
/// shows one pin, the newest first, with the message's own words cut off with
/// an ellipsis when they do not fit. Clicking it opens that message and moves
/// the line on to the next pin, so repeated clicks walk the chat's pins. A bar
/// at its left has a segment per pin and lights the one shown.
fn pin_banner(app: &mut App, ui: &mut egui::Ui, chat: &Chat, floating: bool) {
    let now = crate::util::now();
    if let Some(wait) = app.chat_pins.get(&chat.id).and_then(|rows| {
        rows.iter()
            .filter(|row| row.expires_at > now)
            .map(|row| row.expires_at)
            .min()
    }) {
        let left = wait.saturating_sub(now).clamp(0, 86_400) as u64;
        ui.ctx()
            .request_repaint_after(Duration::from_secs(left.saturating_add(1)));
    }
    let rows = live_pins(
        app.chat_pins.get(&chat.id).map_or(&[][..], Vec::as_slice),
        now,
    );
    if rows.is_empty() {
        return;
    }
    // Which pin is shown is the view's own state, kept per chat.
    let cursor_id = pin_cursor_id(&chat.id);
    let cursor = ui.ctx().data(|data| data.get_temp::<String>(cursor_id));
    let shown = shown_pin(&rows, cursor.as_deref());
    let row = &rows[shown];
    let palette = app.palette;
    let (bar, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), PIN_ROW_HEIGHT), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let radius = if floating { PIN_ROW_HEIGHT / 2.0 } else { 0.0 };
    ui.painter().rect_filled(
        bar,
        radius,
        if response.hovered() {
            palette.surface_hover
        } else if floating {
            palette.frosted(palette.panel)
        } else {
            palette.panel
        },
    );
    // Newest pin at the bottom, so the bar fills upward as pins are added.
    let segments = pin_bar_segments(bar.top() + 8.0, bar.height() - 16.0, rows.len());
    let left = bar.left() + PIN_ROW_INSET + if floating { 8.0 } else { 0.0 };
    for (index, (top, bottom)) in segments.iter().enumerate() {
        let pin = rows.len() - 1 - index;
        let colour = if pin == shown {
            palette.accent
        } else {
            palette.dim
        };
        ui.painter().rect_filled(
            Rect::from_x_y_ranges(left..=left + PIN_BAR_WIDTH, *top..=*bottom),
            PIN_BAR_WIDTH / 2.0,
            colour,
        );
    }
    let icon_left = left + PIN_BAR_WIDTH + 10.0;
    let icon = Rect::from_center_size(
        pos2(icon_left + PIN_ICON / 2.0, bar.center().y),
        Vec2::splat(PIN_ICON),
    );
    theme::paint_icon(ui, Icon::Pin, icon, 14.0, palette.accent);
    let text_left = icon_left + PIN_ICON + 8.0;
    let text_width =
        (bar.right() - PIN_ROW_INSET - if floating { 8.0 } else { 0.0 } - text_left).max(1.0);
    let label = widgets::line(
        ui,
        &crate::markup::plain(&app.resolve_mention_tokens(&row.text), &[]),
        theme::regular(13.0),
        palette.text,
        text_width,
        1,
    );
    label.paint(
        ui,
        pos2(text_left, bar.center().y - label.size().y / 2.0),
        palette.text,
    );
    theme::reveal_focus(&response);
    let described = crate::i18n::gettext(app.locale, "Pinned message {index} of {count}: {text}")
        .replace("{index}", &(shown + 1).to_string())
        .replace("{count}", &rows.len().to_string())
        .replace("{text}", &row.text);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &described)
    });
    if response.clicked() {
        app.actions.push(Action::OpenMessage {
            chat: chat.id.clone(),
            message: row.id.clone(),
        });
        let next = rows[next_pin(rows.len(), shown)].id.clone();
        ui.ctx().data_mut(|data| data.insert_temp(cursor_id, next));
    }
}

/// Whether the chat has a pin the line under the header would show.
fn has_live_pins(app: &App, chat: &Chat) -> bool {
    let now = crate::util::now();
    app.chat_pins
        .get(&chat.id)
        .is_some_and(|rows| rows.iter().any(|row| row.expires_at > now))
}

/// Where the pin line keeps the id of the pin it shows, per chat.
fn pin_cursor_id(chat: &str) -> egui::Id {
    egui::Id::new(("pin-cursor", chat))
}

fn empty(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let rect = ui.max_rect();
    let center = rect.center() - vec2(0.0, 30.0);
    theme::logo(
        ui,
        center - vec2(0.0, 60.0),
        72.0,
        palette.surface,
        palette.dim,
    );
    ui.painter().text(
        center,
        Align2::CENTER_CENTER,
        "Zeus",
        theme::bold(24.0),
        palette.text,
    );
    ui.painter().text(
        center + vec2(0.0, 30.0),
        Align2::CENTER_CENTER,
        if app.chats.is_empty() {
            crate::i18n::gettext(app.locale, "Your chats appear on the left as they load.")
        } else {
            crate::i18n::gettext(app.locale, "Select a chat on the left.")
        },
        theme::regular(14.0),
        palette.secondary,
    );
    if app.settings.show_shortcut_hints {
        ui.painter().text(
            center + vec2(0.0, 56.0),
            Align2::CENTER_CENTER,
            super::keys::label(
                crate::i18n::gettext(app.locale, "Ctrl+K to search · ? for keyboard shortcuts")
                    .as_ref(),
            ),
            theme::regular(12.5),
            palette.dim,
        );
    }
}

/// The share of a vibrant header, from its bottom, over which its frost and
/// veil begin to ease off.
pub(super) const HEADER_EASE: f32 = 0.35;
/// How far below a vibrant header its frost and veil carry on, easing off.
pub(super) const HEADER_TAIL: f32 = 24.0;
/// Rows of the veil's ease, enough for its curve to look smooth.
const VEIL_STEPS: usize = 8;

/// The veil over a vibrant header's frost: `top` at the top, then
/// easing to nothing over the bottom `ease` points of `rect` along the same
/// curve as the frost, so the messages under it never stop at a line. Its
/// top corners follow the card's `radius`, so they stay round over a
/// see-through window too.
pub(super) fn header_veil(top: Color32, rect: Rect, ease: f32, radius: f32) -> egui::Shape {
    let start = rect.bottom() - ease;
    let radius = radius.clamp(0.0, (rect.height() - ease).max(0.0));
    let mut mesh = egui::Mesh::default();
    let mut row = |y: f32, inset: f32, color: Color32| {
        mesh.colored_vertex(pos2(rect.left() + inset, y), color);
        mesh.colored_vertex(pos2(rect.right() - inset, y), color);
    };
    // Down each top corner's quarter circle, then straight.
    if radius > 0.0 {
        for step in 0..CORNER_STEPS {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / CORNER_STEPS as f32;
            let y = rect.top() + radius - radius * angle.cos();
            row(y, radius - radius * angle.sin(), top);
        }
    }
    row(rect.top() + radius, 0.0, top);
    for step in 0..=VEIL_STEPS {
        let t = step as f32 / VEIL_STEPS as f32;
        // smoothstep, as the frost's shader eases.
        let strength = 1.0 - t * t * (3.0 - 2.0 * t);
        row(start + ease * t, 0.0, top.gamma_multiply(strength));
    }
    let rows = mesh.vertices.len() as u32 / 2;
    for row in 0..rows - 1 {
        let at = row * 2;
        mesh.add_triangle(at, at + 1, at + 3);
        mesh.add_triangle(at, at + 3, at + 2);
    }
    egui::Shape::mesh(mesh)
}

/// Rows down each of the veil's rounded top corners.
const CORNER_STEPS: usize = 8;

/// How round the glass under a vibrant header's name is: straight-sided,
/// leaving room around the text, rather than a capsule.
const NAME_GLASS_RADIUS: f32 = 18.0;
/// The room around the name and status on their glass: wide at the sides,
/// and little above, so the glass keeps clear of the window's top edge.
const NAME_GLASS_PADDING: Vec2 = vec2(14.0, 3.5);
/// The room above and below a name alone on its glass.
const NAME_GLASS_SINGLE: f32 = 7.0;
/// How tall the name and status stand together on glass: less than the
/// avatar, so the two lines sit a little closer around its middle.
const NAME_GLASS_LINES: f32 = 32.0;
/// How wide the round glass under each of a vibrant header's buttons is.
const BUTTON_GLASS: f32 = 36.0;
/// The space between two of a vibrant header's buttons, beyond their glass.
pub(super) const BUTTON_GAP: f32 = 10.0;

/// The round glass under one of a vibrant header's buttons.
pub(super) fn glass_button(palette: &Palette, button: Rect) -> egui::Shape {
    glass(
        palette,
        Rect::from_center_size(button.center(), Vec2::splat(BUTTON_GLASS)),
        BUTTON_GLASS / 2.0,
    )
}

/// Glass under a vibrant header's name or one of its buttons: the veil
/// thickened where they sit, with a faint edge, as Messages floats them.
fn glass(palette: &Palette, rect: Rect, radius: f32) -> egui::Shape {
    let radius = CornerRadius::from(radius);
    egui::Shape::Vec(vec![
        egui::Shape::rect_filled(rect, radius, palette.vibrant_pill(palette.chat)),
        egui::Shape::rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, palette.text.gamma_multiply(0.08)),
            egui::StrokeKind::Inside,
        ),
    ])
}

fn header(app: &mut App, ui: &mut egui::Ui, chat: &Chat) -> Rect {
    let palette = app.palette;
    let title = app.chat_title(chat);
    // Beside the list or the rail, the header's row sits as far down as the
    // list's title row, so the two read as one line; the card's own gutter
    // already gives part of it. A narrow window shows the chat alone and
    // keeps the header tight.
    let drop = if super::narrow(ui.ctx()) {
        0
    } else {
        (super::chats::HEADER_DROP - super::chats::CARD_GAP) as i8
    };
    egui::Panel::top("chat-header")
        .show_separator_line(false)
        .frame(
            Frame::new()
                // On a vibrant window the messages scroll on under it,
                // frosted, with the veil drawn beneath the header instead.
                .fill(if palette.vibrant() {
                    Color32::TRANSPARENT
                } else {
                    palette.panel
                })
                .inner_margin(Margin {
                    left: 14,
                    right: 14,
                    top: 8 + drop,
                    bottom: 8,
                }),
        )
        .show(ui, |ui| {
            // The chat list, expanded or collapsed, clears the traffic lights.
            if theme::chrome_in_headers(ui.ctx()) {
                super::titlebar_drag(ui, ui.max_rect());
            }
            let narrow = super::narrow(ui.ctx());
            let row = ui.horizontal(|ui| {
                // Give both rows a fixed height so their contents align.
                ui.set_min_height(HEADER_ROW);
                // With no chat list beside it, the header clears the traffic
                // lights itself.
                if narrow && theme::macos_chrome(ui.ctx()) {
                    ui.add_space(theme::traffic_light_inset(ui.ctx()));
                } else if app.sidebar_mode() == crate::model::SidebarDisplayMode::CollapsedIconsOnly
                    && app.page == crate::model::Page::Chats
                {
                    // Beside the rail alone, the lights reach over this header.
                    ui.add_space(super::rail::lights_overhang(ui.ctx()));
                }
                let picture = app.avatar(&chat.id);
                let (subtitle, color) = subtitle(app, chat);
                let call_here = app.call_live_here(&chat.id);
                // More and Search, and Back in a narrow window, and the call
                // buttons in a person's or a group's chat, each on its own
                // glass with room around it on a vibrant window.
                let call_buttons = if callable(chat, app.me.as_deref()) {
                    if call_here { 1.0 } else { 2.0 }
                } else {
                    0.0
                };
                let buttons = if narrow { 3.0 } else { 2.0 } + call_buttons;
                let right_controls = if palette.vibrant() {
                    buttons * 36.0 + (buttons - 0.5) * BUTTON_GAP + (BUTTON_GLASS - 30.0)
                } else {
                    buttons * 36.0
                };
                // On a vibrant window the name and the buttons sit on glass
                // pills, drawn beneath them once they are measured.
                let name_pill = ui.painter().add(egui::Shape::Noop);
                let mut text = Rect::NOTHING;
                let mut avatar = Rect::NOTHING;
                // Treat the avatar, name, and subtitle as one info button.
                let block = ui
                    .scope(|ui| {
                        // Fix the child height before centering its contents.
                        ui.allocate_ui_with_layout(
                            vec2(
                                (ui.available_width() - right_controls).max(80.0),
                                HEADER_ROW,
                            ),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let avatar_response = widgets::avatar(
                                    ui,
                                    &palette,
                                    &title,
                                    &chat.id,
                                    40.0,
                                    picture.as_deref(),
                                );
                                avatar = avatar_response.rect;
                                if chat.disappearing_timer().is_some() {
                                    widgets::paint_disappearing_badge(
                                        ui,
                                        &palette,
                                        avatar_response.rect,
                                    );
                                }
                                // Room for the name's glass pill to clear the avatar.
                                ui.add_space(if palette.vibrant() { 14.0 } else { 4.0 });
                                ui.vertical(|ui| {
                                    let width = (ui.available_width() - right_controls).max(80.0);
                                    ui.set_max_width(width);
                                    // On glass the lines centre on the avatar: the
                                    // column starts at the row's top otherwise.
                                    if palette.vibrant() {
                                        let column = if subtitle.is_empty() {
                                            40.0
                                        } else {
                                            NAME_GLASS_LINES
                                        };
                                        let top = avatar.center().y - column / 2.0;
                                        ui.add_space((top - ui.cursor().top()).max(0.0));
                                    }
                                    if subtitle.is_empty() {
                                        // Center the name on the avatar.
                                        ui.allocate_ui_with_layout(
                                            vec2(width, 40.0),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                text = widgets::rich_text(
                                                    ui,
                                                    &title,
                                                    // Lighter on glass, as Apple's.
                                                    if palette.vibrant() {
                                                        theme::medium(15.0)
                                                    } else {
                                                        theme::semibold(17.0)
                                                    },
                                                    palette.text,
                                                )
                                                .rect;
                                            },
                                        );
                                    } else {
                                        // Align the name and subtitle with the avatar edges,
                                        // or on glass a little closer, around its middle.
                                        ui.allocate_ui_with_layout(
                                            vec2(
                                                width,
                                                if palette.vibrant() {
                                                    NAME_GLASS_LINES
                                                } else {
                                                    40.0
                                                },
                                            ),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                text = widgets::rich_text(
                                                    ui,
                                                    &title,
                                                    theme::semibold(if palette.vibrant() {
                                                        14.0
                                                    } else {
                                                        15.0
                                                    }),
                                                    palette.text,
                                                )
                                                .rect;
                                                ui.with_layout(
                                                    Layout::bottom_up(Align::Min),
                                                    |ui| {
                                                        text = text.union(
                                                            widgets::rich_text(
                                                                ui,
                                                                &subtitle,
                                                                theme::regular(
                                                                    if palette.vibrant() {
                                                                        11.5
                                                                    } else {
                                                                        12.5
                                                                    },
                                                                ),
                                                                color,
                                                            )
                                                            .rect,
                                                        );
                                                    },
                                                );
                                            },
                                        );
                                    }
                                });
                            },
                        );
                    })
                    .response;
                if palette.vibrant() && text.is_positive() {
                    // A lone name gets Apple's roomier padding.
                    let padding = vec2(
                        NAME_GLASS_PADDING.x,
                        if subtitle.is_empty() {
                            NAME_GLASS_SINGLE
                        } else {
                            NAME_GLASS_PADDING.y
                        },
                    );
                    // Centred on the avatar, whatever room the lines leave
                    // above and below their glyphs.
                    let glass_rect = Rect::from_center_size(
                        pos2(text.center().x, avatar.center().y),
                        text.size() + 2.0 * padding,
                    );
                    ui.painter()
                        .set(name_pill, glass(&palette, glass_rect, NAME_GLASS_RADIUS));
                }
                let block = ui
                    .interact(block.rect, ui.id().with("chat-header-info"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if block.clicked() {
                    app.actions.push(Action::OpenInfo(chat.id.clone()));
                }
                // The item and the width that has to hold it are measured from
                // the same localized label: a translation wider than the
                // English one would otherwise be clipped.
                let leave_label = if chat.is_channel() {
                    crate::i18n::gettext(app.locale, "Leave channel")
                } else {
                    crate::i18n::gettext(app.locale, "Leave group")
                };
                let select_messages_label = crate::i18n::gettext(app.locale, "Select messages");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Each button on a vibrant window floats on its own glass,
                    // with room around it.
                    if palette.vibrant() {
                        ui.add_space(BUTTON_GAP / 2.0);
                    }
                    let more_glass = ui.painter().add(egui::Shape::Noop);
                    let more = theme::icon_button(
                        ui,
                        Icon::Ellipsis,
                        18.0,
                        palette.secondary,
                        palette.text,
                        &crate::i18n::gettext(app.locale, "More"),
                    );
                    if palette.vibrant() {
                        ui.painter()
                            .set(more_glass, glass_button(&palette, more.rect));
                    }
                    let info_label = crate::i18n::gettext(app.locale, "Info");
                    let pin_label = if chat.pinned {
                        crate::i18n::gettext(app.locale, "Unpin")
                    } else {
                        crate::i18n::gettext(app.locale, "Pin to top")
                    };
                    let archive_label = if chat.archived {
                        crate::i18n::gettext(app.locale, "Unarchive")
                    } else {
                        crate::i18n::gettext(app.locale, "Archive")
                    };
                    let clear_label = crate::i18n::gettext(app.locale, "Clear chat");
                    let copy_number_label = crate::i18n::gettext(app.locale, "Copy number");
                    let close_label = crate::i18n::gettext(app.locale, "Close chat");
                    let width = widgets::menu_width(
                        ui,
                        &[
                            info_label.as_ref(),
                            select_messages_label.as_ref(),
                            pin_label.as_ref(),
                            archive_label.as_ref(),
                            clear_label.as_ref(),
                            leave_label.as_ref(),
                            copy_number_label.as_ref(),
                            close_label.as_ref(),
                        ],
                        true,
                    );
                    // Demo/test: hold this menu open for a screenshot.
                    #[cfg(any(test, feature = "demo"))]
                    if app.open_header_menu.as_deref() == Some(chat.id.as_str()) {
                        egui::Popup::open_id(ui.ctx(), more.id.with("popup"));
                    }
                    egui::Popup::menu(&more)
                        .width(width)
                        .frame(widgets::menu_frame(&palette))
                        .show_frosted(ui.ctx(), |ui| {
                            if widgets::menu_item(
                                ui,
                                &palette,
                                Some(Icon::Info),
                                info_label.as_ref(),
                            ) {
                                app.actions.push(Action::OpenInfo(chat.id.clone()));
                            }
                            if widgets::menu_item(
                                ui,
                                &palette,
                                Some(Icon::Check),
                                select_messages_label.as_ref(),
                            ) {
                                app.actions.push(Action::StartSelection);
                            }
                            if widgets::menu_item(
                                ui,
                                &palette,
                                Some(if chat.pinned { Icon::PinOff } else { Icon::Pin }),
                                pin_label.as_ref(),
                            ) {
                                app.actions
                                    .push(Action::SetPinned(chat.id.clone(), !chat.pinned));
                            }
                            if widgets::menu_item(
                                ui,
                                &palette,
                                Some(Icon::Archive),
                                archive_label.as_ref(),
                            ) {
                                app.actions
                                    .push(Action::SetArchived(chat.id.clone(), !chat.archived));
                            }
                            // Clearing reaches the phone, so it waits for a
                            // connection, as deleting does in the chat list.
                            if widgets::menu_item_enabled(
                                ui,
                                &palette,
                                Some(Icon::Eraser),
                                clear_label.as_ref(),
                                app.is_connected(),
                            ) {
                                app.actions
                                    .push(Action::ShowDialog(Dialog::ConfirmClearChat(
                                        chat.id.clone(),
                                    )));
                            }
                            widgets::menu_separator(ui, &palette);
                            if chat.can_leave(&app.our_ids())
                                && widgets::menu_item(
                                    ui,
                                    &palette,
                                    Some(Icon::LogOut),
                                    leave_label.as_ref(),
                                )
                            {
                                app.actions
                                    .push(Action::ShowDialog(Dialog::ConfirmLeaveGroup(
                                        chat.id.clone(),
                                    )));
                            }
                            if let Some(phone) = chat.phone()
                                && widgets::menu_item(
                                    ui,
                                    &palette,
                                    Some(Icon::Copy),
                                    copy_number_label.as_ref(),
                                )
                            {
                                app.actions.push(Action::CopyText(format!("+{phone}")));
                            }
                            if widgets::menu_item(ui, &palette, Some(Icon::X), close_label.as_ref())
                            {
                                app.actions.push(Action::CloseChat);
                            }
                        });
                    // A person's chat calls them and a group's starts a call
                    // in the group; a channel, a broadcast list and the chat
                    // with ourselves have no call button. While this chat has
                    // the call, the button ends it.
                    if call_buttons > 0.0 {
                        let (call_tooltip, call_icon, call_fill, call) = if call_here {
                            (
                                crate::i18n::gettext(app.locale, "Hang up").into_owned(),
                                Icon::PhoneOff,
                                palette.danger,
                                Action::HangupCall,
                            )
                        } else {
                            (
                                crate::i18n::gettext(app.locale, "Voice call").into_owned(),
                                Icon::Phone,
                                palette.secondary,
                                Action::StartCall(chat.id.clone()),
                            )
                        };
                        // On a vibrant window each sits on its own glass, as
                        // Search and More do.
                        if palette.vibrant() {
                            ui.add_space(BUTTON_GAP);
                        }
                        let call_glass = ui.painter().add(egui::Shape::Noop);
                        let call_button = theme::icon_button(
                            ui,
                            call_icon,
                            18.0,
                            call_fill,
                            palette.text,
                            &call_tooltip,
                        );
                        if palette.vibrant() {
                            ui.painter()
                                .set(call_glass, glass_button(&palette, call_button.rect));
                        }
                        if call_button.clicked() {
                            app.actions.push(call);
                        }
                        if !call_here {
                            let tip = crate::i18n::gettext(app.locale, "Video call");
                            if palette.vibrant() {
                                ui.add_space(BUTTON_GAP);
                            }
                            let video_glass = ui.painter().add(egui::Shape::Noop);
                            let video = theme::icon_button(
                                ui,
                                Icon::Video,
                                18.0,
                                palette.secondary,
                                palette.text,
                                &tip,
                            );
                            if palette.vibrant() {
                                ui.painter()
                                    .set(video_glass, glass_button(&palette, video.rect));
                            }
                            if video.clicked() {
                                app.actions.push(Action::StartVideoCall(chat.id.clone()));
                            }
                        }
                    }
                    let searching = app.chat_search_open;
                    let tip = format!(
                        "{} ({})",
                        crate::i18n::gettext(app.locale, "Search messages"),
                        super::keys::label("Ctrl+F")
                    );
                    if palette.vibrant() {
                        ui.add_space(BUTTON_GAP);
                    }
                    let search_glass = ui.painter().add(egui::Shape::Noop);
                    let search = theme::icon_button(
                        ui,
                        Icon::Search,
                        18.0,
                        if searching {
                            palette.accent
                        } else {
                            palette.secondary
                        },
                        palette.text,
                        &tip,
                    )
                    .tab_stop(Stop::ChatSearch);
                    if palette.vibrant() {
                        ui.painter()
                            .set(search_glass, glass_button(&palette, search.rect));
                    }
                    if search.clicked() {
                        app.actions.push(if searching {
                            Action::CloseChatSearch
                        } else {
                            Action::OpenChatSearch
                        });
                    }
                    // A narrow window shows the list or the chat, not both;
                    // this is the way back to the list.
                    if narrow {
                        if palette.vibrant() {
                            ui.add_space(BUTTON_GAP);
                        }
                        let back_glass = ui.painter().add(egui::Shape::Noop);
                        // On the Calls tab the list underneath is the call
                        // history.
                        let back_label = if app.page == crate::model::Page::Calls {
                            crate::i18n::gettext(app.locale, "Back to the calls").into_owned()
                        } else {
                            "Back to chats".to_owned()
                        };
                        let back = theme::icon_button(
                            ui,
                            Icon::ArrowLeft,
                            18.0,
                            palette.secondary,
                            palette.text,
                            &back_label,
                        )
                        .tab_stop(Stop::Back);
                        if palette.vibrant() {
                            ui.painter()
                                .set(back_glass, glass_button(&palette, back.rect));
                        }
                        ui.ctx()
                            .data_mut(|data| data.insert_temp(back_button_id(), back.rect));
                        if back.clicked() {
                            app.actions.push(Action::CloseChat);
                        }
                    }
                });
            });
            ui.ctx()
                .data_mut(|data| data.insert_temp(header_row_id(), row.response.rect));
        })
        .response
        .rect
}

/// Where a narrow window's Back was laid out, for interaction tests.
pub(crate) fn back_button_id() -> egui::Id {
    egui::Id::new("conversation-back")
}

/// Where the conversation header's row was laid out, for layout tests.
pub(crate) fn header_row_id() -> egui::Id {
    egui::Id::new("conversation-header-row")
}

/// Chat-header subtitle.
fn subtitle(app: &App, chat: &Chat) -> (String, Color32) {
    let palette = app.palette;
    let typing = app.typing_in(&chat.id);
    if !typing.is_empty() {
        let text = if chat.is_group() {
            let names: Vec<&str> = typing.iter().map(|(_, name)| name.as_str()).collect();
            match names.as_slice() {
                [] => String::new(),
                [one] => format!("{one} is typing…"),
                [rest @ .., last] => format!("{} and {last} are typing…", rest.join(", ")),
            }
        } else {
            "typing…".to_owned()
        };
        return (text, palette.accent);
    }
    if chat.is_group() {
        let names = app.participant_names(chat);
        return (
            if names.is_empty() {
                "Group".to_owned()
            } else {
                names
            },
            palette.secondary,
        );
    }
    if let Some(presence) = app.presence.get(&chat.id) {
        if presence.online {
            return ("online".to_owned(), palette.accent);
        }
        if let Some(seen) = presence.last_seen {
            return (crate::util::last_seen(app.locale, seen), palette.secondary);
        }
    }
    match chat.phone() {
        Some(phone) if !app.is_saved_contact(&chat.id) => {
            (crate::util::phone(phone), palette.secondary)
        }
        _ => (String::new(), palette.secondary),
    }
}

/// Byte position of a freshly typed standalone trigger immediately before
/// the text cursor. Colons inside times and URLs, and `@` inside addresses,
/// remain ordinary text.
fn standalone_trigger(text: &str, cursor: usize, trigger: char) -> Option<usize> {
    let cursor = text
        .char_indices()
        .nth(cursor)
        .map_or(text.len(), |(at, _)| at);
    let (at, found) = text[..cursor].char_indices().next_back()?;
    if found != trigger {
        return None;
    }
    (at == 0
        || text[..at]
            .chars()
            .next_back()
            .is_some_and(|character| !character.is_alphanumeric()))
    .then_some(at)
}

/// Active mention query from its `@` through the current text cursor.
fn active_mention(text: &str, start: Option<usize>, cursor: usize) -> Option<(usize, &str)> {
    let start = start?;
    let end = text
        .char_indices()
        .nth(cursor)
        .map_or(text.len(), |(at, _)| at);
    let query = text.get(start.checked_add(1)?..end)?;
    (!query.contains(['@', '\n'])).then_some((end, query))
}

/// Active emoji query from its `:` through the current text cursor. Spaces
/// and punctuation end autocomplete without changing what the user typed.
fn active_emoji(text: &str, start: Option<usize>, cursor: usize) -> Option<(usize, &str)> {
    let start = start?;
    let after = start.checked_add(1)?;
    if text.get(start..after) != Some(":") {
        return None;
    }
    let end = text
        .char_indices()
        .nth(cursor)
        .map_or(text.len(), |(at, _)| at);
    let query = text.get(after..end)?;
    query
        .chars()
        .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | '+'))
        .then_some((end, query))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EmojiSuggestion {
    emoji: &'static str,
    shortcode: String,
    name: &'static str,
}

fn emoji_match_score(emoji: &emojis::Emoji, query: &str) -> Option<u8> {
    let name = emoji.name();
    let shortcodes = emoji.shortcodes();
    if shortcodes.clone().any(|code| code == query) {
        Some(0)
    } else if shortcodes.clone().any(|code| code.starts_with(query)) {
        Some(1)
    } else if name == query {
        Some(2)
    } else if name.starts_with(query)
        || name
            .split([' ', '-', '_'])
            .any(|word| word.starts_with(query))
    {
        Some(3)
    } else if shortcodes.clone().any(|code| code.contains(query)) {
        Some(4)
    } else if name.contains(query) {
        Some(5)
    } else {
        None
    }
}

fn emoji_suggestion(emoji: &'static emojis::Emoji) -> EmojiSuggestion {
    let shortcode = emoji
        .shortcode()
        .map_or_else(|| emoji.name().replace([' ', '-'], "_"), str::to_owned);
    EmojiSuggestion {
        emoji: emoji.as_str(),
        shortcode: format!(":{shortcode}:"),
        name: emoji.name(),
    }
}

/// Completions for `:query`. `emojis::iter` yields each emoji once, and a
/// skin-tone-capable one such as 👍 reports `Some(SkinTone::Default)`.
fn emoji_candidates(app: &App, query: &str) -> Vec<EmojiSuggestion> {
    const LIMIT: usize = 6;
    let query = query.to_lowercase();
    let mut seen = HashSet::new();
    if query.is_empty() {
        let recent = app
            .settings
            .recent_emoji
            .iter()
            .filter_map(|emoji| emojis::get(emoji))
            .chain(emojis::iter())
            .filter(|emoji| seen.insert(emoji.as_str()))
            .take(LIMIT)
            .map(emoji_suggestion)
            .collect();
        return recent;
    }

    let mut found: Vec<_> = emojis::iter()
        .enumerate()
        .filter_map(|(order, emoji)| {
            emoji_match_score(emoji, &query).map(|score| (score, order, emoji))
        })
        .collect();
    found.sort_by_key(|(score, order, _)| (*score, *order));
    found
        .into_iter()
        .filter(|(_, _, emoji)| seen.insert(emoji.as_str()))
        .take(LIMIT)
        .map(|(_, _, emoji)| emoji_suggestion(emoji))
        .collect()
}

fn take_plain_key(ui: &mut egui::Ui, key: Key) -> bool {
    ui.input_mut(|input| {
        let mut taken = false;
        input.events.retain(|event| {
            if taken {
                return true;
            }
            let matches = matches!(
                event,
                egui::Event::Key {
                    key: found,
                    pressed: true,
                    modifiers,
                    ..
                } if *found == key && *modifiers == Modifiers::NONE
            );
            taken |= matches;
            !matches
        });
        taken
    })
}

/// Slack-style emoji suggestions above the composer. The composer keeps
/// focus, so ordinary typing continues refining the query.
fn emoji_suggestions(app: &mut App, ui: &mut egui::Ui, field: egui::Id) {
    let cursor = egui::TextEdit::load_state(ui.ctx(), field)
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.primary.index.0)
        .unwrap_or_else(|| app.composer.chars().count());
    let Some((end, query)) = active_emoji(&app.composer, app.emoji_start, cursor) else {
        app.emoji_start = None;
        return;
    };
    let start = app.emoji_start.expect("checked above");
    let candidates = emoji_candidates(app, query);
    if candidates.is_empty() {
        return;
    }

    let down = take_plain_key(ui, Key::ArrowDown);
    let up = take_plain_key(ui, Key::ArrowUp);
    if down {
        app.emoji_selected = (app.emoji_selected + 1) % candidates.len();
    }
    if up {
        app.emoji_selected = (app.emoji_selected + candidates.len() - 1) % candidates.len();
    }
    app.emoji_selected = app.emoji_selected.min(candidates.len() - 1);
    let submit = take_plain_key(ui, Key::Enter) || take_plain_key(ui, Key::Tab);
    let mut picked = submit.then(|| candidates[app.emoji_selected].clone());
    let palette = app.palette;

    let list = widgets::raised(ui, &palette, suggestion_frame(&palette), |ui| {
        let row_height = 36.0;
        ui.spacing_mut().item_spacing.y = 0.0;
        for (index, candidate) in candidates.iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
            if index == app.emoji_selected {
                ui.painter().rect_filled(
                    rect,
                    SUGGESTION_ROW_RADIUS,
                    palette.accent.gamma_multiply(0.18),
                );
                ui.painter().rect_stroke(
                    rect,
                    SUGGESTION_ROW_RADIUS,
                    Stroke::new(1.0, palette.accent),
                    egui::StrokeKind::Inside,
                );
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(rect, SUGGESTION_ROW_RADIUS, palette.surface_hover);
            }

            let emoji = widgets::line(
                ui,
                candidate.emoji,
                theme::regular(22.0),
                palette.text,
                30.0,
                1,
            );
            emoji.paint(
                ui,
                pos2(rect.left() + 6.0, rect.center().y - emoji.size().y / 2.0),
                palette.text,
            );
            let shortcode = widgets::line(
                ui,
                &candidate.shortcode,
                theme::medium(13.0),
                palette.text,
                (rect.width() * 0.4).max(100.0),
                1,
            );
            let text_x = rect.left() + 42.0;
            shortcode.paint(
                ui,
                pos2(text_x, rect.center().y - shortcode.size().y / 2.0),
                palette.text,
            );
            let name_x = text_x + shortcode.size().x + 12.0;
            let name = widgets::line(
                ui,
                candidate.name,
                theme::regular(12.5),
                palette.secondary,
                (rect.right() - name_x - 8.0).max(0.0),
                1,
            );
            name.paint(
                ui,
                pos2(name_x, rect.center().y - name.size().y / 2.0),
                palette.secondary,
            );
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                app.emoji_selected = index;
                picked = Some(candidate.clone());
            }
        }
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(suggestion_list_id(), list.response.rect));
    strip_gap(ui);
    if let Some(candidate) = picked {
        app.actions.push(Action::InsertEmojiCompletion {
            emoji: candidate.emoji.to_owned(),
            start,
            end,
        });
    }
}

/// Group-member suggestions above the composer.
fn mention_picker(app: &mut App, ui: &mut egui::Ui, chat: &Chat, field: egui::Id) {
    let cursor = egui::TextEdit::load_state(ui.ctx(), field)
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.primary.index.0)
        .unwrap_or_else(|| app.composer.chars().count());
    let Some((end, query)) = active_mention(&app.composer, app.mention_start, cursor) else {
        app.mention_start = None;
        return;
    };
    let start = app.mention_start.expect("checked above");
    let candidates = app.mention_candidates(chat, query);
    if candidates.is_empty() {
        return;
    }
    let down = take_plain_key(ui, Key::ArrowDown);
    let up = take_plain_key(ui, Key::ArrowUp);
    if down {
        app.mention_selected = (app.mention_selected + 1) % candidates.len();
    }
    if up {
        app.mention_selected = (app.mention_selected + candidates.len() - 1) % candidates.len();
    }
    app.mention_selected = app.mention_selected.min(candidates.len() - 1);
    let submit = take_plain_key(ui, Key::Enter) || take_plain_key(ui, Key::Tab);
    let mut picked = submit.then(|| candidates[app.mention_selected].clone());
    let palette = app.palette;
    let everyone_alias = crate::mentions::everyone_alias(app.locale);

    let list = widgets::raised(ui, &palette, suggestion_frame(&palette), |ui| {
        let row_height = 38.0;
        egui::ScrollArea::vertical()
            .id_salt("mention-members")
            .max_height(row_height * candidates.len().min(5) as f32)
            .auto_shrink([false, true])
            .show_rows(ui, row_height, candidates.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for index in range {
                    let (id, label) = &candidates[index];
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), row_height),
                        Sense::click(),
                    );
                    if index == app.mention_selected || response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            SUGGESTION_ROW_RADIUS,
                            palette.surface_hover,
                        );
                    }
                    let avatar = Rect::from_center_size(
                        pos2(rect.left() + 19.0, rect.center().y),
                        Vec2::splat(28.0),
                    );
                    let everyone = id == crate::mentions::ALL_ID;
                    if everyone {
                        ui.painter().circle_filled(
                            avatar.center(),
                            avatar.width() / 2.0,
                            palette.avatar(crate::util::hue(id)),
                        );
                        theme::paint_icon(
                            ui,
                            Icon::Users,
                            avatar,
                            avatar.width() * 0.5,
                            Color32::WHITE,
                        );
                    } else {
                        let picture = app.avatar(id);
                        widgets::paint_avatar(
                            ui,
                            &palette,
                            avatar,
                            label.trim_start_matches('~'),
                            id,
                            picture.as_deref(),
                        );
                    }
                    let detail = if everyone
                        && !everyone_alias.eq_ignore_ascii_case(crate::mentions::ALL_USER)
                    {
                        everyone_alias.clone()
                    } else {
                        crate::model::phone_of(id)
                            .map(crate::util::phone)
                            .unwrap_or_default()
                    };
                    let detail = widgets::line(
                        ui,
                        &detail,
                        theme::regular(11.5),
                        palette.secondary,
                        (rect.width() * 0.36).min(150.0),
                        1,
                    );
                    let name = widgets::line(
                        ui,
                        label,
                        theme::medium(13.5),
                        palette.text,
                        rect.width() - detail.size().x - 62.0,
                        1,
                    );
                    name.paint(
                        ui,
                        pos2(rect.left() + 40.0, rect.center().y - name.size().y / 2.0),
                        palette.text,
                    );
                    detail.paint(
                        ui,
                        pos2(
                            rect.right() - detail.size().x - 8.0,
                            rect.center().y - detail.size().y / 2.0,
                        ),
                        palette.secondary,
                    );
                    if response.hovered() {
                        app.mention_selected = index;
                    }
                    if (down || up) && index == app.mention_selected {
                        response.scroll_to_me(Some(Align::Center));
                    }
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        picked = Some((id.clone(), label.clone()));
                    }
                }
            });
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(suggestion_list_id(), list.response.rect));
    strip_gap(ui);
    if let Some((id, name)) = picked {
        app.actions.push(Action::InsertMention {
            id,
            name,
            start,
            end,
        });
    }
}

/// Draws the composer and queues sends with the originating account and selected mentions.
fn composer(app: &mut App, ui: &mut egui::Ui, chat: &Chat) {
    let palette = app.palette;
    // The selection bar and the composer trade places: the one leaving
    // fades where it was from a copy of its last pass, while the one
    // coming fades in, the bar rising into place on a spring.
    let calm = selection_calm(ui.ctx(), app.window_focused);
    let motion = selection_motion(
        ui.ctx(),
        &chat.id,
        app.selection
            .as_ref()
            .filter(|(selected_chat, _)| *selected_chat == chat.id)
            .map(|(_, ids)| ids.as_slice()),
        calm,
    );
    // Where the composer's own shapes start, once the leaving bar is painted.
    let mut composer_from: Option<(egui::LayerId, egui::layers::ShapeIdx)> = None;
    let shown = egui::Panel::bottom("composer")
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(Color32::TRANSPARENT)
                .inner_margin(Margin {
                    left: COMPOSER_MARGIN,
                    right: COMPOSER_MARGIN,
                    top: 12,
                    bottom: COMPOSER_MARGIN,
                }),
        )
        .show(ui, |ui| {
            let layer = ui.layer_id();
            if let Some((selected_chat, selected)) = app.selection.clone()
                && selected_chat == chat.id
            {
                paint_kept_bar(ui.ctx(), layer, &chat.id, false, 1.0 - motion.opacity, 0.0);
                let start = super::swipe::mark(ui);
                selection_bar(app, ui, &chat.id, &selected);
                if !calm {
                    keep_bar(ui.ctx(), layer, &chat.id, true, start);
                }
                settle_bar(
                    ui.ctx(),
                    layer,
                    start,
                    motion.opacity,
                    SELECT_BAR_RISE * (1.0 - motion.slide),
                );
                return;
            }
            paint_kept_bar(
                ui.ctx(),
                layer,
                &chat.id,
                true,
                motion.opacity,
                SELECT_BAR_RISE * (1.0 - motion.slide),
            );
            composer_from = Some((layer, super::swipe::mark(ui)));
            // When an admin opens or closes the group to everyone, the
            // composer and the read-only note fade in as they swap.
            let open = theme::animate(
                ui.ctx(),
                egui::Id::new(("composer-open", chat.id.as_str())),
                chat.can_send(),
                theme::MOTION_PANEL,
            );
            ui.multiply_opacity(if chat.can_send() { open } else { 1.0 - open });
            if !chat.can_send() {
                if chat.kind == crate::model::ChatKind::Broadcast {
                    // A channel we left says so; the rest are only read-only.
                    ui.vertical_centered(|ui| {
                        theme::text(
                            ui,
                            if chat.left {
                                crate::i18n::gettext(app.locale, "You left this channel")
                            } else {
                                crate::i18n::gettext(
                                    app.locale,
                                    "Channels are read-only in Zeus",
                                )
                            }
                            .as_ref(),
                            theme::regular(13.5),
                            palette.secondary,
                        );
                    });
                    return;
                }
                if chat.locked {
                    ui.vertical_centered(|ui| {
                        theme::text(ui, "Locked chats are read-only in Zeus", theme::regular(13.5), palette.secondary);
                    });
                    return;
                }
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    // A group we left says so instead of blaming the admins:
                    // either we left it here, or the phone says we are no
                    // longer a member.
                    let ours = app.our_ids();
                    let left = chat.left
                        || (!ours.is_empty()
                            && !chat.participants.is_empty()
                            && !chat.lists_any(&ours));
                    if left {
                        theme::text(
                            ui,
                            crate::i18n::gettext(app.locale, "You left this group"),
                            theme::regular(13.5),
                            palette.secondary,
                        );
                    } else {
                        ui.horizontal(|ui| {
                            let width = 230.0;
                            ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
                            theme::text(ui, "Only", theme::regular(13.5), palette.secondary);
                            theme::text(ui, "admins", theme::semibold(13.5), palette.accent);
                            theme::text(
                                ui,
                                "can send messages",
                                theme::regular(13.5),
                                palette.secondary,
                            );
                        });
                    }
                    ui.add_space(8.0);
                });
                return;
            }
            // A refused voice message waits in its own chat only.
            let unsent_voice = app
                .unsent_voice
                .as_ref()
                .filter(|(unsent, _)| *unsent == chat.id)
                .map(|(_, samples)| samples.len());
            if let Some(samples) = unsent_voice
                && app.editing.is_none()
            {
                unsent_voice_strip(app, ui, samples);
            }
            if app.editing.is_some() {
                edit_strip(app, ui);
            } else if let Some(reply_id) = app.reply_to.clone() {
                let quoted = app.reply_preview(&reply_id, &chat.id).cloned();
                match quoted {
                    Some(quoted) => reply_strip(app, ui, &quoted),
                    None if reply_id.chat.is_some() => {
                        // Losing the source page must never silently turn a
                        // private reply into an unquoted send. The worker
                        // resolves the original from the archive on Send.
                        let palette = app.palette;
                        let strip = widgets::raised(ui, &palette, strip_frame(&palette), |ui| {
                            ui.set_width(ui.available_width().max(0.0));
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_max_width((ui.available_width() - 40.0).max(0.0));
                                    widgets::rich_text(
                                        ui,
                                        &crate::i18n::gettext(app.locale, "Original group message unavailable"),
                                        theme::regular(12.5),
                                        palette.secondary,
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if theme::icon_button(
                                        ui, Icon::X, 16.0, palette.secondary, palette.text,
                                        "Cancel reply (Esc)",
                                    ).clicked() {
                                        app.actions.push(Action::CancelReply);
                                    }
                                });
                            });
                        });
                        ui.ctx().data_mut(|data| data.insert_temp(reply_strip_id(), strip.response.rect));
                        strip_gap(ui);
                    }
                    None => app.reply_to = None,
                }
            }
            app.prepare_composer_undo(ui.ctx());
            let id = egui::Id::new("composer-text");
            let has_focus = ui.memory(|memory| memory.has_focus(id));
            let enter_sends = app.settings.enter_sends;
            let (typed_colon, typed_at) = ui.input(|input| {
                let typed = |needle: &str| {
                    input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Text(text) if text == needle))
                };
                (has_focus && typed(":"), has_focus && typed("@"))
            });
            forget_pending_posters(ui.ctx(), app);
            if !app.pending.is_empty() {
                pending_strip(app, ui);
            }
            if app.recording.is_some() {
                composer_glass(ui, &palette, |ui| recording_strip(app, ui));
                return;
            }
            emoji_suggestions(app, ui, id);
            mention_picker(app, ui, chat, id);
            // `consume_key(NONE, Enter)` also matches Shift+Enter. Check the
            // event modifiers directly. An active suggestion list consumes
            // plain Enter first when it has a selection.
            let send_key = has_focus
                && ui.input_mut(|input| {
                    let mut sent = false;
                    input.events.retain(|event| {
                        if sent {
                            return true;
                        }
                        let is_send = matches!(
                            event,
                            egui::Event::Key {
                                key: Key::Enter,
                                pressed: true,
                                modifiers,
                                ..
                            } if !modifiers.shift && !modifiers.alt
                                && (if enter_sends { !modifiers.command && !modifiers.ctrl } else { modifiers.command })
                        );
                        sent |= is_send;
                        !is_send
                    });
                    sent
                });
            let mut send_click = false;
            let line_height = ui
                .painter()
                .layout_no_wrap("x".to_owned(), theme::regular(BODY_SIZE), palette.text)
                .size()
                .y;
            // Every control sits in a band as tall as a one-line field at the
            // bottom of the row, centred on it. The field grows to six lines
            // above that band, so the buttons stay beside its last line.
            // A one-line field, like the composer before the rounded field:
            // the text line with padding, the send button as tall as that.
            let line = (line_height + COMPOSER_PADDING)
                .round()
                .max(COMPOSER_CONTROL);
            let button_width = line;
            let view_once = app.editing.is_none()
                && (app.pending_supports_view_once() || app.pending_view_once);
            let view_once_width = if view_once { COMPOSER_CONTROL + ui.spacing().item_spacing.x } else { 0.0 };
            let field_margin = ((line - line_height) / 2.0).round().max(0.0);
            // Measure this frame's draft at the text column's width, so the
            // field and the panel holding it grow on the keystroke that wraps
            // a line rather than a frame later, which made them jump.
            let wrap_id = id.with("wrap");
            let text_height = ui
                .ctx()
                .data(|data| data.get_temp::<f32>(wrap_id))
                .map(|wrap| {
                    let format =
                        egui::TextFormat::simple(theme::regular(BODY_SIZE), palette.text);
                    crate::bidi::layout_editor(ui, &app.composer, &format, wrap, true)
                        .0
                        .size()
                        .y
                })
                .or_else(|| ui.ctx().read_response(id).map(|previous| previous.rect.height()))
                .unwrap_or(line_height)
                .clamp(line_height, line_height * 6.0);
            let row_height = (text_height + 2.0 * field_margin).max(line);
            let pill = composer_glass(ui, &palette, |ui| {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), row_height),
                Layout::left_to_right(Align::Max),
                |ui| {
                // The plus sits in the field's rounded left end, centred as
                // the send button is in the right one.
                ui.add_space((line / 2.0 - PLUS_EDGE / 2.0).max(0.0));
                if app.editing.is_none() {
                    let tools = last_line(ui, line, |ui| theme::icon_button(
                        ui,
                        Icon::Plus,
                        22.0,
                        if app.composer_tools_open {
                            palette.accent
                        } else {
                            palette.secondary
                        },
                        palette.text,
                        &crate::i18n::gettext(app.locale, "Attach"),
                    ))
                    .tab_stop(Stop::Attach);
                    composer_tools_menu(app, chat, &tools);
                    // Plus and emoji sit close together, as a pair.
                    ui.add_space(COMPOSER_PAIR_GAP - ui.spacing().item_spacing.x);
                    // The button shows the picker's tab used last.
                    let (mode_icon, mode_label) = match app.settings.picker_mode {
                        PickerTab::Emoji => (Icon::Smile, "Emoji, GIFs, and stickers"),
                        PickerTab::Gifs => (Icon::Gif, "GIFs, emoji, and stickers"),
                        PickerTab::Stickers => (Icon::Sticker, "Stickers, emoji, and GIFs"),
                    };
                    // The icon fades in when the mode changes.
                    let mode_fade = ui.ctx().animate_bool_with_time_and_easing(
                        egui::Id::new(("picker-mode", app.settings.picker_mode as u8)),
                        true,
                        theme::MOTION_QUICK,
                        theme::ease_out,
                    );
                    let smile = last_line(ui, line, |ui| ui.scope(|ui| {
                        ui.multiply_opacity(mode_fade);
                        theme::icon_button(
                        ui,
                        mode_icon,
                        22.0,
                        if app.picker.is_some() {
                            palette.accent
                        } else {
                            palette.secondary
                        },
                        palette.text,
                        mode_label,
                    )}).inner).tab_stop(Stop::Emoji);
                    app.picker_anchor = Some(smile.rect);
                    if smile.clicked() {
                        if app.composer_tools_open {
                            app.actions.push(Action::SetComposerTools(false));
                        }
                        app.actions.push(Action::TogglePicker(app.settings.picker_mode));
                    }
                    // The text follows the pair as closely as the emoji
                    // follows the plus (the field's own left margin included).
                    ui.add_space(COMPOSER_TEXT_GAP - ui.spacing().item_spacing.x - 8.0);
                }
                // The send button closes the row, flush with the field's end.
                let field_width =
                    (ui.available_width() - button_width - view_once_width - ui.spacing().item_spacing.x).max(0.0);
                // The ink is centred on the controls, not the line box: the
                // span from a capital's top to a descender's bottom sits as far
                // from the field's top as from its bottom. Inter's box leaves
                // more room above the capitals than below the descenders, and
                // at fractional scales the glyphs round to the pixel grid off
                // centre in it. Measured at this scale, snapped to a pixel.
                let ink_middle = ink_middle(ui, line_height);
                let top = fastframe_text::snap_to_pixels(
                    row_height - line / 2.0 - (text_height - line_height) - ink_middle,
                    ui.ctx().pixels_per_point(),
                )
                .max(0.0);
                let bottom = (row_height - text_height - top).max(0.0);
                Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(Margin {
                        left: 8,
                        right: 8,
                        top: 0,
                        bottom: 0,
                    })
                    .show(ui, |ui| {
                        ui.set_width((field_width - 16.0).max(0.0));
                        ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add_space(top);
                        // Grow from one to six lines, then scroll. The height
                        // is this frame's draft, measured above, rather than
                        // the scroll area's memory of the last frame.
                        egui::ScrollArea::vertical()
                            .id_salt("composer-scroll")
                            .max_height(text_height)
                            .min_scrolled_height(text_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                // Keep emoji in the buffer so character offsets match, then
                                // paint their color bitmaps over the transparent glyphs.
                                let mut clusters: Vec<(usize, usize, String)> = Vec::new();
                                let format = egui::TextFormat::simple(
                                    theme::regular(BODY_SIZE),
                                    palette.text,
                                );
                                let composer_rtl = crate::bidi::base_rtl(&app.composer);
                                let mut layouter = |ui: &egui::Ui,
                                                    text: &dyn egui::TextBuffer,
                                                    wrap: f32| {
                                    let (galley, found) = crate::bidi::layout_editor(
                                        ui,
                                        text.as_str(),
                                        &format,
                                        wrap,
                                        true,
                                    );
                                    clusters = found;
                                    galley
                                };
                                let wrap = ui.available_width();
                                ui.ctx().data_mut(|data| data.insert_temp(wrap_id, wrap));
                                let output = egui::TextEdit::multiline(&mut app.composer)
                                    .id(id)
                                    .frame(Frame::NONE)
                                    .margin(Margin::ZERO)
                                    .hint_text(
                                        egui::RichText::new(if app.pending.is_empty() {
                                            crate::i18n::gettext(app.locale, "Type a message")
                                                .into_owned()
                                        } else {
                                            crate::i18n::gettext(app.locale, "Add a caption")
                                                .into_owned()
                                        })
                                        .color(palette.dim)
                                        .font(theme::regular(BODY_SIZE)),
                                    )
                                    .font(theme::regular(BODY_SIZE))
                                    .text_color(palette.text)
                                    .desired_rows(1)
                                    .desired_width(f32::INFINITY)
                                    .horizontal_align(if composer_rtl {
                                        Align::RIGHT
                                    } else {
                                        Align::LEFT
                                    })
                                    .return_key(if enter_sends {
                                        Some(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                                    } else {
                                        Some(KeyboardShortcut::new(Modifiers::NONE, Key::Enter))
                                    })
                                    .layouter(&mut layouter)
                                    .show(ui);
                                for (start, length, cluster) in &clusters {
                                    let Some(bounds) = crate::bidi::char_bounds(
                                        &output.galley,
                                        *start,
                                        start + length,
                                    ) else {
                                        continue;
                                    };
                                    // Skip emoji clusters split across rows.
                                    if bounds.height() > line_height * 1.5 {
                                        continue;
                                    }
                                    let rect = bounds.translate(output.galley_pos.to_vec2());
                                    crate::emoji::paint_cluster(ui, cluster, rect);
                                }
                                // The row was sized from last frame's text. When a
                                // keystroke wraps or unwraps a line, lay the frame
                                // out again instead of showing the field a frame
                                // late, which made it jump while typing.
                                // egui sizes the box before applying the keystroke,
                                // and text typed into an empty field reaches its
                                // galley a frame later still, so measure the
                                // edited draft itself.
                                let painted = Rect::from_min_size(
                                    output.galley_pos,
                                    output.galley.size(),
                                );
                                let measured = crate::bidi::layout_editor(
                                    ui,
                                    &app.composer,
                                    &format,
                                    wrap,
                                    true,
                                )
                                .0
                                .size()
                                .y
                                .clamp(line_height, line_height * 6.0);
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(composer_text_id(), painted);
                                });
                                if (measured - text_height).abs() > 0.5 {
                                    ui.ctx().request_discard("composer height changed");
                                }
                                let response = output.response.response.clone().tab_stop(Stop::Composer);
                                ui.ctx().accesskit_node_builder(response.id, |node| node.set_label("Message"));
                                if response.changed() {
                                    app.actions.push(Action::Composing {
                                        chat: chat.id.clone(),
                                        composing: true,
                                    });
                                    let cursor = output
                                        .cursor_range
                                        .map(|range| range.primary.index.0)
                                        .unwrap_or_else(|| app.composer.chars().count());
                                    if typed_colon
                                        && let Some(at) = standalone_trigger(
                                            &app.composer,
                                            cursor,
                                            ':',
                                        )
                                    {
                                        app.picker = None;
                                        app.emoji_start = Some(at);
                                        app.emoji_selected = 0;
                                        app.mention_start = None;
                                    } else if typed_at
                                        && chat.is_group()
                                        && !chat.participants.is_empty()
                                        && let Some(at) = standalone_trigger(
                                            &app.composer,
                                            cursor,
                                            '@',
                                        )
                                    {
                                        app.emoji_start = None;
                                        app.mention_start = Some(at);
                                        app.mention_selected = 0;
                                    } else if app.emoji_start.is_some() {
                                        app.emoji_selected = 0;
                                    }
                                }
                                let cursor = output
                                    .cursor_range
                                    .map(|range| range.primary.index.0)
                                    .unwrap_or_else(|| app.composer.chars().count());
                                if app.emoji_start.is_some()
                                    && active_emoji(&app.composer, app.emoji_start, cursor).is_none()
                                {
                                    app.emoji_start = None;
                                }
                                if app.mention_start.is_some()
                                    && active_mention(&app.composer, app.mention_start, cursor)
                                        .is_none()
                                {
                                    app.mention_start = None;
                                }
                                // The composer waits for the image preview to
                                // close before taking focus back.
                                if app.focus_composer
                                    && app.image_preview.is_none()
                                    && app.sticker_view.is_none()
                                {
                                    app.focus_composer = false;
                                    response.request_focus();
                                    if std::mem::take(&mut app.composer_caret_end) {
                                        let mut state = egui::TextEdit::load_state(ui.ctx(), response.id)
                                            .unwrap_or_default();
                                        state.cursor.set_char_range(Some(
                                            egui::text::CCursorRange::one(egui::text::CCursor::new(
                                                app.composer.chars().count(),
                                            )),
                                        ));
                                        egui::TextEdit::store_state(ui.ctx(), response.id, state);
                                        ui.ctx().request_repaint();
                                    }
                                }
                            });
                        ui.add_space(bottom);
                        });
                    });
                if view_once
                    && last_line(ui, line, |ui| view_once_button(app, ui))
                        .tab_stop(Stop::ViewOnce)
                        .clicked()
                {
                    app.actions.push(Action::SetPendingViewOnce(!app.pending_view_once));
                }
                let ready = !app.composer.trim().is_empty()
                    || !app.pending.is_empty()
                    || (unsent_voice.is_some() && app.editing.is_none());
                let (fill, hover, icon) = if ready {
                    (palette.accent, palette.accent_hover, palette.on_accent)
                } else {
                    (palette.surface, palette.surface_hover, palette.dim)
                };
                last_line(ui, line, |ui| {
                    // An empty composer changes the send button to record.
                    let recording = !ready && app.editing.is_none();
                    let (icon_kind, tint, tip) = if recording {
                        (Icon::Mic, palette.secondary, "Record a voice message")
                    } else if app.editing.is_some() {
                        (Icon::Check, icon, "Send")
                    } else {
                        (Icon::Send, icon, "Send")
                    };
                    let send = theme::circle_button_morph(
                        ui,
                        "composer-send",
                        icon_kind,
                        button_width,
                        fill,
                        hover,
                        tint,
                        tip,
                    )
                    .tab_stop(Stop::Send);
                    if recording {
                        if send.clicked() {
                            app.actions.push(Action::StartRecording);
                        }
                    } else {
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(send_flight_id().with("button"), send.rect));
                    if send.clicked() {
                        send_click = true;
                    }
                    }
                });
            },
                    );
                });
            // Clicking into the field draws no accent ring around it: the caret
            // shows where typing goes. Tabbing to it still outlines the field.
            if ui.ctx().data(|data| {
                data.get_temp::<bool>(theme::keyboard_focus_id())
                    .unwrap_or(false)
            }) {
                theme::focus_outline(ui, id, pill.response.rect, f32::from(COMPOSER_RADIUS));
            }
            ui.ctx()
                .data_mut(|data| data.insert_temp(composer_pill_id(), pill.response.rect));
            if (send_key || send_click)
                && (!app.composer.trim().is_empty() || !app.pending.is_empty())
            {
                let text = std::mem::take(&mut app.composer);
                if app.pending.is_empty() {
                    // The bubble leaves from where its text sat in the field,
                    // as iMessage's does, in the field's colours. A draft
                    // scrolled past the field's edge leaves from inside it.
                    let pill_rect = pill.response.rect;
                    let typed = ui
                        .ctx()
                        .data(|data| data.get_temp::<Rect>(composer_text_id()))
                        .map(|typed| {
                            let bottom =
                                typed.bottom().min(pill_rect.bottom()).max(pill_rect.top());
                            Rect::from_x_y_ranges(
                                typed.x_range(),
                                (bottom - typed.height()).max(pill_rect.top())..=bottom,
                            )
                        })
                        .unwrap_or(pill_rect);
                    let flight = SendFlight {
                        chat: chat.id.clone(),
                        from: typed,
                        field: composer_pill(&palette).fill,
                        ink: palette.text,
                        sent: ui.input(|input| input.time),
                        wall: crate::util::now(),
                        message: None,
                    };
                    // Fast sends queue: each keeps its own flight.
                    let now = flight.sent;
                    ui.ctx().data_mut(|data| {
                        let flights =
                            data.get_temp_mut_or_default::<Vec<SendFlight>>(send_flight_id());
                        flights.retain(|old| now - old.sent < 3.0);
                        flights.push(flight);
                    });
                    app.actions.push(Action::SendText {
                        chat: chat.id.clone(),
                        text,
                        quoting: app.reply_to.clone(),
                    });
                } else {
                    app.actions.push(Action::SendPending {
                        account: app.account().id.clone(),
                        mentions: app.composer_mentions(),
                        chat: chat.id.clone(),
                        caption: text,
                    });
                }
                app.focus_composer = true;
            } else if (send_key || send_click) && unsent_voice.is_some() && app.editing.is_none() {
                // An empty composer with a refused voice message: Send tries
                // that message again.
                app.actions.push(Action::SendRecording);
                app.focus_composer = true;
            }
        });
    // The composer as drawn, kept to fade out when a selection takes its
    // place, then faded in itself while the selection bar leaves.
    if let Some((layer, start)) = composer_from {
        if !calm {
            keep_bar(ui.ctx(), layer, &chat.id, false, start);
        }
        settle_bar(ui.ctx(), layer, start, 1.0 - motion.opacity, 0.0);
    }
    // A bottom panel is placed from last frame's height. When the composer
    // grows or shrinks, lay the frame out again so it never shows the field
    // hanging below the window or a gap above it for a frame.
    let height = shown.response.rect.height();
    let height_id = egui::Id::new("composer-panel-height");
    let previous = ui.ctx().data_mut(|data| {
        let previous = data.get_temp::<f32>(height_id);
        data.insert_temp(height_id, height);
        previous
    });
    if previous.is_some_and(|previous| (previous - height).abs() > 0.5) {
        ui.ctx().request_discard("composer height changed");
    }
    // Toasts sit above the composer so they never cover its buttons.
    ui.ctx()
        .data_mut(|data| data.insert_temp(super::composer_rect_id(), shown.response.rect));
}

/// Corner radius of the composer's rounded field.
pub(crate) const COMPOSER_RADIUS: u8 = 24;
/// Space between the composer's field and the chat card's sides and bottom.
/// The card's corners are this much rounder than the field's, so the two
/// curves run parallel (see `chats::CARD_RADIUS`).
pub(crate) const COMPOSER_MARGIN: i8 = 12;
/// Padding around the text of a one-line composer row. The row, and the send
/// and record button, are one text line plus this; the controls are centred
/// on it.
const COMPOSER_PADDING: f32 = 14.0;
/// Height of the plus and emoji buttons: a row is never shorter, or they
/// would stretch it and pull the text off its centre.
const COMPOSER_CONTROL: f32 = 36.0;
/// Space between the composer's rounded field and the buttons at its ends.
const COMPOSER_INSET: i8 = 6;
/// Space between the plus and emoji buttons' hit areas. Each area pads its
/// 22-point icon by six points a side, so this leaves eight between the
/// icons, a pair.
const COMPOSER_PAIR_GAP: f32 = -4.0;
/// From the emoji button's edge to the text. The plus's glyph leaves more of
/// its box empty than the round emoji's, so this is wider than the pair's
/// gap: at it, the text starts as far from the emoji's ink as the emoji
/// starts from the plus's.
pub(crate) const COMPOSER_TEXT_GAP: f32 = 10.0;
/// Width of the plus button: its 22-point icon and `icon_button`'s padding.
const PLUS_EDGE: f32 = 34.0;

/// Where the composer's text was painted in the frame's last pass, for
/// layout tests.
pub(crate) fn composer_text_id() -> egui::Id {
    egui::Id::new("composer-text-rect")
}

/// Where the composer's rounded field was drawn, for layout tests.
pub(crate) fn composer_pill_id() -> egui::Id {
    egui::Id::new("composer-pill")
}

/// The open chat's message list scroll offset and viewport height, as
/// `(offset.y, height)`, as of its last frame: for tests that need them
/// precisely. `App::at_bottom` covers ordinary UI checks.
pub(crate) fn scroll_metrics_id(chat: &ChatId) -> egui::Id {
    egui::Id::new(("message-scroll-metrics", chat))
}

/// How far below the top of a composer text row the middle of its ink sits,
/// at this scale: halfway from a capital's top to a descender's bottom.
fn ink_middle(ui: &egui::Ui, line_height: f32) -> f32 {
    let format = egui::TextFormat::simple(theme::regular(BODY_SIZE), Color32::WHITE);
    let (galley, _) = crate::bidi::layout_editor(ui, "Hy", &format, f32::INFINITY, true);
    widgets::ink_middle(&galley).unwrap_or(line_height / 2.0)
}

/// Lays out a control centred in the composer's last-line band, however
/// tall the draft has grown, so every control shares one vertical centre.
fn last_line<R>(ui: &mut egui::Ui, line: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(0.0, line), Layout::left_to_right(Align::Center), add)
        .inner
}

/// The rounded field that holds the composer's controls, or the recorder.
fn composer_pill(palette: &Palette) -> Frame {
    // Glass over a vibrant conversation, as the header's.
    let (fill, edge) = if palette.vibrant() {
        (
            palette.frosted(palette.chat),
            Stroke::new(1.0, palette.text.gamma_multiply(0.16)),
        )
    } else {
        (palette.bubble_in, Stroke::NONE)
    };
    Frame::new()
        .fill(fill)
        .stroke(edge)
        .corner_radius(CornerRadius::same(COMPOSER_RADIUS))
        // The end buttons are inset by as much at the sides as above and
        // below, so they sit evenly in the rounded ends.
        .inner_margin(Margin {
            left: COMPOSER_INSET,
            right: COMPOSER_INSET,
            top: COMPOSER_INSET,
            bottom: COMPOSER_INSET,
        })
}

/// The composer's field over blurred glass: what scrolls on under it is
/// frosted within its rounded ends, under its see-through fill.
fn composer_glass<R>(
    ui: &mut egui::Ui,
    palette: &Palette,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let glass = ui.painter().add(egui::Shape::Noop);
    let shown = widgets::raised(ui, palette, composer_pill(palette), add);
    if palette.vibrant() {
        ui.painter().set(
            glass,
            super::glass::sheet_rounded(shown.response.rect, f32::from(COMPOSER_RADIUS)),
        );
    }
    shown
}

/// The plus menu beside the composer: send files, documents or a location,
/// or create a poll.
fn composer_tools_menu(app: &mut App, chat: &Chat, plus: &egui::Response) {
    let id = plus.id.with("composer-tools");
    let mut open = app.composer_tools_open;
    let picker_open = app.picker.is_some();
    if plus.clicked() {
        open = !open;
        if open && picker_open {
            app.actions.push(Action::ClosePicker);
        }
    }
    let mut draw_open = open;
    if draw_open && !picker_open {
        egui::Popup::menu(plus)
            .id(id)
            .open_bool(&mut draw_open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
            .width(190.0)
            .frame(widgets::menu_frame(&app.palette))
            .show_frosted(&plus.ctx, |ui| {
                let send_files = crate::i18n::gettext(app.locale, "Send files");
                if widgets::menu_item(ui, &app.palette, Some(Icon::Paperclip), &send_files) {
                    app.actions.push(Action::Attach { documents: false });
                }
                // Pictures picked here keep their own bytes instead of
                // becoming compressed photos.
                let document = crate::i18n::gettext(app.locale, "Send as document");
                if widgets::menu_item(ui, &app.palette, Some(Icon::FileText), &document) {
                    app.actions.push(Action::Attach { documents: true });
                }
                let send_location = crate::i18n::gettext(app.locale, "Send location");
                if widgets::menu_item(ui, &app.palette, Some(Icon::MapPin), &send_location) {
                    app.actions
                        .push(Action::ShowDialog(Dialog::SendLocation(chat.id.clone())));
                }
                let create_poll = crate::i18n::gettext(app.locale, "Create poll");
                if widgets::menu_item(ui, &app.palette, Some(Icon::ListChecks), &create_poll) {
                    app.actions
                        .push(Action::ShowDialog(Dialog::CreatePoll(chat.id.clone())));
                }
            });
    }
    if draw_open != app.composer_tools_open {
        app.actions.push(Action::SetComposerTools(draw_open));
    }
}

/// Offers a refused voice message for another try, or to discard it.
/// From a strip above the composer (reply, edit, unsent voice) to the field
/// it belongs to: close enough to read as one piece.
pub(crate) const STRIP_GAP: f32 = 3.0;

/// Leaves [`STRIP_GAP`] below a strip, the layout's own spacing included.
fn strip_gap(ui: &mut egui::Ui) {
    ui.add_space(STRIP_GAP - ui.spacing().item_spacing.y);
}

/// Corner radius of the strips above the composer and of its suggestion
/// lists: rounder than a bubble, closer to the composer's own ends.
const STRIP_RADIUS: u8 = 22;

/// The frame of a strip above the composer (reply, edit, unsent voice).
/// Like the composer it takes the incoming bubble's colour, which in the
/// light theme is white on the chat rather than the grey interface surface.
/// Its content starts 14 points in, so a reply's accent bar lines up with
/// the composer's plus below it.
fn strip_frame(palette: &Palette) -> Frame {
    Frame::new()
        .fill(palette.bubble_in)
        .corner_radius(CornerRadius::same(STRIP_RADIUS))
        .inner_margin(Margin {
            left: 14,
            right: 10,
            top: 6,
            bottom: 6,
        })
}

/// The emoji and @mention suggestion lists, raised above the composer like
/// the strips, with the same fill and corners.
fn suggestion_frame(palette: &Palette) -> Frame {
    Frame::new()
        .fill(palette.bubble_in)
        .corner_radius(CornerRadius::same(STRIP_RADIUS))
        .inner_margin(Margin::same(SUGGESTION_INSET))
}

/// Space between a suggestion list's edge and its rows.
const SUGGESTION_INSET: i8 = 4;
/// A suggestion row's corners, concentric with the list's.
const SUGGESTION_ROW_RADIUS: f32 = (STRIP_RADIUS as i8 - SUGGESTION_INSET) as f32;

fn unsent_voice_strip(app: &mut App, ui: &mut egui::Ui, samples: usize) {
    let palette = app.palette;
    let seconds = (samples as f64 / f64::from(crate::voice::RATE))
        .round()
        .max(1.0) as u32;
    let label = crate::i18n::gettext(app.locale, "Voice message ({duration}) not sent")
        .replace("{duration}", &crate::util::duration(seconds));
    let discard = crate::i18n::gettext(app.locale, "Discard voice message");
    widgets::raised(ui, &palette, strip_frame(&palette), |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::Mic, 16.0, palette.danger);
            theme::text(ui, &label, theme::semibold(12.5), palette.danger);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let button = theme::icon_button(
                    ui,
                    Icon::X,
                    16.0,
                    palette.secondary,
                    palette.text,
                    discard.as_ref(),
                );
                #[cfg(test)]
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::new("unsent-voice-discard"), button.rect)
                });
                if button.clicked() {
                    app.actions.push(Action::DiscardUnsentVoice);
                }
            });
        });
    });
    strip_gap(ui);
}

fn edit_strip(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    widgets::raised(ui, &palette, strip_frame(&palette), |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::Pencil, 16.0, palette.accent);
            theme::text(ui, "Editing message", theme::semibold(12.5), palette.accent);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(
                    ui,
                    Icon::X,
                    16.0,
                    palette.secondary,
                    palette.text,
                    "Stop editing (Esc)",
                )
                .clicked()
                {
                    app.actions.push(Action::CancelEdit);
                }
            });
        });
    });
    strip_gap(ui);
}

fn reply_strip(app: &mut App, ui: &mut egui::Ui, quoted: &Message) {
    let palette = app.palette;
    let who = if quoted.from_me {
        "You".to_owned()
    } else {
        app.display_name_or(&quoted.sender, quoted.sender_name.as_deref())
    };
    let summary = markup::plain(&quoted.summary_in(app.locale), &app.mention_list(quoted));
    let strip = widgets::raised(ui, &palette, strip_frame(&palette), |ui| {
        ui.set_width(ui.available_width().max(0.0));
        ui.horizontal(|ui| {
            let (bar, _) = ui.allocate_exact_size(vec2(3.0, 34.0), Sense::hover());
            ui.painter().rect_filled(bar, 2.0, palette.accent);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.set_max_width((ui.available_width() - 40.0).max(0.0));
                widgets::rich_text(
                    ui,
                    &format!("Replying to {who}"),
                    theme::semibold(12.5),
                    palette.accent,
                );
                widgets::rich_text(ui, &summary, theme::regular(12.5), palette.secondary);
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(
                    ui,
                    Icon::X,
                    16.0,
                    palette.secondary,
                    palette.text,
                    "Cancel reply (Esc)",
                )
                .clicked()
                {
                    app.actions.push(Action::CancelReply);
                }
            });
        });
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(reply_strip_id(), strip.response.rect));
    strip_gap(ui);
}

/// Where the emoji or @mention suggestion list was drawn, for layout tests.
pub(crate) fn suggestion_list_id() -> egui::Id {
    egui::Id::new("suggestion-list")
}

/// Where the reply strip was drawn, for layout tests.
pub(crate) fn reply_strip_id() -> egui::Id {
    egui::Id::new("reply-strip")
}

/// App data needed while drawing a checked-out conversation.
#[derive(Clone)]
struct View<'a> {
    palette: Palette,
    locale: crate::i18n::Locale,
    chat: &'a Chat,
    me: Option<&'a str>,
    auto_download: bool,
    connected: bool,
    account_receipts_off: bool,
    poll_voting: &'a HashSet<(ChatId, String)>,
    interactive_pending: &'a HashSet<(ChatId, String)>,
    /// Whether this chat is selecting whole messages.
    selecting: bool,
    /// The ids of the selected messages, when selecting.
    selected: &'a [String],
    anchor: Option<&'a str>,
    /// Demo/test: keep this message's context menu open.
    open_menu: Option<&'a str>,
    /// Ids of the starred messages of this chat, for the mark in the footer.
    starred: Option<&'a HashSet<String>>,
    /// Ids of this chat's pinned messages, for the menu and the bubble mark.
    pins: Option<&'a HashSet<String>>,
    /// Active pins of this chat, for the notices in the transcript.
    pin_notices: &'a [crate::archive::Pinned],
    reaction: Option<&'a str>,
    /// The reaction picker was opened from the message's context menu.
    reaction_menu: bool,
    reaction_emoji: &'a [(String, u32)],
    /// Which actions the message menu shows, and its layout.
    menu: crate::settings::MessageMenu,
    keyboard_navigation: &'a std::cell::Cell<bool>,
    /// Resolves a name with the message's stored name as fallback.
    names_or: &'a dyn Fn(&str, Option<&str>) -> String,
    /// Resolves mention names without replacing our name with "You".
    mention_names: &'a dyn Fn(&str) -> String,
    avatars: &'a HashMap<String, Option<PathBuf>>,
    contacts: &'a crate::model::Contacts,
    now: i64,
    /// Animate media only while this window is active.
    animate: bool,
    /// The motion photo of this chat whose clip plays.
    motion_playing: Option<&'a str>,
    player: &'a crate::audio::Player,
    /// The speed this chat's voice messages play at.
    voice_speed: f32,
    /// Whether the time and ticks sit inside every bubble. The Nothing
    /// style leaves them below the bubble unless Always show time is on.
    time_inside: bool,
    video: &'a crate::video::Player,
    copy_rows: Option<&'a std::sync::Mutex<Vec<crate::transcript::Row>>>,
    /// What touchpad swipes do in the open chat; only a reply is the
    /// message rows' to do.
    swipes: crate::settings::Swipes,
    /// How strongly doodles show behind the bubbles and the colour under
    /// them, when there are any: each bubble lays a soft scrim over them
    /// (#29).
    doodles: Option<(f32, Color32)>,
    /// Where the transcript collects its bubbles' scrims, to lay them all
    /// under the first bubble once the rows are drawn: each is painted after
    /// the bubble above it, and would otherwise veil that bubble's lower
    /// edge and text. A row drawn alone paints its scrim in place.
    scrims: Option<&'a std::cell::RefCell<Vec<(egui::LayerId, egui::layers::ShapeIdx)>>>,
    /// Where the transcript collects the top of each message row it lays
    /// out, so the list can rise for a new message in a wave, each row a
    /// little after the one below it.
    rows: Option<&'a std::cell::RefCell<Vec<f32>>>,
}

/// Protocol album parent carried by pictures and ordinary videos.
/// Draws the deleted messages leaving from just below the message `above`
/// (`None` for the top of the loaded history). They only paint: their view
/// has no copy rows, menus or selection, and their actions are dropped.
fn leaving_rows(
    ui: &mut egui::Ui,
    view: &View<'_>,
    leaving: &[super::leaving::Leaving],
    above: Option<&str>,
    now: std::time::Instant,
) {
    for ghost in leaving
        .iter()
        .filter(|ghost| ghost.after.as_deref() == above)
    {
        let ghost_view = View {
            selecting: false,
            selected: &[],
            anchor: None,
            open_menu: None,
            reaction: None,
            copy_rows: None,
            scrims: None,
            rows: None,
            ..view.clone()
        };
        let message = &ghost.message;
        let id = bubble_id(&view.chat.id, &message.id).with("leaving");
        super::leaving::show(
            ui,
            id,
            super::leaving::progress(ghost, now),
            message.from_me,
            |ui| {
                let mut dropped = Vec::new();
                bubble(
                    ui,
                    &ghost_view,
                    message,
                    (!ghost.album.is_empty()).then_some(ghost.album.as_slice()),
                    RunPlace {
                        show_sender: false,
                        show_avatar: false,
                        tailed: true,
                        first_in_run: true,
                        last_in_run: true,
                        last_outgoing: false,
                    },
                    true,
                    &mut dropped,
                );
            },
        );
    }
}

pub(super) fn album_id(message: &Message) -> Option<&str> {
    match &message.content {
        Content::Image { media, .. }
        | Content::Video {
            media, note: false, ..
        } => media.album.as_deref(),
        _ => None,
    }
}

/// The consecutive messages belonging to the album beginning at `start`.
/// A single marked item is rendered normally because there is nothing to group.
fn album_run(messages: &[Message], start: usize) -> Option<&[Message]> {
    let first = messages.get(start)?;
    let album = album_id(first)?;
    let mut end = start + 1;
    while let Some(message) = messages.get(end) {
        if album_id(message) != Some(album)
            || message.from_me != first.from_me
            || message.sender != first.sender
            || crate::util::day_key(message.timestamp) != crate::util::day_key(first.timestamp)
        {
            break;
        }
        end += 1;
    }
    (end > start + 1).then_some(&messages[start..end])
}

/// The colours egui paints a scroll bar's handle in, idle, hovered, and held.
fn handle_colors(visuals: &mut egui::Visuals) -> [&mut egui::Color32; 3] {
    let widgets = &mut visuals.widgets;
    [
        &mut widgets.inactive.fg_stroke.color,
        &mut widgets.hovered.fg_stroke.color,
        &mut widgets.active.fg_stroke.color,
    ]
}

/// Whether egui holds a text selection in any label.
fn has_text_selection(ctx: &egui::Context) -> bool {
    ctx.plugin_opt::<egui::text_selection::LabelSelectionState>()
        .is_some_and(|plugin| plugin.lock().has_selection())
}

/// The rows the transcript keeps laid out for egui's text selection while it
/// skips the rest of a long history.
///
/// egui drops a selection when a pass does not draw both of its end labels,
/// but needs the labels between them only to copy their text. An end moves
/// only into a label drawn while a pointer button is pressed, held or let go
/// (keys move it inside its own label), so the rows laid out on those passes
/// hold both ends, wherever the view goes after. A primary press without
/// Shift either ends the selection or starts it again in the label under the
/// pointer, so it keeps only the rows near the view. The frame that copies
/// lays out every row from the first such row to the last, so egui and the
/// transcript read the text of every selected message.
struct TextEnds {
    /// The rows that may hold an end, by message id.
    ids: HashSet<String>,
    /// The rows of messages this pass laid out where an end may have moved.
    moved: Vec<String>,
    /// The span of message indices a copy this pass reads.
    copy: Option<std::ops::RangeInclusive<usize>>,
    /// A primary press without Shift or another button.
    fresh_press: bool,
    /// Any button pressed, held or let go.
    busy: bool,
}

impl TextEnds {
    fn begin(ui: &egui::Ui, mut ids: HashSet<String>, messages: &[Message]) -> Self {
        if !has_text_selection(ui.ctx()) {
            ids.clear();
        }
        let copying = !ids.is_empty()
            && ui.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Copy | egui::Event::Cut))
            });
        let copy = if copying {
            let first = messages
                .iter()
                .position(|message| ids.contains(&message.id));
            let last = messages
                .iter()
                .rposition(|message| ids.contains(&message.id));
            first.zip(last).map(|(first, last)| first..=last)
        } else {
            None
        };
        let (fresh_press, busy) = ui.input(|input| {
            let pointer = &input.pointer;
            let others = [
                egui::PointerButton::Secondary,
                egui::PointerButton::Middle,
                egui::PointerButton::Extra1,
                egui::PointerButton::Extra2,
            ]
            .into_iter()
            .any(|button| pointer.button_down(button) || pointer.button_pressed(button));
            (
                pointer.primary_pressed() && !input.modifiers.shift && !others,
                pointer.any_down() || pointer.any_pressed() || pointer.any_released(),
            )
        });
        Self {
            ids,
            moved: Vec::new(),
            copy,
            fresh_press,
            busy,
        }
    }

    /// Whether the row of `members`, starting at message `index`, must be
    /// laid out wherever the view is.
    fn holds(&self, index: usize, members: &[Message]) -> bool {
        self.copy.as_ref().is_some_and(|span| span.contains(&index))
            || (!self.ids.is_empty() && members.iter().any(|member| self.ids.contains(&member.id)))
    }

    /// Notes a row this pass laid out; `near` says whether it lies near the
    /// view, where a fresh press can have put both ends.
    fn laid_out(&mut self, members: &[Message], near: bool) {
        if self.busy && (near || !self.fresh_press) {
            self.moved
                .extend(members.iter().map(|member| member.id.clone()));
        }
    }

    /// The rows to keep laid out from the next pass on, once every row that
    /// could move an end has been drawn.
    fn finish(self, ctx: &egui::Context) -> HashSet<String> {
        if !has_text_selection(ctx) {
            return HashSet::new();
        }
        let mut ids = if self.fresh_press {
            HashSet::new()
        } else {
            self.ids
        };
        ids.extend(self.moved);
        ids
    }
}

/// A row height to assume for a message that has not been laid out yet. Rows
/// near the viewport are always measured, and a change in the height of a row
/// above the viewport moves the scroll offset with it, so this only shapes the
/// scrollbar until the reader scrolls near the row.
fn estimated_height(message: &Message, width: f32, new_day: bool, notices: usize) -> f32 {
    // Bubbles take at most 72% of the transcript, and 560 points.
    let bubble = ((width * 0.72).min(560.0) - 20.0).max(40.0);
    let text_rows = |text: &str| {
        let per_row = bubble / 7.5;
        (text.chars().count() as f32 / per_row).ceil().max(1.0)
    };
    let caption_rows = |caption: &Option<String>| {
        caption
            .as_deref()
            .map_or(0.0, |caption| text_rows(caption) * 19.0)
    };
    let body = match &message.content {
        Content::Text { text, preview } => {
            text_rows(text) * 19.0 + if preview.is_some() { 60.0 } else { 0.0 }
        }
        Content::Interactive { text, .. } => text_rows(text) * 19.0 + 60.0,
        Content::Image { caption, .. } | Content::Video { caption, .. } => {
            200.0 + caption_rows(caption)
        }
        Content::Document { caption, .. } => 70.0 + caption_rows(caption),
        Content::Sticker { .. } => 140.0,
        Content::Audio { .. } => 60.0,
        _ => 40.0,
    };
    // Bubble padding, the sender line, and the row spacing, plus the date
    // chip above the first message of a day, and any pin notice this row
    // carries.
    40.0 + body + if new_day { 36.0 } else { 0.0 } + notices as f32 * PIN_NOTICE_HEIGHT
}

/// The pins the transcript places, oldest first. `chat_pins` hands them over
/// newest first, and a notice belongs above the first message that came after
/// its pin, so the list goes back into time order before it is placed.
fn pin_notices_in_time_order(rows: &[crate::archive::Pinned]) -> Vec<crate::archive::Pinned> {
    let mut sorted = rows.to_vec();
    sorted.sort_by_key(|row| row.pinned_at);
    sorted
}

/// How many notices, from the front of the list, belong above a message that
/// arrived at `timestamp`.
fn notices_above(notices: &[crate::archive::Pinned], timestamp: i64) -> usize {
    notices
        .iter()
        .take_while(|notice| notice.pinned_at <= timestamp)
        .count()
}

/// The notice a pin leaves in the transcript, where the pin happened, as the
/// phone shows it.
fn pin_notice(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: crate::i18n::Locale,
    names: &dyn Fn(&str, Option<&str>) -> String,
    row: &crate::archive::Pinned,
) {
    let label = pin_notice_label(locale, row, names);
    ui.vertical_centered(|ui| {
        widgets::chip(ui, palette, &label);
    });
}

/// The words a pin notice shows: who pinned it, or a neutral line when the
/// archive does not say.
fn pin_notice_label(
    locale: crate::i18n::Locale,
    row: &crate::archive::Pinned,
    names: &dyn Fn(&str, Option<&str>) -> String,
) -> String {
    if row.pinner.by_me {
        return crate::i18n::gettext(locale, "You pinned a message").to_string();
    }
    if row.pinner.sender.is_empty() {
        return crate::i18n::gettext(locale, "A message was pinned").to_string();
    }
    crate::i18n::gettext(locale, "{} pinned a message")
        .replace("{}", &names(&row.pinner.sender, None))
}

/// The pins the line under the header shows: one that has run out its seven
/// days leaves, whether or not a refresh has reached this window yet.
fn live_pins(rows: &[crate::archive::Pinned], now: i64) -> Vec<crate::archive::Pinned> {
    rows.iter()
        .filter(|row| row.expires_at > now)
        .cloned()
        .collect()
}

/// What one pin notice takes in the transcript: the chip and its spacing.
const PIN_NOTICE_HEIGHT: f32 = 32.0;

/// Incoming messages carry their sender's picture and name in groups only,
/// as on WhatsApp.
fn shows_sender_pictures(chat: &Chat) -> bool {
    chat.is_group()
}

/// How long the sticky date stays after the transcript stops moving.
const STICKY_DATE_HOLD: f64 = 1.0;
/// How long the sticky date takes to fade out.
const STICKY_DATE_FADE: f32 = theme::MOTION_PANEL;

/// Keeps the first visible message's date at the top of the transcript once
/// its day separator has scrolled away, while the reader scrolls: it fades
/// out a moment after the transcript stops moving, as on the phone.
/// Following new messages at the bottom does not count as scrolling.
fn sticky_date<R>(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: crate::i18n::Locale,
    chat: &Chat,
    output: &egui::scroll_area::ScrollAreaOutput<R>,
    visible_day: Option<(i64, f32)>,
    under: f32,
) {
    let id = egui::Id::new(("visible-message-date", &chat.id));
    let ctx = ui.ctx().clone();
    let now = ui.input(|input| input.time);
    let offset = output.state.offset.y;
    let viewport = output.inner_rect;
    let at_bottom = offset + viewport.height() >= output.content_size.y - BOTTOM_SLACK;
    let (last_offset, was_at_bottom, mut moved_at) = ctx
        .data(|data| data.get_temp::<(f32, bool, f64)>(id))
        .unwrap_or((offset, at_bottom, f64::NEG_INFINITY));
    if (offset - last_offset).abs() > 0.5 && !(at_bottom && was_at_bottom) {
        moved_at = now;
    }
    ctx.data_mut(|data| data.insert_temp(id, (offset, at_bottom, moved_at)));
    let since = now - moved_at;
    let scrolling = since < STICKY_DATE_HOLD;
    if scrolling {
        // One wake-up for the moment the fade begins; the fade itself
        // repaints through `animate_bool` and stops once it is done.
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(STICKY_DATE_HOLD - since));
    }
    // It appears at once when scrolling starts and fades only on the way out.
    let fade = if scrolling { 0.0 } else { STICKY_DATE_FADE };
    let opacity = ctx.animate_bool_with_time(id, scrolling, fade);
    let Some((timestamp, separator_top)) = visible_day else {
        return;
    };
    // Below the header a vibrant window lays over the list, not under its blur.
    let top = viewport.top() + under;
    if opacity <= 0.0 || separator_top >= top {
        return;
    }
    // Paint after the scroll contents, inside their clip, without changing
    // their height or taking clicks away from the messages underneath.
    let mut overlay = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(viewport.with_min_y(top).shrink2(vec2(0.0, 8.0)))
            .layout(Layout::top_down(Align::Center)),
    );
    overlay.set_clip_rect(viewport);
    overlay.multiply_opacity(opacity);
    widgets::chip(
        &mut overlay,
        palette,
        &crate::util::day_label(locale, timestamp),
    );
}

/// The tail still counts as the bottom this far from the content end.
/// `scroll_to_rect` stops a few points short, so a one-point test never sees
/// a reader who is following the newest message.
const BOTTOM_SLACK: f32 = 24.0;

/// Each row of the list starts rising for a new message this long after
/// the row below it, as in iMessage, so the list ripples up from the bottom.
const WAVE_STEP: f64 = 0.015;
/// Rows this far up and beyond all start together.
const WAVE_ROWS: usize = 6;
/// The longest any row waits to start rising.
const WAVE_CAP: f64 = WAVE_STEP * WAVE_ROWS as f64;

/// How long a row with `rows_below` rows under it waits before it rises.
fn wave_delay(rows_below: usize) -> f64 {
    WAVE_STEP * rows_below.min(WAVE_ROWS) as f64
}

/// Slides the list up into place when a new last message lands while the
/// reader is at the bottom, instead of jumping by its height in one frame.
/// Only the picture moves: the scroll offset, and so every click, is already
/// where it ends.
/// A glide under way: the list's last height and newest message, to notice
/// the next one, and each lift still under way: when it began and how far
/// it carries the list. A message landing mid-glide adds a lift of its own,
/// so the one in flight carries on rather than snapping back.
#[derive(Clone)]
struct Glide {
    newest: Option<String>,
    height: f32,
    at_bottom: bool,
    lifts: Vec<(f64, f32)>,
}

impl Glide {
    fn id(chat: &str) -> egui::Id {
        egui::Id::new(("message-glide", chat))
    }

    /// How far below its place a row with `rows_below` rows under it is
    /// drawn at `now`: every lift rises on the sent bubble's own vertical
    /// spring, the row starting [`wave_delay`] after the newest.
    fn shift(&self, now: f64, rows_below: usize) -> f32 {
        let delay = wave_delay(rows_below);
        self.lifts
            .iter()
            .map(|&(start, distance)| distance * (1.0 - SEND_RISE.value(now - start - delay)))
            .sum()
    }

    /// Whether any row is still on its way at `now`.
    fn moving(&self, now: f64) -> bool {
        self.lifts
            .iter()
            .any(|&(start, _)| !SEND_RISE.settled(now - start - WAVE_CAP))
    }
}

/// Moves `shape` down by what `shift_at` gives for the height of its
/// middle. A list of shapes moves each by its own, as the bubbles' scrims
/// gathered under the first row are.
fn shift_shape(shape: &mut egui::Shape, shift_at: &impl Fn(f32) -> f32) {
    if let egui::Shape::Vec(shapes) = shape {
        for shape in shapes {
            shift_shape(shape, shift_at);
        }
        return;
    }
    let shift = shift_at(shape.visual_bounding_rect().center().y);
    if shift != 0.0 {
        shape.translate(vec2(0.0, shift));
    }
}

/// A transcript shape: its clip shares the viewport's vertical span, or
/// sits inside it. A vertical scroll clips y to the viewport and leaves x
/// on the parent, so the clip is wider than the viewport and an equality
/// check never matches. The scrollbar's clip is taller, so it stays put.
fn slides_with_transcript(clip: Rect, viewport: Rect) -> bool {
    let along = (clip.top() - viewport.top()).abs() <= 1.0
        && (clip.bottom() - viewport.bottom()).abs() <= 1.0;
    let inside = clip.top() >= viewport.top() - 0.5 && clip.bottom() <= viewport.bottom() + 0.5;
    along || inside
}

/// Message text is painted with the panel clip, which is taller than the
/// viewport, so [`slides_with_transcript`] skips it. The scrollbar shares
/// that clip and sits on the trailing edge.
fn shape_follows_transcript(shape: &egui::Shape, viewport: Rect) -> bool {
    let bounds = shape.visual_bounding_rect();
    bounds.intersects(viewport) && bounds.left() < viewport.right() - 18.0
}

/// Lifts the list for a new last message and draws it rising into place:
/// each row a [`wave_delay`] after the row below it, on the sent bubble's
/// vertical spring. `rows` are the tops of the rows laid out this pass, and
/// `layers` the layers they were drawn on, each with its first shape of the
/// pass: the transcript's, and the one a message edited in its bubble is
/// lifted onto.
fn glide_new_message<R>(
    ui: &egui::Ui,
    chat: &str,
    newest: Option<String>,
    output: &egui::scroll_area::ScrollAreaOutput<R>,
    layers: &[(egui::LayerId, egui::layers::ShapeIdx)],
    rows: &[f32],
) {
    let id = Glide::id(chat);
    let now = ui.input(|input| input.time);
    let height = output.content_size.y;
    let viewport = output.inner_rect;
    let at_bottom = output.state.offset.y + viewport.height() >= height - BOTTOM_SLACK;
    let before = ui.ctx().data(|data| data.get_temp::<Glide>(id));
    let mut lifts = before
        .as_ref()
        .map_or_else(Vec::new, |glide| glide.lifts.clone());
    lifts.retain(|&(start, _)| !SEND_RISE.settled(now - start - WAVE_CAP));
    let mut glide = Glide {
        newest: newest.clone(),
        height,
        at_bottom,
        lifts,
    };
    // The offset this pass just stored is what the next pass lays out at.
    // Discard once so the shown pass is already there, and the shift below
    // can hold the picture on the old line. A later pass sees the same
    // newest and does not discard again.
    let mut started = false;
    if let Some(before) = &before
        && before.newest.is_some()
        && before.newest != newest
        && before.at_bottom
        && at_bottom
        && height > before.height
    {
        // A message arriving mid-glide adds a lift to what is left, so fast
        // sends do not snap the list back to its old place. The newest row
        // never starts more than a view below its place.
        let left = before.shift(now, 0);
        let lift = (height - before.height).min((viewport.height() - left).max(0.0));
        if lift > 0.0 {
            glide.lifts.push((now, lift));
            started = true;
        }
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, glide.clone()));
    if started {
        ui.ctx()
            .request_discard("easing the transcript for a new message");
    }
    if !glide.moving(now) {
        return;
    }
    // Rows are laid out top down; sorted all the same, so a row's place in
    // the wave is a search: the rows whose tops lie below a point.
    let mut tops = rows.to_vec();
    tops.sort_by(f32::total_cmp);
    let shifts: Vec<f32> = (0..=WAVE_ROWS)
        .map(|below| glide.shift(now, below).min(viewport.height()))
        .collect();
    let shift_at = |y: f32| {
        let below = tops.len() - tops.partition_point(|top| *top <= y);
        shifts[below.min(WAVE_ROWS)]
    };
    ui.ctx().graphics_mut(|graphics| {
        for &(layer, first_shape) in layers {
            let list = graphics.entry(layer);
            for index in first_shape.0..list.next_idx().0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    let clip_slides = slides_with_transcript(clipped.clip_rect, viewport);
                    if clip_slides || shape_follows_transcript(&clipped.shape, viewport) {
                        // The clip stays on a viewport-shaped shape. Panel-clipped
                        // text would otherwise slide over the header, so that clip
                        // is cut to the viewport for the slide.
                        shift_shape(&mut clipped.shape, &shift_at);
                        if !clip_slides {
                            clipped.clip_rect = clipped.clip_rect.intersect(viewport);
                        }
                    }
                });
            }
        }
    });
    ui.ctx().request_repaint();
}

/// The conversation's messages. `under` is how much of the list's top lies
/// beneath the header, which a vibrant window lays over it.
fn messages(app: &mut App, ui: &mut egui::Ui, chat: &Chat, under: f32, over: f32) {
    let palette = app.palette;
    // Taken up front: `names_or` below borrows the rest of `app` for the
    // whole function, so a pending scroll must come out before that.
    let pending_scroll = app.scroll_page.take();
    // A scroll gesture that began over the messages stays with them when
    // the pointer drifts off (#274), taken here for the same reason.
    let carried = app.scroll_route.take(crate::app::ScrollPane::Messages);
    // An explicit jump (Ctrl+End, or the return-to-bottom button) must reach
    // the bottom even while a message bubble retains keyboard focus.
    let scroll_forced = std::mem::take(&mut app.scroll_to_bottom_forced);
    // Check out the conversation while drawing rows and collecting actions.
    let mut conversation = app.conversations.remove(&chat.id).unwrap_or_default();
    let typing = app.typing_in(&chat.id);
    let uploads: Vec<crate::model::Upload> = app
        .uploads
        .iter()
        .filter(|upload| upload.chat == chat.id)
        .cloned()
        .collect();
    let mut avatars = HashMap::new();
    if shows_sender_pictures(chat) {
        let mut senders: HashSet<String> = conversation
            .messages
            .iter()
            .filter(|message| !message.from_me)
            .map(|message| message.sender.clone())
            .collect();
        // A typing participant may not have a visible message.
        senders.extend(typing.iter().map(|(id, _)| id.clone()));
        for sender in senders {
            let picture = app.avatar(&sender);
            avatars.insert(sender, picture);
        }
    }
    let names_or = |id: &str, hint: Option<&str>| app.display_name_or(id, hint);
    let mention_names = |id: &str| app.mention_name(id);
    let keyboard_navigation = std::cell::Cell::new(false);
    let scrims = std::cell::RefCell::new(Vec::new());
    let row_tops = std::cell::RefCell::new(Vec::new());
    let view = View {
        palette,
        locale: app.locale,
        chat,
        me: app.me.as_deref(),
        auto_download: app.account().settings.auto_download,
        connected: app.link.is_connected(),
        account_receipts_off: app.account_receipts_off,
        poll_voting: &app.poll_voting,
        interactive_pending: &app.interactive_sending,
        selecting: app
            .selection
            .as_ref()
            .is_some_and(|(selected_chat, _)| *selected_chat == chat.id),
        selected: app
            .selection
            .as_ref()
            .filter(|(selected_chat, _)| *selected_chat == chat.id)
            .map_or(&[][..], |(_, ids)| ids.as_slice()),
        anchor: if conversation.loading_older || conversation.fetching_phone {
            None
        } else {
            app.scroll_anchor.as_deref()
        },
        open_menu: app.open_message_menu.as_deref(),
        starred: app.stars.get(&chat.id),
        pins: app.pins.get(&chat.id),
        pin_notices: app
            .chat_pins
            .get(&chat.id)
            .map_or(&[][..], |rows| rows.as_slice()),
        reaction: app
            .reaction_target
            .as_ref()
            .filter(|(id, _)| id == &chat.id)
            .map(|(_, message)| message.as_str()),
        reaction_menu: app.reaction_beside_menu,
        reaction_emoji: &app.settings.reaction_emoji,
        menu: app.settings.message_menu,
        keyboard_navigation: &keyboard_navigation,
        names_or: &names_or,
        mention_names: &mention_names,
        avatars: &avatars,
        contacts: &app.contacts,
        now: crate::util::now(),
        animate: app.window_focused,
        motion_playing: app
            .motion_playing
            .as_ref()
            .filter(|(playing, _)| *playing == chat.id)
            .map(|(_, message)| message.as_str()),
        player: &app.player,
        voice_speed: app.voice_speed_in(Some(&chat.id)),
        time_inside: app.palette.bubbles != theme::BubbleStyle::Nothing
            || app.settings.always_show_time,
        video: &app.video,
        // Whole-message exports are already formatted. Do not let the
        // text-selection annotator reinterpret them as drawn body text.
        copy_rows: (!app.selection.as_ref().is_some_and(|(id, _)| *id == chat.id))
            .then_some(app.copy_rows.as_ref()),
        swipes: app.settings.swipes,
        doodles: app.doodles_behind(),
        scrims: Some(&scrims),
        rows: Some(&row_tops),
    };
    let mut actions = Vec::new();
    let mut anchored = false;
    // The first unread message is the count-th incoming one from the end.
    let divider = app
        .unread_divider
        .as_ref()
        .filter(|divider| divider.chat == chat.id)
        .and_then(|divider| {
            let first = divider.first.clone().or_else(|| {
                conversation
                    .messages
                    .iter()
                    .rev()
                    .filter(|message| !message.from_me)
                    .nth(divider.count.saturating_sub(1) as usize)
                    .map(|message| message.id.clone())
            })?;
            Some((first, divider.count, divider.placed))
        });
    let divider_first = divider.as_ref().map(|(first, ..)| first.clone());
    let mut divider_placed = false;
    let selection: Option<Vec<String>> = app
        .selection
        .as_ref()
        .filter(|(selected_chat, _)| *selected_chat == chat.id)
        .map(|(_, ids)| ids.clone());
    // How far the check column and the selected rows' bands have come in,
    // or gone on their way out once the selection has ended.
    let select_motion = selection_motion(
        ui.ctx(),
        &chat.id,
        selection.as_deref(),
        selection_calm(ui.ctx(), view.animate),
    );
    let selection_leaving = selection.is_none() && select_motion.leaving();
    // A sweep over messages (#246) follows the pointer while the button is
    // held, whichever row took the press, and ends on release.
    let sweep = app
        .sweep
        .as_ref()
        .filter(|sweep| sweep.chat == chat.id)
        .map(|sweep| (sweep.anchor.clone(), sweep.to.clone()));
    if app.sweep.is_some() && (sweep.is_none() || !ui.input(|input| input.pointer.primary_down())) {
        actions.push(Action::EndSweep);
    }
    let sweep = sweep.filter(|_| ui.input(|input| input.pointer.primary_down()));
    let sweep_pointer = ui.input(|input| input.pointer.latest_pos());
    // The row under the pointer: the last laid-out row whose top it is
    // below, or the first one when it is above them all.
    let mut swept_to: Option<&str> = None;
    let mut first_row: Option<&str> = None;
    let scroll_to_bottom =
        app.scroll_to_bottom && divider.as_ref().is_none_or(|(.., placed)| *placed);
    // The message a quote or search result jumped to flashes once in view.
    let jump = app
        .jump_highlight
        .clone()
        .filter(|jump| jump.chat == chat.id);
    let jump_since = std::cell::Cell::new(jump.as_ref().and_then(|jump| jump.since));
    let time = ui.input(|input| input.time);
    // When the transcript was first drawn: messages already there appear in
    // place, and only later ones arrive with motion. Marked here rather
    // than at the first new message, or that one would never pop in.
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or(chat_shown_id(&chat.id), time);
    });
    // A jump to a message glides there instead of cutting, and its flash
    // starts once the glide settles. Reduced motion and a scroll that keeps
    // the reader's place (older history loading in) stay instant.
    let jump_glides = jump.as_ref().is_some_and(|jump| {
        jump.since.is_none()
            && app.scroll_anchor.as_deref() == Some(jump.message.as_str())
            && view.animate
            && !crate::motion::screen_reader_attached(ui.ctx())
    });
    let jump_moving = std::cell::Cell::new(false);
    let jump_scroll_id = ui
        .make_persistent_id(egui::IdSalt::new(("messages", &chat.id)))
        .with("jump-scroll");
    // Do not animate programmatic scrolling. Pending animations can delay a
    // later request to reach the end. Keyboard page, Home, and End scrolls
    // ease by applying small instant steps each frame instead (`KeyScroll`).
    let mut edge_scrolled_up = false;
    // Rows far from the viewport are skipped rather than laid out, keeping the
    // height they last took (or an estimate), so a long history costs the rows
    // near the screen instead of every loaded one. A jump to a message and
    // the unread divider's first placement lay out every row: a jump needs
    // exact positions. A text selection keeps the rows that hold its ends
    // laid out (`TextEnds`). Rows near the viewport are measured again each
    // frame, so a width change or an image that loads corrects them before
    // they come into view.
    let layout_width = ui.available_width();
    let lay_out_all = view.anchor.is_some() || divider.as_ref().is_some_and(|(.., placed)| !placed);
    let mut text_ends = TextEnds::begin(
        ui,
        std::mem::take(&mut conversation.text_ends),
        &conversation.messages,
    );
    let pass = ui.ctx().cumulative_pass_nr();
    let redo = ui.ctx().current_pass_index() > 0;
    let mut rows = std::mem::take(&mut conversation.rows);
    // Forget rows that left the conversation, such as deleted messages.
    if rows.len() > conversation.messages.len() * 2 + 64 {
        let ids: HashSet<&str> = conversation
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        rows.retain(|id, _| ids.contains(id.as_str()));
    }
    // The row the reader's view started at last frame, and where its top lay
    // in the transcript. Wherever it lies now, the offset follows by the
    // difference, so what the reader looks at stays put while rows above it
    // change: rows scrolled past measured for the first time, a page of older
    // history arriving at the top as the reader scrolls up, or a picture
    // above the view taking its height.
    let last_top_row = conversation.top_row.take();
    let mut top_row: Option<(String, f32)> = None;
    let mut grew_above = 0.0;
    // The offset the message list is about to load, the distance a Home
    // scroll still has to cover: the same id the `ScrollArea` below loads.
    // `.id_salt()` hashes its salt into an `IdSalt` before combining it with
    // the `Ui`'s id, so the salt must go through the same `IdSalt::new` here
    // to land on the same id.
    let scroll_id = ui.make_persistent_id(egui::IdSalt::new(("messages", &chat.id)));
    let scroll_state = egui::scroll_area::State::load(ui.ctx(), scroll_id);
    // The list has not been shown before, or showed no rows yet while its
    // messages were on their way: it starts at the top.
    let first_show = scroll_state.is_none() || rows.is_empty();
    let redo_scroll_id = scroll_id.with("redo-scroll");
    let offset = scroll_state.map_or(0.0, |state| state.offset.y);
    // A keyboard scroll under way stops for a jump to a message, for the
    // reaction bar, which holds the view still, and for a wheel or trackpad
    // turn over the message list, before it takes another step.
    let mut key_scroll = conversation.key_scroll.take();
    let list = ui.available_rect_before_wrap();
    let wheel_over_list = ui.input(|input| {
        (input.smooth_scroll_delta.y != 0.0
            || input
                .raw
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::MouseWheel { .. })))
            && input
                .pointer
                .hover_pos()
                .is_some_and(|pos| list.contains(pos))
    });
    // The reaction bar holds the view still.
    let carried = if view.reaction.is_some() {
        0.0
    } else {
        carried
    };
    if view.anchor.is_some() || view.reaction.is_some() || wheel_over_list || carried != 0.0 {
        key_scroll = None;
    }
    let key_duration = ui.style().scroll_animation.duration.max;
    // A key scroll starts one frame back, so it moves on its first frame and
    // a held key's repeats, which restart it, never stall it.
    let key_start = time - f64::from(ui.input(|input| input.stable_dt.min(0.1)));
    let mut pinned = false;
    let mut visible_day = None;
    let reduce_motion = crate::motion::screen_reader_attached(ui.ctx());
    // Deleted messages still leaving go at once without motion.
    let leaving_now = std::time::Instant::now();
    if reduce_motion || !view.animate {
        conversation.leaving.clear();
    } else {
        super::leaving::settle(&mut conversation.leaving, leaving_now);
    }
    if !conversation.leaving.is_empty() {
        ui.ctx().request_repaint();
    }
    // The rows hang on springs while the reader scrolls, as in iOS
    // Messages. They go to their places at once with a screen reader, in a
    // resting window, for a jump, and while something is held or dragged in
    // the list: a sweep, a text selection, a swipe or a carried message.
    let springs_calm = super::scroll_springs::Calm {
        screen_reader: reduce_motion,
        resting: !view.animate || crate::motion::window_resting(ui.ctx()),
        jumping: lay_out_all
            || jump_glides
            || ui
                .ctx()
                .data(|data| data.get_temp::<(f64, f32)>(jump_scroll_id))
                .is_some(),
        holding: app.sweep.is_some()
            || super::carry::lifted()
            || super::carry::carrying(ui.ctx())
            || super::swipe::over(ui.ctx(), list).is_some()
            || ui.input(|input| {
                input.pointer.primary_down()
                    && input.pointer.is_decidedly_dragging()
                    && input.pointer.press_origin().is_some_and(|origin| {
                        list.contains(origin) && origin.x < list.right() - 16.0
                    })
            }),
    };
    let mut springs = super::scroll_springs::Springs::load(ui.ctx(), &chat.id);
    if springs_calm.allows_springs() {
        springs.advance(time);
    } else {
        springs.clear();
    }
    let row_shifts = springs.shifts();
    let mut laid_rows = Vec::new();
    let mut content_top_drawn = None;
    let layer = ui.layer_id();
    // The message edited in its bubble, whose row goes on a layer above the
    // scrim the rest of the chat recedes under (`paint_edit_focus`).
    let edited = edit_focus(ui.ctx(), &chat.id).map(|(id, _)| id);
    let (_, lifted_layer) = edit_focus_layers(&chat.id);
    let first_shape = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    let newest = conversation
        .messages
        .last()
        .map(|message| message.id.clone());
    // The messages scroll on under the frosted header and composer; the
    // scroll bar keeps to the clear stretch between them.
    let clear = ui.available_rect_before_wrap();
    let bar_rect = Rect::from_x_y_ranges(
        clear.x_range(),
        (clear.top() + under)..=(clear.bottom() - over).max(clear.top() + under),
    );
    // The scroll bar's handle takes the accent, as the chat list's does; the
    // rows put the plain colours back for their own widgets.
    let accent = ui.visuals().selection.stroke.color;
    let plain = handle_colors(ui.visuals_mut()).map(|color| std::mem::replace(color, accent));
    let mut scrim_slot = None;
    let output = egui::ScrollArea::vertical()
        .id_salt(("messages", &chat.id))
        .scroll_bar_rect(bar_rect)
        .auto_shrink([false, false])
        .stick_to_bottom(view.reaction.is_none())
        .scroll_source(if view.reaction.is_some() {
            egui::scroll_area::ScrollSource::NONE
        } else {
            Default::default()
        })
        .animated(false)
        .show(ui, |ui| {
            for (color, plain) in handle_colors(ui.visuals_mut()).into_iter().zip(plain) {
                *color = plain;
            }
            // Under every row: the bubbles' scrims gather here at the end.
            scrim_slot = Some(ui.painter().add(egui::Shape::Noop));
            // Scroll while selecting near an edge. `scroll_with_delta` also
            // releases stick-to-bottom; setting the offset directly does not.
            let viewport = ui.clip_rect();
            ui.ctx().data_mut(|data| {
                data.insert_temp(transcript_span_id(&chat.id), viewport.y_range());
            });
            // A row lifted over the edit scrim keeps to the clear stretch
            // between the header and the composer, which sit under the scrim.
            let between = Rect::from_x_y_ranges(viewport.x_range(), bar_rect.y_range());
            let lifted_clip = viewport.intersect(between);
            // Where the transcript starts, so a row's place in it does not
            // depend on the offset.
            let content_top = ui.max_rect().top();
            content_top_drawn = Some(content_top);
            *app.selection_view.lock().unwrap_or_else(|p| p.into_inner()) = Some(viewport);
            if carried != 0.0 {
                ui.scroll_with_delta_animation(
                    vec2(0.0, carried),
                    egui::style::ScrollAnimation::none(),
                );
            }
            // What the pass before this redo scrolled, once it is painted.
            if let Some(scrolled) = ui
                .ctx()
                .data_mut(|data| data.remove_temp::<f32>(redo_scroll_id))
                .filter(|_| redo)
            {
                ui.scroll_with_delta_animation(
                    vec2(0.0, -scrolled),
                    egui::style::ScrollAnimation::none(),
                );
            }
            // Keep ordinary conversation-space clicks useful: after reading,
            // the next keystroke should go straight to the composer. Register
            // this before the message controls so text, links, media, and
            // selection interactions remain in front of the background.
            let background = ui.interact(
                viewport,
                ui.id().with(("message-background", &chat.id)),
                Sense::click(),
            );
            // Only a drag that has moved past a click, such as selecting
            // text, scrolls; a click near an edge does not.
            let held_inside = ui.input(|input| {
                input.pointer.primary_down()
                    && input.pointer.is_decidedly_dragging()
                    && input.pointer.press_origin().is_some_and(|origin| {
                        viewport.contains(origin) && origin.x < viewport.right() - 16.0
                    })
            });
            // A pass redone after rows above the view were first measured
            // has had this frame's step already, in the pass before it.
            if view.reaction.is_none()
                && held_inside
                && !redo
                && let Some(pointer) = ui.input(|input| input.pointer.latest_pos())
            {
                let delta = edge_scroll(pointer.y, viewport.top(), viewport.bottom());
                if delta != 0.0 {
                    if delta < 0.0 {
                        edge_scrolled_up = true;
                    }
                    key_scroll = None;
                    ui.scroll_with_delta_animation(
                        vec2(0.0, -delta),
                        egui::style::ScrollAnimation::none(),
                    );
                    ui.ctx().request_repaint();
                }
            }
            // A chat shorter than the view sits at its bottom, next to the
            // composer, as on WhatsApp: the space above it is what the
            // messages left free last frame.
            let natural_id = scroll_id.with("natural-height");
            let natural = ui.ctx().data(|data| data.get_temp::<f32>(natural_id));
            // The spacing after the filler is part of the space it takes.
            let filler = bottom_filler(viewport.height(), natural, ui.spacing().item_spacing.y);
            // While a button is held (a sweep, a text selection, a click),
            // the rows stay where the pointer found them: a sweep turns the
            // composer into the selection bar, and the view's new height
            // would otherwise move every row under the pointer.
            let filler_id = scroll_id.with("filler");
            let held = ui.input(|input| input.pointer.any_down());
            let filler = match ui.ctx().data(|data| data.get_temp::<f32>(filler_id)) {
                Some(kept) if held => kept,
                _ => filler,
            };
            ui.ctx()
                .data_mut(|data| data.insert_temp(filler_id, filler));
            if filler > 0.0 {
                ui.add_space(filler);
            }
            let side = if palette.bubbles == theme::BubbleStyle::Messages {
                MESSAGES_SIDE_MARGIN
            } else {
                WHATSAPP_SIDE_MARGIN
            };
            let measured = Frame::new()
                .inner_margin(Margin::symmetric(side, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y =
                        if palette.bubbles == theme::BubbleStyle::Nothing {
                            2.0
                        } else {
                            3.0
                        };
                    if under > 0.0 {
                        ui.add_space(under);
                    }
                    top_of_history(ui, &palette, &conversation, chat, &mut actions);
                    let mut previous: Option<&Message> = None;
                    let mut day_separator_top = f32::NEG_INFINITY;
                    // The pin notices still to place, in time order. Each one
                    // sits above the first message that came after it, so the
                    // row it belongs to carries its height.
                    let notices = pin_notices_in_time_order(view.pin_notices);
                    let mut placed = 0;
                    // Rows within a few viewports of the screen are laid out
                    // and their height remembered, so scrolling finds them
                    // measured before they show.
                    let margin = (viewport.height() * 3.0).max(600.0);
                    let last_outgoing = conversation.messages.iter().rposition(|message| {
                        message.from_me && !matches!(message.content, Content::Revoked)
                    });
                    for (message_index, message) in conversation.messages.iter().enumerate() {
                        let index = message_index;
                        let above = message_index
                            .checked_sub(1)
                            .map(|before| conversation.messages[before].id.as_str());
                        leaving_rows(ui, &view, &conversation.leaving, above, leaving_now);
                        // WhatsApp marks every item in a media album with the
                        // same parent id. Only its first item owns a row; the
                        // remaining consecutive items are painted in that row.
                        if selection.is_none()
                            && message_index > 0
                            && album_id(message).is_some()
                            && album_id(&conversation.messages[message_index - 1])
                                == album_id(message)
                            && conversation.messages[message_index - 1].from_me == message.from_me
                            && conversation.messages[message_index - 1].sender == message.sender
                            && crate::util::day_key(
                                conversation.messages[message_index - 1].timestamp,
                            ) == crate::util::day_key(message.timestamp)
                        {
                            rows.insert(
                                message.id.clone(),
                                RowHeight {
                                    height: 0.0,
                                    pass: Some(pass),
                                },
                            );
                            previous = Some(message);
                            continue;
                        }
                        let album = selection
                            .is_none()
                            .then(|| album_run(&conversation.messages, message_index))
                            .flatten();
                        let row_contains = |id: &str| {
                            message.id == id
                                || album.is_some_and(|items| items.iter().any(|item| item.id == id))
                        };
                        let row_is_anchor = view.anchor.is_some_and(row_contains);
                        let before = ui.cursor().top();
                        let new_day = previous.is_none_or(|previous| {
                            crate::util::day_key(previous.timestamp)
                                != crate::util::day_key(message.timestamp)
                        });
                        if new_day {
                            day_separator_top = before + 8.0;
                        }
                        if top_row.is_none() && before >= viewport.top() - 0.5 {
                            top_row = Some((message.id.clone(), before - content_top));
                        }
                        if let Some((id, was)) = &last_top_row
                            && *id == message.id
                        {
                            grew_above = before - content_top - was;
                        }
                        let here = notices_above(&notices[placed..], message.timestamp);
                        let known = rows.get(&message.id).copied();
                        let height = known.map_or_else(
                            || estimated_height(message, layout_width, new_day, here),
                            |row| row.height,
                        );
                        // A pass redone after the offset followed rows that
                        // grew keeps to the rows it laid out before (and any
                        // now on screen): measuring rows the shift brought
                        // into range would move the view once more.
                        let reach = if redo
                            && known
                                .and_then(|row| row.pass)
                                .is_none_or(|last| last + 1 != pass)
                        {
                            0.0
                        } else {
                            margin
                        };
                        let near = before + height >= viewport.top() - reach
                            && before <= viewport.bottom() + reach;
                        let members = album.unwrap_or(std::slice::from_ref(message));
                        if !lay_out_all && !near && !text_ends.holds(message_index, members) {
                            // The body rect of a row that just left the
                            // layout marks where it last was, not where it is.
                            if known
                                .is_some_and(|row| row.pass.is_some_and(|last| last + 1 == pass))
                            {
                                let id = bubble_id(&chat.id, &message.id).with("body");
                                ui.ctx().data_mut(|data| data.remove::<Rect>(id));
                                for member in album.unwrap_or(std::slice::from_ref(message)) {
                                    forget_bubble(ui.ctx(), &chat.id, &member.id);
                                }
                            }
                            ui.add_space(height);
                            if known.is_none() {
                                rows.insert(message.id.clone(), RowHeight { height, pass: None });
                            }
                            previous = Some(message);
                            continue;
                        }
                        // A row laid out again after a skip still has the
                        // rect it last had on screen, where other rows are
                        // now. The bubble registers its click targets from
                        // it, so drop it rather than let it take their clicks.
                        if known.is_none_or(|row| row.pass.is_none_or(|last| last + 1 < pass)) {
                            let id = bubble_id(&chat.id, &message.id).with("rect");
                            ui.ctx().data_mut(|data| data.remove::<Rect>(id));
                        }
                        if sweep.is_some() {
                            first_row.get_or_insert(message.id.as_str());
                            if sweep_pointer.is_some_and(|pointer| pointer.y >= before) {
                                swept_to = Some(message.id.as_str());
                            }
                        }
                        // The row is laid out where its spring has it, its
                        // slot where it belongs, so clicks land on what is
                        // drawn. Always in a row of its own, so its widgets
                        // keep their ids whether it springs or not.
                        let row_shift = row_shifts.get(&message.id).copied().unwrap_or(0.0);
                        // The message edited in its bubble goes on the layer
                        // above the scrim the rest of the chat recedes under.
                        let lifted = edited.as_ref().is_some_and(|id| *id == message.id);
                        let outer = &mut *ui;
                        let mut row_ui = outer.new_child(
                            egui::UiBuilder::new()
                                .max_rect(
                                    outer
                                        .available_rect_before_wrap()
                                        .translate(vec2(0.0, row_shift)),
                                )
                                .layout(*outer.layout())
                                .layer_id(if lifted {
                                    lifted_layer
                                } else {
                                    outer.layer_id()
                                }),
                        );
                        if lifted {
                            row_ui.set_clip_rect(lifted_clip);
                        }
                        let ui = &mut row_ui;
                        if new_day {
                            ui.add_space(8.0);
                            ui.vertical_centered(|ui| {
                                widgets::chip(
                                    ui,
                                    &palette,
                                    &crate::util::day_label(app.locale, message.timestamp),
                                );
                            });
                            ui.add_space(4.0);
                        }
                        for _ in 0..here {
                            let Some(notice) = notices.get(placed) else {
                                break;
                            };
                            pin_notice(ui, &palette, view.locale, view.names_or, notice);
                            placed += 1;
                        }
                        if let Some((id, count, placed)) = &divider
                            && row_contains(id)
                        {
                            ui.add_space(6.0);
                            let label = crate::i18n::ngettext(
                                app.locale,
                                "{} unread message",
                                "{} unread messages",
                                *count,
                            )
                            .replace("{}", &count.to_string());
                            let response = ui
                                .vertical_centered(|ui| widgets::chip(ui, &palette, &label))
                                .inner;
                            if !placed && view.anchor.is_none() {
                                // Clear of the header that may lie over the top.
                                ui.scroll_to_rect(
                                    response.rect.with_min_y(response.rect.top() - under),
                                    Some(Align::Min),
                                );
                                divider_placed = true;
                            }
                            ui.add_space(4.0);
                        }
                        let show_sender = shows_sender_pictures(chat)
                            && !message.from_me
                            && (new_day
                                || previous.is_none_or(|previous| {
                                    previous.sender != message.sender || previous.from_me
                                }));
                        // The first message of a run from one side, as the
                        // phone draws it: a little apart, with a tail.
                        let first_in_run =
                            new_day || previous.is_none_or(|previous| !same_run(previous, message));
                        let last_in_run = last_of_run(&conversation.messages, index);
                        let last_outgoing_row = last_outgoing.is_some_and(|last| {
                            last == message_index
                                || album.is_some_and(|members| {
                                    members.iter().any(|member| {
                                        conversation
                                            .messages
                                            .get(last)
                                            .is_some_and(|latest| latest.id == member.id)
                                    })
                                })
                        });
                        // Messages curls its tail under a run's last bubble.
                        // The sender's picture: beside the first message of a
                        // run in WhatsApp's style, beside the last in Messages
                        // and Nothing.
                        let show_avatar = if matches!(
                            palette.bubbles,
                            theme::BubbleStyle::Messages | theme::BubbleStyle::Nothing
                        ) {
                            shows_sender_pictures(chat) && !message.from_me && last_in_run
                        } else {
                            show_sender
                        };
                        let tailed = if palette.bubbles == theme::BubbleStyle::Messages {
                            last_bubble_of_run(&palette, &conversation.messages, index)
                        } else {
                            first_in_run
                        };
                        if first_in_run && !new_day && previous.is_some() {
                            ui.add_space(if palette.bubbles == theme::BubbleStyle::Nothing {
                                12.0
                            } else {
                                RUN_GAP
                            });
                        }
                        let flash = jump
                            .as_ref()
                            .filter(|jump| row_contains(&jump.message))
                            .map(|_| (ui.painter().add(egui::Shape::Noop), ui.cursor().top()));
                        // While selecting, every row gains a check box in a
                        // column on the left and a band behind it, as in
                        // WhatsApp Web, painted under the row once its height
                        // is known. They stay a moment as a selection ends,
                        // on their way out.
                        let band = (selection.is_some() || selection_leaving)
                            .then(|| (ui.painter().add(egui::Shape::Noop), ui.cursor().top()));
                        let row_start = super::swipe::mark(ui);
                        let response = if selection.is_some() {
                            ui.horizontal_top(|ui| {
                                ui.add_space(SELECT_GUTTER);
                                ui.vertical(|ui| {
                                    bubble(
                                        ui,
                                        &view,
                                        message,
                                        album,
                                        RunPlace {
                                            show_sender,
                                            show_avatar,
                                            tailed,
                                            first_in_run,
                                            last_in_run,
                                            last_outgoing: last_outgoing_row,
                                        },
                                        reduce_motion,
                                        &mut actions,
                                    )
                                })
                                .inner
                            })
                            .inner
                        } else {
                            bubble(
                                ui,
                                &view,
                                message,
                                album,
                                RunPlace {
                                    show_sender,
                                    show_avatar,
                                    tailed,
                                    first_in_run,
                                    last_in_run,
                                    last_outgoing: last_outgoing_row,
                                },
                                reduce_motion,
                                &mut actions,
                            )
                        };
                        // As the check column comes in or leaves, a row it
                        // moves is laid out at once where it goes and drawn
                        // sliding there from where it was, so no text is laid
                        // out again at a width that changes every frame.
                        let column_shift = select_motion.row_shift(message.from_me);
                        if column_shift != 0.0 {
                            let layer = ui.layer_id();
                            ui.ctx().graphics_mut(|graphics| {
                                let list = graphics.entry(layer);
                                for index in row_start.0..list.next_idx().0 {
                                    list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                                        clipped.shape.translate(vec2(column_shift, 0.0));
                                    });
                                }
                            });
                        }
                        if let Some((slot, top)) = flash {
                            if row_is_anchor && response.is_some() && !jump_glides {
                                jump_since.set(Some(time));
                            }
                            let strength = jump_since
                                .get()
                                .map_or(0.0, |since| JumpHighlight::strength(time - since));
                            if strength > 0.0 {
                                // Behind the row, across the whole message
                                // view, like WhatsApp's.
                                let band = Rect::from_x_y_ranges(
                                    viewport.x_range(),
                                    top - 3.0..=ui.min_rect().bottom() + 3.0,
                                );
                                ui.painter().set(
                                    slot,
                                    egui::Shape::rect_filled(
                                        band,
                                        0.0,
                                        palette.accent.gamma_multiply(0.22 * strength),
                                    ),
                                );
                            }
                            if jump_since.get().is_some() {
                                crate::motion::request_frame(ui.ctx());
                            }
                        }
                        if let (Some(selected), Some(response), Some((slot, top))) =
                            (&selection, &response, band)
                        {
                            let checked = selected.contains(&message.id);
                            let selectable = crate::app::can_select(&message.content);
                            // While selecting, a click anywhere on the row picks
                            // the message: its check box, text, links and media,
                            // and the strip beside it, from one edge of the view
                            // to the other (#241). Registered after the row, so
                            // it takes those clicks, and drags, which sweep
                            // messages rather than text (#246).
                            // Half the gap to each neighbour, so the space
                            // between rows picks one of them too.
                            let row = Rect::from_x_y_ranges(
                                viewport.x_range(),
                                top..=ui.min_rect().bottom(),
                            )
                            .expand2(vec2(0.0, ui.spacing().item_spacing.y / 2.0));
                            let pick = ui.interact(
                                row,
                                bubble_id(&chat.id, &message.id).with("pick"),
                                if selectable {
                                    Sense::click_and_drag()
                                } else {
                                    Sense::hover()
                                },
                            );
                            // The band fades in as the row joins the
                            // selection and out as it leaves.
                            let tick = select_motion.tick(&message.id, checked, time);
                            let opacity = select_motion.opacity;
                            let fill = if tick > 0.0 {
                                palette.accent.gamma_multiply(0.16 * tick * opacity)
                            } else if pick.hovered() {
                                palette.text.gamma_multiply(0.05 * opacity)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter()
                                .set(slot, egui::Shape::rect_filled(row, 0.0, fill));
                            // Beside the bubble as drawn this frame: the
                            // response carries the rect it had the frame before.
                            let drawn = ui
                                .ctx()
                                .data(|data| {
                                    data.get_temp::<Rect>(
                                        bubble_id(&chat.id, &message.id).with("rect"),
                                    )
                                })
                                .unwrap_or(response.rect);
                            // In from the left edge as the selection starts.
                            let left = ui.max_rect().left();
                            let check = select_motion.check_rect(left, drawn.center().y);
                            // Only forwardable messages get a box.
                            if selectable {
                                select_box(
                                    ui,
                                    &palette,
                                    check,
                                    tick,
                                    select_motion.pop(&message.id, time),
                                    select_motion.opacity,
                                );
                                if pick.has_focus()
                                    && ui.ctx().data(|data| {
                                        data.get_temp::<bool>(theme::keyboard_focus_id())
                                            .unwrap_or(false)
                                    })
                                {
                                    keyboard_navigation.set(true);
                                }
                                theme::reveal_focus(&pick);
                                theme::focus_outline(ui, pick.id, check.expand(3.0), 4.0);
                                pick.widget_info(|| {
                                    let sender = if message.from_me {
                                        crate::i18n::gettext(view.locale, "You").into_owned()
                                    } else {
                                        (view.names_or)(
                                            &message.sender,
                                            message.sender_name.as_deref(),
                                        )
                                    };
                                    let summary: String = message
                                        .content
                                        .summary_in(view.locale)
                                        .chars()
                                        .take(120)
                                        .collect();
                                    let label = crate::i18n::gettext(
                                        view.locale,
                                        "Select message from {sender}, {time}: {summary}",
                                    )
                                    .replace("{sender}", &sender)
                                    .replace(
                                        "{time}",
                                        &crate::util::moment_stamp(view.locale, message.timestamp),
                                    )
                                    .replace("{summary}", &summary);
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Checkbox,
                                        pick.enabled(),
                                        checked,
                                        label,
                                    )
                                });
                                #[cfg(any(test, feature = "demo"))]
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        bubble_id(&chat.id, &message.id).with("check"),
                                        check,
                                    );
                                });
                            }
                            if pick.drag_started() {
                                actions.push(Action::SweepMessages {
                                    anchor: message.id.clone(),
                                    to: message.id.clone(),
                                });
                            }
                            let keyboard_clicked = (pick.clicked() && pick.has_focus()
                                || response.clicked() && response.has_focus())
                                && ui.input(|input| {
                                    input.raw.events.iter().any(|event| {
                                        matches!(
                                            event,
                                            egui::Event::Key {
                                                key: egui::Key::Enter,
                                                pressed: true,
                                                ..
                                            }
                                        )
                                    })
                                });
                            theme::focus_outline(ui, response.id, check.expand(3.0), 4.0);
                            let popup = egui::Popup::menu(&pick)
                                .open_memory(if pick.secondary_clicked() || keyboard_clicked {
                                    Some(egui::SetOpenCommand::Bool(true))
                                } else {
                                    None
                                })
                                .frame(widgets::menu_frame(&palette));
                            let popup = if keyboard_clicked {
                                popup.at_position(row.left_top() + vec2(12.0, 8.0))
                            } else {
                                popup.at_pointer_fixed()
                            };
                            popup.show_frosted(ui.ctx(), |ui| {
                                selection_menu(
                                    ui,
                                    &view,
                                    &conversation.messages,
                                    selected,
                                    &mut actions,
                                );
                            });
                            if selectable
                                && !keyboard_clicked
                                && (response.clicked() || pick.clicked())
                            {
                                let shift = ui.input(|input| input.modifiers.shift);
                                actions.push(if shift {
                                    Action::SelectRange(message.id.clone())
                                } else {
                                    Action::ToggleSelected(message.id.clone())
                                });
                            }
                        } else if let Some(response) = &response
                            && response.clicked()
                            && ui.input(|input| input.modifiers.command)
                            && crate::app::can_select(&message.content)
                        {
                            // Ctrl-click (Command-click on macOS) starts a selection.
                            actions.push(Action::SelectMessage(message.id.clone()));
                        }
                        // A selection just ended: its bands and check boxes
                        // fade and slide out the way they came. Only paint,
                        // so the rows take clicks as they do at rest.
                        if selection.is_none()
                            && let (Some(response), Some((slot, top))) = (&response, band)
                        {
                            let ticked = select_motion.ticked.contains(&message.id);
                            let tick = select_motion.tick(&message.id, ticked, time);
                            if tick > 0.0 {
                                let row = Rect::from_x_y_ranges(
                                    viewport.x_range(),
                                    top..=ui.min_rect().bottom(),
                                )
                                .expand2(vec2(0.0, ui.spacing().item_spacing.y / 2.0));
                                let alpha = 0.16 * tick * select_motion.opacity;
                                let fill = palette.accent.gamma_multiply(alpha);
                                ui.painter()
                                    .set(slot, egui::Shape::rect_filled(row, 0.0, fill));
                            }
                            if crate::app::can_select(&message.content) {
                                let drawn = ui
                                    .ctx()
                                    .data(|data| {
                                        data.get_temp::<Rect>(
                                            bubble_id(&chat.id, &message.id).with("rect"),
                                        )
                                    })
                                    .unwrap_or(response.rect);
                                let left = ui.max_rect().left();
                                let check = select_motion.check_rect(left, drawn.center().y);
                                select_box(
                                    ui,
                                    &palette,
                                    check,
                                    tick,
                                    select_motion.pop(&message.id, time),
                                    select_motion.opacity,
                                );
                            }
                        }
                        if let Some(response) = response
                            && row_is_anchor
                        {
                            if jump_glides {
                                let state = ui
                                    .ctx()
                                    .data(|data| data.get_temp::<(f64, f32)>(jump_scroll_id));
                                // Rows are laid out in full while the anchor
                                // is set, so the distance is exact.
                                let distance = response.rect.center().y - viewport.center().y;
                                let interrupted = ui.input(|input| {
                                    input.smooth_scroll_delta != Vec2::ZERO
                                        || input.events.iter().any(|event| {
                                            matches!(event, egui::Event::MouseWheel { .. })
                                        })
                                        || (input.pointer.is_decidedly_dragging()
                                            && input
                                                .pointer
                                                .press_origin()
                                                .is_some_and(|origin| viewport.contains(origin)))
                                        || input.events.iter().any(|event| {
                                            matches!(event, egui::Event::Key { pressed: true, .. })
                                        })
                                });
                                match state {
                                    None => {
                                        let (instant, _) = jump_plan(distance, viewport.height());
                                        if instant != 0.0 {
                                            ui.scroll_with_delta_animation(
                                                vec2(0.0, -instant),
                                                egui::style::ScrollAnimation::none(),
                                            );
                                        }
                                        ui.ctx().data_mut(|data| {
                                            data.insert_temp(jump_scroll_id, (time, 0.0_f32))
                                        });
                                        jump_moving.set(true);
                                        ui.ctx().request_repaint();
                                    }
                                    Some(_) if interrupted => {
                                        ui.ctx().data_mut(|data| {
                                            data.remove_temp::<(f64, f32)>(jump_scroll_id)
                                        });
                                        jump_since.set(Some(time));
                                        anchored = true;
                                    }
                                    Some((start, done)) => {
                                        let raw = ((time - start) as f32 / JUMP_SCROLL_SECONDS)
                                            .clamp(0.0, 1.0);
                                        let eased = ease_out_strong(raw);
                                        let share = if eased >= 1.0 {
                                            1.0
                                        } else {
                                            ((eased - done) / (1.0 - done)).clamp(0.0, 1.0)
                                        };
                                        ui.scroll_with_delta_animation(
                                            vec2(0.0, -distance * share),
                                            egui::style::ScrollAnimation::none(),
                                        );
                                        if raw >= 1.0 {
                                            ui.ctx().data_mut(|data| {
                                                data.remove_temp::<(f64, f32)>(jump_scroll_id)
                                            });
                                            jump_since.set(Some(time));
                                            anchored = true;
                                        } else {
                                            ui.ctx().data_mut(|data| {
                                                data.insert_temp(
                                                    jump_scroll_id,
                                                    (start, eased.max(done)),
                                                )
                                            });
                                            jump_moving.set(true);
                                            ui.ctx().request_repaint();
                                        }
                                    }
                                }
                            } else {
                                response.scroll_to_me(Some(Align::Center));
                                anchored = true;
                            }
                        }
                        let drawn_bottom = ui.min_rect().bottom() - row_shift;
                        let row_end = ui.cursor().top() - row_shift;
                        let ui = outer;
                        ui.add_space((row_end - ui.cursor().top()).max(0.0));
                        let measured = ui.cursor().top() - before;
                        if springs_calm.allows_springs()
                            && before + measured >= viewport.top() - viewport.height()
                            && before <= viewport.bottom() + viewport.height()
                        {
                            laid_rows.push(super::scroll_springs::Laid {
                                id: message.id.clone(),
                                top: before - content_top,
                                bottom: drawn_bottom - content_top,
                            });
                        }
                        if visible_day.is_none()
                            && before + measured > viewport.top()
                            && before < viewport.bottom()
                        {
                            visible_day = Some((message.timestamp, day_separator_top));
                        }
                        text_ends.laid_out(
                            members,
                            before + measured >= viewport.top() - margin
                                && before <= viewport.bottom() + margin,
                        );
                        rows.insert(
                            message.id.clone(),
                            RowHeight {
                                height: measured,
                                pass: Some(pass),
                            },
                        );
                        previous = Some(message);
                    }
                    let newest = conversation
                        .messages
                        .last()
                        .map(|message| message.id.as_str());
                    leaving_rows(ui, &view, &conversation.leaving, newest, leaving_now);
                    if let Some((anchor, to)) = &sweep
                        && let Some(row) = swept_to.or(first_row)
                        && row != to
                    {
                        actions.push(Action::SweepMessages {
                            anchor: anchor.clone(),
                            to: row.to_owned(),
                        });
                    }
                    // A pin made after the newest message keeps its notice at
                    // the end of the transcript.
                    for notice in &notices[placed..] {
                        pin_notice(ui, &palette, view.locale, view.names_or, notice);
                    }
                    upload_rows(ui, &palette, &uploads);
                    if !typing.is_empty() {
                        typing_indicator(ui, &view, &typing);
                    }
                    // Room to scroll the last message clear of a composer
                    // that floats over the list's end.
                    ui.add_space((over - 4.0).max(0.0));
                    // An explicit jump (Ctrl+End, or the return-to-bottom
                    // button) wins over a message bubble's retained keyboard
                    // focus; other triggers still defer to it, so an
                    // automatic pin does not pull the view away from what a
                    // keyboard-navigating reader is looking at.
                    if scroll_to_bottom
                        && (scroll_forced || !keyboard_navigation.get())
                        && view.reaction.is_none()
                    {
                        ui.scroll_to_rect_animation(
                            Rect::from_min_size(ui.cursor().min, Vec2::ZERO),
                            None,
                            egui::style::ScrollAnimation::none(),
                        );
                        pinned = true;
                    }
                })
                .response
                .rect
                .height();
            if natural.is_none_or(|natural| (natural - measured).abs() > 0.5) {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(natural_id, measured));
                ui.ctx().request_repaint();
            }
            if background.clicked() {
                actions.push(Action::FocusComposer);
            }
            // A keyboard PgUp/PgDn/Home/End scroll moves by the next slice of
            // its eased distance each frame, as an instant scroll: relative
            // steps compose with the row-height compensation below, and
            // `scroll_with_delta` releases stick-to-bottom like edge-scroll
            // above. An instant jump to the end this frame wins over it.
            if pinned {
                key_scroll = None;
            } else if let Some(kind) = pending_scroll
                && view.reaction.is_none()
            {
                // A repeated page key adds a page to the distance left, so a
                // held key keeps moving; anything else starts over.
                let page = match kind {
                    Scroll::PageUp => viewport.height() * 0.9,
                    Scroll::PageDown => -(viewport.height() * 0.9),
                    Scroll::Top | Scroll::Bottom => 0.0,
                };
                let left = key_scroll
                    .filter(|scroll| scroll.kind == kind)
                    .map_or(0.0, |scroll| scroll.remaining);
                key_scroll = Some(KeyScroll::new(kind, left + page, key_start, key_duration));
            }
            if let Some(scroll) = &mut key_scroll {
                let fraction = scroll.advance(time);
                let step = match scroll.kind {
                    Scroll::PageUp | Scroll::PageDown => {
                        let step = scroll.remaining * fraction;
                        scroll.remaining -= step;
                        step
                    }
                    Scroll::Top => offset * fraction,
                    // Measured again every frame, so messages that arrive
                    // on the way are included.
                    Scroll::Bottom => {
                        -(ui.min_rect().bottom() - viewport.bottom()).max(0.0) * fraction
                    }
                };
                ui.scroll_with_delta_animation(
                    vec2(0.0, step),
                    egui::style::ScrollAnimation::none(),
                );
                ui.ctx().request_repaint();
            }
        });
    for (color, plain) in handle_colors(ui.visuals_mut()).into_iter().zip(plain) {
        *color = plain;
    }
    // Before the glide, which slides the slot with the rows.
    if let Some(slot) = scrim_slot {
        lay_scrims_under(ui.ctx(), layer, slot, &scrims.borrow());
    }
    // A row lifted over the edit scrim rises with the rows around it.
    let mut glided = vec![(layer, first_shape)];
    if edited.is_some() {
        glided.push((lifted_layer, egui::layers::ShapeIdx(0)));
    }
    glide_new_message(ui, &chat.id, newest, &output, &glided, &row_tops.borrow());
    app.scroll_route
        .place(crate::app::ScrollPane::Messages, output.inner_rect);
    sticky_date(ui, &palette, app.locale, chat, &output, visible_day, under);
    let at_bottom =
        output.state.offset.y + output.inner_rect.height() >= output.content_size.y - BOTTOM_SLACK;
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            scroll_metrics_id(&chat.id),
            (output.state.offset.y, output.inner_rect.height()),
        );
    });
    // Keep the view at the end while initial content expands, until the user
    // scrolls with the wheel, trackpad, or scrollbar.
    let bar = Rect::from_min_max(
        pos2(output.inner_rect.right() - 16.0, output.inner_rect.top()),
        output.inner_rect.right_bottom(),
    );
    let reader_scrolled = ui.input(|input| {
        input.smooth_scroll_delta.y != 0.0
            || input
                .raw
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::MouseWheel { .. }))
            || (input.pointer.primary_down()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| bar.contains(pos)))
    });
    // The reader's own scroll this frame pushes the rows when the next frame
    // draws them at the new offset, against the scroll and more the further
    // they are from the pointer. A redone pass already had this frame's.
    if springs_calm.allows_springs() {
        let moved = super::scroll_springs::Moved {
            by_reader: reader_scrolled || carried != 0.0,
            by_app: pinned
                || key_scroll.is_some()
                || jump_moving.get()
                || first_show
                || divider_placed,
        };
        let delta = moved.push(output.state.offset.y - offset);
        if !redo
            && delta != 0.0
            && let Some(content_top) = content_top_drawn
        {
            let viewport = output.inner_rect;
            let pointer = ui
                .input(|input| input.pointer.latest_pos())
                .map_or(viewport.center().y, |pointer| {
                    pointer.y.clamp(viewport.top(), viewport.bottom())
                });
            springs.scrolled(delta, pointer - content_top);
        }
        springs.finish(laid_rows);
        if springs.moving() {
            crate::motion::request_frame(ui.ctx());
        }
    }
    springs.store(ui.ctx(), &chat.id);
    let complete = conversation.complete;
    let loading = conversation.loading_older;
    let fetching = conversation.fetching_phone;
    let fetching_explicit = conversation.phone_explicit;
    let exhausted = conversation.phone_exhausted;
    conversation.rows = rows;
    conversation.top_row = top_row;
    conversation.text_ends = text_ends.finish(ui.ctx());
    // Keep the rows on screen where they were when rows above them changed
    // height or arrived, unless this frame scrolled on purpose (to the end, a
    // jump, or the divider) or sticks to the end, where egui keeps the offset
    // anyway.
    // Home targets an absolute offset (0), so it is skipped while Home
    // scrolls, rather than pushing the offset away from the top; PgUp/PgDn
    // steps are relative and still get it. The pass is redone at the new
    // offset, so the shift never shows.
    if grew_above.abs() >= 0.5
        && !lay_out_all
        && !scroll_to_bottom
        && !at_bottom
        && key_scroll.is_none_or(|scroll| scroll.kind != Scroll::Top)
    {
        ui.ctx()
            .request_discard("transcript rows above the viewport changed height");
        let mut state = output.state;
        if ui.ctx().will_discard() {
            // The redo paints where this pass did, moved by the shift alone,
            // and scrolls by what this pass scrolled only after painting, as
            // every frame does. Started from the scrolled offset, it would
            // paint this frame's scroll at once and the next frame's not at
            // all: a stutter each time a row was measured mid-scroll.
            let scrolled = state.offset.y - offset;
            state.offset.y = (offset + grew_above).max(0.0);
            ui.ctx()
                .data_mut(|data| data.insert_temp(redo_scroll_id, scrolled));
        } else {
            state.offset.y = (state.offset.y + grew_above).max(0.0);
        }
        state.store(ui.ctx(), output.id);
    }
    // A jump to the end moves the offset only after the rows were painted at
    // the old one. On a chat's first showing that is the top of its first
    // page, which would flash for a frame before the end; redo the pass at
    // the end instead.
    if first_show && pinned && !redo && (output.state.offset.y - offset).abs() >= 0.5 {
        ui.ctx().request_discard("scrolled to the end");
    }
    if reader_scrolled {
        key_scroll = None;
    }
    if let Some(scroll) = key_scroll
        && scroll.done()
    {
        key_scroll = None;
        if scroll.kind == Scroll::Bottom {
            // Keep later messages pinned, as Ctrl+End already does.
            app.scroll_to_bottom = true;
        }
    }
    conversation.key_scroll = key_scroll;
    app.conversations
        .insert(chat.id.clone(), std::mem::take(&mut conversation));
    // Fixed to the message it first found, whatever arrives after it.
    if let Some(divider) = app
        .unread_divider
        .as_mut()
        .filter(|divider| divider.chat == chat.id && divider.first.is_none())
    {
        divider.first = divider_first;
    }
    // Scrolling down to the end reads what the divider marked.
    if at_bottom
        && !app.at_bottom
        && app
            .unread_divider
            .as_ref()
            .is_some_and(|divider| divider.chat == chat.id && divider.placed)
    {
        app.unread_divider = None;
    }
    app.at_bottom = at_bottom;
    if app.scroll_to_bottom && (reader_scrolled || (keyboard_navigation.get() && !scroll_forced)) {
        app.scroll_to_bottom = false;
    }
    if divider_placed {
        if let Some(divider) = app
            .unread_divider
            .as_mut()
            .filter(|divider| divider.chat == chat.id)
        {
            divider.placed = true;
        }
        app.scroll_to_bottom = false;
    }
    if let Some(jump) = &mut app.jump_highlight
        && jump.chat == chat.id
    {
        jump.since = jump_since.get();
        if jump
            .since
            .is_some_and(|since| time - since > JumpHighlight::DURATION)
        {
            app.jump_highlight = None;
        }
    }
    if jump_moving.get()
        && let Some(divider) = app
            .unread_divider
            .as_mut()
            .filter(|divider| divider.chat == chat.id)
    {
        divider.placed = true;
    }
    if anchored {
        app.scroll_anchor = None;
        // The message the reader jumped to wins over the unread divider,
        // which would otherwise scroll away from it on the next frame.
        if let Some(divider) = app
            .unread_divider
            .as_mut()
            .filter(|divider| divider.chat == chat.id)
        {
            divider.placed = true;
        }
    } else if let Some(anchor) = app.scroll_anchor.clone()
        && !loading
        && !fetching
        && !app
            .conversations
            .get(&chat.id)
            .is_some_and(|c| c.message(&anchor).is_some())
        && app
            .conversations
            .get(&chat.id)
            .is_none_or(|c| c.complete || !c.messages.is_empty())
    {
        // Keep the anchor when the first page is still loading. Event::Messages
        // will request it again.
        app.scroll_anchor = None;
    }
    // At the top, load more from the archive and then the phone. Short chats
    // request more immediately.
    let fits = output.content_size.y <= output.inner_rect.height() + 1.0;
    let near_top = output.state.offset.y < 80.0;
    // Only a transcript the reader scrolled up through is their ask; a short
    // chat, or one still on its way to the end, is not.
    let explicit = near_top && !fits && !scroll_to_bottom;
    // A reader reaching the top while an automatic request waits claims it.
    let claim = fetching && explicit && !fetching_explicit;
    if (near_top || fits)
        && ((!complete && !loading) || (complete && (!fetching || claim) && !exhausted))
    {
        actions.push(Action::LoadOlder {
            chat: chat.id.clone(),
            explicit,
        });
    }
    swipe_chat(app, ui, chat, output.inner_rect, &mut actions);
    app.actions.extend(actions);
    if edge_scrolled_up {
        // Scrolling up releases stick-to-bottom.
        app.scroll_to_bottom = false;
    }
    // Show a return-to-bottom button while reading older messages.
    // It scales and fades in and out, as the other floating controls do.
    let appear = ui.ctx().animate_bool_with_time_and_easing(
        ui.id().with("to-bottom"),
        !at_bottom,
        if reduce_motion {
            0.0
        } else {
            theme::MOTION_QUICK
        },
        theme::ease_out,
    );
    if appear > 0.0 {
        let rect = output.inner_rect;
        let center = pos2(rect.right() - 34.0, rect.bottom() - over - 34.0);
        let button = Rect::from_center_size(center, Vec2::splat(40.0 * (0.7 + 0.3 * appear)));
        // On a layer above the messages, as the header and composer are,
        // so its glass joins theirs on the sheet (see [`super::glass`]).
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::new(
                    egui::Order::Middle,
                    ui.id().with("to-bottom-layer"),
                ))
                .max_rect(button)
                .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
        );
        child.multiply_opacity(appear);
        let unread = app.chat(&chat.id).map_or(0, |chat| chat.unread);
        // It floats over the messages: a soft shadow lifts it off them.
        child.painter().add(
            egui::epaint::Shadow {
                offset: [0, 3],
                blur: 10,
                spread: 0,
                color: palette.lift_shadow(),
            }
            .as_shape(button, CornerRadius::same(20)),
        );
        // Frosted over the messages it floats on; solid while Blur is off.
        child
            .painter()
            .add(super::glass::sheet_rounded(button, 20.0));
        if theme::circle_button(
            &mut child,
            Icon::ArrowDown,
            button.width(),
            palette.frosted(palette.overlay),
            palette.surface_hover,
            palette.text,
            "Newest message",
        )
        .clicked()
        {
            app.actions.push(Action::ScrollToBottom);
        }
        // On the button's layer, so the button does not cover it.
        if unread > 0 {
            widgets::badge(
                &child,
                &palette,
                pos2(center.x + 14.0, center.y - 16.0),
                unread,
                false,
            );
        }
    }
}

/// The disc behind a message's react or reply button: opaque, so the chat's
/// wallpaper does not show through, and lifted like a bubble.
fn floating_circle(ui: &egui::Ui, palette: &Palette, rect: Rect, hovered: bool, opacity: f32) {
    let radius = CornerRadius::from(rect.width() / 2.0);
    let mut shadow = palette.bubble_shadow();
    shadow.color = shadow.color.gamma_multiply(opacity);
    ui.painter().add(shadow.as_shape(rect, radius));
    let fill = if hovered {
        palette.surface_hover
    } else {
        palette.overlay
    };
    ui.painter().circle_filled(
        rect.center(),
        rect.width() / 2.0,
        fill.gamma_multiply(opacity),
    );
}

/// Loading state above the oldest visible message.
fn top_of_history(
    ui: &mut egui::Ui,
    palette: &Palette,
    conversation: &Conversation,
    _chat: &Chat,
    _actions: &mut [Action],
) {
    ui.vertical_centered(|ui| {
        if !conversation.complete {
            if conversation.loading_older {
                theme::spinner(ui, 18.0, palette.accent);
            } else {
                ui.add_space(18.0);
            }
        } else if conversation.fetching_phone {
            ui.horizontal(|ui| {
                let width = 260.0;
                ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
                theme::spinner(ui, 16.0, palette.accent);
                theme::text(
                    ui,
                    "Loading older messages from your phone…",
                    theme::regular(12.5),
                    palette.secondary,
                );
            });
        } else if conversation.messages.is_empty() {
            ui.add_space(24.0);
            if conversation.fetching_phone {
                widgets::chip(ui, palette, "Loading messages from your phone…");
            } else {
                widgets::chip(ui, palette, "No messages here yet");
            }
        } else {
            ui.add_space(6.0);
        }
    });
}

/// A padded avatar-and-dots row, without a message bubble or typing label.
fn typing_indicator(ui: &mut egui::Ui, view: &View<'_>, typers: &[(String, String)]) {
    let palette = view.palette;
    // Keep the indicator comfortably separated from the last message and
    // composer. Its height stays fixed while the dots animate, so motion
    // never shifts the transcript layout.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let show_avatars = shows_sender_pictures(view.chat);
        let count = if show_avatars { typers.len().min(3) } else { 0 };
        let step = SENDER_AVATAR * 0.6;
        let avatars_width = if count > 0 {
            SENDER_AVATAR + step * (count - 1) as f32
        } else {
            0.0
        };
        let avatar_gap = if show_avatars { 8.0 } else { 0.0 };
        let row_height = 36.0;
        ui.allocate_ui_with_layout(
            vec2(
                avatars_width + avatar_gap + TYPING_BUBBLE_PAD_X * 2.0 + 28.0,
                row_height,
            ),
            Layout::left_to_right(Align::Center),
            |ui| {
                // Message rows put the bubble the avatar's width plus 8 from
                // the edge; the gaps here must add up to exactly that.
                ui.spacing_mut().item_spacing.x = 0.0;
                if show_avatars {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(avatars_width, row_height), Sense::hover());
                    if ui.is_rect_visible(rect) {
                        for (index, (id, name)) in typers.iter().take(count).enumerate() {
                            let avatar = Rect::from_min_size(
                                pos2(
                                    rect.left() + step * index as f32,
                                    rect.center().y - SENDER_AVATAR / 2.0,
                                ),
                                Vec2::splat(SENDER_AVATAR),
                            );
                            if index > 0 {
                                // Outline overlapping avatars with the chat background.
                                ui.painter().circle_filled(
                                    avatar.center(),
                                    SENDER_AVATAR / 2.0 + 1.5,
                                    palette.chat,
                                );
                            }
                            widgets::paint_avatar(
                                ui,
                                &palette,
                                avatar,
                                name.trim_start_matches('~'),
                                id,
                                view.avatars.get(id).and_then(|picture| picture.as_deref()),
                            );
                        }
                    }
                }
                ui.add_space(avatar_gap);
                typing_bubble(ui, view);
            },
        );
    });
    ui.add_space(2.0);
}

const TYPING_BUBBLE_PAD_X: f32 = 14.0;
const TYPING_BUBBLE_PAD_Y: f32 = 11.0;

/// The dots inside an incoming bubble, with the theme's fill and corners.
fn typing_bubble(ui: &mut egui::Ui, view: &View<'_>) {
    let palette = view.palette;
    let corners = match palette.bubbles {
        theme::BubbleStyle::Nothing => widgets::grouped_bubble_corners(false, true, true),
        theme::BubbleStyle::Messages => CornerRadius::same(widgets::MESSAGES_BUBBLE_RADIUS),
        _ => widgets::bubble_corners(Some(widgets::Side::Left)),
    };
    let fill = ui.painter().add(egui::Shape::Noop);
    let inner = Frame::new()
        .inner_margin(Margin::symmetric(
            TYPING_BUBBLE_PAD_X as i8,
            TYPING_BUBBLE_PAD_Y as i8,
        ))
        .show(ui, |ui| typing_dots(ui, &palette));
    ui.painter().set(
        fill,
        egui::epaint::RectShape::filled(inner.response.rect, corners, palette.bubble_in),
    );
    // Where and when it was drawn, so the message it announced can grow out
    // of it, and since when it has been up, for its pop and its breath.
    let time = ui.input(|input| input.time);
    let pass = ui.ctx().cumulative_pass_nr();
    let id = typing_bubble_id(&view.chat.id);
    let (seen, shown) = ui.ctx().data_mut(|data| {
        let seen = TypingSeen::after(
            data.get_temp::<TypingSeen>(id),
            inner.response.rect,
            time,
            pass,
        );
        data.insert_temp(id, seen);
        (seen, data.get_temp::<f64>(chat_shown_id(&view.chat.id)))
    });
    // Still while motion is reduced or the window rests.
    if !view.animate || crate::motion::screen_reader_attached(ui.ctx()) {
        return;
    }
    let elapsed = time - seen.since;
    // It pops out of its tail corner as iMessage's does, unless it was
    // already up when the chat appeared, and breathes while it stays.
    let popping = shown.is_none_or(|shown| seen.since > shown) && !POP.settled(elapsed);
    let (pop, opacity) = if popping {
        (pop_scale(elapsed), pop_opacity(elapsed))
    } else {
        (1.0, 1.0)
    };
    let transform = scale_about(
        inner.response.rect.left_bottom(),
        pop * breath_scale(elapsed),
    );
    let layer = ui.layer_id();
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        let end = list.next_idx();
        for index in fill.0..end.0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                clipped.shape.transform(transform);
                if opacity < 1.0 {
                    egui::epaint::shape_transform::adjust_colors(
                        &mut clipped.shape,
                        move |colour| {
                            if *colour != Color32::PLACEHOLDER {
                                *colour = colour.gamma_multiply(opacity);
                            }
                        },
                    );
                }
            });
        }
    });
    // The dots ask for every frame while they show; the pop needs them even
    // so, and the breath rides on theirs.
    if popping {
        crate::motion::request_frame(ui.ctx());
    }
}

fn typing_bubble_id(chat: &str) -> egui::Id {
    egui::Id::new(("typing-bubble", chat))
}

/// Where and when the typing bubble was last drawn, in which pass, and
/// since when it has been up without a break.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TypingSeen {
    rect: Rect,
    drawn: f64,
    pass: u64,
    since: f64,
}

impl TypingSeen {
    /// The record for a typing bubble drawn at `rect` at `time`, in `pass`,
    /// after `before`. Drawn in the pass before, or earlier in this one, it
    /// is the same appearance however long ago that was, since a resting
    /// window draws seldom; after a gap it has appeared again.
    fn after(before: Option<Self>, rect: Rect, time: f64, pass: u64) -> Self {
        let since = before
            .filter(|seen| seen.pass + 1 >= pass)
            .map_or(time, |seen| seen.since);
        Self {
            rect,
            drawn: time,
            pass,
            since,
        }
    }
}

fn typing_dots(ui: &mut egui::Ui, palette: &Palette) {
    let radius = 3.0;
    let gap = 5.0;
    let lift = 3.0;
    let (rect, _) = ui.allocate_exact_size(
        vec2(radius * 6.0 + gap * 2.0, radius * 2.0 + lift),
        Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    // Every frame, paced by vsync like egui's own animations; drawn only
    // while visible, and a hidden window gets no frames at all. Stepped
    // while a screen reader is attached: see `motion`.
    crate::motion::request_frame(ui.ctx());
    let time = ui.input(|input| input.time);
    for index in 0..3 {
        let phase = (time * std::f64::consts::TAU / 1.05 - f64::from(index) * 0.9).sin() as f32;
        let rise = 0.5 + 0.5 * phase;
        let center = pos2(
            rect.left() + radius + (radius * 2.0 + gap) * index as f32,
            rect.bottom() - radius - rise * lift,
        );
        ui.painter().circle_filled(
            center,
            radius,
            palette.secondary.gamma_multiply(0.45 + 0.55 * rise),
        );
    }
}

const REACTION_AFFORDANCE_SIZE: f32 = 26.0;
const REACTION_AFFORDANCE_GAP: f32 = 6.0;

/// Places the hover reaction control beside the bubble, swapping sides near an edge.
pub fn reaction_affordance_rect(bubble: Rect, bounds: Rect, own: bool) -> Rect {
    let size = Vec2::splat(REACTION_AFFORDANCE_SIZE);
    let before = bubble.left() - REACTION_AFFORDANCE_GAP - size.x;
    let after = bubble.right() + REACTION_AFFORDANCE_GAP;
    let (preferred, fallback) = if own {
        (before, after)
    } else {
        (after, before)
    };
    let fits = |x: f32| x >= bounds.left() && x + size.x <= bounds.right();
    let x = if fits(preferred) {
        preferred
    } else if fits(fallback) {
        fallback
    } else {
        preferred.clamp(bounds.left(), (bounds.right() - size.x).max(bounds.left()))
    };
    // Centred on the whole bubble, quote and footer included.
    let y = (bubble.center().y - size.y / 2.0)
        .clamp(bounds.top(), (bounds.bottom() - size.y).max(bounds.top()));
    Rect::from_min_size(pos2(x, y), size)
}

/// Includes the small gap so moving from the bubble to the control does not hide it.
fn reaction_affordance_visible(pointer: Option<egui::Pos2>, bubble: Rect, button: Rect) -> bool {
    pointer.is_some_and(|pointer| bubble.union(button).contains(pointer))
}

fn open_reaction_picker_action(chat: &str, message: &str) -> Action {
    Action::OpenReactionPicker {
        chat: chat.to_owned(),
        message: message.to_owned(),
        beside_menu: false,
    }
}

/// The screen-reader label names whose message the hover control reacts to, so
/// a user can tell the controls apart. The visual tooltip stays a short "React".
fn reaction_button_label(from_me: bool, sender: &str) -> String {
    if from_me {
        "React to your message".to_owned()
    } else {
        format!("React to {sender}'s message")
    }
}

/// Draws a Smile control beside a hovered message and opens the existing picker.
fn reaction_affordance(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    bubble: &egui::Response,
    actions: &mut Vec<Action>,
) {
    let bounds = ui.clip_rect().shrink(2.0);
    if bounds.width() < REACTION_AFFORDANCE_SIZE || bounds.height() < REACTION_AFFORDANCE_SIZE {
        return;
    }
    // Only the part of the bubble on screen can be hovered. Anchoring to it
    // keeps a bubble scrolled mostly out of view from parking its controls,
    // and catching the pointer, at the edge of the transcript.
    let shown = bubble.rect.intersect(bounds);
    if shown.height() < REACTION_AFFORDANCE_SIZE || shown.width() <= 0.0 {
        return;
    }
    // A call in the chat is a record, as in WhatsApp: no reactions, no replies.
    if matches!(message.content, Content::Call { .. }) {
        return;
    }
    let rect = reaction_affordance_rect(shown, bounds, message.from_me);
    let pointer = ui.input(|input| input.pointer.interact_pos());
    // Stay hidden under any floating layer, as the context menu does. The
    // affordance sits beside the bubble, so test the bubble itself rather than
    // the pointer, which may already be over the button, outside the bubble's
    // layer.
    let uncovered = ui
        .ctx()
        .layer_id_at(bubble.rect.center())
        .is_none_or(|layer| layer == bubble.layer_id);

    // Publish where the control actually landed, the same way the bubble rect is
    // published, so tests can act on the real rect instead of guessing it.
    ui.ctx()
        .data_mut(|data| data.insert_temp(bubble.id.with("react-rect"), rect));

    // Acquire under the same layer as the bubble so the row strip keeps clicks.
    let response = ui.interact(rect, bubble.id.with("react"), Sense::click());
    let sender = (view.names_or)(&message.sender, message.sender_name.as_deref());
    let label = reaction_button_label(message.from_me, &sender);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
    });
    theme::reveal_focus(&response);
    // Reply sits one step further out, where the chat accepts messages.
    let can_reply = view.chat.can_send()
        && !matches!(
            message.content,
            Content::Revoked | Content::PhoneOnly { .. }
        );
    let step = REACTION_AFFORDANCE_SIZE + 4.0;
    let outward = if rect.center().x < bubble.rect.center().x {
        -step
    } else {
        step
    };
    let reply_rect = rect.translate(vec2(outward, 0.0));
    let reply_fits = can_reply && bounds.contains_rect(reply_rect);
    let reach = if reply_fits {
        rect.union(reply_rect)
    } else {
        rect
    };
    // Not while the row slides for a swipe.
    let swiping = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(bubble_id(&view.chat.id, &message.id).with("swiping")));
    let swiping = swiping.unwrap_or(false);
    let revealed = !swiping && reaction_affordance_visible(pointer, shown, reach);
    // The buttons ease in and out with the pointer, so they do not snap back
    // beside a row that just settled from a swipe; a swipe starting takes
    // them at once, before they ride along. They only paint: nothing about
    // them takes room, so the transcript never moves for them.
    let fade = theme::animate(
        ui.ctx(),
        bubble.id.with("affordance"),
        uncovered && revealed,
        if swiping { 0.0 } else { theme::MOTION_QUICK },
    );
    let opacity = |focused: bool| if focused { 1.0 } else { fade };
    if ui.is_rect_visible(rect) && opacity(response.has_focus()) > 0.0 {
        let opacity = opacity(response.has_focus());
        floating_circle(ui, &view.palette, rect, response.hovered(), opacity);
        theme::paint_icon(
            ui,
            Icon::Smile,
            rect.shrink(5.0),
            15.0,
            if response.hovered() {
                view.palette.text
            } else {
                view.palette.secondary
            }
            .gamma_multiply(opacity),
        );
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("React")
        .clicked()
    {
        actions.push(open_reaction_picker_action(&view.chat.id, &message.id));
    }
    if !reply_fits {
        return;
    }
    let reply = ui.interact(reply_rect, bubble.id.with("reply"), Sense::click());
    reply.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Reply")
    });
    theme::reveal_focus(&reply);
    if ui.is_rect_visible(reply_rect) && opacity(reply.has_focus()) > 0.0 {
        let opacity = opacity(reply.has_focus());
        floating_circle(ui, &view.palette, reply_rect, reply.hovered(), opacity);
        theme::paint_icon(
            ui,
            Icon::Reply,
            reply_rect.shrink(5.0),
            15.0,
            if reply.hovered() {
                view.palette.text
            } else {
                view.palette.secondary
            }
            .gamma_multiply(opacity),
        );
    }
    if reply
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Reply")
        .clicked()
    {
        actions.push(Action::Reply(message.id.clone()));
    }
}

/// Where a message sits in its sender's run: whether it names the sender,
/// shows their picture, and carries the bubble's tail.
#[derive(Clone, Copy)]
struct RunPlace {
    show_sender: bool,
    show_avatar: bool,
    tailed: bool,
    first_in_run: bool,
    last_in_run: bool,
    last_outgoing: bool,
}

fn send_flight_id() -> egui::Id {
    egui::Id::new("send-flight")
}

/// A sent text on its way from the composer to its bubble: `from` is where
/// its text sat in the composer's field, `field` and `ink` the field's fill
/// and text colour, and `message` the row it became, with the time that row
/// was first drawn.
#[derive(Clone)]
struct SendFlight {
    chat: String,
    from: Rect,
    field: Color32,
    ink: Color32,
    sent: f64,
    /// Unix seconds at the send, to tell its message from older fresh ones.
    wall: i64,
    message: Option<(String, f64)>,
}

/// The message a sent text is flying to, once its row has been drawn.
#[cfg(test)]
pub(crate) fn flying_to(ctx: &egui::Context) -> Option<String> {
    ctx.data(|data| data.get_temp::<Vec<SendFlight>>(send_flight_id()))
        .and_then(|flights| flights.into_iter().find_map(|flight| flight.message))
        .map(|(id, _)| id)
}

/// How far a bubble's scrim reaches past its edge, and fades over.
const SCRIM_REACH: f32 = 24.0;

/// Moves the bubbles' scrims at `scrims` into `slot`, which was reserved
/// before the first row, so each wash lies under every bubble instead of
/// over the bottom of the one above it. Arrival and send animations have
/// already moved or faded each scrim with its bubble by now, and a scrim
/// lifted onto another layer left an empty shape behind.
fn lay_scrims_under(
    ctx: &egui::Context,
    layer: egui::LayerId,
    slot: egui::layers::ShapeIdx,
    scrims: &[(egui::LayerId, egui::layers::ShapeIdx)],
) {
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        let mut shapes = Vec::new();
        for &(_, index) in scrims.iter().filter(|(on, _)| *on == layer) {
            list.mutate_shape(index, |clipped| {
                shapes.push(std::mem::replace(&mut clipped.shape, egui::Shape::Noop));
            });
        }
        list.mutate_shape(slot, |clipped| clipped.shape = egui::Shape::Vec(shapes));
    });
}

/// A soft, wide wash behind a bubble, so doodles fade out before they reach
/// its text at any intensity (#29): the wallpaper's colour leaning towards
/// the bubble's `fill`, so it reads as the bubble's shadow, not a glow.
/// Stronger doodles get a stronger wash; none, none.
fn doodle_scrim(
    bubble: Rect,
    fill: Color32,
    doodles: Option<(f32, Color32)>,
) -> Option<egui::Shape> {
    let (intensity, wall) = doodles.filter(|(intensity, _)| *intensity > 0.0)?;
    let strength = (0.45 + 0.15 * intensity).clamp(0.45, 0.8);
    let [r, g, b, _] = wall.lerp_to_gamma(fill, 0.12).to_srgba_unmultiplied();
    let wash = Color32::from_rgba_unmultiplied(r, g, b, (strength * 255.0).round() as u8);
    Some(egui::Shape::Rect(
        egui::epaint::RectShape::filled(
            bubble.expand(SCRIM_REACH / 2.0),
            CornerRadius::same(theme::RADIUS_CARD),
            wash,
        )
        .with_blur_width(SCRIM_REACH),
    ))
}

/// How old a message may be when first drawn and still be animated in: one
/// just sent or received, not history or a chat opened later.
const ARRIVAL_FRESH: i64 = 3;
/// How long ago the typing bubble may have been drawn for a received
/// message to grow out of it: the frame before, with room for a slow one.
const TYPING_HANDOFF: f64 = 0.2;

// iMessage's send and receive, as observed: Apple publishes no values.

/// A sent bubble's way up from the composer, and the list's as it makes
/// room for any new last message: a spring that passes its place a little.
const SEND_RISE: crate::motion::Spring = crate::motion::Spring::new(0.5, 0.2);
/// A sent bubble's way across to its column: critically damped, so it never
/// swings past it.
const SEND_ACROSS: crate::motion::Spring = crate::motion::Spring::new(0.35, 0.0);
/// How long a sent bubble takes to turn from the composer field's colours
/// to its own.
const SEND_TINT_SECONDS: f64 = 0.18;
/// A received bubble pops out of its tail corner on this spring, from
/// [`POP_FROM_SCALE`] of its size; so does the typing bubble.
const POP: crate::motion::Spring = crate::motion::Spring::new(0.45, 0.25);
const POP_FROM_SCALE: f32 = 0.6;
/// How long a popping bubble takes to become opaque: quickly, so it is seen
/// growing rather than fading.
const POP_FADE_SECONDS: f64 = 0.1;
/// How much the typing bubble swells at the top of a breath, and how long
/// one breath takes.
const BREATH_SCALE: f32 = 0.03;
const BREATH_SECONDS: f64 = 1.8;

/// Whether a sent bubble has reached its place, both ways, `elapsed`
/// seconds after its row was first drawn.
fn send_settled(elapsed: f64) -> bool {
    SEND_RISE.settled(elapsed) && SEND_ACROSS.settled(elapsed)
}

/// The size of a popping bubble `elapsed` seconds in: from
/// [`POP_FROM_SCALE`] to a hair past 1, settling on 1.
fn pop_scale(elapsed: f64) -> f32 {
    POP_FROM_SCALE + (1.0 - POP_FROM_SCALE) * POP.value(elapsed)
}

/// The opacity of a popping bubble `elapsed` seconds in.
fn pop_opacity(elapsed: f64) -> f32 {
    theme::ease_out((elapsed / POP_FADE_SECONDS) as f32)
}

/// The typing bubble's size `elapsed` seconds after it appeared: a slow
/// breath from 1 to `1 + BREATH_SCALE` and back.
fn breath_scale(elapsed: f64) -> f32 {
    let phase = (elapsed.max(0.0) * std::f64::consts::TAU / BREATH_SECONDS).cos() as f32;
    1.0 + BREATH_SCALE * 0.5 * (1.0 - phase)
}

/// Scales about `pivot`.
fn scale_about(pivot: egui::Pos2, scale: f32) -> egui::emath::TSTransform {
    egui::emath::TSTransform::new(pivot.to_vec2() * (1.0 - scale), scale)
}

/// How long the glide to a jumped-to message takes, in seconds.
const JUMP_SCROLL_SECONDS: f32 = 0.35;

/// Plans a jump of `distance` points (positive scrolls down) in a view
/// `viewport` points tall: a target more than two views away is first reached
/// instantly up to one view short of it, and the rest glides. Returns the
/// instant part and the gliding part.
fn jump_plan(distance: f32, viewport: f32) -> (f32, f32) {
    if distance.abs() > viewport * 2.0 {
        let instant = distance - distance.signum() * viewport;
        (instant, distance - instant)
    } else {
        (0.0, distance)
    }
}

/// Strong ease-out, `cubic-bezier(0.23, 1, 0.32, 1)`: a jump's glide to a
/// quoted message.
fn ease_out_strong(t: f32) -> f32 {
    cubic_bezier(t, 0.23, 1.0, 0.32, 1.0)
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at time `t`: Newton's method finds the
/// curve parameter whose x is `t`, and its y is the eased value.
fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let mut u = t;
    for _ in 0..8 {
        let x = bezier_sample(u, x1, x2);
        let dx = bezier_slope(u, x1, x2);
        if dx.abs() < 1e-6 {
            break;
        }
        u = (u - (x - t) / dx).clamp(0.0, 1.0);
    }
    bezier_sample(u, y1, y2)
}

fn bezier_sample(t: f32, a: f32, b: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
}

fn bezier_slope(t: f32, a: f32, b: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * a + 6.0 * u * t * (b - a) + 3.0 * t * t * (1.0 - b)
}

/// Stiffness and damping for Motion's `{ duration, bounce }` spring. `duration`
/// is milliseconds. Velocity is 0 and mass is 1, as in that config.
fn spring_from_duration(duration_ms: f64, bounce: f64) -> (f64, f64) {
    const SAFE_MIN: f64 = 0.001;
    const ROOT_STEPS: i32 = 12;
    let mass = 1.0;
    let velocity = 0.0;
    let damping_ratio = (1.0 - bounce).clamp(0.05, 1.0);
    let duration = (duration_ms / 1000.0).clamp(0.01, 10.0);
    let angular = |undamped: f64| undamped * (1.0 - damping_ratio * damping_ratio).sqrt();
    let envelope = |undamped: f64| {
        let decay = undamped * damping_ratio;
        let delta = decay * duration;
        let a = decay - velocity;
        let b = angular(undamped);
        SAFE_MIN - (a / b) * (-delta).exp()
    };
    let derivative = |undamped: f64| {
        let decay = undamped * damping_ratio;
        let delta = decay * duration;
        let d = delta * velocity + velocity;
        let e = damping_ratio.powi(2) * undamped.powi(2) * duration;
        let f = (-delta).exp();
        let g = angular(undamped.powi(2));
        let factor = if -envelope(undamped) + SAFE_MIN > 0.0 {
            -1.0
        } else {
            1.0
        };
        (factor * ((d - e) * f)) / g
    };
    let mut undamped = 5.0 / duration;
    for _ in 1..ROOT_STEPS {
        let slope = derivative(undamped);
        if slope.abs() < 1e-9 {
            break;
        }
        undamped -= envelope(undamped) / slope;
    }
    let stiffness = undamped * undamped * mass;
    let damping = damping_ratio * 2.0 * (mass * stiffness).sqrt();
    (stiffness, damping)
}

/// Underdamped spring from 0 to 1 at `t_ms`, the form Motion samples.
fn spring_unit(t_ms: f64, stiffness: f64, damping: f64) -> f64 {
    let mass = 1.0;
    let damping_ratio = damping / (2.0 * (stiffness * mass).sqrt());
    let undamped = (stiffness / mass).sqrt() / 1000.0;
    let angular = undamped * (1.0 - damping_ratio * damping_ratio).sqrt();
    let initial_delta = 1.0;
    let envelope = (-damping_ratio * undamped * t_ms).exp();
    1.0 - envelope
        * ((damping_ratio * undamped * initial_delta) / angular * (angular * t_ms).sin()
            + initial_delta * (angular * t_ms).cos())
}

/// Draws a message row and returns its bubble response for scrolling. A
/// bubble whose card is being carried to another chat stays behind faintly,
/// and a press on one may pick it up (`carry`).
fn bubble(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: Option<&[Message]>,
    place: RunPlace,
    reduce_motion: bool,
    actions: &mut Vec<Action>,
) -> Option<egui::Response> {
    let layer = ui.layer_id();
    let start = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    // Where the row starts, for its place in the list's wave.
    if let Some(rows) = view.rows {
        rows.borrow_mut().push(ui.cursor().top());
    }
    let response = match super::carry::ghost(ui.ctx(), &view.chat.id, &message.id) {
        Some(opacity) => {
            ui.scope(|ui| {
                ui.multiply_opacity(opacity);
                bubble_arriving(ui, view, message, album, place, reduce_motion, actions)
            })
            .inner
        }
        None => bubble_arriving(ui, view, message, album, place, reduce_motion, actions),
    };
    let rect = ui
        .ctx()
        .data(|data| data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("rect")));
    if let Some(rect) = rect
        && !view.selecting
        && !matches!(message.content, Content::Revoked | Content::Call { .. })
    {
        swipe_to_reply(ui, view, message, start, rect, actions);
    }
    if (!view.selecting || view.selected.contains(&message.id))
        && let Some(rect) = rect
    {
        let fill = if message.from_me {
            view.palette.bubble_out
        } else {
            view.palette.bubble_in
        };
        super::carry::watch(
            ui,
            &view.chat.id,
            album.unwrap_or(std::slice::from_ref(message)),
            view.selected,
            start,
            rect,
            fill,
        );
    }
    response
}

/// Two fingers swiped across a message the way Settings give a reply: the
/// row follows them with a rubber band while a reply icon grows in the space
/// it leaves, and letting go past the threshold starts the reply. Short of
/// it the row eases back.
fn swipe_to_reply(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    start: egui::layers::ShapeIdx,
    rect: Rect,
    actions: &mut Vec<Action>,
) {
    use crate::settings::{SwipeAction, SwipeSlot};
    let ctx = ui.ctx().clone();
    let clip = ui.clip_rect();
    let area = Rect::from_x_y_ranges(clip.x_range(), rect.expand2(vec2(0.0, 4.0)).y_range());
    let pull = super::swipe::over(&ctx, area);
    let reply = |right| {
        (view.swipes.get(SwipeSlot::of(true, right)) == SwipeAction::Reply && view.chat.can_send())
            .then_some(Icon::Reply)
    };
    if let Some(pull) = pull
        && pull.fired
        && reply(pull.right).is_some()
    {
        actions.push(Action::Reply(message.id.clone()));
    }
    let lane = Rect::from_x_y_ranges(
        (rect.left() - 12.0).max(clip.left())..=(rect.right() + 12.0).min(clip.right()),
        rect.y_range(),
    );
    let id = bubble_id(&view.chat.id, &message.id);
    let pulled = super::swipe::slide_row(
        ui,
        id.with("swipe"),
        start,
        lane,
        pull,
        reply,
        &view.palette,
    );
    // The hover buttons would ride along into the badge's space.
    ctx.data_mut(|data| data.insert_temp(id.with("swiping"), pulled > 0.0));
}

/// A sideways swipe across the open chat whose action is on the whole chat
/// (a reply is the message rows'): a badge slides in at the side the
/// fingers come from, and letting go past the threshold acts.
fn swipe_chat(app: &mut App, ui: &egui::Ui, chat: &Chat, area: Rect, actions: &mut Vec<Action>) {
    use crate::settings::SwipeAction;
    let pull = super::swipe::over(ui.ctx(), area);
    if let Some(pull) = pull.filter(|pull| pull.fired) {
        let action = super::swipe::action(app, true, pull.right);
        if action != SwipeAction::Reply {
            actions.extend(super::swipe::actions(app, action, chat, None));
        }
    }
    let whole = |right| {
        let action = super::swipe::action(app, true, right);
        (action != SwipeAction::Reply && super::swipe::available(app, action, chat, false))
            .then(|| super::swipe::icon(action, chat))
    };
    let id = egui::Id::new(("swipe-open-chat", &chat.id));
    super::swipe::edge_badge(ui, id, area, pull, whole, &app.palette);
}

/// Draws a message row. A message just sent rises out of the composer into
/// its place, as iMessage's does; one just received grows out of the typing
/// bubble, or pops out of its tail corner, as does a send that left no
/// flight.
fn bubble_arriving(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: Option<&[Message]>,
    place: RunPlace,
    reduce_motion: bool,
    actions: &mut Vec<Action>,
) -> Option<egui::Response> {
    // Under a second, only on a send or a receipt: unlike the looping
    // animations `motion` paces, these need no screen-reader exception (and
    // on Windows any accessibility client, not only a reader, sets it).
    let fresh = view.animate && (crate::util::now() - message.timestamp).abs() <= ARRIVAL_FRESH;
    if !fresh {
        return bubble_row(ui, view, message, album, place, reduce_motion, actions);
    }
    let now = ui.input(|input| input.time);
    let layer = ui.layer_id();
    let start = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    if message.from_me {
        // Rows draw oldest first and flights queue oldest first, so each
        // unclaimed flight takes the next of our messages sent since it.
        let flight = ui.ctx().data_mut(|data| {
            let flights = data.get_temp_mut_or_default::<Vec<SendFlight>>(send_flight_id());
            let claimed = |flights: &[SendFlight]| {
                flights
                    .iter()
                    .position(|f| f.message.as_ref().is_some_and(|(id, _)| *id == message.id))
            };
            if claimed(flights).is_none()
                && let Some(free) = flights.iter_mut().find(|f| {
                    f.chat == view.chat.id
                        && f.message.is_none()
                        && now - f.sent < 2.0
                        && message.timestamp >= f.wall - 1
                })
            {
                free.message = Some((message.id.clone(), now));
            }
            claimed(flights).map(|index| flights[index].clone())
        });
        if let Some((flight, landed)) = flight.and_then(|flight| {
            let landed = flight.message.as_ref()?.1;
            Some((flight, landed))
        }) {
            let response = bubble_row(ui, view, message, album, place, reduce_motion, actions);
            let elapsed = now - landed;
            if send_settled(elapsed) {
                return response;
            }
            let Some(rect) = ui.ctx().data(|data| {
                data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("rect"))
            }) else {
                return response;
            };
            // iMessage's send: the bubble keeps its own size and leaves with
            // its text where the composer's text sat, then rises on a spring
            // that passes its place a little while it slides across on a
            // critically damped one.
            let margin = bubble_margin(&view.palette, message, false);
            let leave = vec2(
                flight.from.left() - (rect.left() + f32::from(margin.left)),
                flight.from.bottom() - (rect.bottom() - f32::from(margin.bottom)),
            );
            let across = SEND_ACROSS.value(elapsed);
            let up = SEND_RISE.value(elapsed);
            // The list rises to meet a new last message; aim where the row is
            // drawn now, not where it ends, or the bubble passes it.
            let gliding = ui
                .ctx()
                .data(|data| data.get_temp::<Glide>(Glide::id(&view.chat.id)))
                .map_or(0.0, |glide| glide.shift(now, 0));
            let offset = vec2(
                leave.x * (1.0 - across),
                gliding + (leave.y - gliding) * (1.0 - up),
            );
            let transform = egui::emath::TSTransform::from_translation(offset);
            // It leaves in the field's colours and turns into its own over
            // the first moments: the field's version lies over it, fading.
            let veil = 1.0 - theme::ease_out((elapsed / SEND_TINT_SECONDS) as f32);
            let own_text = view.palette.on_bubble(true).text;
            // Lifted onto a layer above the chat, so it can cross the composer
            // and the list's edge on its way, and the list's own glide leaves it be.
            let flying = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("send-flight"));
            let clip = ui.ctx().content_rect();
            ui.ctx().graphics_mut(|graphics| {
                let list = graphics.entry(layer);
                let end = list.next_idx();
                let shapes: Vec<egui::Shape> = list
                    .all_entries()
                    .skip(start.0)
                    .take(end.0 - start.0)
                    .map(|clipped| clipped.shape.clone())
                    .collect();
                for index in start.0..end.0 {
                    list.reset_shape(egui::layers::ShapeIdx(index));
                }
                let veiled = if veil > 0.0 {
                    field_veil(&shapes, rect, (flight.field, flight.ink), own_text, veil)
                } else {
                    Vec::new()
                };
                let above = graphics.entry(flying);
                for mut shape in shapes.into_iter().chain(veiled) {
                    shape.transform(transform);
                    above.add(clip, shape);
                }
            });
            ui.ctx().request_repaint();
            return response;
        }
    }
    // Received, or a send that left no flight. The first time it is drawn
    // starts its spring, unless it was already there when the chat itself
    // first appeared.
    let shown = ui
        .ctx()
        .data_mut(|data| *data.get_temp_mut_or(chat_shown_id(&view.chat.id), now));
    let id = egui::Id::new(("landing", &view.chat.id, &message.id));
    // A received message whose typing bubble was drawn a frame ago grows out
    // of that bubble. Taken once, when it lands: the typing bubble is gone
    // after that.
    let typing = (!message.from_me)
        .then(|| {
            ui.ctx()
                .data(|data| data.get_temp::<TypingSeen>(typing_bubble_id(&view.chat.id)))
        })
        .flatten()
        .filter(|seen| now - seen.drawn <= TYPING_HANDOFF)
        .map(|seen| seen.rect);
    let (landed, typing) = ui
        .ctx()
        .data_mut(|data| *data.get_temp_mut_or_insert_with(id, || (now, typing)));
    let elapsed = now - landed;
    if POP.settled(elapsed) || landed <= shown {
        return bubble_row(ui, view, message, album, place, reduce_motion, actions);
    }
    let row = ui.scope(|ui| {
        if typing.is_none() {
            ui.multiply_opacity(pop_opacity(elapsed));
        }
        bubble_row(ui, view, message, album, place, reduce_motion, actions)
    });
    // From the tail's corner of the bubble itself, not of the row around it
    // with its picture and reactions.
    let bubble = ui
        .ctx()
        .data(|data| data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("rect")))
        .unwrap_or(row.response.rect);
    let pivot = if message.from_me {
        bubble.right_bottom()
    } else {
        bubble.left_bottom()
    };
    let transform = if let Some(typing) = typing {
        // Out of the typing bubble: from its bottom left and its height,
        // fully opaque, springing to the row's own place and size.
        let p = POP.value(elapsed);
        let from_scale = (typing.height() / row.response.rect.height().max(1.0)).clamp(0.5, 1.0);
        let scale = from_scale + (1.0 - from_scale) * p;
        let offset = (typing.left_bottom() - pivot) * (1.0 - p);
        egui::emath::TSTransform::from_translation(offset) * scale_about(pivot, scale)
    } else {
        // iMessage's receive: it pops out of its tail corner, from a little
        // over half its size to a hair past its own, and settles.
        scale_about(pivot, pop_scale(elapsed))
    };
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        let end = list.next_idx();
        for index in start.0..end.0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                clipped.shape.transform(transform);
            });
        }
    });
    ui.ctx().request_repaint();
    row.inner
}

/// When the open chat's transcript was first drawn: messages already there
/// then appear in place.
fn chat_shown_id(chat: &str) -> egui::Id {
    egui::Id::new(("chat-shown", chat))
}

/// The composer field's version of a sent bubble, to lay over the bubble
/// at `opacity` as it leaves: its backdrop in the field's `fill`, and its
/// body text, in the bubble's own `text` colour, in the field's `ink`.
/// Fading it out turns the field into the bubble. The time, ticks and
/// scrim are left to the bubble beneath.
fn field_veil(
    shapes: &[egui::Shape],
    bubble: Rect,
    (fill, ink): (Color32, Color32),
    text: Color32,
    opacity: f32,
) -> Vec<egui::Shape> {
    // The backdrop is the first shape the size of the bubble: its tail may
    // reach a few points past it, a scrim or shadow further, blurred.
    let near = bubble.expand(8.0);
    let core = bubble.shrink(1.0);
    let mut backdrop = false;
    let mut veil = Vec::new();
    for shape in shapes {
        let mut shape = shape.clone();
        match &shape {
            // The body, not the time beside it.
            egui::Shape::Text(body)
                if body.fallback_color == text
                    || body.override_text_color == Some(text)
                    || body
                        .galley
                        .job
                        .sections
                        .iter()
                        .any(|section| section.format.color == text) =>
            {
                egui::epaint::shape_transform::adjust_colors(&mut shape, move |colour| {
                    if *colour == text {
                        *colour = ink;
                    }
                });
            }
            // Emoji are pictures painted over the text, the same in both.
            egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {}
            _ if !backdrop && !blurred(&shape) && {
                let bounds = shape.visual_bounding_rect();
                near.contains_rect(bounds) && bounds.contains_rect(core)
            } =>
            {
                backdrop = true;
                egui::epaint::shape_transform::adjust_colors(&mut shape, move |colour| {
                    if *colour != Color32::PLACEHOLDER && colour.a() > 0 {
                        *colour = fill.gamma_multiply(f32::from(colour.a()) / 255.0);
                    }
                });
            }
            _ => continue,
        }
        egui::epaint::shape_transform::adjust_colors(&mut shape, move |colour| {
            if *colour != Color32::PLACEHOLDER {
                *colour = colour.gamma_multiply(opacity);
            }
        });
        veil.push(shape);
    }
    veil
}

/// Whether a shape is a soft one, a shadow or scrim, rather than a solid.
fn blurred(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Rect(rect) => rect.blur_width > 0.0,
        egui::Shape::Vec(shapes) => shapes.iter().any(blurred),
        _ => false,
    }
}

/// Draws a message row and returns its bubble response for scrolling.
fn bubble_row(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: Option<&[Message]>,
    place: RunPlace,
    reduce_motion: bool,
    actions: &mut Vec<Action>,
) -> Option<egui::Response> {
    let actions_before = actions.len();
    let RunPlace {
        show_sender,
        show_avatar,
        tailed,
        first_in_run,
        last_in_run,
        last_outgoing,
    } = place;
    let own = message.from_me;
    // Greys that read on the panel can vanish on the bubble; use its own.
    let view = &View {
        palette: view.palette.on_bubble(own),
        ..*view
    };
    let with_avatar = !own && shows_sender_pictures(view.chat);
    let carousel = matches!(&message.content, Content::Interactive { card: Some(card), .. } if !card.carousel.is_empty());
    let share = if carousel {
        0.95
    } else if view.palette.bubbles == theme::BubbleStyle::Messages {
        MESSAGES_WIDTH_SHARE
    } else if view.palette.bubbles == theme::BubbleStyle::Nothing {
        0.70
    } else {
        WHATSAPP_WIDTH_SHARE
    };
    let max_width = ((ui.available_width() * share).min(if carousel { 920.0 } else { 560.0 })
        - if with_avatar {
            SENDER_AVATAR + 8.0
        } else {
            0.0
        })
    .max(0.0);
    // Register the empty strip beside the bubble from its previous rect, before
    // the row, so the avatar, the bubble, and the reactions win clicks, and the
    // text wins drags. A drag that starts here, off the text, sweeps messages
    // into a selection (#246).
    let id = bubble_id(&view.chat.id, &message.id);
    let previous = ui.ctx().data(|data| data.get_temp::<Rect>(id.with("rect")));
    if let Some(rect) = previous {
        let strip = Rect::from_x_y_ranges(ui.max_rect().x_range(), rect.y_range());
        let strip = ui.interact(strip, id.with("row"), Sense::click_and_drag());
        if strip.clicked() {
            actions.push(Action::FocusComposer);
        }
        // A drag from the bubble's own padding or media, under which this
        // strip runs too, picks the message up instead.
        if strip.drag_started() && !super::carry::lift_on_drag(ui.ctx(), &view.chat.id, &message.id)
        {
            actions.push(Action::SweepMessages {
                anchor: message.id.clone(),
                to: message.id.clone(),
            });
        }
        reply_on_double_click(&strip, view, message, actions);
    }
    let mut response = None;
    ui.with_layout(
        Layout::top_down(if own { Align::Max } else { Align::Min }),
        |ui| {
            if with_avatar {
                ui.horizontal_top(|ui| {
                    let (space, _) =
                        ui.allocate_exact_size(Vec2::splat(SENDER_AVATAR), Sense::hover());
                    // Keep bubble content vertically laid out inside the row.
                    ui.vertical(|ui| {
                        response = Some(bubble_frame(
                            ui,
                            view,
                            message,
                            album,
                            show_sender,
                            tailed,
                            first_in_run,
                            last_in_run,
                            max_width,
                            actions,
                        ));
                    });
                    if !show_avatar {
                        return;
                    }
                    // Messages and Nothing put the avatar beside the run's last
                    // message, level with its bottom; WhatsApp beside the
                    // first, at its top.
                    let rect = match &response {
                        Some(bubble)
                            if matches!(
                                view.palette.bubbles,
                                theme::BubbleStyle::Messages | theme::BubbleStyle::Nothing
                            ) =>
                        {
                            Rect::from_min_size(
                                pos2(
                                    space.left(),
                                    (bubble.rect.bottom() - SENDER_AVATAR).max(space.top()),
                                ),
                                Vec2::splat(SENDER_AVATAR),
                            )
                        }
                        _ => space,
                    };
                    let avatar = ui.interact(rect, id.with("avatar"), Sense::CLICK);
                    theme::focus_outline(ui, avatar.id, rect, SENDER_AVATAR / 2.0);
                    if avatar
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        actions.push(Action::OpenInfo(message.sender.clone()));
                    }
                    if ui.is_rect_visible(rect) {
                        let name = (view.names_or)(&message.sender, message.sender_name.as_deref());
                        widgets::paint_avatar(
                            ui,
                            &view.palette,
                            rect,
                            name.trim_start_matches('~'),
                            &message.sender,
                            view.avatars
                                .get(&message.sender)
                                .and_then(|picture| picture.as_deref()),
                        );
                    }
                });
            } else {
                response = Some(bubble_frame(
                    ui,
                    view,
                    message,
                    album,
                    show_sender,
                    tailed,
                    first_in_run,
                    last_in_run,
                    max_width,
                    actions,
                ));
            }
            // An album row carries every member's reactions, each set
            // reacting to its own item.
            let members = album.unwrap_or(std::slice::from_ref(message));
            if members.iter().any(|member| !member.reactions.is_empty()) {
                ui.add_space(-5.0);
                // Chips hang off the bubble's far side: the right of a
                // received message, the left of one we sent.
                let bubble_width = ui
                    .ctx()
                    .data(|data| data.get_temp::<Rect>(id.with("rect")))
                    .map_or(max_width, |rect| rect.width());
                let lead = if with_avatar {
                    SENDER_AVATAR + 8.0
                } else {
                    0.0
                };
                let layout = if own {
                    Layout::left_to_right(Align::Center)
                } else {
                    Layout::right_to_left(Align::Center)
                };
                ui.allocate_ui_with_layout(vec2(lead + bubble_width, 0.0), layout, |ui| {
                    // A right-to-left row lays its first item last.
                    let mut shown = members
                        .iter()
                        .filter(|member| !member.reactions.is_empty())
                        .collect::<Vec<_>>();
                    if !own {
                        shown.reverse();
                    }
                    for member in shown {
                        reactions(ui, view, member, actions);
                    }
                });
                ui.add_space(2.0);
            }
            // Members without a pill record that, so the first reaction
            // that arrives live pops like any later change.
            for member in members.iter().filter(|member| member.reactions.is_empty()) {
                note_no_reactions(ui, view, member);
            }
        },
    );
    // A picture without a caption already carries its time over its corner:
    // a second one under the bubble would say it twice.
    let time_on_picture = album.is_none() && time_over_picture(message);
    if !view.time_inside && !view.selecting && !time_on_picture {
        let bubble_rect = ui
            .ctx()
            .data(|data| data.get_temp::<Rect>(id.with("rect")))
            .or_else(|| response.as_ref().map(|response| response.rect));
        // A photo, video or link card opens itself: only the text and the
        // bubble around it show the time.
        let media = [id.with("picture"), id.with("preview"), id.with("quote")]
            .map(|key| ui.ctx().data(|data| data.get_temp::<Rect>(key)));
        let clicked_inside = bubble_rect.is_some_and(|rect| {
            ui.input(|input| {
                input.pointer.button_clicked(egui::PointerButton::Primary)
                    && input.pointer.interact_pos().is_some_and(|position| {
                        rect.contains(position)
                            && !media.iter().flatten().any(|media| media.contains(position))
                    })
            })
        });
        let metadata_id = id.with("metadata-visible");
        let default_visible = last_outgoing && message.from_me;
        let mut visible = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(metadata_id))
            .unwrap_or(default_visible);
        let on_media = ui.input(|input| {
            input.pointer.interact_pos().is_some_and(|position| {
                media.iter().flatten().any(|media| media.contains(position))
            })
        });
        // A click that opened something (a link, a quote's message, a file,
        // a sender) did what it was for; the time stays as it was.
        let opened = actions[actions_before..].iter().any(|action| {
            matches!(
                action,
                Action::OpenUrl(_)
                    | Action::ScrollTo(_)
                    | Action::OpenMessage { .. }
                    | Action::PreviewImage { .. }
                    | Action::PreviewSticker(_)
                    | Action::OpenFile(_)
                    | Action::OpenInfo(_)
                    | Action::PlayVideo { .. }
            )
        });
        let bubble_clicked = !opened
            && (clicked_inside
                || (!on_media && response.as_ref().is_some_and(egui::Response::clicked)));
        if bubble_clicked {
            visible = !visible;
            ui.ctx()
                .data_mut(|data| data.insert_temp(metadata_id, visible));
        }
        let animation_time = if reduce_motion {
            0.0
        } else {
            theme::MOTION_QUICK
        };
        let progress = ui.ctx().animate_bool_with_time_and_easing(
            metadata_id.with("reveal"),
            visible,
            animation_time,
            theme::ease_out,
        );
        if progress > 0.0 {
            let display_message = album.and_then(|members| members.last()).unwrap_or(message);
            if reference_timestamp(
                ui,
                view,
                display_message,
                id.with("metadata-row"),
                progress,
                bubble_rect,
            )
            .clicked()
                && !bubble_clicked
            {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(metadata_id, false));
            }
        }
    }
    response
}

/// The message time and delivery state sit just below a bubble when selected.
fn reference_timestamp(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    id: egui::Id,
    progress: f32,
    bubble: Option<Rect>,
) -> egui::Response {
    let own = message.from_me;
    const SPACING: f32 = 4.0;
    const ROW_HEIGHT: f32 = 16.0;
    let full_height = SPACING + ROW_HEIGHT;
    let reveal_height = full_height * progress.clamp(0.0, 1.0);
    let available = ui.available_rect_before_wrap();
    let slot_top = ui.cursor().top();
    let slot = Rect::from_min_size(
        pos2(available.left(), slot_top),
        vec2(available.width(), reveal_height),
    );
    ui.allocate_space(vec2(available.width(), reveal_height));

    // Keep the metadata at its final position and reveal it through a clip
    // that grows with the row. The transcript's ordinary layout then moves
    // subsequent messages in step, instead of jumping by the whole row.
    // Under the bubble, ending at its right edge on either side. A bubble
    // narrower than the time and ticks (a two-letter reply) lets the row reach
    // past its left edge instead of cutting the time short.
    let time = crate::util::clock(message.timestamp);
    let needed = reference_timestamp_width(ui, &time, own);
    let x = bubble.map_or(available.x_range(), |bubble| bubble.x_range());
    let x = widen_to(x, needed, available.x_range());
    let row_rect = Rect::from_x_y_ranges(x, slot_top + SPACING..=slot_top + SPACING + ROW_HEIGHT);
    let clip = Rect::from_min_max(slot.min, pos2(slot.right(), slot.top() + reveal_height))
        .intersect(ui.clip_rect());
    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(row_rect)
            .layout(Layout::top_down(Align::Max)),
    );
    row_ui.set_clip_rect(clip);
    row_ui.multiply_opacity(progress);
    row_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if own {
            match message.status {
                Delivery::Pending => {
                    widgets::rich_text(ui, "•••", theme::semibold(12.0), view.palette.secondary);
                }
                Delivery::Failed => {
                    widgets::rich_text(ui, "✗", theme::semibold(12.0), view.palette.danger);
                }
                status => {
                    let ticks = shown_delivery(view, message);
                    if matches!(
                        status,
                        Delivery::Sent | Delivery::Delivered | Delivery::Read | Delivery::Played
                    ) {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        tick_motion(ui, view, message, ticks, rect, view.palette.secondary);
                    }
                }
            }
            widgets::rich_text(
                ui,
                &time,
                theme::regular(10.0),
                view.palette.secondary.gamma_multiply(0.82),
            );
        } else {
            widgets::rich_text(
                ui,
                &time,
                theme::regular(10.0),
                view.palette.secondary.gamma_multiply(0.82),
            );
        }
    });
    ui.interact(row_rect.intersect(clip), id, Sense::click())
}

/// The room the time and delivery mark under a bubble take, with their gap.
fn reference_timestamp_width(ui: &egui::Ui, time: &str, own: bool) -> f32 {
    let time = ui
        .painter()
        .layout_no_wrap(time.to_owned(), theme::regular(10.0), Color32::WHITE)
        .size()
        .x;
    // The mark is 14 points of ticks, or "•••" / "✗" while sending or failed.
    time + if own { 14.0 + 4.0 } else { 0.0 } + 2.0
}

/// `span` grown to the left to be `needed` wide, or shifted right when that
/// would leave `bounds`. The right edge only moves for a span narrower than
/// `needed` that sits against the left edge of `bounds`.
fn widen_to(span: egui::Rangef, needed: f32, bounds: egui::Rangef) -> egui::Rangef {
    if span.span() >= needed {
        return span;
    }
    let left = (span.max - needed).max(bounds.min);
    egui::Rangef::new(left, (left + needed).max(span.max))
}

/// How long a delivery tick's change runs.
const TICK_SECONDS: f64 = 0.14;

/// Whether a status moved on since it was last drawn. No earlier status, or a
/// failure, is not an advance.
fn status_advanced(previous: Option<Delivery>, now: Delivery) -> bool {
    matches!(previous, Some(before) if before != now) && now != Delivery::Failed
}

/// Draws the delivery ticks, in `plain` for the states that are not read or
/// failed. A live advance scales the new glyph in about its centre with a
/// fade, and a change to read blends the colour.
fn tick_motion(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    shown: Delivery,
    rect: Rect,
    plain: Color32,
) {
    let now = ui.input(|input| input.time);
    let status_id = egui::Id::new(("tick-status", &view.chat.id, &message.id));
    let motion_id = egui::Id::new(("tick-motion", &view.chat.id, &message.id));
    let calm = crate::motion::screen_reader_attached(ui.ctx());
    // Only a status seen on the frame before counts as earlier: a bubble
    // scrolled back into view after a change shows its ticks as they are.
    let frame = ui.ctx().cumulative_frame_nr();
    let previous = seen_last_frame(
        ui.ctx()
            .data(|data| data.get_temp::<(u64, Delivery)>(status_id)),
        frame,
    );
    let advanced = status_advanced(previous, shown);
    ui.ctx().data_mut(|data| {
        data.insert_temp(status_id, (frame, shown));
        if advanced && !calm {
            data.insert_temp(motion_id, (now, previous.unwrap_or(shown)));
        }
    });
    let motion: Option<(f64, Delivery)> = ui.ctx().data(|data| data.get_temp(motion_id));
    let live = motion.filter(|(start, _)| now - start < TICK_SECONDS && !calm);
    let Some((start, before)) = live else {
        widgets::ticks_in(ui, &view.palette, rect, shown, plain);
        return;
    };
    let t = theme::ease_out(((now - start) / TICK_SECONDS) as f32);
    let layer = ui.layer_id();
    let first_shape = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    let same_glyph = matches!(
        before,
        Delivery::Delivered | Delivery::Read | Delivery::Played
    ) && matches!(
        shown,
        Delivery::Delivered | Delivery::Read | Delivery::Played
    );
    if same_glyph {
        // Only the colour moves, from the old state's to the new one's.
        let palette = &view.palette;
        let colour = |status: Delivery| match status {
            Delivery::Read | Delivery::Played => palette.read,
            _ => plain,
        };
        let blended = colour(before).lerp_to_gamma(colour(shown), t);
        widgets::ticks_in(ui, palette, rect, Delivery::Sent, blended);
    } else {
        let old_opacity = ui.opacity();
        ui.multiply_opacity(t);
        widgets::ticks_in(ui, &view.palette, rect, shown, plain);
        ui.set_opacity(old_opacity);
        let scale = 0.7 + 0.3 * t;
        let pivot = rect.center();
        let transform = egui::emath::TSTransform::from_translation(pivot.to_vec2())
            * egui::emath::TSTransform::from_scaling(scale)
            * egui::emath::TSTransform::from_translation(-pivot.to_vec2());
        ui.ctx().graphics_mut(|graphics| {
            let list = graphics.entry(layer);
            let end = list.next_idx();
            for index in first_shape.0..end.0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    clipped.shape.transform(transform);
                });
            }
        });
    }
    ui.ctx().request_repaint();
}

/// Clamps message-selection drags to the view while the pointer is outside it.
/// This keeps a row under the pointer during edge scrolling. The input hook
/// adjusts positions before egui processes them, using the previous frame's
/// view rectangle.
pub struct SelectionLeash {
    pub view: std::sync::Arc<std::sync::Mutex<Option<Rect>>>,
    holding: bool,
}

impl SelectionLeash {
    pub fn new(view: std::sync::Arc<std::sync::Mutex<Option<Rect>>>) -> Self {
        Self {
            view,
            holding: false,
        }
    }
}

impl egui::plugin::Plugin for SelectionLeash {
    fn debug_name(&self) -> &'static str {
        "zapfast-selection-leash"
    }

    fn input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        // A message carried to another chat, or out of the window, goes
        // wherever the pointer does; leaving the window does not drop it.
        if super::carry::lifted() {
            self.holding = false;
            input
                .events
                .retain(|event| !matches!(event, egui::Event::PointerGone));
            return;
        }
        let Some(view) = *self.view.lock().unwrap_or_else(|p| p.into_inner()) else {
            self.holding = false;
            return;
        };
        // The chat list's resize handle reaches into the view from its left
        // edge, as the scroll bar does on the right. Leashing a drag of the
        // handle pinned the pointer, and the list's edge, just inside the
        // view: stuck at its widest, or growing while dragged left (#239).
        let handle = ctx.global_style().interaction.resize_grab_radius_side;
        let inside = |pos: &egui::Pos2| {
            view.contains(*pos) && pos.x >= view.left() + handle && pos.x < view.right() - 16.0
        };
        let mut gone = Vec::new();
        for (index, event) in input.events.iter_mut().enumerate() {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } => {
                    if *pressed {
                        self.holding = inside(pos);
                    } else {
                        if self.holding && !view.contains(*pos) {
                            *pos = clamp_into(*pos, view);
                        }
                        self.holding = false;
                    }
                }
                egui::Event::PointerMoved(pos) if self.holding && !view.contains(*pos) => {
                    *pos = clamp_into(*pos, view);
                }
                // Ignore PointerGone during a drag so selection continues.
                egui::Event::PointerGone if self.holding => gone.push(index),
                _ => {}
            }
        }
        for index in gone.into_iter().rev() {
            input.events.remove(index);
        }
    }
}

fn clamp_into(pos: egui::Pos2, view: Rect) -> egui::Pos2 {
    egui::pos2(
        pos.x.clamp(view.left() + 2.0, view.right() - 18.0),
        pos.y.clamp(view.top() + 2.0, view.bottom() - 2.0),
    )
}

/// Builds the transcript row used when copying across messages.
fn transcript_row(
    view: &View<'_>,
    message: &Message,
    body: String,
    placements: Vec<String>,
) -> crate::transcript::Row {
    message_copy_row(message, body, placements, view.names_or, view.mention_names)
}

/// Copies whole selected messages, including rows outside the viewport, in
/// conversation order rather than the order in which they were selected.
pub(super) fn selected_text(app: &App) -> Option<String> {
    let (chat, selected) = app.selection.as_ref()?;
    if app.open_chat.as_ref() != Some(chat) {
        return None;
    }
    let conversation = app.conversations.get(chat)?;
    let selected: HashSet<&str> = selected.iter().map(String::as_str).collect();
    let names_or = |id: &str, hint: Option<&str>| app.display_name_or(id, hint);
    let mention_names = |id: &str| app.mention_name(id);
    let lines: Vec<String> = conversation
        .messages
        .iter()
        .filter(|message| selected.contains(message.id.as_str()))
        .map(|message| {
            let text = match &message.content {
                Content::Text { text, .. } | Content::Interactive { text, .. } => text.as_str(),
                Content::Image { caption, .. }
                | Content::Video { caption, .. }
                | Content::Document { caption, .. }
                | Content::StickerPack { caption, .. } => caption.as_deref().unwrap_or_default(),
                _ => "",
            };
            let mentions: Vec<markup::Mention> = message
                .mentions
                .iter()
                .map(|mention| markup::Mention {
                    user: mention.user.clone(),
                    name: mention_names(&mention.id),
                })
                .collect();
            let body = markup::plain(text, &mentions);
            let mut row = message_copy_row(message, body, Vec::new(), &names_or, &mention_names);
            // Content without a transcript marker still needs a useful label.
            if row.body.is_empty() && row.marker.is_none() {
                row.body = message.content.full_summary();
            }
            row.line(&row.body)
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Builds shared transcript metadata for drawn text and whole-message exports.
fn message_copy_row(
    message: &Message,
    body: String,
    placements: Vec<String>,
    names_or: &dyn Fn(&str, Option<&str>) -> String,
    mention_names: &dyn Fn(&str) -> String,
) -> crate::transcript::Row {
    let who = if message.from_me {
        mention_names(&message.sender)
    } else {
        names_or(&message.sender, message.sender_name.as_deref())
    };
    let marker = match &message.content {
        Content::Image { .. } => Some("[photo]".to_owned()),
        Content::Video { gif: true, .. } => Some("[GIF]".to_owned()),
        Content::Video { .. } => Some("[video]".to_owned()),
        Content::Audio {
            voice_note: true,
            seconds,
            ..
        } => Some(match seconds {
            Some(seconds) => format!("[voice message, {}]", crate::util::duration(*seconds)),
            None => "[voice message]".to_owned(),
        }),
        Content::Audio { .. } => Some("[audio]".to_owned()),
        Content::Document { file_name, .. } => Some(format!("[document: {file_name}]")),
        Content::Sticker { .. } => Some("[sticker]".to_owned()),
        Content::Location { .. } => Some("[location]".to_owned()),
        Content::LiveLocation { ended, .. } => Some(
            if *ended {
                "[live location ended]"
            } else {
                "[live location]"
            }
            .to_owned(),
        ),
        Content::Contact { display_name, .. } => Some(format!("[contact: {display_name}]")),
        Content::StickerPack { name, .. } => Some(format!("[sticker pack: {name}]")),
        Content::Poll { question, .. } => Some(format!("[poll: {question}]")),
        Content::Interactive {
            card: Some(card), ..
        } if card.image.is_some() || card.carousel.iter().any(|card| card.image.is_some()) => {
            Some("[photo]".to_owned())
        }
        _ => None,
    };
    let reactions = if message.reactions.is_empty() {
        String::new()
    } else {
        let listed: Vec<String> = message
            .reactions
            .iter()
            .map(|reaction| format!("{} {}", reaction.emoji, names_or(&reaction.sender, None)))
            .collect();
        format!(" ({})", listed.join(", "))
    };
    let quote = message.quoted.as_ref().map(|quoted| {
        let name = quoted
            .sender_name
            .clone()
            .unwrap_or_else(|| names_or(&quoted.sender, None));
        let mentions: Vec<markup::Mention> = quoted
            .mentions
            .iter()
            .map(|mention| markup::Mention {
                user: mention.user.clone(),
                name: mention_names(&mention.id),
            })
            .collect();
        let summary = markup::plain(&quoted.summary, &mentions);
        let short: String = summary.chars().take(48).collect();
        let cut = if summary.chars().count() > 48 {
            "…"
        } else {
            ""
        };
        format!("(replying to {name}: \"{short}{cut}\")")
    });
    crate::transcript::Row {
        header: format!("[{}] {}: ", crate::util::copy_stamp(message.timestamp), who),
        body,
        placements,
        marker,
        reactions,
        quote,
    }
}

/// Selection-scroll distance based on pointer proximity to the view edge.
pub fn edge_scroll(pointer: f32, top: f32, bottom: f32) -> f32 {
    const EDGE: f32 = 36.0;
    const PACE: f32 = 0.3;
    if pointer < top + EDGE {
        -(top + EDGE - pointer).min(EDGE * 1.5) * PACE
    } else if pointer > bottom - EDGE {
        (pointer - (bottom - EDGE)).min(EDGE * 1.5) * PACE
    } else {
        0.0
    }
}

/// Whether a chat's header offers to call it: a person's chat calls them, a
/// group's starts a call in the group while group calls are on
/// (`calls::GROUP_CALLS_ENABLED`). Never the chat with ourselves, a channel or
/// a broadcast list.
pub(crate) fn callable(chat: &crate::model::Chat, me: Option<&str>) -> bool {
    match chat.kind {
        crate::model::ChatKind::Direct => me != Some(chat.id.as_str()),
        crate::model::ChatKind::Group => crate::calls::group_calls_enabled(),
        _ => false,
    }
}

/// A call's menu: calling back, in a chat that can take a call. A group's
/// call is called back in its group while group calls are on; a call among
/// several people with no group behind it cannot be started here.
fn call_menu(ui: &mut egui::Ui, view: &View<'_>, group: bool, actions: &mut Vec<Action>) {
    let palette = view.palette;
    let chat = &view.chat.id;
    let back = match view.chat.kind {
        crate::model::ChatKind::Group => crate::calls::group_calls_enabled(),
        crate::model::ChatKind::Direct => !group,
        _ => false,
    };
    if !back {
        ui.add_enabled_ui(false, |ui| {
            widgets::menu_item(
                ui,
                &palette,
                Some(Icon::Phone),
                &crate::i18n::gettext(view.locale, "Group calls happen on the phone"),
            );
        });
        return;
    }
    if widgets::menu_item(
        ui,
        &palette,
        Some(Icon::Phone),
        &crate::i18n::gettext(view.locale, "Voice call"),
    ) {
        actions.push(Action::StartCall(chat.clone()));
    }
    if widgets::menu_item(
        ui,
        &palette,
        Some(Icon::Video),
        &crate::i18n::gettext(view.locale, "Video call"),
    ) {
        actions.push(Action::StartVideoCall(chat.clone()));
    }
}

/// Starts a reply when the response was double-clicked, as the menu's "Reply".
fn reply_on_double_click(
    response: &egui::Response,
    view: &View<'_>,
    message: &Message,
    actions: &mut Vec<Action>,
) {
    if !response.double_clicked() {
        return;
    }
    if can_edit_inline(view, message) {
        start_inline_edit(&response.ctx, view, message);
    } else if !matches!(message.content, Content::Revoked | Content::Call { .. }) {
        actions.push(Action::Reply(message.id.clone()));
    }
}

/// Where the message being edited in its own bubble is kept: its id and the
/// text so far. The state belongs to the view, so nothing reaches the app
/// until the edit is saved.
fn inline_edit_slot(chat: &str) -> egui::Id {
    egui::Id::new(("inline-edit", chat))
}

/// Whether a double-click edits this message in its bubble: our own text,
/// inside the edit window. A mentioning message edits through the composer,
/// which keeps its mentions.
fn can_edit_inline(view: &View<'_>, message: &Message) -> bool {
    message.from_me
        && matches!(message.content, Content::Text { .. })
        && message.mentions.is_empty()
        && view.now - message.timestamp <= crate::app::EDIT_WINDOW.as_secs() as i64
}

/// Opens the message's text for editing in its bubble.
fn start_inline_edit(ctx: &egui::Context, view: &View<'_>, message: &Message) {
    let Content::Text { text, .. } = &message.content else {
        return;
    };
    // The body's size as it was drawn, so the bubble keeps it until the
    // text outgrows it.
    let body =
        ctx.data(|data| data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("body")));
    let slot = inline_edit_slot(&view.chat.id);
    ctx.data_mut(|data| {
        data.insert_temp(slot, (message.id.clone(), text.clone()));
        data.insert_temp(
            slot.with("size"),
            body.map_or(Vec2::ZERO, |body| body.size()),
        );
    });
    ctx.request_repaint();
}

/// How wide the editor of `text` is: as wide as the body was drawn, or the
/// text's own width once it outgrows that, with the time on its last line
/// when it fits there as it does beside the text. Never past `width`.
/// Returns the width and whether the time fits on the last line.
///
/// The rule is [`rich_body`]'s, so the first frame of an edit draws the
/// bubble at the size it had. No room is added for the caret: the field
/// paints it a point past its edge, inside the bubble's padding, and egui
/// wraps a line only once it is half a point wider than the field.
fn inline_editor_width(
    text_width: f32,
    last_row: f32,
    drawn: f32,
    width: f32,
    reserve: Option<f32>,
) -> (f32, bool) {
    let inline = reserve.filter(|reserve| last_row + 8.0 + reserve <= width);
    let needed = match inline {
        Some(reserve) => text_width.max(last_row + 8.0 + reserve),
        None => text_width,
    };
    (needed.max(drawn).min(width), inline.is_some())
}

/// A small label over the bubble being edited, so it is clear that typing
/// goes into it and not into the composer. It floats over the transcript,
/// so the bubble keeps its place and size.
fn editing_label(ui: &egui::Ui, view: &View<'_>, message: &Message, editor: egui::Id, at: Rect) {
    let ctx = ui.ctx();
    let id = editor.with("label");
    let shown = theme::animate(ctx, id, true, theme::MOTION_QUICK);
    // Where the bubble was last drawn, moved with the editor in case the
    // row has moved since.
    let bubble = ctx
        .data(|data| data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("rect")))
        .zip(ctx.data(|data| data.get_temp::<Rect>(editor.with("rect"))))
        .map_or(at, |(bubble, before)| bubble.translate(at.min - before.min));
    ctx.data_mut(|data| data.insert_temp(editor.with("rect"), at));
    let (anchor, pivot) = if message.from_me {
        (bubble.right_top(), egui::Align2::RIGHT_BOTTOM)
    } else {
        (bubble.left_top(), egui::Align2::LEFT_BOTTOM)
    };
    let label = crate::i18n::gettext(view.locale, "Editing message");
    let palette = view.palette;
    egui::Area::new(id)
        .order(egui::Order::Tooltip)
        .interactable(false)
        .fixed_pos(anchor + vec2(0.0, -6.0 + 4.0 * (1.0 - shown)))
        .pivot(pivot)
        .constrain_to(ui.clip_rect())
        .show(ctx, |ui| {
            ui.multiply_opacity(shown);
            widgets::menu_frame(&palette)
                .inner_margin(Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (icon, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        theme::paint_icon(ui, Icon::Pencil, icon, 12.0, palette.accent);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(label.as_ref())
                                    .font(theme::semibold(12.0))
                                    .color(palette.accent),
                            )
                            .selectable(false)
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                });
        });
}

/// How long the rest of the chat takes to recede behind a message edited in
/// its bubble, and to come back once the edit ends: a panel's time, and
/// quicker on the way back, as a surface closes.
const EDIT_FOCUS_IN: f32 = theme::MOTION_PANEL;
const EDIT_FOCUS_OUT: f32 = theme::MOTION_PANEL * theme::CLOSE;

/// The layers the chat recedes on while a message is edited in its bubble:
/// the scrim over the whole conversation, and the edited row, kept directly
/// above the scrim and below menus opened after it.
fn edit_focus_layers(chat: &str) -> (egui::LayerId, egui::LayerId) {
    let id = egui::Id::new(("edit-focus", chat));
    (
        egui::LayerId::new(egui::Order::Foreground, id),
        egui::LayerId::new(egui::Order::Foreground, id.with("row")),
    )
}

/// Where the open chat's transcript was laid out this frame, top to bottom.
/// Own balloons shade down it, and a lifted one, clipped to less, still does.
fn transcript_span_id(chat: &str) -> egui::Id {
    egui::Id::new(("transcript-span", chat))
}

/// The message edited in its bubble, and how far the rest of the chat has
/// receded behind it, from 0 to 1. The chat recedes while the editor has
/// the keyboard, or is about to take it, and comes back once the edit ends,
/// the bubble staying lifted until it has. Asked every frame the chat
/// shows, so the first edit fades in as later ones do; at once with a
/// screen reader.
fn edit_focus(ctx: &egui::Context, chat: &str) -> Option<(String, f32)> {
    let slot = inline_edit_slot(chat);
    let editing = ctx
        .data(|data| data.get_temp::<(String, String)>(slot))
        .map(|(id, _)| id)
        .filter(|id| {
            let editor = bubble_id(chat, id).with("inline-editor");
            ctx.memory(|memory| memory.has_focus(editor))
                || !ctx.data(|data| data.get_temp::<bool>(editor.with("seen")).unwrap_or(false))
        });
    let kept = slot.with("focus");
    let seconds = if editing.is_some() {
        EDIT_FOCUS_IN
    } else {
        EDIT_FOCUS_OUT
    };
    let shown = theme::animate(ctx, kept.with("shown"), editing.is_some(), seconds);
    match editing {
        Some(id) => {
            ctx.data_mut(|data| data.insert_temp(kept, id.clone()));
            Some((id, shown))
        }
        None if shown > 0.0 => ctx
            .data(|data| data.get_temp::<String>(kept))
            .map(|id| (id, shown)),
        None => {
            ctx.data_mut(|data| data.remove::<String>(kept));
            None
        }
    }
}

/// The scrim's colour: the palette's shadow at 22% in a light theme and 40%
/// in a dark one, where the bubble needs more to stand out.
fn edit_focus_shade(palette: &Palette) -> Color32 {
    let [r, g, b, _] = palette.shadow.to_srgba_unmultiplied();
    let weight: f32 = if palette.dark { 0.40 } else { 0.22 };
    Color32::from_rgba_unmultiplied(r, g, b, (weight * 255.0).round() as u8)
}

/// Recedes the chat behind a message edited in its bubble: a scrim over the
/// whole conversation, header and composer included, over a frost of it
/// where the window can frost (never on macOS, whose single Metal pass
/// cannot read back what it drew). The edited row is drawn on the layer
/// just above it, and the "Editing message" label above that. The scrim
/// holds no widgets, so clicks and scrolling reach what lies under it as
/// they did.
fn paint_edit_focus(ctx: &egui::Context, palette: &Palette, chat: &str, card: Rect, radius: f32) {
    let Some((_, shown)) = edit_focus(ctx, chat) else {
        return;
    };
    let (scrim, row) = edit_focus_layers(chat);
    ctx.set_sublayer(scrim, row);
    let painter = ctx.layer_painter(scrim).with_clip_rect(card);
    painter.add(super::glass::veil(card, radius, shown));
    painter.rect_filled(
        card,
        radius,
        edit_focus_shade(palette).gamma_multiply(shown),
    );
}

/// Draws the editor in place of the message's text when it is being edited.
/// Enter saves, Shift+Enter breaks the line, Escape or a click elsewhere
/// puts the old text back. The bubble keeps the size it had and grows only
/// as the text outgrows it, with the time beside the last line where it
/// was. Returns `None` when this message is not being edited, else the
/// footer's place on the last line, as [`rich_body`] does.
fn inline_editor(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    width: f32,
    reserve: Option<f32>,
    actions: &mut Vec<Action>,
) -> Option<Option<Rect>> {
    let slot = inline_edit_slot(&view.chat.id);
    let (id, mut text) = ui
        .ctx()
        .data(|data| data.get_temp::<(String, String)>(slot))?;
    if id != message.id {
        return None;
    }
    let drawn = ui
        .ctx()
        .data(|data| data.get_temp::<Vec2>(slot.with("size")))
        .unwrap_or(Vec2::ZERO);
    let palette = view.palette;
    let editor = bubble_id(&view.chat.id, &message.id).with("inline-editor");
    let focused = ui.memory(|memory| memory.has_focus(editor));
    let (save, cancel) = if focused {
        ui.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::Enter),
                input.consume_key(Modifiers::NONE, Key::Escape),
            )
        })
    } else {
        (false, false)
    };
    // Measured as it would wrap in the widest bubble, then drawn no wider
    // than that needs, so the bubble does not jump to its widest. The field
    // lays its text out the same way, so what was measured is what it draws.
    let font = theme::regular(BODY_SIZE);
    let lay_out = |ui: &egui::Ui, text: &str, wrap: f32| {
        ui.fonts_mut(|fonts| fonts.layout(text.to_owned(), font.clone(), palette.text, wrap))
    };
    let measured = lay_out(ui, &text, width);
    let last_row = measured.rows.last().map_or(0.0, |row| row.row.size.x);
    // The time stays beside the last line where the body put it there:
    // right-to-left text keeps it beside a single line only, and text under
    // a quote or a preview spans the card, as in `rich_body`.
    let card = message.quoted.is_some()
        || matches!(
            &message.content,
            Content::Text {
                preview: Some(_),
                ..
            }
        );
    let single = measured.rows.len() == 1 && !card;
    let reserve = reserve.filter(|_| !crate::bidi::message_rtl(&text) || single);
    let (fitted, inline) =
        inline_editor_width(measured.size().x, last_row, drawn.x, width, reserve);
    let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
        lay_out(ui, buffer.as_str(), wrap)
    };
    let output = egui::TextEdit::multiline(&mut text)
        .id(editor)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .font(theme::regular(BODY_SIZE))
        .text_color(palette.text)
        .desired_rows(1)
        .desired_width(fitted)
        // No shorter than the body was either, such as a big emoji-only
        // line, until the text needs more.
        .min_size(vec2(0.0, drawn.y))
        .layouter(&mut layouter)
        .show(ui);
    editing_label(ui, view, message, editor, output.response.rect);
    let first = !ui
        .ctx()
        .data(|data| data.get_temp::<bool>(editor.with("seen")).unwrap_or(false));
    if first {
        // Start with the caret after the text, and keep the field up to date
        // with the focus from here on.
        output.response.request_focus();
        let mut state = output.state.clone();
        let end = egui::text::CCursor::new(text.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ui.ctx(), editor);
        ui.ctx()
            .data_mut(|data| data.insert_temp(editor.with("seen"), true));
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(slot, (id.clone(), text.clone())));
    let finish = |ctx: &egui::Context| {
        ctx.data_mut(|data| {
            data.remove::<(String, String)>(slot);
            data.remove::<Vec2>(slot.with("size"));
            data.remove::<bool>(editor.with("seen"));
            data.remove::<Rect>(editor.with("rect"));
        });
    };
    let lost = output.response.lost_focus() && !save && !cancel;
    if save {
        let saved = text.trim().to_owned();
        let unchanged = matches!(&message.content, Content::Text { text, .. } if *text == saved);
        if !saved.is_empty() && !unchanged {
            actions.push(Action::SaveEdit {
                chat: view.chat.id.clone(),
                id: message.id.clone(),
                text: saved,
            });
        }
        finish(ui.ctx());
    } else if cancel || lost {
        finish(ui.ctx());
    }
    let rect = output.response.rect;
    Some(inline.then(|| {
        Rect::from_min_max(
            pos2(rect.right() - reserve.unwrap_or(0.0), rect.bottom() - 15.0),
            rect.right_bottom(),
        )
    }))
}

/// Stable message-bubble id used by interaction tests.
pub fn bubble_id(chat: &str, message: &str) -> egui::Id {
    egui::Id::new(("bubble", chat, message))
}

/// Where a video was drawn in its bubble, the rect the viewer flies from and
/// back to.
pub fn video_flight_id(chat: &str, message: &str) -> egui::Id {
    bubble_id(chat, message).with("video-flight")
}

/// Empties the message's picture while the viewer flies it, so it is not seen
/// twice: everything painted until [`show_source`] is drawn clear. Returns the
/// opacity to give back.
fn hide_source(ui: &mut egui::Ui, message: &Message) -> f32 {
    let before = ui.opacity();
    if crate::ui::image_preview::source_hidden(ui.ctx(), &message.chat, &message.id) {
        ui.set_opacity(0.0);
    }
    before
}

/// Gives back what [`hide_source`] took.
fn show_source(ui: &mut egui::Ui, opacity: f32) {
    ui.set_opacity(opacity);
}

/// Lets go of what a message's row left in `ctx.data` once the row has left the
/// layout. egui never frees temporary data, and every message that was ever on
/// screen would keep a dozen entries for the whole session. These are only
/// where the row's parts were last drawn, and the row writes them again
/// whenever it is laid out. Left alone: the arrival spring's start (without it
/// an old message would spring in again), the time toggle (the reader's own
/// choice), and the menu and reaction rects (a popup may still be anchored
/// to them).
fn forget_bubble(ctx: &egui::Context, chat: &str, message: &str) {
    let id = bubble_id(chat, message);
    ctx.data_mut(|data| {
        for part in [
            "rect",
            "footer",
            "picture",
            "preview",
            "quote",
            "card",
            "call-box",
            "album-tile",
            "video-flight",
            "forwarded",
            "carousel-viewport",
        ] {
            data.remove::<Rect>(id.with(part));
        }
        data.remove::<bool>(id.with("swiping"));
        data.remove::<bool>(id.with("video-flight").with("note"));
    });
}

/// Where a voice message's speed chip was drawn, for interaction tests.
pub fn speed_chip_id(chat: &str, message: &str) -> egui::Id {
    egui::Id::new(("speed-chip", chat, message))
}

/// Where a speed choice in a voice message's menu was drawn, for
/// interaction tests.
pub fn speed_button_id(chat: &str, message: &str, speed: f32) -> egui::Id {
    egui::Id::new(("speed", chat, message, speed.to_bits()))
}

/// Draws a playback speed pill labelled with `speed`, highlighted when
/// `active` and faded while that speed is still `preparing`.
fn speed_pill(
    ui: &mut egui::Ui,
    view: &View<'_>,
    size: Vec2,
    speed: f32,
    active: bool,
    preparing: bool,
) -> egui::Response {
    let palette = view.palette;
    let label = crate::audio::speed_label(speed);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            active,
            format!("Playback speed {label}"),
        )
    });
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        // The resting fill uses the hover step because incoming bubbles
        // share the resting surface colour.
        let fill = if active {
            palette
                .accent
                .gamma_multiply(if hovered { 0.42 } else { 0.30 })
        } else if hovered {
            palette.surface_active
        } else {
            palette.surface_hover
        };
        ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
        let colour = if active {
            palette.accent
        } else {
            palette.secondary
        };
        let colour = if preparing {
            colour.gamma_multiply(0.5)
        } else {
            colour
        };
        let galley = ui
            .painter()
            .layout_no_wrap(label, theme::medium(11.0), colour);
        ui.painter()
            .galley(rect.center() - galley.size() / 2.0, galley, colour);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The message menu's row of every playback speed for a playable voice
/// message, so 1.25x and 1.75x are reachable without cycling.
/// Choosing `speed` for a voice message of `chat`: a chat with its own speed
/// changes that, and any other changes the speed in Settings.
fn speed_action(chat: &Chat, speed: f32) -> Action {
    if chat.voice_speed.is_some() {
        Action::SetChatVoiceSpeed {
            chat: chat.id.clone(),
            speed: Some(speed),
        }
    } else {
        Action::SetVoiceSpeed(speed)
    }
}

fn speed_menu_row(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    actions: &mut Vec<Action>,
) {
    let speed = view.voice_speed;
    let preparing = view.player.preparing_speed(&message.id);
    widgets::menu_separator(ui, &view.palette);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 28.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_space(6.0);
            for option in crate::audio::SPEEDS {
                let selected = option == speed;
                let response = speed_pill(
                    ui,
                    view,
                    vec2(44.0, 24.0),
                    option,
                    selected,
                    preparing && selected,
                );
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        speed_button_id(&view.chat.id, &message.id, option),
                        response.rect,
                    );
                });
                if response.clicked() {
                    actions.push(speed_action(view.chat, option));
                    ui.close();
                }
            }
        },
    );
}

/// Whether `message` draws without a bubble around it: stickers, round video
/// messages, carousels, and in the Messages style a lone picture.
fn bubbleless(palette: &Palette, message: &Message) -> bool {
    matches!(
        &message.content,
        Content::Sticker { .. } | Content::Video { note: true, .. }
    ) || matches!(&message.content, Content::Interactive { card: Some(card), .. } if !card.carousel.is_empty())
        || bare_picture(palette, message)
}

/// Whether `messages[index]` ends its run: the next message is from the other
/// side, another sender, or another day.
fn last_of_run(messages: &[Message], index: usize) -> bool {
    let message = &messages[index];
    messages.get(index + 1).is_none_or(|next| {
        !same_run(message, next)
            || crate::util::day_key(message.timestamp) != crate::util::day_key(next.timestamp)
    })
}

/// Whether `messages[index]` is the last bubble of its run, which Messages
/// tails: messages drawn without one after it, a lone photo or a sticker,
/// leave the tail to it.
fn last_bubble_of_run(palette: &Palette, messages: &[Message], index: usize) -> bool {
    let message = &messages[index];
    !messages[index + 1..]
        .iter()
        .take_while(|next| {
            same_run(message, next)
                && crate::util::day_key(message.timestamp) == crate::util::day_key(next.timestamp)
        })
        .any(|next| !bubbleless(palette, next))
}

/// Whether `next` continues the run `message` belongs to: the same side,
/// and in a group the same sender.
fn same_run(message: &Message, next: &Message) -> bool {
    message.from_me == next.from_me && (message.from_me || message.sender == next.sender)
}

/// A Nothing bubble's room around its text.
const NOTHING_TEXT_MARGIN: Margin = Margin::symmetric(14, 10);

/// Room between a call bubble's edge and the chip that holds its glyph
/// and words.
const CALL_BUBBLE_PAD: i8 = 4;

/// Room around a bubble's contents: none around a picture that stands
/// without one, Messages' insets in its style, WhatsApp's otherwise.
/// A call keeps four points, so its chip sits just inside the bubble.
fn bubble_margin(palette: &Palette, message: &Message, over_picture: bool) -> Margin {
    if matches!(message.content, Content::Call { .. }) {
        return Margin::same(CALL_BUBBLE_PAD);
    }
    if bare_picture(palette, message) {
        Margin::ZERO
    } else if palette.bubbles == theme::BubbleStyle::Messages {
        MESSAGES_BUBBLE_MARGIN
    } else if palette.bubbles == theme::BubbleStyle::Nothing {
        if compact_picture_message(message) {
            Margin::symmetric(6, 6)
        } else if compact_attachment_message(message) {
            Margin::same(NOTHING_ATTACHMENT_RIM)
        } else {
            NOTHING_TEXT_MARGIN
        }
    } else if over_picture && compact_picture_message(message) {
        // A photo alone keeps an even rim, as in WhatsApp.
        Margin::same(4)
    } else {
        Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: if over_picture { 6 } else { 5 },
        }
    }
}

/// Draws a message bubble and its menu.
#[allow(clippy::too_many_arguments)]
fn bubble_frame(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: Option<&[Message]>,
    show_sender: bool,
    tailed: bool,
    first_in_run: bool,
    last_in_run: bool,
    max_width: f32,
    actions: &mut Vec<Action>,
) -> egui::Response {
    let palette = view.palette;
    let own = message.from_me;
    let fill = if bubbleless(&palette, message) {
        Color32::TRANSPARENT
    } else if own {
        palette.bubble_out
    } else {
        palette.bubble_in
    };
    // Register the bubble from its previous rect before its contents so inner
    // links, quotes, and attachments win clicks. The bubble handles right-click.
    let bubble_id = bubble_id(&view.chat.id, &message.id);
    // Store the final rect separately. Reusing the early response would keep
    // the first frame's rect.
    let rect_id = bubble_id.with("rect");
    let previous = ui.ctx().data(|data| data.get_temp::<Rect>(rect_id));
    let early = previous.map(|rect| ui.interact(rect, bubble_id, Sense::CLICK));
    // Painted once the contents are measured, beneath them, over a scrim.
    let scrim = ui.painter().add(egui::Shape::Noop);
    let backdrop = ui.painter().add(egui::Shape::Noop);
    let tail = (tailed && fill != Color32::TRANSPARENT).then_some(if own {
        widgets::Side::Right
    } else {
        widgets::Side::Left
    });
    // A picture without a caption carries its time over its corner, so
    // the bubble closes under it as evenly as it opens above it.
    let over_picture = album.is_none() && time_over_picture(message);
    let mut tiles: Vec<(Rect, String)> = Vec::new();
    // Where a photo that carries its time fills the bubble, to darken with
    // the bubble while its menu is open.
    let mut photo: Option<Rect> = None;
    let inner = Frame::new()
        .inner_margin(bubble_margin(&palette, message, over_picture))
        .show(ui, |ui| {
            ui.set_max_width(max_width);
            ui.spacing_mut().item_spacing.y = 4.0;
            if show_sender && view.chat.is_group() {
                let name = (view.names_or)(&message.sender, message.sender_name.as_deref());
                let response = widgets::rich_text(
                    ui,
                    &name,
                    theme::semibold(13.0),
                    palette.sender(crate::util::hue(&message.sender)),
                );
                let response = ui
                    .interact(
                        response.rect,
                        ui.id().with(("sender", &message.id)),
                        Sense::CLICK,
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() {
                    actions.push(Action::OpenInfo(message.sender.clone()));
                }
            }
            // Reserve the label's line now and paint it once the contents
            // are measured, at their start: an own bubble lays out from the
            // right, and its width is known only then.
            let forwarded = message.forwarded.then(|| forwarded_label(ui, &palette));
            // Cards share the bubble's settled width: at least CARD_WIDTH and
            // no more than the cap. Text spans that width and stays left-aligned.
            // Bubbles without cards use the natural text width.
            let inset = if matches!(message.content, Content::Call { .. }) {
                f32::from(CALL_BUBBLE_PAD) * 2.0
            } else if !bare_picture(&palette, message)
                && palette.bubbles == theme::BubbleStyle::Messages
            {
                MESSAGES_BUBBLE_MARGIN.sum().x
            } else if !bare_picture(&palette, message)
                && palette.bubbles == theme::BubbleStyle::Nothing
            {
                28.0
            } else {
                20.0
            };
            let cap = ((max_width - inset).min(ui.available_width())).max(0.0);
            let last = album
                .and_then(|messages| messages.last())
                .unwrap_or(message);
            // An album's footer is its last member's, marks included.
            let starred = view.starred.is_some_and(|ids| ids.contains(&last.id));
            let pinned = view.pins.is_some_and(|ids| ids.contains(&last.id));
            let reserve =
                message_footer_width(ui, last, starred, pinned, &palette, view.time_inside);
            let settled = album
                .map(|_| CARD_WIDTH.min(cap))
                .or_else(|| settled_width(ui, view, message, cap));
            let slot = if let Some(messages) = album {
                for member in messages {
                    if let Some(quoted) = &member.quoted {
                        quote_block(ui, view, member, quoted, settled.unwrap_or(cap), actions);
                    }
                }
                tiles = album_content(ui, view, messages, settled.unwrap_or(cap), actions);
                None
            } else {
                match settled {
                    Some(width) => {
                        if let Some(quoted) = &message.quoted {
                            quote_block(ui, view, message, quoted, width, actions);
                        }
                        content(ui, view, message, width, reserve, actions)
                    }
                    None => content(ui, view, message, cap, reserve, actions),
                }
            };
            if over_picture && let Some(picture) = slot {
                photo = Some(picture);
                footer_over_picture(ui, view, message, picture, starred, pinned, actions);
            } else {
                footer(ui, view, last, slot, starred, pinned, actions);
            }
            if matches!(message.content, Content::Poll { .. }) {
                super::polls::results_button(
                    ui,
                    &palette,
                    view.locale,
                    message,
                    settled.unwrap_or(cap),
                    actions,
                );
            }
            if let Content::Interactive {
                card: Some(card), ..
            } = &message.content
            {
                interactive_buttons(ui, view, message, card, settled.unwrap_or(cap), actions);
            }
            if let Some((reserved, galley)) = forwarded {
                let at = pos2(ui.min_rect().left(), reserved.top());
                let painted = paint_forwarded_label(ui, &palette, at, galley);
                ui.ctx().data_mut(|data| {
                    data.insert_temp(bubble_id.with("forwarded"), painted);
                });
            }
        });
    // While its menu is open the bubble darkens a little, so it is plain
    // which message the menu acts on. Read from the popup's state as the last
    // frame left it, since the menu shows only after the bubble is drawn.
    let shade = menu_shade(ui.ctx(), &palette, bubble_id);
    // Lifted over the scrim of an edit in place, a bubble goes without its
    // wash, which would light the scrim around it, and still shades down the
    // whole transcript, though its clip keeps to the stretch between the
    // header and the composer.
    let lifted = ui.layer_id() == edit_focus_layers(&view.chat.id).1;
    let span = if lifted {
        ui.ctx()
            .data(|data| data.get_temp::<egui::Rangef>(transcript_span_id(&view.chat.id)))
            .unwrap_or(ui.clip_rect().y_range())
    } else {
        ui.clip_rect().y_range()
    };
    if fill != Color32::TRANSPARENT && ui.is_rect_visible(inner.response.rect) {
        if !lifted && let Some(shape) = doodle_scrim(inner.response.rect, fill, view.doodles) {
            ui.painter().set(scrim, shape);
            if let Some(scrims) = view.scrims {
                scrims.borrow_mut().push((ui.layer_id(), scrim));
            }
        }
        let mut shape = if palette.bubbles == theme::BubbleStyle::Nothing {
            widgets::grouped_bubble_shape(inner.response.rect, fill, own, first_in_run, last_in_run)
        } else {
            widgets::bubble_shape(
                &palette,
                inner.response.rect,
                fill,
                tail,
                ui.ctx().pixels_per_point(),
                // The transcript's viewport: own balloons shade down it.
                span,
            )
        };
        // The whole shape, tail, gradient and edges included.
        if shade > 0.0 {
            shade_shape(&mut shape, shade);
        }
        ui.painter().set(backdrop, shape);
    } else if shade > 0.0
        && photo.is_none()
        && tiles.is_empty()
        && ui.is_rect_visible(inner.response.rect)
    {
        // A sticker, a round video or a carousel has no fill to darken: a
        // faint patch behind it marks it instead.
        ui.painter().set(
            backdrop,
            egui::Shape::rect_filled(
                inner.response.rect,
                CornerRadius::from(picture_radius(&palette, message)),
                Color32::from_black_alpha(shade_alpha(shade)),
            ),
        );
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(rect_id, inner.response.rect));
    let bubble = early.unwrap_or_else(|| ui.interact(inner.response.rect, bubble_id, Sense::CLICK));
    if view.selecting {
        return bubble;
    }
    theme::reveal_focus(&bubble);
    let corners = bubble_outline_corners(
        &palette,
        message,
        fill,
        tail,
        own,
        first_in_run,
        last_in_run,
    );
    theme::focus_outline_corners(ui, bubble.id, inner.response.rect, corners);
    if ui.ctx().data(|data| {
        data.get_temp::<bool>(theme::keyboard_focus_id())
            .unwrap_or(false)
    }) && let Some(focused) = ui
        .memory(|memory| memory.focused())
        .and_then(|id| ui.ctx().read_response(id))
        && (focused.id == bubble.id || inner.response.rect.contains(focused.rect.center()))
    {
        view.keyboard_navigation.set(true);
        if focused.gained_focus() && !ui.clip_rect().contains_rect(focused.rect) {
            ui.scroll_to_rect_animation(
                inner.response.rect.expand(4.0),
                None,
                egui::style::ScrollAnimation::none(),
            );
        }
    }
    // The context menu stays open beside the picker only when the picker came
    // from the menu itself.
    let reacting = view.reaction_menu && view.reaction == Some(message.id.as_str());
    // Inner widgets own their clicks, so this fires only on the bubble's padding
    // and footer. Double-click on the body keeps selecting the word.
    reply_on_double_click(&bubble, view, message, actions);

    reaction_affordance(ui, view, message, &bubble, actions);
    // Read right-click from input because inner widgets own their responses.
    // The whole row counts, the empty strip beside the bubble included, as in
    // other messaging apps (#240). Count only the part inside the transcript's
    // viewport: the chat header shares this layer, and a bubble scrolled under
    // it is hidden there. Open only when no floating layer covers the chat
    // panel.
    let viewport = ui.clip_rect();
    let shown =
        Rect::from_x_y_ranges(viewport.x_range(), bubble.rect.y_range()).intersect(viewport);
    let right_clicked = ui.input(|input| {
        input.pointer.secondary_clicked()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| shown.contains(pos))
    }) && ui
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|pos| {
            ui.ctx()
                .layer_id_at(pos)
                .is_none_or(|layer| layer == bubble.layer_id)
        });
    // In an album the menu belongs to the member under the pointer, so a
    // reply, reaction, forward or deletion reaches that item, not the first.
    let target_id = bubble_id.with("album-target");
    if album.is_some() && right_clicked {
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let hit = tiles
            .iter()
            .find(|(rect, _)| pointer.is_some_and(|pos| rect.contains(pos)))
            .map(|(_, id)| id.clone());
        ui.ctx().data_mut(|data| {
            if let Some(id) = hit {
                data.insert_temp(target_id, id);
            } else {
                data.remove::<String>(target_id);
            }
        });
    }
    if let Some(members) = album
        && let Some(opened) = view.open_menu
        && members.iter().any(|member| member.id == opened)
    {
        ui.ctx()
            .data_mut(|data| data.insert_temp(target_id, opened.to_owned()));
    }
    let menu_target = album
        .and_then(|members| {
            let id = ui.ctx().data(|data| data.get_temp::<String>(target_id))?;
            members.iter().find(|member| member.id == id)
        })
        .unwrap_or(message);
    let force_menu = match album {
        Some(members) => view
            .open_menu
            .is_some_and(|opened| members.iter().any(|member| member.id == opened)),
        None => view.open_menu == Some(message.id.as_str()),
    };
    let reacting = reacting
        || (album.is_some()
            && view.reaction_menu
            && view.reaction == Some(menu_target.id.as_str()));
    let width = menu_width(ui, view.locale, view.reaction_emoji, view.menu, menu_target);
    let keyboard_clicked =
        bubble.clicked() && bubble.has_focus() && !ui.input(|input| input.pointer.any_click());
    let open = if right_clicked || force_menu || reacting || keyboard_clicked {
        Some(egui::SetOpenCommand::Bool(true))
    } else if bubble.clicked() {
        Some(egui::SetOpenCommand::Bool(false))
    } else {
        None
    };
    let popup = egui::Popup::menu(&bubble)
        .open_memory(open)
        .close_behavior(if reacting {
            egui::PopupCloseBehavior::IgnoreClicks
        } else {
            egui::PopupCloseBehavior::CloseOnClickOutside
        })
        .width(width);
    let popup = if reacting {
        // Keep the menu next to the picker, anchored to this message rather
        // than whichever pointer position happened to open the emoji grid.
        let screen = ui.ctx().content_rect();
        let menu = ui
            .ctx()
            .data(|data| data.get_temp::<Rect>(bubble_id.with("menu-rect")))
            .unwrap_or(bubble.rect);
        let x = menu.left().clamp(
            screen.left() + 8.0,
            (screen.right() - width - 468.0).max(screen.left() + 8.0),
        );
        popup.at_position(pos2(x, menu.top()))
    } else if force_menu || keyboard_clicked {
        popup.at_position(bubble.rect.left_top() + vec2(12.0, 8.0))
    } else {
        popup.at_pointer_fixed()
    };
    let menu = popup.show_lifted(ui.ctx(), &palette, |ui| {
        context_menu(
            ui,
            view,
            menu_target,
            album.unwrap_or_default(),
            MenuOrigin::Chat,
            actions,
        );
    });
    if let Some(menu) = menu {
        ui.ctx()
            .data_mut(|data| data.insert_temp(bubble_id.with("menu-rect"), menu.response.rect));
    }
    // A photo hides the fill under it, so it darkens under a veil as dark
    // as the fill's shade, in the message layer so the menu stays above it.
    if shade > 0.0 {
        let veil = Color32::from_black_alpha(shade_alpha(shade));
        if let Some(photo) = photo {
            ui.painter().rect_filled(
                photo,
                CornerRadius::from(picture_radius(&palette, message)),
                veil,
            );
        }
        if let Some(first) = album.and_then(|members| members.first()) {
            // Each tile takes the seams' small radius: at the album's
            // rounder outer corners the veil reaches a hair past the
            // picture, onto the fill that has darkened already.
            let radius = ALBUM_SEAM_RADIUS.min(picture_radius(&palette, first));
            for (tile, _) in &tiles {
                ui.painter()
                    .rect_filled(*tile, CornerRadius::from(radius), veil);
            }
        }
    }
    if reacting && !egui::Popup::is_id_open(ui.ctx(), bubble_id.with("popup")) {
        actions.push(Action::ClosePicker);
    }
    // This frame's final rect, for later scrolling, with the bubble's own
    // clicks: the frame alone only senses hover, so a Ctrl-click to select
    // or a click to add to a selection would never register.
    inner.response.union(bubble)
}

/// Minimum shared width for cards inside message bubbles.
const CARD_WIDTH: f32 = 320.0;
const CAROUSEL_CARD_WIDTH: f32 = 280.0;
/// Location cards: a map preview across the top, then the details.
const LOCATION_CARD_WIDTH: f32 = 300.0;
const CAROUSEL_GAP: f32 = 8.0;

/// Returns the shared card width, bounded by [`CARD_WIDTH`] and `cap`.
fn settled_width(ui: &egui::Ui, view: &View<'_>, message: &Message, cap: f32) -> Option<f32> {
    if matches!(
        message.content,
        Content::Location { .. } | Content::LiveLocation { .. }
    ) {
        return Some(LOCATION_CARD_WIDTH.min(cap));
    }
    if let Content::Interactive {
        card: Some(card), ..
    } = &message.content
        && !card.carousel.is_empty()
    {
        // Keep short carousels, their heading and their timestamp together.
        // Wider strips still occupy the cap and scroll horizontally.
        let count = card.carousel.len();
        let cards = count as f32 * (CAROUSEL_CARD_WIDTH + 20.0);
        let gaps = count.saturating_sub(1) as f32 * CAROUSEL_GAP;
        return Some((cards + gaps).min(cap));
    }
    if let Content::Interactive {
        card: Some(card), ..
    } = &message.content
        && card.image.is_some()
    {
        return Some(CARD_WIDTH.min(cap));
    }
    let card = message.quoted.is_some()
        || match &message.content {
            Content::Text { preview, .. } => preview.is_some(),
            Content::Document { .. } | Content::Audio { .. } | Content::Poll { .. } => true,
            Content::Interactive { card, .. } => card.is_some(),
            // Videos without a poster use the file-row layout, except the
            // round ones, which always draw as a circle.
            Content::Video { note, .. } => !note && message.thumbnail.is_none(),
            _ => false,
        };
    card.then(|| {
        let floor = CARD_WIDTH.min(cap).max(0.0);
        natural_text_width(ui, view, message, cap).map_or(floor, |width| width.clamp(floor, cap))
    })
}

/// Widest wrapped text row, including footer space on the last line.
fn natural_text_width(ui: &egui::Ui, view: &View<'_>, message: &Message, cap: f32) -> Option<f32> {
    let palette = view.palette;
    let text = match &message.content {
        Content::Text { text, .. } | Content::Interactive { text, .. } => text,
        Content::Image {
            caption: Some(caption),
            ..
        }
        | Content::Video {
            caption: Some(caption),
            ..
        }
        | Content::Document {
            caption: Some(caption),
            ..
        } => caption,
        _ => return None,
    };
    let style = markup::Style {
        size: BODY_SIZE,
        color: palette.text,
        secondary: palette.secondary,
        link: palette.link,
        mention: palette.accent,
    };
    let laid = markup::layout(ui, text, &mentions_of(view, message), &style, cap);
    let widest = laid
        .galley
        .rows
        .iter()
        .map(|row| row.row.size.x)
        .fold(0.0, f32::max);
    let last = laid.galley.rows.last().map_or(0.0, |row| row.row.size.x);
    let starred = view.starred.is_some_and(|ids| ids.contains(&message.id));
    let pinned = view.pins.is_some_and(|ids| ids.contains(&message.id));
    let reserve = message_footer_width(ui, message, starred, pinned, &palette, view.time_inside);
    Some(if last + 8.0 + reserve <= cap {
        widest.max(last + 8.0 + reserve)
    } else {
        widest
    })
}

/// Width of a quote's accent bar.
const QUOTE_BAR: f32 = 4.0;
/// Room between a quote's text and the bar on its left, and its right edge.
const QUOTE_SIDE: i8 = 10;
/// Corner radius of a quote.
const QUOTE_RADIUS: u8 = 12;
/// The even ring a Nothing bubble keeps around a card at its edge.
const CARD_RING: i8 = 6;

/// How far a card at a bubble's edge (a quote, a link preview) reaches past
/// the bubble's text padding, and how round it is. In the Nothing style it
/// sits in a thin even ring, its corners following the bubble's 18, while
/// the text keeps the wider padding; elsewhere it keeps to the padding.
fn edge_card(ui: &egui::Ui, palette: &Palette, radius: u8) -> (Margin, u8) {
    if palette.bubbles != theme::BubbleStyle::Nothing {
        return (Margin::ZERO, radius);
    }
    let pad = NOTHING_TEXT_MARGIN;
    // Nothing drawn above it yet: it meets the bubble's top too.
    let top = if ui.min_rect().height() < 1.0 {
        CARD_RING - pad.top
    } else {
        0
    };
    let side = CARD_RING - pad.left;
    (
        Margin {
            left: side,
            right: side,
            top,
            bottom: 0,
        },
        widgets::NOTHING_BUBBLE_RADIUS - CARD_RING as u8,
    )
}

/// Draws a stored quote and routes navigation to its source conversation.
/// How a quoted reply is coloured inside its bubble.
#[derive(Clone, Copy, Debug, PartialEq)]
struct QuoteColours {
    fill: Color32,
    bar: Color32,
    name: Color32,
    text: Color32,
}

/// The quote's colours. On your own Messages balloon, iMessage blue, it is
/// all white, as Messages draws what sits on that blue: a bold white name,
/// its text at Apple's secondary 60% white
/// (`CKUITheme.blue_balloonTranslationSecondaryTextColor`), the bar at 60%
/// and the fill a 15% white wash over the bubble. Elsewhere the bar and name
/// take `tint`, the quoted sender's colour, over the window's.
fn quote_colours(palette: &Palette, own: bool, tint: impl FnOnce() -> Color32) -> QuoteColours {
    if own && palette.bubbles == theme::BubbleStyle::Messages {
        return QuoteColours {
            fill: Color32::WHITE.gamma_multiply(0.15),
            bar: Color32::WHITE.gamma_multiply(0.6),
            name: Color32::WHITE,
            text: Color32::WHITE.gamma_multiply(0.6),
        };
    }
    let tint = tint();
    QuoteColours {
        fill: palette.window.gamma_multiply(0.35),
        bar: tint,
        name: tint,
        text: palette.secondary,
    }
}

fn quote_block(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    quoted: &crate::model::Quoted,
    width: f32,
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    let mine = view.me == Some(quoted.sender.as_str());
    let who = if mine {
        "You".to_owned()
    } else {
        (view.names_or)(&quoted.sender, quoted.sender_name.as_deref())
    };
    let summary = markup::plain(
        &quoted.summary_in(view.locale),
        &quote_mentions(view, quoted),
    );
    // As in WhatsApp, the bar and name take the quoted sender's colour, the
    // one their name has in groups, kept readable on this bubble.
    let bubble = if message.from_me {
        palette.bubble_out
    } else {
        palette.bubble_in
    };
    let colours = quote_colours(&palette, message.from_me, || {
        theme::readable_on(
            bubble,
            if mine {
                palette.accent
            } else {
                palette.sender(crate::util::hue(&quoted.sender))
            },
            palette.text,
            3.0,
        )
    });
    let (bleed, radius) = edge_card(ui, &palette, QUOTE_RADIUS);
    // The same room on both sides of the text: after the bar and before the
    // card's right edge.
    let padding = Margin {
        left: QUOTE_BAR as i8 + QUOTE_SIDE,
        right: QUOTE_SIDE,
        top: 6,
        bottom: 7,
    };
    let response = Frame::new()
        .fill(colours.fill)
        .corner_radius(CornerRadius::same(radius))
        .outer_margin(bleed)
        .inner_margin(padding)
        .show(ui, |ui| {
            // Include frame margins in the settled width. Use a bounded,
            // left-aligned layout because own bubbles inherit right-to-left flow.
            let inner_width = (width - bleed.sum().x - padding.sum().x).max(0.0);
            ui.allocate_ui_with_layout(
                vec2(inner_width, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_width(inner_width);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    widgets::rich_text(ui, &who, theme::semibold(12.5), colours.name);
                    widgets::rich_text(ui, &summary, theme::regular(12.5), colours.text);
                },
            );
        })
        .response;
    let card = response.rect - bleed;
    // The bar is the card's own left edge, cut to its rounded corners.
    widgets::quote_bar(
        ui.painter(),
        card,
        f32::from(radius),
        QUOTE_BAR,
        colours.bar,
    );
    ui.ctx().data_mut(|data| {
        data.insert_temp(bubble_id(&view.chat.id, &message.id).with("quote"), card);
    });
    let response = ui
        .interact(
            card,
            ui.id().with(("quote", &message.id, &quoted.id)),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        if let Some(chat) = &quoted.chat {
            actions.push(Action::OpenMessage {
                chat: chat.clone(),
                message: quoted.id.clone(),
            });
        } else {
            actions.push(Action::ScrollTo(quoted.id.clone()));
        }
    }
}

/// Diameter of the tinted disc behind a call entry's glyph.
const CALL_BADGE: f32 = 36.0;
/// Size of the glyph inside it.
const CALL_GLYPH: f32 = 18.0;
/// Room between the disc and the words.
const CALL_BADGE_GAP: f32 = 10.0;
/// Room between the title and the line under it.
const CALL_LINE_GAP: f32 = 2.0;
/// Height of the time's line, as [`footer`] draws it.
const FOOTER_HEIGHT: f32 = 15.0;
/// Room inside the chip, beside the glyph and the words.
const CALL_CHIP_PAD_X: f32 = 8.0;
/// Room inside the chip, above and below the glyph and the words.
const CALL_CHIP_PAD_Y: f32 = 4.0;
/// Corner of the chip. The bubble's own radius is 10, and the chip sits
/// four points inside it.
const CALL_CHIP_RADIUS: u8 = 6;
/// `#000` at 5%. 13 is `0.05 * 255`, rounded.
const CALL_CHIP_FILL: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 13);

/// Draws a call entry: the glyph and the words in a chip, the time on the
/// detail's line when there is one and on the title's line when there is
/// not. A tap calls back the same way, with video when the call carried it.
/// Painted at measured positions inside one allocation, so an own bubble's
/// right-to-left layout cannot reorder or stretch it. Returns the time's
/// slot, or `None` when the time needs a line of its own.
#[expect(clippy::too_many_arguments)]
fn call_entry(
    ui: &mut egui::Ui,
    palette: &Palette,
    entry: &super::calls::Entry,
    locale: crate::i18n::Locale,
    tint: Color32,
    width: f32,
    reserve: f32,
    chat: &str,
    message_id: &str,
    can_call: bool,
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    let title = entry.title(locale);
    let detail = entry.detail(locale);
    let title_galley =
        ui.painter()
            .layout_no_wrap(title.clone(), theme::medium(14.0), palette.text);
    let detail_galley = detail.as_ref().map(|detail| {
        ui.painter()
            .layout_no_wrap(detail.clone(), theme::regular(12.5), palette.secondary)
    });
    let title_width = title_galley.size().x;
    let detail_width = detail_galley.as_ref().map_or(0.0, |galley| galley.size().x);
    // The time follows the words on their line, 8 points after them. With a
    // detail that line is the detail's; without one it is the title's, so a
    // missed call's clock lines up with its label.
    let lead = CALL_BADGE + CALL_BADGE_GAP;
    let usable = (width - CALL_CHIP_PAD_X * 2.0).max(0.0);
    let inline = if detail_galley.is_some() {
        lead + detail_width + 8.0 + reserve <= usable
    } else {
        lead + title_width + 8.0 + reserve <= usable
    };
    let text_span = if detail_galley.is_none() {
        if inline {
            title_width + 8.0 + reserve
        } else {
            title_width
        }
    } else if inline {
        title_width.max(detail_width + 8.0 + reserve)
    } else {
        title_width.max(detail_width)
    };
    let second_height = match (&detail_galley, inline) {
        (Some(galley), true) => galley.size().y.max(FOOTER_HEIGHT),
        (Some(galley), false) => galley.size().y,
        (None, _) => 0.0,
    };
    let block_height = title_galley.size().y
        + if second_height > 0.0 {
            CALL_LINE_GAP + second_height
        } else {
            0.0
        };
    let content_width = lead + text_span + CALL_CHIP_PAD_X * 2.0;
    // Span the bubble when a sender's name above made it wider, so the time
    // keeps to the bubble's end as it does under text.
    let row_width = content_width
        .max(ui.min_rect().width())
        .min(width.max(content_width));
    let row_height = block_height.max(CALL_BADGE) + CALL_CHIP_PAD_Y * 2.0;
    let sense = if can_call {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(row_width, row_height), sense);
    response.widget_info(|| {
        let label = match &detail {
            Some(detail) => format!("{title}, {detail}"),
            None => title.clone(),
        };
        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label)
    });
    if can_call {
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            let chat = chat.to_owned();
            actions.push(if entry.video {
                Action::StartVideoCall(chat)
            } else {
                Action::StartCall(chat)
            });
        }
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(bubble_id(chat, message_id).with("call-box"), rect);
    });
    let content_left = rect.left() + CALL_CHIP_PAD_X;
    let content_right = rect.right() - CALL_CHIP_PAD_X;
    let badge = Rect::from_center_size(
        pos2(content_left + CALL_BADGE / 2.0, rect.center().y),
        Vec2::splat(CALL_BADGE),
    );
    let text_left = badge.right() + CALL_BADGE_GAP;
    let top = rect.center().y - block_height / 2.0;
    let second_top = top + title_galley.size().y + CALL_LINE_GAP;
    let second_middle = second_top + second_height / 2.0;
    // No detail: the title, the glyph and the time share one centre. A
    // detail keeps the time on its own line.
    let time_middle = if detail_galley.is_some() {
        second_middle
    } else {
        rect.center().y
    };
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(CALL_CHIP_RADIUS), CALL_CHIP_FILL);
        ui.painter()
            .circle_filled(badge.center(), CALL_BADGE / 2.0, tint.gamma_multiply(0.16));
        theme::paint_icon(
            ui,
            entry.icon(),
            Rect::from_center_size(badge.center(), Vec2::splat(CALL_GLYPH)),
            CALL_GLYPH,
            tint,
        );
        ui.painter()
            .galley(pos2(text_left, top), title_galley, palette.text);
        if let Some(galley) = detail_galley {
            let at = pos2(text_left, second_middle - galley.size().y / 2.0);
            ui.painter().galley(at, galley, palette.secondary);
        }
    }
    inline.then(|| {
        Rect::from_min_size(
            pos2(content_right - reserve, time_middle - FOOTER_HEIGHT / 2.0),
            vec2(reserve, FOOTER_HEIGHT),
        )
    })
}

/// Keeps row contents left-to-right inside right-aligned own bubbles.
fn mirrored_row(
    ui: &mut egui::Ui,
    own: bool,
    first: impl FnOnce(&mut egui::Ui),
    second: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        if own {
            second(ui);
            first(ui);
        } else {
            first(ui);
            second(ui);
        }
    });
}

/// Size of the forwarded label's arrow.
const FORWARDED_ICON: f32 = 14.0;
/// Gap between the forwarded label's arrow and its word.
const FORWARDED_GAP: f32 = 4.0;
/// Height of the forwarded label's line: its word's height, without the
/// taller line an icon and a label in a row would take.
const FORWARDED_HEIGHT: f32 = 15.0;

/// Reserves the line for a forwarded message's label at the top of its
/// bubble and returns that space with the laid-out word, for
/// [`paint_forwarded_label`] once the bubble's contents are measured.
fn forwarded_label(ui: &mut egui::Ui, palette: &Palette) -> (Rect, std::sync::Arc<egui::Galley>) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        "Forwarded",
        0.0,
        egui::TextFormat {
            font_id: theme::regular(12.5),
            color: palette.dim,
            italics: true,
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    let width = FORWARDED_ICON + FORWARDED_GAP + galley.size().x;
    let (reserved, response) =
        ui.allocate_exact_size(vec2(width, FORWARDED_HEIGHT), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "Forwarded"));
    // Sit closer to what follows than the bubble's usual spacing.
    ui.add_space(-1.0);
    (reserved, galley)
}

/// Paints a forwarded label with its left edge at `at`, as WhatsApp does:
/// at the start of the bubble on either side. Returns where it went.
fn paint_forwarded_label(
    ui: &egui::Ui,
    palette: &Palette,
    at: egui::Pos2,
    galley: std::sync::Arc<egui::Galley>,
) -> Rect {
    let rect = Rect::from_min_size(
        at,
        vec2(
            FORWARDED_ICON + FORWARDED_GAP + galley.size().x,
            FORWARDED_HEIGHT,
        ),
    );
    let icon = Rect::from_min_size(
        pos2(rect.left(), rect.center().y - FORWARDED_ICON / 2.0),
        Vec2::splat(FORWARDED_ICON),
    );
    theme::paint_icon(ui, Icon::Forward, icon, FORWARDED_ICON, palette.dim);
    ui.painter().galley(
        pos2(
            icon.right() + FORWARDED_GAP,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        palette.dim,
    );
    rect
}

/// Painted size of the star mark in the bubble footer.
const FOOTER_MARK: f32 = 13.0;
/// One gap: time to the mark.
const FOOTER_MARK_GAP: f32 = 8.0;
/// Room one visible mark keeps next to the clock.
const FOOTER_MARK_SLOT: f32 = FOOTER_MARK + FOOTER_MARK_GAP;
const FOOTER_STAR: Color32 = Color32::from_rgb(0xEA, 0xB3, 0x08);

/// The clock's own width at the footer's size: the floor a footer starts from,
/// and the one part of it that depends on the reader's clock format.
fn clock_width(ui: &egui::Ui, message: &Message) -> f32 {
    ui.painter()
        .layout_no_wrap(
            crate::util::clock(message.timestamp),
            theme::regular(11.0),
            Color32::WHITE,
        )
        .size()
        .x
}

/// Width of the message footer, including the room its marks take.
fn footer_width(ui: &egui::Ui, message: &Message, starred: bool, pinned: bool) -> f32 {
    let font = theme::regular(11.0);
    let time = clock_width(ui, message);
    let edited = if message.edited {
        ui.painter()
            .layout_no_wrap("edited".to_owned(), font, Color32::WHITE)
            .size()
            .x
            + 4.0
    } else {
        0.0
    };
    let not_sent = if not_sent(message) {
        ui.painter()
            .layout_no_wrap(NOT_SENT.to_owned(), theme::medium(11.0), Color32::WHITE)
            .size()
            .x
            + 6.0
    } else {
        0.0
    };
    // Only a message that carries a mark reserves its slot, as WhatsApp does:
    // a plain footer is the clock and the ticks alone. Starring or pinning
    // grows that bubble's footer by one slot each.
    let marks =
        if starred { FOOTER_MARK_SLOT } else { 0.0 } + if pinned { FOOTER_MARK_SLOT } else { 0.0 };
    time + edited + not_sent + if message.from_me { 19.0 } else { 0.0 } + marks
}

/// The reference style leaves the clock and delivery state outside the bubble;
/// only edited and pinned/starred marks still reserve room inside it.
fn message_footer_width(
    ui: &egui::Ui,
    message: &Message,
    starred: bool,
    pinned: bool,
    palette: &Palette,
    time_inside: bool,
) -> f32 {
    if time_inside {
        return footer_width(ui, message, starred, pinned);
    }
    let edited = if message.edited {
        ui.painter()
            .layout_no_wrap("edited".to_owned(), theme::regular(11.0), palette.dim)
            .size()
            .x
            + 4.0
    } else {
        0.0
    };
    edited
        + if starred { FOOTER_MARK_SLOT } else { 0.0 }
        + if pinned { FOOTER_MARK_SLOT } else { 0.0 }
}

/// Whether the message's time and ticks sit over its picture rather than
/// on a line of their own: a picture without a caption, as in WhatsApp.
fn time_over_picture(message: &Message) -> bool {
    matches!(message.content, Content::Image { caption: None, .. })
}

/// The ticks a message in this view shows (see [`display_delivery`]).
fn shown_delivery(view: &View<'_>, message: &Message) -> Delivery {
    display_delivery(
        message.status,
        message.from_me,
        view.chat.kind,
        view.account_receipts_off,
    )
}

/// A DM read receipt is not visible in WhatsApp when our account has read
/// receipts off. Keep the archived status intact, but match that display here.
pub(super) fn display_delivery(
    status: Delivery,
    from_me: bool,
    chat_kind: crate::model::ChatKind,
    account_receipts_off: bool,
) -> Delivery {
    if account_receipts_off
        && chat_kind == crate::model::ChatKind::Direct
        && from_me
        && status == Delivery::Read
    {
        Delivery::Delivered
    } else {
        status
    }
}

/// Whether a picture stands without a bubble, as Messages draws a photo
/// with nothing else in its message: it takes the bubble's round shape
/// itself. A quote or the forwarded label keeps the bubble around it.
fn bare_picture(palette: &Palette, message: &Message) -> bool {
    palette.bubbles == theme::BubbleStyle::Messages
        && time_over_picture(message)
        && message.quoted.is_none()
        && !message.forwarded
}

/// How far toward black a bubble goes while its menu is open, in a light
/// palette: enough to mark the message the menu acts on, and no more.
const MENU_SHADE_LIGHT: f32 = 0.07;
/// The same in a dark palette, where a bubble needs more to show it.
const MENU_SHADE_DARK: f32 = 0.12;

/// How far toward black the bubble `bubble_id` is drawn now: its palette's
/// full shade while its menu is open, nothing once it has closed, easing
/// between the two over [`theme::MOTION_QUICK`] as the menu opens and
/// closes, at once with a screen reader. Only a bubble whose menu is open or
/// still fading keeps its progress, so the bubbles scrolled past leave
/// nothing behind.
fn menu_shade(ctx: &egui::Context, palette: &Palette, bubble_id: egui::Id) -> f32 {
    let open = egui::Popup::is_id_open(ctx, bubble_id.with("popup"));
    let id = bubble_id.with("menu-shade");
    let pass = ctx.cumulative_pass_nr();
    // The progress, and the pass it last moved in.
    let previous = ctx.data(|data| data.get_temp::<(f32, u64)>(id));
    if !open && previous.is_none() {
        return 0.0;
    }
    let progress = if let Some((progress, moved)) = previous
        && moved == pass
    {
        // Drawn again in the same pass: it has moved this frame already.
        progress
    } else if !crate::motion::screen_reader_attached(ctx) {
        let step = ctx.input(|input| input.stable_dt).min(0.1) / theme::MOTION_QUICK;
        let towards = if open { step } else { -step };
        let from = previous.map_or(0.0, |(progress, _)| progress);
        (from + towards).clamp(0.0, 1.0)
    } else if open {
        1.0
    } else {
        0.0
    };
    if progress > 0.0 {
        ctx.data_mut(|data| data.insert_temp(id, (progress, pass)));
    } else {
        ctx.data_mut(|data| data.remove::<(f32, u64)>(id));
    }
    if progress > 0.0 && progress < 1.0 {
        ctx.request_repaint();
    }
    menu_shade_depth(palette) * theme::ease_out(progress)
}

/// A bubble's full shade under its menu in `palette`.
fn menu_shade_depth(palette: &Palette) -> f32 {
    if palette.dark {
        MENU_SHADE_DARK
    } else {
        MENU_SHADE_LIGHT
    }
}

/// `colour` `amount` of the way to black, as opaque as it was.
fn shaded(colour: Color32, amount: f32) -> Color32 {
    let darker = colour.lerp_to_gamma(Color32::BLACK, amount);
    Color32::from_rgba_premultiplied(darker.r(), darker.g(), darker.b(), colour.a())
}

/// A bubble of `fill` as it is drawn while its menu is open.
#[cfg(test)]
pub(crate) fn menu_shaded(palette: &Palette, fill: Color32) -> Color32 {
    shaded(fill, menu_shade_depth(palette))
}

/// Darkens every colour in `shape` `amount` of the way to black, keeping
/// each one's opacity, so a gradient, a tail and anti-aliased edges darken
/// alike.
fn shade_shape(shape: &mut egui::Shape, amount: f32) {
    egui::epaint::shape_transform::adjust_colors(shape, move |colour| {
        if *colour != Color32::PLACEHOLDER {
            *colour = shaded(*colour, amount);
        }
    });
}

/// The opacity of black that, laid over a picture, darkens it as much as
/// `amount` darkens a fill.
fn shade_alpha(amount: f32) -> u8 {
    (amount.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The corners of a message's bubble, as drawn, for the ring that marks it
/// focused.
fn bubble_outline_corners(
    palette: &Palette,
    message: &Message,
    fill: Color32,
    tail: Option<widgets::Side>,
    own: bool,
    first_in_run: bool,
    last_in_run: bool,
) -> CornerRadius {
    if fill == Color32::TRANSPARENT {
        return CornerRadius::from(picture_radius(palette, message));
    }
    match palette.bubbles {
        theme::BubbleStyle::Nothing => {
            widgets::grouped_bubble_corners(own, first_in_run, last_in_run)
        }
        theme::BubbleStyle::Messages => CornerRadius::same(widgets::MESSAGES_BUBBLE_RADIUS),
        _ => widgets::bubble_corners(tail),
    }
}

/// How round a picture's corners are: a bubble's, when it has none.
fn picture_radius(palette: &Palette, message: &Message) -> f32 {
    if bare_picture(palette, message) {
        f32::from(widgets::MESSAGES_BUBBLE_RADIUS)
    } else if palette.bubbles == theme::BubbleStyle::Nothing
        && (compact_picture_message(message)
            || message.quoted.is_some()
            || message.forwarded
            || matches!(
                message.content,
                Content::Image {
                    caption: Some(_),
                    ..
                } | Content::Video {
                    caption: Some(_),
                    ..
                }
            ))
    {
        f32::from(widgets::NOTHING_BUBBLE_RADIUS) - f32::from(CARD_RING)
    } else if palette.bubbles == theme::BubbleStyle::Messages {
        // The bubble's corners less the even rim around the picture.
        f32::from(widgets::MESSAGES_BUBBLE_RADIUS) - f32::from(MESSAGES_BUBBLE_MARGIN.top)
    } else {
        // The bubble's own corners less the few points around the picture.
        f32::from(widgets::BUBBLE_RADIUS) - 4.0
    }
}

/// A photo without accompanying text can use a tighter frame than a text
/// balloon; quotes and forwarded labels keep the room they need.
fn compact_picture_message(message: &Message) -> bool {
    (matches!(message.content, Content::Image { caption: None, .. })
        || (matches!(
            message.content,
            Content::Video {
                caption: None,
                note: false,
                ..
            }
        ) && !compact_attachment_message(message)))
        && message.quoted.is_none()
        && !message.forwarded
}

/// How far a picture or video in a Nothing bubble that also holds a quote,
/// a forwarded label or a caption reaches past the text padding, so it sits
/// in the same even ring as a picture alone. Without a caption it meets the
/// bubble's bottom too.
fn picture_bleed(ui: &egui::Ui, palette: &Palette, message: &Message, captioned: bool) -> Margin {
    if compact_picture_message(message) {
        return Margin::ZERO;
    }
    if palette.bubbles != theme::BubbleStyle::Nothing {
        // Messages and WhatsApp: the picture keeps the room the bubble gives
        // above it on every side, so its rim is even.
        let margin = bubble_margin(palette, message, false);
        let side = margin.top - margin.left;
        return Margin {
            left: side,
            right: side,
            top: 0,
            bottom: if captioned {
                0
            } else {
                margin.top - margin.bottom
            },
        };
    }
    let mut bleed = edge_card(ui, palette, 0).0;
    if !captioned {
        bleed.bottom = CARD_RING - NOTHING_TEXT_MARGIN.bottom;
    }
    bleed
}

/// File cards have their own inset surface inside the message balloon.
fn compact_attachment_message(message: &Message) -> bool {
    matches!(message.content, Content::Document { .. })
        || matches!(
            &message.content,
            Content::Video { media, .. } if message.thumbnail.is_none() && media.path.is_none()
        )
}

/// Space between a picture's edges and the time drawn over it: the scrim
/// around the time keeps a few points clear of the rounded corner.
const OVER_PICTURE_INSET: Vec2 = vec2(10.0, 6.0);

/// Paints the time and ticks over the bottom corner of a picture without a
/// caption, in white on a soft dark scrim so they read on any picture.
fn footer_over_picture(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    picture: Rect,
    starred: bool,
    pinned: bool,
    actions: &mut Vec<Action>,
) {
    let font = theme::regular(11.0);
    let time =
        ui.painter()
            .layout_no_wrap(crate::util::clock(message.timestamp), font, Color32::WHITE);
    let failed = not_sent(message).then(|| {
        ui.painter()
            .layout_no_wrap(NOT_SENT.to_owned(), theme::medium(11.0), Color32::WHITE)
    });
    let tick_width = if message.from_me { 19.0 } else { 0.0 };
    let marks =
        if starred { FOOTER_MARK_SLOT } else { 0.0 } + if pinned { FOOTER_MARK_SLOT } else { 0.0 };
    let width = time.size().x
        + failed.as_ref().map_or(0.0, |galley| galley.size().x + 6.0)
        + tick_width
        + marks;
    // A narrow picture (a tall sticker, a small photo) cannot hold the whole
    // time and ticks, so the bubble widens until it can.
    let needed = width + OVER_PICTURE_INSET.x * 2.0;
    let narrow = needed > picture.width();
    let picture = Rect::from_min_size(
        picture.min,
        vec2(picture.width().max(needed), picture.height()),
    );
    // Only then: a picture that reaches into the bubble's padding already
    // has the bubble around it.
    if narrow {
        ui.expand_to_include_x(picture.right());
    }
    let row = Rect::from_min_max(
        pos2(
            picture.right() - OVER_PICTURE_INSET.x - width,
            picture.bottom() - OVER_PICTURE_INSET.y - 15.0,
        ),
        picture.right_bottom() - OVER_PICTURE_INSET,
    );
    if ui.is_rect_visible(picture) {
        let scrim = row.expand2(vec2(6.0, 2.0)).intersect(picture);
        ui.painter()
            .rect_filled(scrim, scrim.height() / 2.0, Color32::from_black_alpha(110));
    }
    let mut x = row.right();
    if message.from_me {
        let ticks = Rect::from_center_size(pos2(x - 7.5, row.center().y), Vec2::splat(15.0));
        // Pops as the status advances, like the ticks under a bubble's text.
        tick_motion(
            ui,
            view,
            message,
            shown_delivery(view, message),
            ticks,
            Color32::WHITE,
        );
        x -= tick_width;
    }
    x -= time.size().x;
    ui.painter().galley(
        pos2(x, row.center().y - time.size().y / 2.0),
        time,
        Color32::WHITE,
    );
    let status =
        Rect::from_x_y_ranges(x..=row.right(), row.center().y - 7.5..=row.center().y + 7.5);
    status_opens_info(ui, view, message, status, actions);
    if let Some(failed) = failed {
        x -= failed.size().x + 6.0;
        let label = Rect::from_min_size(
            pos2(x, row.center().y - failed.size().y / 2.0),
            failed.size(),
        );
        ui.painter().galley(label.min, failed, Color32::WHITE);
        let status = Rect::from_min_max(label.min, pos2(row.right(), label.max.y));
        let response = ui.interact(
            status,
            ui.id().with(("not-sent", &message.id)),
            Sense::hover(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, true, NOT_SENT_HINT)
        });
        response.on_hover_text(NOT_SENT_HINT);
    }
    if pinned {
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::Pin,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, row.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            Color32::WHITE,
        );
    }
    if starred {
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::StarFill,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, row.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            FOOTER_STAR,
        );
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(footer_id(&message.chat, &message.id), row);
    });
}

/// Where a message's time and ticks were drawn, for layout tests.
pub fn footer_id(chat: &str, message: &str) -> egui::Id {
    bubble_id(chat, message).with("footer")
}

/// Whether "Message info" has receipts to show: our own sent message.
fn has_message_info(message: &Message) -> bool {
    message.from_me
        && !matches!(message.content, Content::Revoked)
        && !matches!(
            message.status,
            Delivery::None | Delivery::Pending | Delivery::Failed
        )
}

/// Clicking our time and ticks opens "Message info", as in WhatsApp.
fn status_opens_info(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    status: Rect,
    actions: &mut Vec<Action>,
) {
    if !has_message_info(message) {
        return;
    }
    let label = crate::i18n::gettext(view.locale, "Message info");
    let response = ui.interact(
        status.expand(2.0),
        ui.id().with(("status", &message.id)),
        Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(&*label);
    if response.clicked() {
        actions.push(Action::ShowDialog(Dialog::MessageInfo {
            chat: message.chat.clone(),
            message: message.id.clone(),
        }));
    }
}

fn not_sent(message: &Message) -> bool {
    message.from_me && message.status == Delivery::Failed
}

/// Paints the time and ticks at the bubble's right edge without widening it.
fn footer(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    slot: Option<Rect>,
    starred: bool,
    pinned: bool,
    actions: &mut Vec<Action>,
) {
    let palette = &view.palette;
    if !view.time_inside {
        reference_footer_marks(ui, message, slot, starred, pinned, palette);
        return;
    }
    let font = theme::regular(11.0);
    let time = ui.painter().layout_no_wrap(
        crate::util::clock(message.timestamp),
        font.clone(),
        palette.secondary,
    );
    let edited = message.edited.then(|| {
        ui.painter()
            .layout_no_wrap("edited".to_owned(), font, palette.dim)
    });
    // A red dot alone does not say what went wrong or what to do. The word
    // uses the text colour: the danger red on an outgoing bubble is too faint
    // to read, and the red icon beside it already carries the alarm.
    let failed = not_sent(message).then(|| {
        ui.painter()
            .layout_no_wrap(NOT_SENT.to_owned(), theme::medium(11.0), palette.text)
    });
    let tick_width = if message.from_me { 19.0 } else { 0.0 };
    let width = time.size().x
        + edited.as_ref().map_or(0.0, |galley| galley.size().x + 4.0)
        + failed.as_ref().map_or(0.0, |galley| galley.size().x + 6.0)
        + tick_width;
    let rect = match slot {
        Some(slot) => slot,
        None => {
            let row_width = ui.min_rect().width().max(width);
            let (rect, _) = ui.allocate_exact_size(vec2(row_width, 15.0), Sense::hover());
            rect
        }
    };
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            footer_id(&message.chat, &message.id),
            Rect::from_min_max(pos2(rect.right() - width, rect.top()), rect.max),
        );
    });
    let mut x = rect.right();
    if message.from_me {
        let ticks = Rect::from_center_size(pos2(x - 7.5, rect.center().y), Vec2::splat(15.0));
        tick_motion(
            ui,
            view,
            message,
            shown_delivery(view, message),
            ticks,
            view.palette.secondary,
        );
        x -= tick_width;
    }
    x -= time.size().x;
    ui.painter().galley(
        pos2(x, rect.center().y - time.size().y / 2.0),
        time,
        palette.secondary,
    );
    let status = Rect::from_x_y_ranges(
        x..=rect.right(),
        rect.center().y - 7.5..=rect.center().y + 7.5,
    );
    status_opens_info(ui, view, message, status, actions);
    if pinned {
        // Left of the clock, in the room `footer_width` counted for it.
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::Pin,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, rect.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            palette.secondary,
        );
    }
    if let Some(edited) = edited {
        x -= edited.size().x + 4.0;
        ui.painter().galley(
            pos2(x, rect.center().y - edited.size().y / 2.0),
            edited,
            palette.dim,
        );
    }
    if let Some(failed) = failed {
        x -= failed.size().x + 6.0;
        let label = Rect::from_min_size(
            pos2(x, rect.center().y - failed.size().y / 2.0),
            failed.size(),
        );
        ui.painter().galley(label.min, failed, palette.text);
        // Explain on hover and to screen readers; hovering takes no clicks
        // from the bubble.
        let status = Rect::from_min_max(label.min, pos2(rect.right(), label.max.y));
        let response = ui.interact(
            status,
            ui.id().with(("not-sent", &message.id)),
            Sense::hover(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, true, NOT_SENT_HINT)
        });
        response.on_hover_text(NOT_SENT_HINT);
    }
    if starred {
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::StarFill,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, rect.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            FOOTER_STAR,
        );
    }
}

fn reference_footer_marks(
    ui: &mut egui::Ui,
    message: &Message,
    slot: Option<Rect>,
    starred: bool,
    pinned: bool,
    palette: &Palette,
) {
    let width = message_footer_width(ui, message, starred, pinned, palette, false);
    if width <= 0.0 {
        return;
    }
    let rect = match slot {
        Some(slot) => slot,
        None => {
            let row_width = ui.min_rect().width().max(width);
            let (rect, _) = ui.allocate_exact_size(vec2(row_width, 15.0), Sense::hover());
            rect
        }
    };
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            footer_id(&message.chat, &message.id),
            Rect::from_min_max(pos2(rect.right() - width, rect.top()), rect.max),
        );
    });
    let mut x = rect.right();
    if pinned {
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::Pin,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, rect.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            palette.secondary,
        );
    }
    if message.edited {
        let galley =
            ui.painter()
                .layout_no_wrap("edited".to_owned(), theme::regular(11.0), palette.dim);
        x -= galley.size().x + 4.0;
        ui.painter().galley(
            pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            palette.dim,
        );
    }
    if starred {
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::StarFill,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, rect.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            FOOTER_STAR,
        );
    }
}

/// The message's reactions as one pill: its four most used emoji, with no
/// counts. Clicking it lists who reacted with what.
fn reactions(ui: &mut egui::Ui, view: &View<'_>, message: &Message, actions: &mut Vec<Action>) {
    let palette = view.palette;
    let counts = super::reaction_list::tally(message);
    let mine = message.reactions.iter().any(|reaction| reaction.from_me);
    // The pill pops in when a reaction arrives live and pops again when its
    // emoji or counts change. The first sight of a message only records.
    let now = ui.input(|input| input.time);
    let calm = crate::motion::screen_reader_attached(ui.ctx());
    let tally_id = egui::Id::new(("reaction-tally", &view.chat.id, &message.id));
    let motion_id = egui::Id::new(("reaction-motion", &view.chat.id, &message.id));
    let frame = ui.ctx().cumulative_frame_nr();
    let previous = seen_last_frame(
        ui.ctx()
            .data(|data| data.get_temp::<(u64, Vec<(String, u32)>)>(tally_id)),
        frame,
    );
    let change = reaction_change(previous.as_deref(), &counts);
    ui.ctx().data_mut(|data| {
        data.insert_temp(tally_id, (frame, counts.clone()));
        if change != ReactionChange::None && !calm {
            data.insert_temp(motion_id, (now, change == ReactionChange::Arrived));
        }
    });
    let motion: Option<(f64, bool)> = ui.ctx().data(|data| data.get_temp(motion_id));
    let live = motion.filter(|(start, _)| now - start < REACTION_POP_SECONDS && !calm);
    let old_opacity = ui.opacity();
    let first_shape = ui.ctx().graphics(|graphics| {
        graphics
            .get(ui.layer_id())
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    let progress = live.map(|(start, arrived)| {
        let (stiffness, damping) = spring_from_duration(350.0, 0.3);
        let spring = spring_unit((now - start) * 1000.0, stiffness, damping) as f32;
        let fade = theme::ease_out(((now - start) as f32 / theme::MOTION_QUICK).clamp(0.0, 1.0));
        (spring, arrived, fade)
    });
    if let Some((_, true, fade)) = progress {
        ui.multiply_opacity(fade);
    }
    let shown: Vec<&str> = counts
        .iter()
        .take(4)
        .map(|(emoji, _)| emoji.as_str())
        .collect();
    let line = widgets::line(
        ui,
        &shown.join(" "),
        theme::regular(13.5),
        palette.text,
        240.0,
        1,
    );
    let size = line.size() + vec2(18.0, 10.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            bubble_id(&view.chat.id, &message.id).with("pill-rect"),
            rect,
        )
    });
    if ui.is_rect_visible(rect) {
        // Lifted as gently as a hovered chat-list row, so the pill
        // stands off the bubble it overlaps.
        let mut shadow = palette.bubble_shadow();
        shadow.color = shadow.color.gamma_multiply(0.6);
        ui.painter()
            .add(shadow.as_shape(rect, CornerRadius::from(rect.height() / 2.0)));
        // No outline: a pill holding our own reaction is tinted with the accent.
        ui.painter().rect_filled(
            rect,
            rect.height() / 2.0,
            if mine {
                palette.overlay.lerp_to_gamma(palette.accent, 0.35)
            } else {
                palette.overlay
            },
        );
        line.paint(ui, rect.center() - line.size() / 2.0, palette.text);
    }
    let names: Vec<String> = message
        .reactions
        .iter()
        .map(|reaction| {
            let who = if reaction.from_me {
                "You".to_owned()
            } else {
                (view.names_or)(&reaction.sender, None)
            };
            format!("{} {who}", reaction.emoji)
        })
        .collect();
    if let Some((spring, arrived, _)) = progress {
        let from = if arrived { 0.8 } else { 0.92 };
        let scale = from + (1.0 - from) * spring;
        let pivot = if message.from_me {
            rect.right_bottom()
        } else {
            rect.left_bottom()
        };
        let transform = egui::emath::TSTransform::from_translation(pivot.to_vec2())
            * egui::emath::TSTransform::from_scaling(scale)
            * egui::emath::TSTransform::from_translation(-pivot.to_vec2());
        let layer = ui.layer_id();
        ui.ctx().graphics_mut(|graphics| {
            let list = graphics.entry(layer);
            let end = list.next_idx();
            for index in first_shape.0..end.0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    clipped.shape.transform(transform);
                });
            }
        });
        ui.ctx().request_repaint();
    }
    ui.set_opacity(old_opacity);
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(names.join("\n"));
    if response.clicked() {
        actions.push(super::reaction_list::open(&view.chat.id, &message.id));
    }
}

/// Records that a message was drawn with no reactions.
fn note_no_reactions(ui: &egui::Ui, view: &View<'_>, message: &Message) {
    let tally_id = egui::Id::new(("reaction-tally", &view.chat.id, &message.id));
    let frame = ui.ctx().cumulative_frame_nr();
    ui.ctx()
        .data_mut(|data| data.insert_temp(tally_id, (frame, Vec::<(String, u32)>::new())));
}

/// How long a reaction pill's pop runs.
const REACTION_POP_SECONDS: f64 = 0.35;

/// The value recorded for a row when it was last drawn, if that was the frame
/// before this one (or this frame). A row that was off screen since, scrolled
/// back into view, has no earlier state: a change made while it was away is
/// not a live one, so nothing pops for it.
fn seen_last_frame<T>(stored: Option<(u64, T)>, frame: u64) -> Option<T> {
    stored.and_then(|(at, value)| (frame.saturating_sub(at) <= 1).then_some(value))
}

/// What a message's reaction tally did since it was last drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReactionChange {
    None,
    Arrived,
    Changed,
}

/// Compares the tally drawn before with the one now. No earlier tally means
/// the message was only just seen, which is not a change.
fn reaction_change(previous: Option<&[(String, u32)]>, now: &[(String, u32)]) -> ReactionChange {
    match previous {
        None => ReactionChange::None,
        Some(before) if before == now => ReactionChange::None,
        Some([]) if !now.is_empty() => ReactionChange::Arrived,
        Some(_) => ReactionChange::Changed,
    }
}

const QUICK_REACTIONS: [&str; 6] = ["👍", "❤️", "😂", "😮", "😢", "🙏"];

/// Our existing reaction to a message.
pub(crate) fn own_reaction(message: &Message) -> Option<&str> {
    message
        .reactions
        .iter()
        .find(|reaction| reaction.from_me)
        .map(|reaction| reaction.emoji.as_str())
}

/// Emoji to send for a reaction choice: empty string removes our current one.
pub(crate) fn reaction_choice(current: Option<&str>, emoji: &str) -> String {
    if current == Some(emoji) {
        String::new()
    } else {
        emoji.to_owned()
    }
}

/// Width a message menu needs: its longest entry, or the quick-reaction row
/// when that is wider. Both the bubble and a starred row use it, so the two
/// menus come out the same size.
pub(crate) fn menu_width(
    ui: &egui::Ui,
    locale: crate::i18n::Locale,
    reaction_emoji: &[(String, u32)],
    menu: crate::settings::MessageMenu,
    message: &Message,
) -> f32 {
    let quick = quick_reactions(message, reaction_emoji).len() as f32 + 1.0;
    // Icons only is as wide as the reactions above it, one more point so a
    // row of buttons as long as theirs does not wrap on rounding.
    if menu.layout == crate::settings::MenuLayout::Icons {
        return quick * 36.0 + 13.0;
    }
    let labels = message_menu_labels(locale);
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    widgets::menu_width(ui, &refs, true).max(quick * 36.0 + 12.0)
}

/// Every label the menu of a message other than a call can show, in `locale`.
/// The menu is as wide as the longest one, and which one that is depends on
/// the language: a label left out here is cut when its translation is the
/// longest.
fn message_menu_labels(locale: crate::i18n::Locale) -> Vec<String> {
    use crate::i18n::gettext;
    // Delete's two choices are a submenu's, which is as wide as they are.
    [
        gettext(locale, "Reply"),
        gettext(locale, "Reply privately"),
        gettext(locale, "Star"),
        gettext(locale, "Unstar"),
        gettext(locale, "Pin message"),
        gettext(locale, "Unpin message"),
        gettext(locale, "Forward"),
        gettext(locale, "Select"),
        gettext(locale, "Copy text"),
        gettext(locale, "Edit"),
        gettext(locale, "Delete"),
        gettext(locale, "Message info"),
        gettext(locale, "Add to favorites"),
        gettext(locale, "Copy image"),
        gettext(locale, "Save as…"),
        gettext(locale, "Show in folder"),
        gettext(locale, "Downloading…"),
        gettext(locale, "Download"),
    ]
    .into_iter()
    .map(|label| label.into_owned())
    .collect()
}

/// Where a message menu was opened from. A starred row has no composer, no
/// selection bar, and no revoke window of its own, so the entries that need one
/// ask for the chat to be opened at the message first.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuOrigin {
    Chat,
    List,
}

/// How a left-list row's message is marked, for the menu's Star label.
#[derive(Clone, Copy)]
pub(crate) struct Marks {
    pub starred: bool,
}

/// The message menu for a row drawn outside the conversation: the starred
/// list. It is the chat's own menu, with the marks the list already knows, so
/// both places offer exactly the same entries.
pub(crate) fn list_menu(
    app: &App,
    ui: &mut egui::Ui,
    chat: &Chat,
    message: &Message,
    marks: Marks,
    actions: &mut Vec<Action>,
) {
    let stars: HashSet<String> = marks
        .starred
        .then(|| message.id.clone())
        .into_iter()
        .collect();
    let names_or = |id: &str, hint: Option<&str>| app.display_name_or(id, hint);
    let mention_names = |id: &str| app.mention_name(id);
    let avatars = HashMap::new();
    let keyboard_navigation = std::cell::Cell::new(false);
    let view = View {
        palette: app.palette,
        locale: app.locale,
        chat,
        me: app.me.as_deref(),
        auto_download: app.settings.auto_download,
        connected: app.link.is_connected(),
        poll_voting: &app.poll_voting,
        interactive_pending: &app.interactive_sending,
        // Nothing scrolls, highlights, or selects in a list row.
        anchor: None,
        open_menu: None,
        reaction: None,
        reaction_menu: false,
        reaction_emoji: &app.settings.reaction_emoji,
        menu: app.settings.message_menu,
        keyboard_navigation: &keyboard_navigation,
        names_or: &names_or,
        mention_names: &mention_names,
        avatars: &avatars,
        contacts: &app.contacts,
        now: crate::util::now(),
        // A row drawn outside the conversation holds still.
        animate: false,
        player: &app.player,
        voice_speed: app.voice_speed_in(Some(&chat.id)),
        time_inside: app.palette.bubbles != theme::BubbleStyle::Nothing
            || app.settings.always_show_time,
        video: &app.video,
        account_receipts_off: app.account_receipts_off,
        copy_rows: Some(app.copy_rows.as_ref()),
        swipes: app.settings.swipes,
        doodles: app.doodles_behind(),
        scrims: None,
        rows: None,
        selecting: false,
        selected: &[],
        motion_playing: None,
        starred: marks.starred.then_some(&stars),
        pins: app.pins.get(&message.chat),
        pin_notices: &[],
    };
    context_menu(ui, &view, message, &[], MenuOrigin::List, actions);
}

/// Quick reactions plus our current reaction when needed.
fn quick_reactions<'a>(message: &'a Message, preferred: &'a [(String, u32)]) -> Vec<&'a str> {
    let mut list = Vec::new();
    for emoji in preferred
        .iter()
        .map(|(emoji, _)| emoji.as_str())
        .chain(QUICK_REACTIONS.iter().copied())
    {
        if emojis::get(emoji).is_some() && !list.contains(&emoji) {
            list.push(emoji);
        }
        if list.len() == QUICK_REACTIONS.len() {
            break;
        }
    }
    if let Some(mine) = own_reaction(message)
        && !list.contains(&mine)
    {
        list.push(mine);
    }
    list
}

/// Checks every selected message's revoke eligibility with one scan of loaded history.
fn selection_can_revoke(messages: &[Message], selected: &[String], now: i64) -> bool {
    if selected.is_empty() {
        return false;
    }
    let mut remaining: std::collections::HashSet<&str> =
        selected.iter().map(String::as_str).collect();
    for message in messages {
        if remaining.remove(message.id.as_str()) {
            if !message.from_me
                || matches!(message.content, Content::Revoked)
                || now - message.timestamp > crate::app::REVOKE_WINDOW.as_secs() as i64
            {
                return false;
            }
            if remaining.is_empty() {
                return true;
            }
        }
    }
    false
}

/// Draws batch actions for the selected messages, including separate deletion choices.
fn selection_menu(
    ui: &mut egui::Ui,
    view: &View<'_>,
    messages: &[Message],
    selected: &[String],
    actions: &mut Vec<Action>,
) {
    selection_delete_items(
        ui,
        &view.palette,
        view.locale,
        &view.chat.id,
        selection_can_revoke(messages, selected, view.now),
        selected,
        actions,
    );
}

/// The two ways to delete a selection, shared by the selection's context
/// menu and the selection bar's Delete menu. Each opens the confirmation.
fn selection_delete_items(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: crate::i18n::Locale,
    chat: &str,
    can_revoke: bool,
    selected: &[String],
    actions: &mut Vec<Action>,
) {
    for (label, for_everyone, enabled) in [
        (
            crate::i18n::gettext(locale, "Delete for everyone"),
            true,
            can_revoke,
        ),
        (
            crate::i18n::gettext(locale, "Delete for me"),
            false,
            !selected.is_empty(),
        ),
    ] {
        if widgets::menu_item_enabled(ui, palette, Some(Icon::Trash), &label, enabled) {
            actions.push(Action::ShowDialog(Dialog::ConfirmDeleteSelection {
                chat: chat.to_owned(),
                messages: selected.to_vec(),
                for_everyone,
            }));
        }
    }
}

/// Delay between one quick reaction starting to spring in and the next.
const QUICK_STAGGER: f32 = 0.025;
/// Length of a quick reaction's spring, in seconds.
const QUICK_SPRING_SECONDS: f32 = 0.35;
/// Bounce of a quick reaction's spring.
const QUICK_SPRING_BOUNCE: f64 = 0.3;
/// Size of the quick reaction under the pointer.
const QUICK_HOVER_SCALE: f32 = 1.3;
/// Size of the quick reaction while it is pressed.
const QUICK_PRESS_SCALE: f32 = 0.9;

/// Seconds since the quick reactions were first drawn for this menu. A gap
/// of a few frames between draws means the menu was closed, so it starts over.
fn quick_reactions_opened(ui: &egui::Ui, calm: bool) -> f32 {
    if calm {
        return 0.0;
    }
    let now = ui.input(|input| input.time);
    let id = ui.id().with("quick-reactions-open");
    let (start, _) = ui
        .ctx()
        .data(|data| data.get_temp::<(f64, f64)>(id))
        .filter(|(_, seen)| now - seen < 0.25)
        .unwrap_or((now, now));
    ui.ctx().data_mut(|data| data.insert_temp(id, (start, now)));
    let elapsed = (now - start) as f32;
    if elapsed < QUICK_SPRING_SECONDS + QUICK_STAGGER * 8.0 {
        ui.ctx().request_repaint();
    }
    elapsed
}

/// How far along the spring the `index`th quick reaction is, 0 to 1 with
/// a little overshoot, `elapsed` seconds after the menu opened.
fn quick_reaction_progress(elapsed: f32, index: usize) -> f32 {
    let local = elapsed - QUICK_STAGGER * index as f32;
    if local <= 0.0 {
        return 0.0;
    }
    if local >= QUICK_SPRING_SECONDS * 1.5 {
        return 1.0;
    }
    let (stiffness, damping) = spring_from_duration(
        f64::from(QUICK_SPRING_SECONDS) * 1000.0,
        QUICK_SPRING_BOUNCE,
    );
    spring_unit(f64::from(local) * 1000.0, stiffness, damping) as f32
}

/// The spring-driven size of a quick reaction chasing `target`, kept per
/// emoji so a new target starts from where it is now.
fn quick_reaction_lift(ctx: &egui::Context, id: egui::Id, target: f32, calm: bool) -> f32 {
    if calm {
        return target;
    }
    let id = id.with("lift");
    let (mut value, mut velocity) = ctx
        .data(|data| data.get_temp::<(f32, f32)>(id))
        .unwrap_or((1.0, 0.0));
    let dt = ctx.input(|input| input.stable_dt).min(0.05);
    let (stiffness, damping) = spring_from_duration(
        f64::from(QUICK_SPRING_SECONDS) * 1000.0,
        QUICK_SPRING_BOUNCE,
    );
    // The constants already read in seconds, as `spring_unit` scales its ms.
    let (stiffness, damping) = (stiffness as f32, damping as f32);
    let steps = 4;
    let h = dt / steps as f32;
    for _ in 0..steps {
        let pull = stiffness * (target - value) - damping * velocity;
        velocity += pull * h;
        value += velocity * h;
    }
    let settled = (value - target).abs() < 0.001 && velocity.abs() < 0.01;
    if settled {
        value = target;
        velocity = 0.0;
    } else {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(id, (value, velocity)));
    value
}

/// Offers actions supported by the selected message, including private replies for eligible group senders.
fn context_menu(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: &[Message],
    origin: MenuOrigin,
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    let chat = &view.chat.id;
    if let Content::Call { group, .. } = message.content {
        call_menu(ui, view, group, actions);
        return;
    }
    let mine = own_reaction(message);
    let (opened, calm) = ui
        .allocate_ui_with_layout(
            vec2(ui.available_width(), 34.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let calm = crate::motion::screen_reader_attached(ui.ctx());
                let opened = quick_reactions_opened(ui, calm);
                for (index, emoji) in quick_reactions(message, view.reaction_emoji)
                    .into_iter()
                    .enumerate()
                {
                    let chosen = mine == Some(emoji);
                    let line =
                        widgets::line(ui, emoji, theme::regular(20.0), palette.text, 40.0, 1);
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                    let first_shape = super::swipe::mark(ui);
                    let old_opacity = ui.opacity();
                    let entry = if calm {
                        1.0
                    } else {
                        quick_reaction_progress(opened, index)
                    };
                    let target = if response.is_pointer_button_down_on() {
                        QUICK_PRESS_SCALE
                    } else if response.hovered() {
                        QUICK_HOVER_SCALE
                    } else {
                        1.0
                    };
                    let lift = quick_reaction_lift(ui.ctx(), response.id, target, calm);
                    ui.set_opacity(old_opacity * entry.clamp(0.0, 1.0));
                    theme::focus_outline(ui, response.id, rect, 17.0);
                    if chosen {
                        ui.painter()
                            .circle_filled(rect.center(), 17.0, palette.surface_active);
                        ui.painter().circle_stroke(
                            rect.center(),
                            16.0,
                            Stroke::new(1.5, palette.accent),
                        );
                    } else if response.hovered() {
                        ui.painter()
                            .circle_filled(rect.center(), 17.0, palette.surface_hover);
                    }
                    line.paint(ui, rect.center() - line.size() / 2.0, palette.text);
                    ui.set_opacity(old_opacity);
                    let scale = (0.4 + 0.6 * entry) * lift;
                    if (scale - 1.0).abs() > f32::EPSILON {
                        let centre = rect.center().to_vec2();
                        let transform =
                            egui::emath::TSTransform::new(centre * (1.0 - scale), scale);
                        let layer = ui.layer_id();
                        ui.ctx().graphics_mut(|graphics| {
                            let list = graphics.entry(layer);
                            for shape in first_shape.0..list.next_idx().0 {
                                list.mutate_shape(egui::layers::ShapeIdx(shape), |clipped| {
                                    clipped.shape.transform(transform);
                                });
                            }
                        });
                    }
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let response = if chosen {
                        response.on_hover_text(
                            crate::i18n::gettext(view.locale, "Remove your reaction").into_owned(),
                        )
                    } else {
                        response
                    };
                    if response.clicked() {
                        crate::haptics::tap(crate::haptics::Feel::Alignment);
                        // Selecting our current reaction removes it.
                        actions.push(Action::React {
                            chat: chat.clone(),
                            message: message.id.clone(),
                            emoji: reaction_choice(mine, emoji),
                        });
                        ui.close();
                    }
                }
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                theme::focus_outline(ui, response.id, rect, 17.0);
                if ui.is_rect_visible(rect) {
                    let hovered = response.hovered();
                    ui.painter().circle_filled(
                        rect.center(),
                        17.0,
                        if hovered {
                            palette.surface_hover
                        } else {
                            palette.surface
                        },
                    );
                    ui.painter().circle_stroke(
                        rect.center(),
                        16.0,
                        Stroke::new(1.0, palette.outline),
                    );
                    theme::paint_icon(ui, Icon::Plus, rect, 16.0, palette.secondary);
                }
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(
                        crate::i18n::gettext(view.locale, "React with any emoji").into_owned(),
                    )
                    .clicked()
                {
                    actions.push(Action::OpenReactionPicker {
                        chat: chat.clone(),
                        message: message.id.clone(),
                        beside_menu: true,
                    });
                }
                (opened, calm)
            },
        )
        .inner;
    widgets::menu_separator(ui, &palette);
    let mut rows = MenuRows {
        icons: view.menu.layout == crate::settings::MenuLayout::Icons,
        opened,
        calm,
        // The icons come in after the reactions, each a little later.
        index: quick_reactions(message, view.reaction_emoji).len() + 1,
    };
    if rows.icons {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            message_rows(ui, view, message, album, origin, &mut rows, actions);
        });
    } else {
        message_rows(ui, view, message, album, origin, &mut rows, actions);
    }
}

/// The actions of a message's menu below its reactions, in the layout the
/// settings chose and with only the rows they leave on.
fn message_rows(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: &[Message],
    origin: MenuOrigin,
    rows: &mut MenuRows,
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    let chat = &view.chat.id;
    let menu = view.menu;
    // Answering first: Reply and a private reply, and Edit. Then what to do
    // with its contents, keeping it, and its media, with deleting it set
    // apart below. Replying and forwarding are off until Settings turns them
    // on: they have their own ways in (a double click or a swipe, the
    // selection bar, a Ctrl-click).
    let actions_from = ui.cursor().min.y;
    let age = view.now - message.timestamp;
    let can_edit = message.from_me
        && matches!(message.content, Content::Text { .. })
        && age <= crate::app::EDIT_WINDOW.as_secs() as i64;
    let can_revoke = message.from_me
        && !matches!(message.content, Content::Revoked)
        && age <= crate::app::REVOKE_WINDOW.as_secs() as i64;
    if menu.reply
        && !matches!(message.content, Content::Revoked)
        && rows.item(
            ui,
            &palette,
            Icon::Reply,
            &crate::i18n::gettext(view.locale, "Reply"),
        )
    {
        if matches!(origin, MenuOrigin::List) {
            actions.push(Action::OpenMessage {
                chat: chat.clone(),
                message: message.id.clone(),
            });
        }
        actions.push(Action::Reply(message.id.clone()));
    }
    // From the starred list the source chat is not on screen, and the
    // private reply starts from the group the reader is looking at.
    if menu.reply_privately
        && !matches!(origin, MenuOrigin::List)
        && message.private_reply_recipient().is_some()
        && rows.item(
            ui,
            &palette,
            Icon::Reply,
            &crate::i18n::gettext(view.locale, "Reply privately"),
        )
    {
        actions.push(Action::ReplyPrivately {
            chat: chat.clone(),
            message: message.id.clone(),
        });
    }
    if menu.edit
        && can_edit
        && rows.item(
            ui,
            &palette,
            Icon::Pencil,
            &crate::i18n::gettext(view.locale, "Edit"),
        )
    {
        if matches!(origin, MenuOrigin::List) {
            actions.push(Action::OpenMessage {
                chat: chat.clone(),
                message: message.id.clone(),
            });
        }
        actions.push(Action::Edit(message.id.clone()));
    }
    if menu.forward
        && crate::app::can_select(&message.content)
        && rows.item(
            ui,
            &palette,
            Icon::Forward,
            &crate::i18n::gettext(view.locale, "Forward"),
        )
    {
        actions.push(Action::ShowDialog(Dialog::Forward {
            chat: chat.clone(),
            messages: vec![message.id.clone()],
        }));
    }
    let text = match &message.content {
        Content::Text { text, .. } | Content::Interactive { text, .. } => Some(text.clone()),
        Content::Image { caption, .. }
        | Content::Video { caption, .. }
        | Content::Document { caption, .. } => caption.clone(),
        Content::Location {
            latitude,
            longitude,
            ..
        }
        | Content::LiveLocation {
            latitude,
            longitude,
            ..
        } => Some(format!("{latitude},{longitude}")),
        Content::Contact { vcard, .. } => Some(vcard.clone()),
        _ => None,
    };
    if menu.copy_text
        && let Some(text) = text
        && rows.item(
            ui,
            &palette,
            Icon::Copy,
            &crate::i18n::gettext(view.locale, "Copy text"),
        )
    {
        let mentions = mentions_of(view, message);
        actions.push(Action::CopyText(markup::plain(&text, &mentions)));
    }
    if menu.select
        && crate::app::can_select(&message.content)
        && rows.item(
            ui,
            &palette,
            Icon::Check,
            &crate::i18n::gettext(view.locale, "Select"),
        )
    {
        if matches!(origin, MenuOrigin::List) {
            actions.push(Action::OpenMessage {
                chat: chat.clone(),
                message: message.id.clone(),
            });
        }
        actions.push(Action::SelectMessage(message.id.clone()));
    }
    let starred = view.starred.is_some_and(|ids| ids.contains(&message.id));
    let star_label = if starred {
        crate::i18n::gettext(view.locale, "Unstar")
    } else {
        crate::i18n::gettext(view.locale, "Star")
    };
    if menu.star
        && !matches!(message.content, Content::Revoked)
        && rows.item(ui, &palette, Icon::Star, &star_label)
    {
        actions.push(Action::SetStar {
            chat: chat.clone(),
            message: message.id.clone(),
            starred: !starred,
        });
        ui.close();
    }
    let pinned = view.pins.is_some_and(|ids| ids.contains(&message.id));
    let pin_label = if pinned {
        crate::i18n::gettext(view.locale, "Unpin message")
    } else {
        crate::i18n::gettext(view.locale, "Pin message")
    };
    if menu.pin
        && !matches!(message.content, Content::Revoked)
        && rows.item(ui, &palette, Icon::Pin, &pin_label)
    {
        actions.push(Action::SetMessagePinned {
            chat: chat.clone(),
            message: message.id.clone(),
            pinned: !pinned,
        });
    }
    if let Some(media) = message.content.media() {
        match &media.path {
            Some(path) => {
                if menu.copy_image
                    && matches!(message.content, Content::Image { .. })
                    && rows.item(
                        ui,
                        &palette,
                        Icon::Copy,
                        &crate::i18n::gettext(view.locale, "Copy image"),
                    )
                {
                    actions.push(Action::CopyImage(path.clone()));
                }
                // In an album the same menu saves the one under the pointer or
                // every member fetched so far.
                let files: Vec<(PathBuf, String)> = album
                    .iter()
                    .filter_map(|member| {
                        let path = member.content.media()?.path.as_ref()?;
                        Some((path.clone(), attachment_name(&member.content, path)))
                    })
                    .collect();
                let in_album = album.len() > 1;
                let save = if !in_album {
                    crate::i18n::gettext(view.locale, "Save as…")
                } else if matches!(message.content, Content::Image { .. }) {
                    crate::i18n::gettext(view.locale, "Save this image")
                } else {
                    crate::i18n::gettext(view.locale, "Save this item")
                };
                if menu.save && rows.item(ui, &palette, Icon::Download, &save) {
                    actions.push(Action::SaveAttachmentAs {
                        path: path.clone(),
                        name: attachment_name(&message.content, path),
                    });
                }
                if menu.save
                    && in_album
                    && files.len() > 1
                    && rows.item(
                        ui,
                        &palette,
                        Icon::Download,
                        &crate::i18n::gettext(
                            view.locale,
                            if album
                                .iter()
                                .all(|member| matches!(member.content, Content::Image { .. }))
                            {
                                "Save all images"
                            } else {
                                "Save all items"
                            },
                        ),
                    )
                {
                    actions.push(Action::SaveAttachments { files });
                }
                if menu.show_in_folder
                    && let Some(folder) = path.parent()
                    && rows.item(
                        ui,
                        &palette,
                        Icon::FileText,
                        &crate::i18n::gettext(view.locale, "Show in folder"),
                    )
                {
                    actions.push(Action::OpenFolder(folder.to_path_buf()));
                }
            }
            None => {
                let downloading = matches!(media.state, MediaState::Downloading);
                if rows.item_enabled(
                    ui,
                    &palette,
                    Icon::Download,
                    &if downloading {
                        crate::i18n::gettext(view.locale, "Downloading…")
                    } else {
                        crate::i18n::gettext(view.locale, "Download")
                    },
                    !downloading,
                ) {
                    actions.push(Action::Download {
                        manual: true,
                        card: None,
                        chat: chat.clone(),
                        message: message.id.clone(),
                    });
                }
            }
        }
    }
    if let Content::Audio { media, .. } = &message.content
        && media.path.is_some()
    {
        speed_menu_row(ui, view, message, actions);
    }
    if let Content::Sticker { media, .. } = &message.content
        && let Some(path) = &media.path
        && rows.item(
            ui,
            &palette,
            Icon::Star,
            &crate::i18n::gettext(view.locale, "Add to favorites"),
        )
    {
        actions.push(Action::SaveSticker(path.clone()));
    }
    // A message with nothing above, such as a deleted one, keeps a single
    // line between its reactions and Delete.
    if ui.cursor().min.y > actions_from {
        rows.separator(ui, &palette);
    }
    if menu.message_info && has_message_info(message) {
        if rows.item(
            ui,
            &palette,
            Icon::Info,
            &crate::i18n::gettext(view.locale, "Message info"),
        ) {
            actions.push(Action::ShowDialog(Dialog::MessageInfo {
                chat: chat.clone(),
                message: message.id.clone(),
            }));
        }
        rows.separator(ui, &palette);
    }
    delete_submenu(ui, view, message, can_revoke, rows, actions);
}

/// The menu's Delete row, which opens the ways to delete `message`: for me,
/// then for everyone while it can still be revoked. Each asks to confirm. In
/// the icons layout it is a trash button that opens the same two choices.
fn delete_submenu(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    can_revoke: bool,
    rows: &mut MenuRows,
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    let for_me = crate::i18n::gettext(view.locale, "Delete for me");
    let for_everyone = crate::i18n::gettext(view.locale, "Delete for everyone");
    let mut items: Vec<&str> = vec![&*for_me];
    if can_revoke {
        items.push(&*for_everyone);
    }
    let choices = |ui: &mut egui::Ui| {
        if widgets::menu_item(ui, &palette, Some(Icon::EyeOff), &for_me) {
            actions.push(Action::ShowDialog(Dialog::ConfirmDeleteMessage {
                chat: view.chat.id.clone(),
                message: message.id.clone(),
                for_everyone: false,
            }));
        }
        if can_revoke && widgets::menu_item(ui, &palette, Some(Icon::Trash), &for_everyone) {
            actions.push(Action::ShowDialog(Dialog::ConfirmDeleteMessage {
                chat: view.chat.id.clone(),
                message: message.id.clone(),
                for_everyone: true,
            }));
        }
    };
    let label = crate::i18n::gettext(view.locale, "Delete");
    if rows.icons {
        let response = rows.icon_button(ui, &palette, Icon::Trash, &label, true);
        widgets::submenu_of(ui, &response, &items, choices);
    } else {
        widgets::submenu(ui, &palette, Icon::Trash, &label, &items, choices);
    }
}

/// How the rows of a message menu are drawn: one per line with its name, or
/// as icon-only buttons in a wrapped row.
struct MenuRows {
    icons: bool,
    /// Seconds since the menu opened, for the icons' entrance.
    opened: f32,
    /// Whether the system asks for calm: the icons then appear at once.
    calm: bool,
    /// The icon's place in the entrance, counted after the reactions.
    index: usize,
}

impl MenuRows {
    fn item(&mut self, ui: &mut egui::Ui, palette: &Palette, icon: Icon, label: &str) -> bool {
        self.item_enabled(ui, palette, icon, label, true)
    }

    fn item_enabled(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        icon: Icon,
        label: &str,
        enabled: bool,
    ) -> bool {
        if !self.icons {
            return widgets::menu_item_enabled(ui, palette, Some(icon), label, enabled);
        }
        let clicked = self
            .icon_button(ui, palette, icon, label, enabled)
            .clicked();
        if clicked {
            ui.close();
        }
        clicked
    }

    /// The line between groups of rows. Icons run on without one.
    fn separator(&self, ui: &mut egui::Ui, palette: &Palette) {
        if !self.icons {
            widgets::menu_separator(ui, palette);
        }
    }

    /// One icon-only button, its name in a tooltip. It comes in like the
    /// quick reactions do (a staggered spring), grows a little under the
    /// pointer, and shrinks while pressed, with the same springs.
    fn icon_button(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        icon: Icon,
        label: &str,
        enabled: bool,
    ) -> egui::Response {
        let index = self.index;
        self.index += 1;
        let (rect, response) = ui.allocate_exact_size(
            Vec2::splat(MENU_ICON_BUTTON),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        theme::reveal_focus(&response);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled && ui.is_enabled(), label)
        });
        let first_shape = super::swipe::mark(ui);
        let old_opacity = ui.opacity();
        let entry = if self.calm {
            1.0
        } else {
            quick_reaction_progress(self.opened, index)
        };
        let open = egui::Popup::is_id_open(
            ui.ctx(),
            egui::containers::menu::SubMenu::id_from_widget_id(response.id),
        );
        let target = if enabled && response.is_pointer_button_down_on() {
            QUICK_PRESS_SCALE
        } else if enabled && (response.hovered() || open) {
            MENU_ICON_HOVER_SCALE
        } else {
            1.0
        };
        let lift = quick_reaction_lift(ui.ctx(), response.id, target, self.calm);
        ui.set_opacity(old_opacity * entry.clamp(0.0, 1.0));
        if enabled && (response.hovered() || open) {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(theme::RADIUS),
                palette.surface_hover,
            );
        }
        theme::paint_icon(
            ui,
            icon,
            rect,
            18.0,
            if enabled { palette.text } else { palette.dim },
        );
        ui.set_opacity(old_opacity);
        let scale = (0.4 + 0.6 * entry) * lift;
        if (scale - 1.0).abs() > f32::EPSILON {
            let centre = rect.center().to_vec2();
            let transform = egui::emath::TSTransform::new(centre * (1.0 - scale), scale);
            let layer = ui.layer_id();
            ui.ctx().graphics_mut(|graphics| {
                let list = graphics.entry(layer);
                for shape in first_shape.0..list.next_idx().0 {
                    list.mutate_shape(egui::layers::ShapeIdx(shape), |clipped| {
                        clipped.shape.transform(transform);
                    });
                }
            });
        }
        if enabled {
            response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(label)
        } else {
            response
        }
    }
}

/// Side of an icon-only menu button.
const MENU_ICON_BUTTON: f32 = 34.0;
/// How far an icon-only button grows under the pointer.
const MENU_ICON_HOVER_SCALE: f32 = 1.12;

fn mentions_of(view: &View<'_>, message: &Message) -> Vec<markup::Mention> {
    message
        .mentions
        .iter()
        .map(|mention| markup::Mention {
            user: mention.user.clone(),
            name: (view.mention_names)(&mention.id),
        })
        .collect()
}

fn quote_mentions(view: &View<'_>, quoted: &crate::model::Quoted) -> Vec<markup::Mention> {
    quoted
        .mentions
        .iter()
        .map(|mention| markup::Mention {
            user: mention.user.clone(),
            name: (view.mention_names)(&mention.id),
        })
        .collect()
}

fn vcard_tel_preference(property: &str) -> Option<u8> {
    let mut best = None;
    for parameter in property.split(';').skip(1) {
        let (name, value) = parameter.split_once('=').unwrap_or(("TYPE", parameter));
        let value = value.trim_matches('"');
        let rank = if name.eq_ignore_ascii_case("PREF") {
            value
                .parse::<u8>()
                .ok()
                .filter(|rank| (1..=100).contains(rank))
        } else if name.eq_ignore_ascii_case("TYPE")
            && value
                .split(',')
                .any(|value| value.eq_ignore_ascii_case("PREF"))
        {
            Some(1)
        } else {
            None
        };
        if let Some(rank) = rank {
            best = Some(best.map_or(rank, |current: u8| current.min(rank)));
        }
    }
    best
}

/// The first card of a shared contact, as the bubble uses it.
#[derive(Debug, PartialEq)]
struct SharedContact {
    name: String,
    /// The telephone number as the card writes it.
    number: String,
    /// The WhatsApp account behind the number, in digits: the `waid`
    /// parameter WhatsApp adds, or a number written in international form.
    /// A local number without a country code cannot name one.
    account: Option<String>,
    /// The first name from the card's structured `N` property, middle names
    /// included, for the contact editor; none when the card has none.
    first_name: Option<String>,
}

/// The first name in a vCard `N` value
/// (`family;given;additional;prefix;suffix`), middle names included.
/// Components are split at unescaped semicolons only.
fn vcard_first_name(value: &str) -> Option<String> {
    let mut parts = vec![String::new()];
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        match character {
            '\\' => match chars.next() {
                Some('n' | 'N') => parts.last_mut()?.push(' '),
                Some(escaped) => parts.last_mut()?.push(escaped),
                None => {}
            },
            ';' => parts.push(String::new()),
            _ => parts.last_mut()?.push(character),
        }
    }
    let part = |index: usize| parts.get(index).map_or("", |part| part.trim());
    let first = [part(1), part(2)]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!first.is_empty()).then_some(first)
}

fn vcard_tel_account(property: &str, value: &str) -> Option<String> {
    let digits = |text: &str| {
        text.chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
    };
    let waid = property.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        name.eq_ignore_ascii_case("WAID")
            .then(|| digits(value.trim_matches('"')))
    });
    waid.or_else(|| value.trim().starts_with('+').then(|| digits(value)))
        .filter(|account| account.len() >= 7)
}

fn shared_contact_details(vcard: &str, fallback_name: &str) -> Option<SharedContact> {
    let mut name = None;
    let mut first_name = None;
    let mut first_phone = None;
    let mut preferred_phone: Option<(u8, (String, Option<String>))> = None;
    let mut first_card: Vec<String> = Vec::new();
    for line in vcard.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with([' ', '\t']) {
            if let Some(previous) = first_card.last_mut() {
                previous.push_str(line.trim_start());
            }
            continue;
        }
        if line.eq_ignore_ascii_case("END:VCARD") && !first_card.is_empty() {
            break;
        }
        if line.eq_ignore_ascii_case("BEGIN:VCARD") {
            if first_card.is_empty() {
                first_card.push(line.to_owned());
            }
            continue;
        }
        if !first_card.is_empty() {
            first_card.push(line.to_owned());
        }
    }
    if first_card.is_empty() {
        first_card.extend(vcard.lines().map(str::to_owned));
    }
    for line in first_card {
        let Some((property, value)) = line.split_once(':') else {
            continue;
        };
        let raw_name = property.split(';').next().unwrap_or(property);
        let property_name = raw_name.rsplit('.').next().unwrap_or(raw_name);
        if property_name.eq_ignore_ascii_case("FN") {
            name = Some(value.trim().to_owned());
        } else if property_name.eq_ignore_ascii_case("N") {
            first_name = vcard_first_name(value);
        } else if property_name.eq_ignore_ascii_case("TEL") {
            let number = value.trim().to_owned();
            if number.chars().filter(char::is_ascii_digit).count() < 7 {
                continue;
            }
            let phone = (number, vcard_tel_account(property, value));
            if let Some(rank) = vcard_tel_preference(property) {
                let replace = preferred_phone
                    .as_ref()
                    .is_none_or(|(best_rank, _)| rank < *best_rank);
                if replace {
                    preferred_phone = Some((rank, phone));
                }
            } else if first_phone.is_none() {
                first_phone = Some(phone);
            }
        }
    }
    let (number, account) = preferred_phone.map(|(_, phone)| phone).or(first_phone)?;
    let name = name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback_name.to_owned());
    Some(SharedContact {
        name,
        number,
        account,
        first_name,
    })
}

/// Draws a message body and returns optional footer space on its last line.
fn content(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    width: f32,
    reserve: f32,
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    let palette = view.palette;
    let own = message.from_me;
    // Nothing-style metadata lives below/on hover, so the text galley must
    // not reserve the usual inline timestamp gap on its final row.
    let footer_reserve = view.time_inside.then_some(reserve);
    // Add non-text messages to cross-message transcript copies.
    let has_body = match &message.content {
        Content::Text { .. } => true,
        // Image-only cards and carousels without a body draw no text.
        Content::Interactive { card, .. } => card.as_ref().is_none_or(|card| {
            !card.body.is_empty() || (card.image.is_none() && card.carousel.is_empty())
        }),
        Content::Image { caption, .. }
        | Content::Video { caption, .. }
        | Content::Document { caption, .. } => caption.is_some(),
        _ => false,
    };
    if !has_body && let Some(copy_rows) = view.copy_rows {
        copy_rows
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(transcript_row(view, message, String::new(), Vec::new()));
    }
    match &message.content {
        Content::Interactive { text, card } => {
            let Some(card) = card else {
                let span = message.quoted.is_some().then_some(width);
                return rich_body(
                    ui,
                    view,
                    message,
                    text,
                    width,
                    footer_reserve,
                    span,
                    actions,
                );
            };
            if !card.carousel.is_empty() {
                carousel(ui, view, message, card, width, actions);
                return None;
            }
            if let Some(image) = &card.image {
                picture(ui, view, message, image, width, None, actions);
                ui.add_space(4.0);
                if card.body.is_empty() {
                    return None;
                }
            }
            let body = if card.body.is_empty() {
                "Interactive message"
            } else {
                &card.body
            };
            rich_body(
                ui,
                view,
                message,
                body,
                width,
                footer_reserve,
                Some(width),
                actions,
            )
        }
        Content::Text { text, preview } => {
            if let Some(preview) = preview {
                preview_card(ui, view, message, preview, width, actions);
            }
            if let Some(footer) = inline_editor(ui, view, message, width, footer_reserve, actions) {
                return footer;
            }
            let span = (message.quoted.is_some() || preview.is_some()).then_some(width);
            rich_body(
                ui,
                view,
                message,
                text,
                width,
                footer_reserve,
                span,
                actions,
            )
        }
        Content::Image {
            caption,
            media,
            motion,
        } => {
            // A photo with a caption, a quote or a label reaches the
            // bubble's ring, as a quote does.
            let bleed = picture_bleed(ui, &palette, message, caption.is_some());
            let opacity = hide_source(ui, message);
            let drawn = Frame::new()
                .outer_margin(bleed)
                .show(ui, |ui| {
                    picture(
                        ui,
                        view,
                        message,
                        media,
                        width - bleed.sum().x,
                        None,
                        actions,
                    )
                })
                .inner;
            show_source(ui, opacity);
            if let Some(motion) = motion {
                motion_clip(ui, view, message, motion, drawn, actions);
            }
            ui.ctx().data_mut(|data| {
                data.insert_temp(bubble_id(&view.chat.id, &message.id).with("picture"), drawn);
            });
            let Some(caption) = caption else {
                // The time and ticks go over the picture's corner.
                return Some(drawn);
            };
            // The caption keeps the bubble's text padding inside the ring.
            let drawn = drawn.width() + bleed.sum().x;
            {
                // Wrap the caption to the settled image or quote width.
                let wrap = if message.quoted.is_some() {
                    width.max(drawn)
                } else {
                    drawn
                };
                rich_body(
                    ui,
                    view,
                    message,
                    caption,
                    wrap,
                    footer_reserve,
                    Some(wrap),
                    actions,
                )
            }
        }
        Content::Sticker { media, animated } => {
            let drawn = picture(ui, view, message, media, width, Some(*animated), actions);
            ui.ctx().data_mut(|data| {
                data.insert_temp(bubble_id(&view.chat.id, &message.id).with("picture"), drawn);
            });
            None
        }
        Content::Video {
            caption,
            media,
            seconds,
            gif,
            note,
        } => {
            let bleed = if *note || compact_attachment_message(message) {
                Margin::ZERO
            } else {
                picture_bleed(ui, &palette, message, caption.is_some())
            };
            let opacity = hide_source(ui, message);
            let shown = Frame::new().outer_margin(bleed).show(ui, |ui| {
                if *note {
                    video_note(ui, view, message, media, *seconds, actions)
                } else {
                    let width = width - bleed.sum().x;
                    video(ui, view, message, media, *seconds, *gif, width, actions)
                }
            });
            show_source(ui, opacity);
            // Its click plays it; the bubble's own click must not count it.
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    bubble_id(&view.chat.id, &message.id).with("picture"),
                    shown.response.rect,
                );
            });
            // The caption keeps to the text padding the video reaches past.
            let drawn = shown.inner + bleed.sum().x;
            caption.as_ref().and_then(|caption| {
                let wrap = if message.quoted.is_some() {
                    width.max(drawn)
                } else {
                    drawn
                };
                rich_body(
                    ui,
                    view,
                    message,
                    caption,
                    wrap,
                    footer_reserve,
                    Some(wrap),
                    actions,
                )
            })
        }
        Content::Audio {
            media,
            seconds,
            waveform,
            ..
        } => voice_player(
            ui, view, message, media, *seconds, waveform, width, reserve, actions,
        ),
        Content::Document {
            media,
            file_name,
            caption,
            pages,
        } => {
            let mut detail = Vec::new();
            if let Some(pages) = pages {
                detail.push(format!(
                    "{pages} page{}",
                    if *pages == 1 { "" } else { "s" }
                ));
            }
            detail.push(crate::util::bytes(media.size));
            attachment(
                ui,
                view,
                message,
                media,
                Icon::FileText,
                file_name,
                &detail.join(" · "),
                width,
                actions,
            );
            caption.as_ref().and_then(|caption| {
                rich_body(
                    ui,
                    view,
                    message,
                    caption,
                    width,
                    footer_reserve,
                    Some(width),
                    actions,
                )
            })
        }
        Content::Location {
            latitude,
            longitude,
            name,
            address,
        } => {
            let title = name
                .clone()
                .unwrap_or_else(|| crate::i18n::gettext(view.locale, "Location").into_owned());
            // A spot sent from here names no place, so the coordinates take the
            // line an address would.
            let detail = address.clone().or_else(|| {
                Some(
                    Spot {
                        latitude: *latitude,
                        longitude: *longitude,
                    }
                    .text(),
                )
            });
            location_card(
                ui,
                view,
                message,
                &message.id,
                &title,
                detail,
                None,
                (*latitude, *longitude),
                actions,
            );
            None
        }
        Content::LiveLocation {
            latitude,
            longitude,
            accuracy_m,
            speed_mps,
            sequence,
            updated,
            newer_on_phone,
            ..
        } => {
            let over = message
                .content
                .live_location_over(message.timestamp, view.now);
            if !over {
                let until_expiry =
                    (message.timestamp + crate::model::LIVE_LOCATION_LIMIT - view.now).max(1);
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs(until_expiry as u64));
            }
            let title = if over {
                crate::i18n::gettext(view.locale, "Live location ended")
            } else {
                crate::i18n::gettext(view.locale, "Live location")
            };
            let detail = (!over).then(|| {
                // An absolute time stays true without repainting.
                let at = if *updated > 0 {
                    *updated
                } else {
                    message.timestamp
                };
                let mut meta = vec![
                    crate::i18n::gettext(view.locale, "Updated {time}")
                        .replace("{time}", &crate::util::clock(at)),
                ];
                if let Some(speed) = speed_mps.filter(|speed| *speed >= 0.5) {
                    meta.push(format!("{:.0} km/h", speed * 3.6));
                }
                if let Some(accuracy) = accuracy_m {
                    meta.push(format!("±{accuracy} m"));
                }
                meta.join(" · ")
            });
            // The phone holds the positions that follow this one: WhatsApp
            // keeps live locations off linked devices, so say where the rest
            // of the share is instead of letting the pin look live.
            let note = (!over && *newer_on_phone).then(|| {
                crate::i18n::gettext(view.locale, "Open WhatsApp on your phone to follow it.")
                    .into_owned()
            });
            // Each position gets its own preview, as a later one replaces the first.
            let key = format!("{}-{sequence}-{updated}", message.id);
            location_card(
                ui,
                view,
                message,
                &key,
                &title,
                detail,
                note,
                (*latitude, *longitude),
                actions,
            );
            None
        }
        Content::Contact {
            display_name,
            vcard,
        } => {
            let icon = |ui: &mut egui::Ui| {
                theme::icon(ui, Icon::Contact, 18.0, palette.accent);
            };
            let details = shared_contact_details(vcard, display_name);
            mirrored_row(ui, own, icon, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 5.0;
                    let name = details
                        .as_ref()
                        .map_or(display_name.as_str(), |contact| contact.name.as_str());
                    widgets::rich_text(ui, name, theme::medium(14.0), palette.text);
                    match details.as_ref() {
                        Some(SharedContact {
                            account: Some(account),
                            first_name,
                            ..
                        }) => {
                            let id = format!("{account}@s.whatsapp.net");
                            ui.horizontal(|ui| {
                                if theme::pill_button(
                                    ui,
                                    &palette,
                                    crate::i18n::gettext(view.locale, "Chat").as_ref(),
                                    true,
                                )
                                .clicked()
                                {
                                    actions.push(Action::StartChat {
                                        id: id.clone(),
                                        name: name.to_owned(),
                                        dismiss_dialog: true,
                                    });
                                }
                                if !view.contacts.contains_key(&id)
                                    && theme::pill_button(
                                        ui,
                                        &palette,
                                        crate::i18n::gettext(view.locale, "Add").as_ref(),
                                        false,
                                    )
                                    .clicked()
                                {
                                    // Split where the card's first name ends, so a
                                    // first name of several words stays whole.
                                    let (first, last) =
                                        crate::util::editor_names(name, first_name.as_deref());
                                    actions.push(Action::NewContact {
                                        phone: account.clone(),
                                        first,
                                        last,
                                        to_phone: None,
                                    });
                                }
                            });
                        }
                        // Without an account there is nothing to open, so
                        // the number stays readable, as it was.
                        Some(contact) => {
                            theme::text(
                                ui,
                                &contact.number,
                                theme::regular(12.5),
                                palette.secondary,
                            );
                        }
                        None => {}
                    }
                });
            });
            None
        }
        Content::StickerPack {
            name,
            publisher,
            count,
            caption,
        } => {
            let icon = |ui: &mut egui::Ui| {
                theme::icon(ui, Icon::Sticker, 20.0, palette.accent);
            };
            mirrored_row(ui, own, icon, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    widgets::rich_text(ui, name, theme::medium(14.0), palette.text);
                    let stickers =
                        crate::i18n::ngettext(view.locale, "{} sticker", "{} stickers", *count)
                            .replace("{}", &count.to_string());
                    let detail = if publisher.trim().is_empty() {
                        stickers
                    } else {
                        format!("{publisher} · {stickers}")
                    };
                    widgets::rich_text(ui, &detail, theme::regular(12.5), palette.secondary);
                    if let Some(caption) = caption {
                        widgets::rich_text(ui, caption, theme::regular(13.5), palette.text);
                    }
                    if theme::link(
                        ui,
                        crate::i18n::gettext(view.locale, "View stickers"),
                        theme::regular(12.5),
                        palette.link,
                    )
                    .clicked()
                    {
                        actions.push(Action::ViewStickerPack(message.id.clone()));
                    }
                });
            });
            None
        }
        Content::Poll { .. } => {
            super::polls::ballot(
                ui,
                &palette,
                view.locale,
                message,
                width,
                view.connected,
                view.poll_voting
                    .contains(&(message.chat.clone(), message.id.clone())),
                actions,
            );
            None
        }
        Content::Call {
            video,
            status,
            duration,
            group,
        } => {
            let entry = super::calls::Entry {
                from_me: own,
                video: *video,
                status: *status,
                duration: *duration,
                group: *group,
            };
            let tint = if status.is_bad() {
                palette.danger
            } else {
                palette.accent
            };
            // Same gate as the call's menu: a group's call while group calls
            // are on, a person's call, never one among several people with
            // no group behind it. Selecting keeps the click for the row.
            let can_call = !view.selecting
                && match view.chat.kind {
                    crate::model::ChatKind::Group => crate::calls::group_calls_enabled(),
                    crate::model::ChatKind::Direct => !group,
                    _ => false,
                };
            call_entry(
                ui,
                &palette,
                &entry,
                view.locale,
                tint,
                width,
                reserve,
                &view.chat.id,
                &message.id,
                can_call,
                actions,
            )
        }
        Content::Revoked => {
            mirrored_row(
                ui,
                own,
                |ui| {
                    theme::icon(ui, Icon::Ban, 14.0, palette.dim);
                },
                |ui| {
                    theme::text(
                        ui,
                        "This message was deleted",
                        theme::regular(13.5),
                        palette.secondary,
                    );
                },
            );
            None
        }
        Content::PhoneOnly {
            live_location: true,
            ..
        } => {
            mirrored_row(
                ui,
                own,
                |ui| {
                    theme::icon(ui, Icon::MapPin, 18.0, palette.accent);
                },
                |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        widgets::rich_text(
                            ui,
                            &crate::i18n::gettext(view.locale, "Live location"),
                            theme::medium(14.0),
                            palette.text,
                        );
                        widgets::rich_text(
                            ui,
                            &crate::i18n::gettext(
                                view.locale,
                                "Open WhatsApp on your phone to follow it.",
                            ),
                            theme::regular(12.5),
                            palette.secondary,
                        );
                    });
                },
            );
            None
        }
        Content::PhoneOnly {
            view_once, once, ..
        } => {
            use crate::i18n::gettext;
            use crate::model::OnceMedia;
            let text = match once {
                Some(OnceMedia::Photo) => gettext(
                    view.locale,
                    "View once photo. For your privacy, it opens only on your phone.",
                ),
                Some(OnceMedia::Video) => gettext(
                    view.locale,
                    "View once video. For your privacy, it opens only on your phone.",
                ),
                Some(OnceMedia::Voice) => gettext(
                    view.locale,
                    "View once voice message. For your privacy, it opens only on your phone.",
                ),
                Some(OnceMedia::Audio) => gettext(
                    view.locale,
                    "View once audio. For your privacy, it opens only on your phone.",
                ),
                None if *view_once => gettext(
                    view.locale,
                    "View once message. For your privacy, it opens only on your phone.",
                ),
                None => gettext(view.locale, "This message can only be seen on your phone."),
            };
            mirrored_row(
                ui,
                own,
                |ui| {
                    theme::icon(ui, Icon::Smartphone, 14.0, palette.dim);
                },
                |ui| {
                    theme::text(ui, text, theme::regular(13.5), palette.secondary);
                },
            );
            None
        }
        Content::Unsupported { what } => {
            mirrored_row(
                ui,
                own,
                |ui| {
                    theme::icon(ui, Icon::CircleAlert, 14.0, palette.dim);
                },
                |ui| {
                    theme::text(
                        ui,
                        format!("Unsupported: {what}"),
                        theme::regular(13.5),
                        palette.secondary,
                    );
                },
            );
            None
        }
    }
}

/// Horizontal, independently cached cards with overlaid previous/next controls.
/// Keep a partial next card visible and preserve native wheel/touchpad scrolling.
fn carousel(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    card: &crate::model::InteractiveCard,
    width: f32,
    actions: &mut Vec<Action>,
) {
    if !card.body.is_empty() {
        rich_body(
            ui,
            view,
            message,
            &card.body,
            width,
            None,
            Some(width),
            actions,
        );
        ui.add_space(4.0);
    }
    // Reserve a glimpse of the next card only when there is another card.
    let inset = if card.carousel.len() > 1 { 36.0 } else { 20.0 };
    let card_width = CAROUSEL_CARD_WIDTH.min((width - inset).max(140.0));
    let id = bubble_id(&message.chat, &message.id);
    for direction in [-1, 1] {
        ui.ctx().data_mut(|data| {
            data.remove::<Rect>(id.with(("carousel-arrow", direction)));
        });
    }
    let output = egui::ScrollArea::horizontal()
        .id_salt(("carousel", &message.chat, &message.id))
        .max_width(width)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false, true])
        .show_viewport(ui, |ui, viewport| {
            let strip = ui
                .with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = CAROUSEL_GAP;
                    for (index, child) in card.carousel.iter().enumerate() {
                        let row = carousel_row(message, index, child);
                        ui.push_id(("carousel-card", index), |ui| {
                            let response = Frame::new()
                                .fill(if message.from_me {
                                    view.palette.bubble_out
                                } else {
                                    view.palette.bubble_in
                                })
                                .corner_radius(10)
                                .inner_margin(Margin {
                                    left: 10,
                                    right: 10,
                                    top: 6,
                                    bottom: 5,
                                })
                                .show(ui, |ui| {
                                    ui.set_width(card_width);
                                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                        ui.set_width(card_width);
                                        if child.image.is_some() {
                                            carousel_picture(
                                                ui, view, message, index, child, card_width,
                                                actions,
                                            );
                                        }
                                        if !child.body.is_empty() {
                                            rich_body(
                                                ui,
                                                view,
                                                &row,
                                                &child.body,
                                                card_width,
                                                None,
                                                Some(card_width),
                                                actions,
                                            );
                                        }
                                        interactive_buttons(
                                            ui, view, &row, child, card_width, actions,
                                        );
                                    });
                                });
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(
                                    bubble_id(&message.chat, &message.id)
                                        .with(("carousel-card", index)),
                                    response.response.rect,
                                )
                            });
                        });
                    }
                })
                .response
                .rect;
            let limit = (strip.width() - viewport.width()).max(0.0);
            let visible = Rect::from_min_size(
                pos2(strip.left() + viewport.left(), strip.top()),
                vec2(viewport.width(), strip.height()),
            );
            if limit > 1.0 {
                for direction in [-1, 1] {
                    let available = if direction < 0 {
                        viewport.left() > 1.0
                    } else {
                        viewport.left() < limit - 1.0
                    };
                    if !available {
                        continue;
                    }
                    let x = if direction < 0 {
                        visible.left() + 26.0
                    } else {
                        visible.right() - 26.0
                    };
                    let rect =
                        Rect::from_center_size(pos2(x, visible.center().y), Vec2::splat(40.0));
                    let arrow = carousel_arrow(
                        ui,
                        &view.palette,
                        rect,
                        id.with(("carousel-arrow", direction)),
                        direction < 0,
                    );
                    if arrow.clicked() {
                        let step = card_width + 20.0 + CAROUSEL_GAP;
                        let target = (viewport.left() + direction as f32 * step).clamp(0.0, limit);
                        // Issue this inside the horizontal ScrollArea so the
                        // conversation's vertical scroll cannot consume it.
                        ui.scroll_with_delta(vec2(viewport.left() - target, 0.0));
                    }
                }
            }
            visible
        });
    super::swipe::yield_to(ui.ctx(), output.inner_rect);
    ui.ctx()
        .data_mut(|data| data.insert_temp(id.with("carousel-viewport"), output.inner));
}

/// Stands in for one carousel card when drawing its text and actions. Only the
/// parent's identity is kept: cloning the whole message would copy every
/// card's thumbnail per card each frame, and the parent's quote and reactions
/// would repeat on each card in copied transcripts.
fn carousel_row(message: &Message, index: usize, card: &crate::model::InteractiveCard) -> Message {
    Message {
        id: format!("{}-card-{index}", message.id),
        chat: message.chat.clone(),
        sender: message.sender.clone(),
        sender_name: message.sender_name.clone(),
        from_me: message.from_me,
        timestamp: message.timestamp,
        history_order: message.history_order,
        content: Content::Interactive {
            text: card.body.clone(),
            card: None,
        },
        status: message.status,
        delivered_at: None,
        read_at: None,
        quoted: None,
        reactions: Vec::new(),
        edited: message.edited,
        mentions: message.mentions.clone(),
        forwarded: false,
        thumbnail: None,
    }
}

fn carousel_arrow(
    ui: &mut egui::Ui,
    palette: &Palette,
    rect: Rect,
    id: egui::Id,
    previous: bool,
) -> egui::Response {
    let label = if previous {
        "Previous card"
    } else {
        "Next card"
    };
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    ui.ctx().data_mut(|data| data.insert_temp(id, rect));
    theme::reveal_focus(&response);
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() || response.has_focus() {
            palette.surface_hover
        } else {
            palette.overlay
        };
        ui.painter()
            .circle_filled(rect.center() + vec2(0.0, 2.0), 21.0, palette.shadow);
        ui.painter().circle_filled(rect.center(), 20.0, fill);
        ui.painter().circle_stroke(
            rect.center(),
            19.5,
            Stroke::new(
                1.0,
                if response.has_focus() {
                    palette.link
                } else {
                    palette.outline
                },
            ),
        );
        theme::paint_icon(
            ui,
            if previous {
                Icon::ChevronLeft
            } else {
                Icon::ChevronRight
            },
            rect,
            if response.is_pointer_button_down_on() {
                21.0
            } else {
                23.0
            },
            palette.text,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

/// How round the corners are where two tiles of an album meet.
const ALBUM_SEAM_RADIUS: f32 = 4.0;
/// The size of the button that downloads a whole album.
const ALBUM_BUTTON: f32 = 56.0;

/// Draws WhatsApp's media-album items as one compact grid with one download
/// button over it, and returns where each member's tile was drawn.
fn album_content(
    ui: &mut egui::Ui,
    view: &View<'_>,
    messages: &[Message],
    width: f32,
    actions: &mut Vec<Action>,
) -> Vec<(Rect, String)> {
    let gap = 3.0;
    let columns = 2;
    let tile = ((width - gap * (columns - 1) as f32) / columns as f32).max(80.0);
    // The grid's outer corners follow the picture's own radius; corners where
    // two tiles meet share one small radius, so the seams read evenly.
    let outer = picture_radius(&view.palette, &messages[0]);
    let inner = ALBUM_SEAM_RADIUS.min(outer);
    let mut tiles = Vec::with_capacity(messages.len());
    for (row, chunk) in messages.chunks(columns).enumerate() {
        // An own bubble lays out right to left; the grid keeps reading order.
        ui.allocate_ui_with_layout(
            vec2(tile * columns as f32 + gap, tile),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (column, message) in chunk.iter().enumerate() {
                    let left = column > 0;
                    let right = column + 1 < chunk.len();
                    let above = row > 0;
                    let below = (row + 1) * columns + column < messages.len();
                    let corner = |first: bool, second: bool| {
                        if first || second { inner } else { outer }.round() as u8
                    };
                    let corners = CornerRadius {
                        nw: corner(left, above),
                        ne: corner(right, above),
                        sw: corner(left, below),
                        se: corner(right, below),
                    };
                    if let Some(rect) = album_tile(
                        ui,
                        view,
                        message,
                        vec2(tile, tile),
                        corners,
                        (row, column),
                        actions,
                    ) {
                        tiles.push((rect, message.id.clone()));
                    }
                }
            },
        );
    }

    for message in messages {
        let caption = match &message.content {
            Content::Image { caption, .. } | Content::Video { caption, .. } => caption.as_deref(),
            _ => None,
        };
        if let Some(caption) = caption.filter(|caption| !caption.is_empty()) {
            let _ = rich_body(
                ui,
                view,
                message,
                caption,
                width,
                None,
                Some(width),
                actions,
            );
        } else if let Some(copy_rows) = view.copy_rows {
            copy_rows
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(transcript_row(view, message, String::new(), Vec::new()));
        }
    }

    album_download_button(ui, view, messages, &tiles, actions);
    tiles
}

/// The one button over an album that still has members to fetch: it downloads
/// them all, and eases away once none is left.
fn album_download_button(
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
    let reduce_motion = crate::motion::screen_reader_attached(ui.ctx());
    let appear = ui.ctx().animate_bool_with_time_and_easing(
        id,
        !missing.is_empty(),
        if reduce_motion {
            0.0
        } else {
            theme::MOTION_QUICK
        },
        theme::ease_out,
    );
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
    let hover = ui.ctx().animate_bool_with_time_and_easing(
        id.with("hover"),
        response.hovered() && !downloading,
        if reduce_motion {
            0.0
        } else {
            theme::MOTION_QUICK
        },
        theme::ease_out,
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

/// Draws one album member with its own download, selection and activation
/// targets, and returns its rect.
fn album_tile(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    size: Vec2,
    corners: CornerRadius,
    (row, column): (usize, usize),
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    let media = message.content.media()?;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let kind = if matches!(message.content, Content::Image { .. }) {
        "Photo"
    } else {
        "Video"
    };
    let state = match (&media.path, &media.state) {
        (Some(_), _) => "Open".to_owned(),
        (None, MediaState::Downloading) => "Downloading".to_owned(),
        (None, MediaState::Failed(error)) => format!("Failed: {error}"),
        (None, MediaState::Idle) => "Download".to_owned(),
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{kind}, {state}"))
    });
    theme::reveal_focus(&response);
    if ui.is_rect_visible(rect) {
        let opacity = hide_source(ui, message);
        ui.painter()
            .rect_filled(rect, corners, view.palette.surface);
        let uri = media
            .path
            .as_ref()
            .filter(|_| matches!(message.content, Content::Image { .. }))
            .map(|path| crate::image_cache::fit_uri(ui.ctx(), path, size))
            .or_else(|| {
                message.thumbnail.as_deref().map(|bytes| {
                    thumbnail_uri(
                        ui.ctx(),
                        &message.chat,
                        &format!("{}-album-{row}-{column}", message.id),
                        bytes,
                    )
                })
            });
        if let Some(uri) = uri {
            let image = egui::Image::new(uri);
            let dimensions = match image.load_for_size(ui.ctx(), size) {
                Ok(egui::load::TexturePoll::Ready { texture }) => Some(texture.size),
                Ok(egui::load::TexturePoll::Pending { .. }) => Some(vec2(
                    media.width.unwrap_or(1) as f32,
                    media.height.unwrap_or(1) as f32,
                )),
                Err(_) => None,
            };
            if let Some(dimensions) = dimensions {
                let ratio = dimensions.x / dimensions.y.max(1.0);
                let target = size.x / size.y.max(1.0);
                let uv = if ratio > target {
                    vec2(target / ratio, 1.0)
                } else {
                    vec2(1.0, ratio / target)
                };
                image
                    .uv(Rect::from_center_size(pos2(0.5, 0.5), uv))
                    .fit_to_exact_size(size)
                    .corner_radius(corners)
                    .paint_at(ui, rect);
            }
        }
        if matches!(message.content, Content::Video { .. }) {
            theme::paint_icon(ui, Icon::Play, rect, 30.0, Color32::WHITE);
        }
        if not_sent(message) {
            let starred = view.starred.is_some_and(|ids| ids.contains(&message.id));
            let pinned = view.pins.is_some_and(|ids| ids.contains(&message.id));
            footer_over_picture(ui, view, message, rect, starred, pinned, actions);
        }
        show_source(ui, opacity);
    }
    // A motion photo in an album keeps its own badge and clip.
    if let Content::Image {
        motion: Some(motion),
        ..
    } = &message.content
    {
        motion_clip(ui, view, message, motion, rect, actions);
    }
    theme::focus_outline_corners(ui, response.id, rect, corners);
    // Each member keeps its own failure: the others stay usable, and a click
    // on this one retries it.
    let response = if let MediaState::Failed(error) = &media.state {
        response.on_hover_text(format!("{error} · Click to retry"))
    } else {
        response
    };
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        if let Some(path) = &media.path {
            // A photo or a clip the viewer plays opens there, over the
            // chat's album; anything else goes to the system's own viewer.
            actions.push(match crate::image_preview::open_target(path, true) {
                crate::image_preview::OpenTarget::Preview => Action::PreviewImage {
                    path: path.clone(),
                    chat: message.chat.clone(),
                    message: message.id.clone(),
                    card: None,
                },
                crate::image_preview::OpenTarget::External => Action::OpenFile(path.clone()),
            });
        } else if !matches!(media.state, MediaState::Downloading) {
            actions.push(Action::Download {
                manual: true,
                card: None,
                chat: message.chat.clone(),
                message: message.id.clone(),
            });
        }
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(album_tile_id(&view.chat.id, &message.id), rect));
    Some(rect)
}

/// Where an album member's tile was drawn, for interaction tests.
pub fn album_tile_id(chat: &str, message: &str) -> egui::Id {
    bubble_id(chat, message).with("album-tile")
}

/// A carousel uses a short image preview; opening it shows the full picture.
fn carousel_picture(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    index: usize,
    card: &crate::model::InteractiveCard,
    width: f32,
    actions: &mut Vec<Action>,
) {
    let Some(media) = &card.image else { return };
    let size = vec2(width, width * 0.56);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let label = match (&media.path, &media.state) {
        (Some(_), _) => "Open card image",
        (None, MediaState::Failed(_)) => "Retry card image download",
        (None, MediaState::Downloading) => "Downloading card image",
        (None, MediaState::Idle) => "Download card image",
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    // Tab can land on a card scrolled out of the carousel's viewport.
    theme::reveal_focus(&response);
    let visible = ui.is_rect_visible(rect);
    if visible {
        ui.painter().rect_filled(rect, 6.0, view.palette.surface);
        let uri = media
            .path
            .as_ref()
            .map(|path| crate::image_cache::fit_uri(ui.ctx(), path, size))
            .or_else(|| {
                card.thumbnail.as_deref().map(|bytes| {
                    thumbnail_uri(
                        ui.ctx(),
                        &message.chat,
                        &format!("{}-card-{index}", message.id),
                        bytes,
                    )
                })
            });
        if let Some(uri) = uri {
            let image = egui::Image::new(uri);
            let dimensions = match image.load_for_size(ui.ctx(), size) {
                Ok(egui::load::TexturePoll::Ready { texture }) => Some(texture.size),
                Ok(egui::load::TexturePoll::Pending { .. }) => Some(vec2(
                    media.width.unwrap_or(16) as f32,
                    media.height.unwrap_or(9) as f32,
                )),
                Err(_) => None,
            };
            if let Some(dimensions) = dimensions {
                let ratio = dimensions.x / dimensions.y.max(1.0);
                let target = size.x / size.y.max(1.0);
                let uv_size = if ratio > target {
                    vec2(target / ratio, 1.0)
                } else {
                    vec2(1.0, ratio / target)
                };
                image
                    .uv(Rect::from_center_size(pos2(0.5, 0.5), uv_size))
                    .fit_to_exact_size(size)
                    .corner_radius(6.0)
                    .paint_at(ui, rect);
            } else if media.path.is_some() {
                theme::paint_icon(ui, Icon::CircleAlert, rect, 24.0, view.palette.danger);
                ui.painter().text(
                    rect.center() + vec2(0.0, 24.0),
                    Align2::CENTER_CENTER,
                    "Could not display this picture. Click to open it.",
                    theme::regular(11.5),
                    view.palette.secondary,
                );
            }
        }
        if media.path.is_none() {
            let disc = Rect::from_center_size(rect.center(), Vec2::splat(42.0));
            ui.painter()
                .circle_filled(disc.center(), 21.0, Color32::from_black_alpha(130));
            match &media.state {
                MediaState::Downloading => theme::paint_spinner(ui, disc, 20.0, Color32::WHITE),
                MediaState::Failed(_) => {
                    theme::paint_icon(ui, Icon::CircleAlert, disc, 20.0, Color32::WHITE);
                    ui.painter().text(
                        rect.center() + vec2(0.0, 34.0),
                        Align2::CENTER_CENTER,
                        "Download failed. Click to retry.",
                        theme::regular(11.5),
                        Color32::WHITE,
                    );
                }
                MediaState::Idle => {
                    theme::paint_icon(ui, Icon::Download, disc, 20.0, Color32::WHITE)
                }
            }
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                6.0,
                Stroke::new(theme::FOCUS_STROKE_WIDTH, view.palette.link),
                egui::StrokeKind::Inside,
            );
        }
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let clicked = response.clicked();
    if let MediaState::Failed(error) = &media.state {
        response.on_hover_text(format!("{error} · Click to retry"));
    }
    if let Some(path) = &media.path {
        if clicked {
            // A card's picture opens in the viewer, among the chat's album,
            // as any other photo does.
            actions.push(match crate::image_preview::open_target(path, true) {
                crate::image_preview::OpenTarget::Preview => Action::PreviewImage {
                    path: path.clone(),
                    chat: message.chat.clone(),
                    message: message.id.clone(),
                    card: Some(index),
                },
                crate::image_preview::OpenTarget::External => Action::OpenFile(path.clone()),
            });
        }
    } else if !matches!(media.state, MediaState::Downloading)
        && (clicked
            || (visible
                && auto_download_allowed(media, false, view.auto_download)
                && matches!(media.state, MediaState::Idle)))
    {
        actions.push(Action::Download {
            manual: clicked,
            chat: message.chat.clone(),
            message: message.id.clone(),
            card: Some(index),
        });
    }
}

/// Full-width action rows below the message timestamp, matching business cards.
/// Show only executable actions as active, with the same keyboard path as clicks.
fn interactive_buttons(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    card: &crate::model::InteractiveCard,
    width: f32,
    actions: &mut Vec<Action>,
) {
    use crate::model::InteractiveAction;
    let palette = &view.palette;
    let spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    for (index, button) in card.buttons.iter().enumerate() {
        let sends = matches!(
            button.action,
            InteractiveAction::Reply | InteractiveAction::Select(_)
        );
        let enabled = button.url.is_some()
            || match &button.action {
                InteractiveAction::Copy(_) => true,
                InteractiveAction::Reply | InteractiveAction::Select(_) => {
                    view.connected
                        && view.chat.can_send()
                        && !message.from_me
                        && !message.edited
                        && !view
                            .interactive_pending
                            .contains(&(message.chat.clone(), message.id.clone()))
                }
                InteractiveAction::Unavailable => false,
            };
        // Muted/link colours target incoming surfaces; outgoing tinted bubbles
        // need the main foreground to keep small labels readable in both themes.
        let color = if message.from_me {
            palette.text
        } else if enabled {
            palette.link
        } else {
            palette.secondary
        };
        let icon = if button.url.is_some() {
            Some(Icon::ExternalLink)
        } else {
            match button.action {
                InteractiveAction::Reply => None,
                InteractiveAction::Select(_) => Some(Icon::ListChecks),
                InteractiveAction::Copy(_) => Some(Icon::Copy),
                InteractiveAction::Unavailable => Some(Icon::Smartphone),
            }
        };
        let line = widgets::line(
            ui,
            &button.label,
            theme::medium(14.0),
            color,
            (width - 48.0).max(1.0),
            2,
        );
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, _) = ui.allocate_exact_size(
            vec2(width, (line.size().y + 22.0).max(44.0)),
            Sense::hover(),
        );
        // Action rows belong to the card itself, including its side padding.
        // The final row follows the bubble's bottom corners and margin.
        let last = index + 1 == card.buttons.len() && !card.needs_phone;
        let row_rect = Rect::from_min_max(
            rect.min - vec2(10.0, 0.0),
            rect.max + vec2(10.0, if last { 5.0 } else { 0.0 }),
        );
        let corners = CornerRadius {
            sw: if last { 10 } else { 0 },
            se: if last { 10 } else { 0 },
            ..CornerRadius::ZERO
        };
        let response = ui.interact(
            row_rect,
            bubble_id(&message.chat, &message.id).with(("interactive-action", index)),
            sense,
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &button.label)
        });
        theme::reveal_focus(&response);
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                bubble_id(&message.chat, &message.id).with(("interactive-button", index)),
                row_rect,
            )
        });
        if ui.is_rect_visible(rect) {
            if enabled && (response.hovered() || response.has_focus()) {
                ui.painter().rect_filled(
                    row_rect,
                    corners,
                    palette
                        .text
                        .gamma_multiply(if response.is_pointer_button_down_on() {
                            0.08
                        } else {
                            0.04
                        }),
                );
            }
            ui.painter().hline(
                row_rect.x_range(),
                rect.top(),
                Stroke::new(1.0, palette.secondary.gamma_multiply(0.2)),
            );
            if response.has_focus() {
                ui.painter().rect_stroke(
                    rect.shrink(2.0),
                    4.0,
                    Stroke::new(1.0, palette.link),
                    egui::StrokeKind::Inside,
                );
            }
            let inset = if icon.is_some() { 22.0 } else { 0.0 };
            let content_width = line.size().x + inset;
            let left = rect.center().x - content_width / 2.0;
            if let Some(icon) = icon {
                theme::paint_icon(
                    ui,
                    icon,
                    Rect::from_center_size(
                        egui::pos2(left + 7.0, rect.center().y),
                        Vec2::splat(14.0),
                    ),
                    14.0,
                    color,
                );
            }
            line.paint(
                ui,
                egui::pos2(left + inset, rect.center().y - line.size().y / 2.0),
                color,
            );
        }
        let reply = |choice| Action::ReplyInteractive {
            chat: message.chat.clone(),
            message: message.id.clone(),
            button: index,
            choice,
        };
        if !enabled {
            let reason = if sends
                && view
                    .interactive_pending
                    .contains(&(message.chat.clone(), message.id.clone()))
            {
                "Sending reply…"
            } else if sends && !view.connected {
                "Connect to WhatsApp to reply"
            } else if sends && !view.chat.can_send() {
                "This conversation is read-only"
            } else if sends && message.from_me {
                "Reply options are for the recipient"
            } else {
                "Open this option in WhatsApp Web or on your phone"
            };
            response.on_hover_text(format!("{}\n{reason}", button.label));
        } else if let Some(url) = &button.url {
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!("{}\n{url}", button.label))
                .clicked()
            {
                actions.push(Action::OpenUrl(url.clone()));
            }
        } else {
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            match &button.action {
                InteractiveAction::Reply => {
                    if response
                        .on_hover_text(format!("Send reply: {}", button.label))
                        .clicked()
                    {
                        actions.push(reply(None));
                    }
                }
                InteractiveAction::Copy(code) => {
                    if response.on_hover_text("Copy code").clicked() {
                        actions.push(Action::CopyText(code.clone()));
                    }
                }
                InteractiveAction::Select(_) => {
                    if response.clicked() {
                        actions.push(Action::ShowDialog(crate::model::Dialog::InteractiveList {
                            chat: message.chat.clone(),
                            message: message.id.clone(),
                            button: index,
                        }));
                    }
                }
                InteractiveAction::Unavailable => {}
            }
        }
    }
    if card.needs_phone {
        ui.add_space(6.0);
        widgets::rich_text(
            ui,
            "More content in WhatsApp Web or on your phone",
            theme::regular(12.0),
            palette.secondary,
        );
    }
    ui.spacing_mut().item_spacing.y = spacing;
}

/// Draws formatted message text. `reserve` leaves footer space on the last
/// line. `span` sets a minimum left-aligned row width for text below cards.
#[allow(clippy::too_many_arguments)]
fn rich_body(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    text: &str,
    width: f32,
    reserve: Option<f32>,
    span: Option<f32>,
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    let palette = view.palette;
    let mentions = mentions_of(view, message);
    let style = markup::Style {
        size: BODY_SIZE,
        color: palette.text,
        secondary: palette.secondary,
        link: palette.link,
        mention: palette.accent,
    };
    let laid = markup::layout(ui, text, &mentions, &style, width);
    let rtl = crate::bidi::message_rtl(text);
    let last_row = laid.galley.rows.last().map_or(0.0, |row| row.row.size.x);
    // Right-aligned text ends at the block's edge. Like official WhatsApp,
    // only a single line keeps the time beside it; otherwise it gets a row.
    let single = laid.galley.rows.len() == 1 && span.is_none();
    let inline = reserve.filter(|reserve| last_row + 8.0 + reserve <= width && (!rtl || single));
    let size = laid.galley.size();
    let mut allocation = match inline {
        Some(reserve) => vec2(size.x.max(last_row + 8.0 + reserve), size.y),
        None => size,
    };
    if let Some(span) = span {
        // Span the card width and keep the text left-aligned in own bubbles.
        allocation.x = allocation.x.max(span);
    }
    // Register the body for transcript formatting when copying across messages.
    if let Some(copy_rows) = view.copy_rows {
        copy_rows
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(transcript_row(
                view,
                message,
                laid.galley.text().to_owned(),
                laid.placements().to_vec(),
            ));
    }
    // Click links and drag to select text.
    // Text selection and web links do not need a sequential Tab stop.
    // The surrounding transcript remains available to accessibility readers.
    let (_, rect) = ui.allocate_space(allocation);
    // egui matches selection endpoints to widgets by id every frame and drops
    // the selection when one is missed. A positional auto id shifts whenever
    // a sibling allocates differently (virtualized rows), killing the
    // selection mid-drag; an explicit id keeps the anchor alive.
    let id = bubble_id(&view.chat.id, &message.id);
    // Starting a browser-style sweep on a plain text message should work from
    // anywhere in its bubble, including the footer and generous side padding.
    // Captioned media keeps the narrower expansion so this response cannot
    // cover the picture or another control inside the same bubble.
    let selection_rect = ui
        .ctx()
        .data(|data| data.get_temp::<Rect>(id.with("rect")))
        .map(|bubble| {
            // Virtualized rows can move between frames. Align the cached bubble
            // to this frame's body before expanding its input target, so it
            // cannot cover a neighboring message after a layout change.
            let bubble = ui
                .ctx()
                .data(|data| data.get_temp::<Rect>(id.with("body")))
                .map(|previous| {
                    let mut bubble = bubble.translate(rect.min - previous.min);
                    bubble.max += rect.size() - previous.size();
                    bubble
                })
                .unwrap_or(bubble);
            if matches!(message.content, Content::Text { .. }) {
                let mut target = bubble.union(rect);
                // Quote and preview cards have their own click actions and
                // were registered before the text. Keep their current-frame
                // rectangles outside this later, enlarged input target.
                for kind in ["quote", "preview"] {
                    let present = if kind == "quote" {
                        message.quoted.is_some()
                    } else {
                        matches!(
                            message.content,
                            Content::Text {
                                preview: Some(_),
                                ..
                            }
                        )
                    };
                    if !present {
                        continue;
                    }
                    if let Some(card) = ui.ctx().data(|data| data.get_temp::<Rect>(id.with(kind))) {
                        target.min.y = target.min.y.max(card.bottom());
                    }
                }
                target
            } else {
                rect.expand2(vec2(10.0, 6.0)).intersect(bubble).union(rect)
            }
        })
        .unwrap_or(rect);
    let response = ui.interact(
        selection_rect,
        id.with("body-text"),
        Sense::CLICK | Sense::DRAG,
    );
    // The expanded target owns pointer input in the padding. Preserve the
    // bubble gestures that lived there before, while double-clicks on actual
    // text still select a word.
    let padding = response
        .interact_pointer_pos()
        .is_some_and(|pointer| !rect.contains(pointer));
    if padding {
        reply_on_double_click(&response, view, message, actions);
        if response.clicked() && !response.double_clicked() {
            let modifiers = ui.input(|input| input.modifiers);
            if view.selecting {
                actions.push(if modifiers.shift {
                    Action::SelectRange(message.id.clone())
                } else {
                    Action::ToggleSelected(message.id.clone())
                });
            } else if modifiers.command {
                actions.push(Action::SelectMessage(message.id.clone()));
            }
        }
    }
    // On our own editable text a double-click edits it, on the words too.
    if !padding && can_edit_inline(view, message) && response.double_clicked() {
        start_inline_edit(ui.ctx(), view, message);
    }
    // Store the body rect for selection tests.
    ui.ctx().data_mut(|data| {
        data.insert_temp(id.with("body"), rect);
    });
    // Keep off-screen selected bodies registered so scrolling does not lose
    // the selection anchor or omit copied text.
    let visible = ui.is_rect_visible(rect);
    let selection_alive = ui.input(|input| input.pointer.primary_down())
        || ui
            .ctx()
            .plugin_opt::<egui::text_selection::LabelSelectionState>()
            .is_some_and(|plugin| plugin.lock().has_selection());
    let origin = if rtl && inline.is_none() {
        pos2(rect.right() - size.x, rect.top())
    } else {
        rect.min
    };
    if visible || selection_alive {
        markup::paint_selectable(ui, &laid, &response, origin, palette.text, visible);
    }
    let message_number = crate::i18n::gettext(view.locale, "Message");
    let copy_number = crate::i18n::gettext(view.locale, "Copy number");
    let menu_labels = [message_number.as_ref(), copy_number.as_ref()];
    for (range, url) in &laid.links {
        let Some(phone) = url.strip_prefix("tel:") else {
            continue;
        };
        let number = phone.strip_prefix('+').unwrap_or(phone);
        let copy_text: String = laid
            .galley
            .text()
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect();
        for (row, bounds) in crate::bidi::char_bounds_by_row(&laid.galley, range.start, range.end) {
            let link = ui
                .interact(
                    bounds.translate(origin.to_vec2()).expand(2.0),
                    bubble_id(&view.chat.id, &message.id).with((
                        "phone-link",
                        range.start,
                        range.end,
                        row,
                    )),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            link.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), number)
            });
            theme::reveal_focus(&link);
            let link = focus::phone_link(link);
            egui::Popup::menu(&link)
                .id(link.id.with("popup"))
                .width(widgets::menu_width(ui, &menu_labels, true))
                .frame(widgets::menu_frame(&view.palette))
                .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                .show_frosted(ui.ctx(), |ui| {
                    #[cfg(test)]
                    let message_rect =
                        Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 28.0));
                    let message_clicked =
                        widgets::menu_item(ui, &view.palette, None, message_number.as_ref());
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(link.id.with("test-message-action"), message_rect);
                    });
                    if message_clicked {
                        actions.push(Action::NewContact {
                            phone: number.to_owned(),
                            first: String::new(),
                            last: String::new(),
                            to_phone: None,
                        });
                    }
                    #[cfg(test)]
                    let copy_rect =
                        Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 28.0));
                    let copy_clicked = widgets::menu_item(
                        ui,
                        &view.palette,
                        Some(Icon::Copy),
                        copy_number.as_ref(),
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(link.id.with("test-copy-action"), copy_rect);
                    });
                    if copy_clicked {
                        actions.push(Action::CopyText(copy_text.clone()));
                    }
                });
        }
    }
    if !laid.links.is_empty()
        && let Some(pos) = response.hover_pos()
        && laid
            .galley
            .rows
            .iter()
            .any(|row| Rect::from_min_size(origin + row.pos.to_vec2(), row.row.size).contains(pos))
    {
        let cursor = laid.galley.cursor_from_pos(pos - origin);
        if let Some(url) = laid.link_at(cursor.index.0) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if !url.starts_with("tel:") && response.clicked() {
                actions.push(Action::OpenUrl(url.to_owned()));
            }
        }
    }
    inline.map(|reserve| {
        Rect::from_min_max(
            pos2(rect.right() - reserve, rect.bottom() - 15.0),
            rect.right_bottom(),
        )
    })
}

/// Link preview with image, title, and description.
fn preview_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    preview: &LinkPreview,
    width: f32,
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    let thumbnail = message.thumbnail.as_deref();
    let domain = preview
        .url
        .split("://")
        .nth(1)
        .unwrap_or(&preview.url)
        .split('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    let (bleed, radius) = edge_card(ui, &palette, 6);
    let response = Frame::new()
        .fill(palette.window.gamma_multiply(0.35))
        .corner_radius(CornerRadius::same(radius))
        .outer_margin(bleed)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            // Include margins in the settled card width.
            let card_width = (width - bleed.sum().x - 16.0).max(0.0);
            ui.set_width(card_width);
            // Limit text to the space beside the thumbnail and keep it
            // left-aligned in own bubbles.
            let column = (card_width - if thumbnail.is_some() { 72.0 } else { 0.0 }).max(0.0);
            ui.allocate_ui_with_layout(vec2(card_width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(card_width);
                ui.horizontal(|ui| {
                    if let Some(bytes) = thumbnail {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                        // Off-screen cards are laid out too; only a visible
                        // one keeps its thumbnail resident.
                        if ui.is_rect_visible(rect) {
                            let uri = thumbnail_uri(ui.ctx(), &message.chat, &message.id, bytes);
                            egui::Image::new(uri)
                                .fit_to_exact_size(rect.size())
                                .corner_radius(4.0)
                                .paint_at(ui, rect);
                        }
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.set_width(column);
                        if let Some(title) = &preview.title {
                            widgets::rich_text(ui, title, theme::semibold(13.5), palette.text);
                        }
                        if let Some(description) = &preview.description {
                            let line = widgets::line(
                                ui,
                                description,
                                theme::regular(12.5),
                                palette.secondary,
                                ui.available_width(),
                                2,
                            );
                            let (rect, _) = ui.allocate_exact_size(line.size(), Sense::hover());
                            line.paint(ui, rect.min, palette.secondary);
                        }
                        theme::text(ui, &domain, theme::regular(12.0), palette.dim);
                    });
                });
            });
        })
        .response;
    let card = response.rect - bleed;
    ui.ctx().data_mut(|data| {
        data.insert_temp(bubble_id(&view.chat.id, &message.id).with("preview"), card);
    });
    let response = ui
        .interact(card, ui.id().with(("preview", &message.id)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        actions.push(Action::OpenUrl(preview.url.clone()));
    }
}

/// A location as a card: the map preview WhatsApp sent across the top, then
/// a pinned title, an optional detail line, and a link to open the spot.
#[expect(clippy::too_many_arguments)]
fn location_card(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    preview_key: &str,
    title: &str,
    detail: Option<String>,
    note: Option<String>,
    (latitude, longitude): (f64, f64),
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    // A bounded, left-aligned layout: own bubbles inherit right-to-left flow,
    // which would otherwise stretch the card and make its width oscillate.
    let card = ui.available_width().min(LOCATION_CARD_WIDTH);
    ui.allocate_ui_with_layout(vec2(card, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(card);
        ui.spacing_mut().item_spacing.y = 2.0;
        // The preview already marks the spot. Crop the square preview to the
        // card's shape.
        if let Some(bytes) = message.thumbnail.as_deref()
            && !bytes.is_empty()
        {
            let uri = thumbnail_uri(ui.ctx(), &message.chat, preview_key, bytes);
            let height = (card * 0.56).round();
            let crop = (1.0 - height / card) / 2.0;
            ui.add(
                egui::Image::new(uri)
                    .maintain_aspect_ratio(false)
                    .uv(Rect::from_min_max(pos2(0.0, crop), pos2(1.0, 1.0 - crop)))
                    .fit_to_exact_size(Vec2::new(card, height))
                    .corner_radius(8.0),
            );
            ui.add_space(4.0);
        }
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::MapPin, 16.0, palette.accent);
            widgets::rich_text(ui, title, theme::medium(14.0), palette.text);
        });
        if let Some(detail) = detail {
            widgets::rich_text(ui, &detail, theme::regular(12.5), palette.secondary);
        }
        if let Some(note) = note {
            widgets::rich_text(ui, &note, theme::regular(12.5), palette.secondary);
        }
        if theme::link(
            ui,
            crate::i18n::gettext(view.locale, "Open in a map").as_ref(),
            theme::regular(12.5),
            palette.link,
        )
        .clicked()
        {
            let spot = Spot {
                latitude,
                longitude,
            };
            actions.push(Action::OpenUrl(crate::geo::map_url(spot)));
        }
    });
}

pub(crate) fn thumbnail_uri(ctx: &egui::Context, chat: &str, id: &str, bytes: &[u8]) -> String {
    let uri = format!(
        "bytes://thumb-{}-{}",
        chat.chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>(),
        id
    );
    crate::image_cache::include(ctx, &uri, bytes);
    uri
}

/// Default image bounds based on [`CARD_WIDTH`].
const PICTURE_WIDTH: f32 = CARD_WIDTH;

/// The widest a picture or video grows: in a Nothing bubble that also holds
/// a quote, a label or a caption it reaches into the text padding on both
/// sides, as the quote above it does, so the two line up.
fn picture_limit(palette: &Palette, message: &Message) -> f32 {
    if palette.bubbles == theme::BubbleStyle::Nothing && !compact_picture_message(message) {
        PICTURE_WIDTH + f32::from(2 * (NOTHING_TEXT_MARGIN.left - CARD_RING))
    } else {
        PICTURE_WIDTH
    }
}
const PICTURE_HEIGHT: f32 = 440.0;
const STICKER_SIDE: f32 = 180.0;
/// Height of a header row, the chat list's and the conversation's alike, so
/// their titles share a centre line.
pub(crate) const HEADER_ROW: f32 = 44.0;

/// Fits an image within bounds without upscaling and with a readable minimum.
fn fit_picture(width: f32, height: f32, max_width: f32, max_height: f32) -> Vec2 {
    let (width, height) = if width > 0.0 && height > 0.0 {
        (width, height)
    } else {
        (4.0, 3.0)
    };
    let max_width = max_width.max(0.0);
    let max_height = max_height.max(0.0);
    let scale = (max_width / width).min(max_height / height).clamp(0.0, 1.0);
    let scale = if width * scale < 120.0 {
        (120.0 / width).min(max_width / width).max(0.0)
    } else {
        scale
    };
    vec2(
        (width * scale).max(0.0),
        (height * scale).max(90.0).max(0.0),
    )
}

/// Fits a sticker to the standard square size.
fn fit_sticker(width: f32, height: f32) -> Vec2 {
    let (width, height) = if width > 0.0 && height > 0.0 {
        (width, height)
    } else {
        (1.0, 1.0)
    };
    let scale = (STICKER_SIDE / width).min(STICKER_SIDE / height);
    vec2(width * scale, height * scale)
}

/// The dimming over a picture while it downloads, by message, so it can lift
/// once the file (a new path) is there.
fn download_veil_id(message: &Message) -> egui::Id {
    egui::Id::new(("download-veil", &message.chat, &message.id))
}

/// What a picture had over it while it downloaded, the dimming and the
/// spinner, at `strength` as it lifts.
fn paint_download_veil(ui: &egui::Ui, rect: Rect, radius: f32, strength: f32) {
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

/// Reserved size for an image or video before and after download.
fn frame_size(
    media: &Media,
    thumbnail_hint: Option<(u32, u32)>,
    max_width: f32,
    max_height: f32,
) -> Vec2 {
    let (w, h) = match (media.width, media.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w as f32, h as f32),
        _ => match thumbnail_hint {
            Some((w, h)) if w > 0 && h > 0 => (w as f32, h as f32),
            _ => (4.0, 3.0),
        },
    };
    fit_picture(w, h, max_width, max_height)
}

/// Draws an image or sticker, using its preview until downloaded. Returns its width.
fn picture(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    media: &Media,
    width: f32,
    sticker: Option<bool>,
    actions: &mut Vec<Action>,
) -> Rect {
    let palette = view.palette;
    let radius = picture_radius(&palette, message);
    let (max_width, max_height) = match sticker {
        Some(_) => (STICKER_SIDE, STICKER_SIDE),
        None => (width.min(picture_limit(&palette, message)), PICTURE_HEIGHT),
    };
    if let Some(path) = &media.path {
        // Phones often leave a sticker's animated flag out, so a downloaded
        // file that says it moves is played as well.
        let sticker = sticker.map(|animated| animated || crate::ui::picker::moves(ui.ctx(), path));
        if sticker == Some(true) {
            let size = fit_sticker(
                media.width.unwrap_or(180) as f32,
                media.height.unwrap_or(180) as f32,
            );
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            if ui.is_rect_visible(rect) {
                // Stickers play and loop on their own while on screen, as on
                // the phone, not only under the pointer.
                match animation::autoplay(ui, path, rect, view.animate, response.hovered()) {
                    animation::Frame::Ready(texture) => {
                        ui.painter().image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    _ => {
                        ui.painter().rect_filled(rect, radius, palette.surface);
                        theme::paint_icon(ui, Icon::Sticker, rect, 32.0, palette.secondary);
                    }
                }
            }
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                actions.push(Action::PreviewSticker(path.clone()));
            }
            return rect;
        }
        // A row that is off screen only reserves its space. Loading the image
        // decodes it and uploads a texture, so it waits until it is scrolled
        // into view, and `image_cache` can release it once it leaves again.
        // The space is the size the picture was last drawn at, when known:
        // a message without dimensions (or with wrong ones) would otherwise
        // take one height on screen and another off it, and a picture across
        // the top edge of a transcript held at its end would flip between
        // them on every frame, shaking the whole chat (#179).
        let fit = |pixels: Vec2| {
            if sticker.is_some() {
                fit_sticker(pixels.x, pixels.y)
            } else {
                fit_picture(pixels.x, pixels.y, max_width, max_height)
            }
        };
        let pixels_id = egui::Id::new(("picture-pixels", path));
        let drawn = ui.ctx().data(|data| data.get_temp::<Vec2>(pixels_id));
        let reserved = drawn.map_or_else(|| frame_size(media, None, max_width, max_height), fit);
        // Shown whole, so decoded only as large as it fits the bounds. A
        // sticker here is still (a moving one played above), so its WebP
        // decodes off the interface thread like a photo.
        let image = if sticker.is_some() {
            widgets::fitted_sticker(ui, path, vec2(max_width, max_height))
        } else {
            widgets::fitted_image_within(ui, path, vec2(max_width, max_height))
        };
        let position = ui.next_widget_position();
        if !ui.is_rect_visible(Rect::from_min_size(position, reserved)) {
            return ui.allocate_exact_size(reserved, Sense::hover()).0;
        }
        // While the picture decodes its blurred thumbnail stands in, and the
        // sharp picture fades in over it once ready. One already decoded, as
        // when scrolled back to, shows at once.
        let placeholder = message
            .thumbnail
            .as_deref()
            .filter(|_| sticker.is_none())
            .map(|bytes| thumbnail_uri(ui.ctx(), &message.chat, &message.id, bytes));
        let sharpen_id = pixels_id.with("sharpen");
        // A picture that just finished downloading lifts the dimming and the
        // spinner it had while it came, rather than dropping them at once.
        let veil = theme::animate(
            ui.ctx(),
            download_veil_id(message),
            false,
            theme::MOTION_PANEL,
        );
        return match image.load_for_size(ui.ctx(), vec2(max_width, max_height)) {
            Ok(egui::load::TexturePoll::Ready { texture }) => {
                if drawn != Some(texture.size) {
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(pixels_id, texture.size));
                }
                let size = fit(texture.size);
                let waited = ui
                    .ctx()
                    .data(|data| data.get_temp::<bool>(sharpen_id))
                    .is_some();
                let sharp = if waited {
                    theme::animate(ui.ctx(), sharpen_id, true, theme::MOTION_PANEL)
                } else {
                    1.0
                };
                if sharp >= 1.0 && waited {
                    ui.ctx().data_mut(|data| data.remove::<bool>(sharpen_id));
                }
                if sharp < 1.0
                    && let Some(uri) = &placeholder
                {
                    egui::Image::new(uri.clone())
                        .fit_to_exact_size(size)
                        .corner_radius(radius)
                        .paint_at(ui, Rect::from_min_size(ui.next_widget_position(), size));
                }
                let response = ui.add(
                    image
                        .fit_to_exact_size(size)
                        .corner_radius(if sticker.is_some() { 0.0 } else { radius })
                        .tint(Color32::WHITE.gamma_multiply(sharp))
                        .sense(Sense::click()),
                );
                let rect = response.rect;
                paint_download_veil(ui, rect, radius, veil);
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    let action = match sticker {
                        Some(_) => Action::PreviewSticker(path.clone()),
                        None => match crate::image_preview::open_target(path, true) {
                            crate::image_preview::OpenTarget::Preview => Action::PreviewImage {
                                path: path.clone(),
                                chat: view.chat.id.clone(),
                                message: message.id.clone(),
                                card: None,
                            },
                            crate::image_preview::OpenTarget::External => {
                                Action::OpenFile(path.clone())
                            }
                        },
                    };
                    actions.push(action);
                }
                rect
            }
            Ok(egui::load::TexturePoll::Pending { .. }) => {
                // A picture released while away loads again at its old size.
                let size = match drawn {
                    Some(pixels) => fit(pixels),
                    None if sticker.is_some() => Vec2::splat(STICKER_SIDE),
                    None => frame_size(media, None, max_width, max_height),
                };
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                if placeholder.is_some() {
                    // The cross-fade starts from the thumbnail once ready.
                    ui.ctx().animate_bool_with_time(sharpen_id, false, 0.0);
                    ui.ctx().data_mut(|data| data.insert_temp(sharpen_id, true));
                }
                if ui.is_rect_visible(rect) {
                    match &placeholder {
                        Some(uri) => egui::Image::new(uri.clone())
                            .fit_to_exact_size(size)
                            .corner_radius(radius)
                            .paint_at(ui, rect),
                        None => {
                            ui.painter().rect_filled(rect, radius, palette.surface);
                            theme::paint_spinner(ui, rect, 22.0, palette.accent);
                        }
                    }
                    paint_download_veil(ui, rect, radius, veil);
                }
                rect
            }
            Err(_) => {
                let size = if sticker.is_some() {
                    Vec2::splat(STICKER_SIDE)
                } else {
                    frame_size(media, None, max_width, max_height)
                };
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                if ui.is_rect_visible(rect) {
                    ui.painter().rect_filled(rect, radius, palette.surface);
                    theme::paint_icon(ui, Icon::CircleAlert, rect, 24.0, palette.danger);
                    ui.painter().text(
                        rect.center() + vec2(0.0, 24.0),
                        Align2::CENTER_CENTER,
                        "Could not display this picture. Click to open it.",
                        theme::regular(11.5),
                        palette.secondary,
                    );
                }
                if response.clicked() {
                    actions.push(Action::OpenFile(path.clone()));
                }
                rect
            }
        };
    }
    let size = frame_size(media, None, max_width, max_height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let thumbnail = message
            .thumbnail
            .as_deref()
            .filter(|_| sticker.is_none())
            .map(|bytes| thumbnail_uri(ui.ctx(), &message.chat, &message.id, bytes));
        match thumbnail {
            Some(uri) => {
                egui::Image::new(uri)
                    .fit_to_exact_size(size)
                    .corner_radius(radius)
                    .paint_at(ui, rect);
                ui.painter()
                    .rect_filled(rect, radius, Color32::from_black_alpha(60));
            }
            None => {
                ui.painter().rect_filled(rect, radius, palette.surface);
            }
        }
        let disc = Rect::from_center_size(rect.center(), Vec2::splat(44.0));
        match &media.state {
            MediaState::Downloading => {
                // Lifted, once the picture is here, by `paint_download_veil`.
                if sticker.is_none() {
                    theme::animate(ui.ctx(), download_veil_id(message), true, 0.0);
                }
                ui.painter()
                    .circle_filled(disc.center(), 22.0, Color32::from_black_alpha(120));
                theme::paint_spinner(ui, disc, 22.0, Color32::WHITE);
            }
            MediaState::Failed(_) => {
                ui.painter()
                    .circle_filled(disc.center(), 22.0, Color32::from_black_alpha(120));
                theme::paint_icon(ui, Icon::CircleAlert, disc, 22.0, palette.danger);
                ui.painter().text(
                    rect.center() + vec2(0.0, 34.0),
                    Align2::CENTER_CENTER,
                    "Download failed. Click to retry.",
                    theme::regular(11.5),
                    Color32::WHITE,
                );
            }
            MediaState::Idle => {
                ui.painter()
                    .circle_filled(disc.center(), 22.0, Color32::from_black_alpha(120));
                theme::paint_icon(
                    ui,
                    if sticker.is_some() {
                        Icon::Sticker
                    } else {
                        Icon::Download
                    },
                    disc,
                    22.0,
                    Color32::WHITE,
                );
                if sticker.is_none() {
                    ui.painter().text(
                        rect.center() + vec2(0.0, 34.0),
                        Align2::CENTER_CENTER,
                        crate::util::bytes(media.size),
                        theme::regular(11.5),
                        Color32::WHITE,
                    );
                }
            }
        }
    }
    let wants = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
        && !matches!(media.state, MediaState::Downloading);
    let auto = ui.is_rect_visible(rect)
        && matches!(media.state, MediaState::Idle)
        && auto_download_allowed(media, sticker.is_some(), view.auto_download);
    if wants || auto {
        actions.push(Action::Download {
            manual: wants,
            card: None,
            chat: view.chat.id.clone(),
            message: message.id.clone(),
        });
    }
    rect
}

/// Draws a motion photo's badge in the corner of its picture, and its clip
/// over the picture while it plays, muted and looping.
fn motion_clip(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    motion: &crate::model::Motion,
    picture: Rect,
    actions: &mut Vec<Action>,
) {
    if !ui.is_rect_visible(picture) {
        return;
    }
    let playing = view.motion_playing == Some(message.id.as_str());
    let toggle = || Action::ToggleMotion {
        chat: view.chat.id.clone(),
        message: message.id.clone(),
    };
    let badge = Rect::from_min_size(picture.min + vec2(6.0, 6.0), Vec2::splat(28.0));
    let response = ui
        .interact(
            badge,
            bubble_id(&view.chat.id, &message.id).with("motion"),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        actions.push(toggle());
    }
    let frame = motion
        .path
        .as_ref()
        .filter(|_| playing)
        .map(|path| (path, animation::frame(ui, path, picture, view.animate)));
    let icon = match (&frame, &motion.state) {
        (Some((_, animation::Frame::Ready(texture))), _) => {
            ui.painter().rect_filled(picture, 6.0, Color32::BLACK);
            paint_texture(
                ui,
                fit_within(texture.size_vec2(), picture),
                texture.id(),
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                6.0,
            );
            Some(Icon::Pause)
        }
        (Some((path, animation::Frame::Unavailable)), _) => {
            // No decoder here plays it: the system player gets the clip.
            actions.push(toggle());
            actions.push(Action::OpenFile((*path).clone()));
            Some(Icon::Play)
        }
        (Some((_, animation::Frame::Pending)), _) => None,
        (None, MediaState::Downloading) if playing => None,
        (None, MediaState::Failed(_)) => Some(Icon::CircleAlert),
        (None, _) => Some(Icon::Play),
    };
    ui.painter()
        .circle_filled(badge.center(), 14.0, Color32::from_black_alpha(140));
    match icon {
        Some(icon) => theme::paint_icon(ui, icon, badge, 14.0, Color32::WHITE),
        None => theme::paint_spinner(ui, badge, 16.0, Color32::WHITE),
    }
    if let MediaState::Failed(error) = &motion.state {
        response.on_hover_text(error);
    }
}

/// Stickers always download when visible, while other media follows the setting.
/// Every automatic download still respects the shared size limit.
fn auto_download_allowed(media: &Media, sticker: bool, auto_download: bool) -> bool {
    media.is_within_download_limit() && (sticker || auto_download)
}

/// Draws a video. GIFs play in place; other videos play in the bubble once
/// clicked, downloading first when needed, with controls along the bottom.
#[allow(clippy::too_many_arguments)]
fn video(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    media: &Media,
    seconds: Option<u32>,
    gif: bool,
    width: f32,
    actions: &mut Vec<Action>,
) -> f32 {
    use crate::video::State;
    let palette = view.palette;
    let radius = picture_radius(&palette, message);
    // A video sent before ZapFast made thumbnails has none; its file is
    // here, so its first frame stands in. Without either, it is a file card.
    let thumbnail = message.thumbnail.as_deref();
    if thumbnail.is_none() && media.path.is_none() {
        let title = if gif { "GIF" } else { "Video" };
        let mut detail = Vec::new();
        if let Some(seconds) = seconds {
            detail.push(crate::util::duration(seconds));
        }
        detail.push(crate::util::bytes(media.size));
        attachment(
            ui,
            view,
            message,
            media,
            Icon::Video,
            title,
            &detail.join(" · "),
            width,
            actions,
        );
        return width;
    }
    let limit = width.min(picture_limit(&palette, message));
    // Without its size, a widescreen frame that fills the bubble: the hint is
    // in pixels, so a bare 16 by 9 would shrink it to the narrowest picture.
    let size = frame_size(
        media,
        Some((1280, 720)),
        limit,
        PICTURE_HEIGHT.min(limit * 1.3),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    ui.ctx()
        .data_mut(|data| data.insert_temp(video_flight_id(&message.chat, &message.id), rect));
    let playing = match (&media.path, gif) {
        (Some(path), true) => Some(animation::frame(
            ui,
            path,
            rect,
            view.animate && response.hovered(),
        )),
        _ => None,
    };
    if let Some(animation::Frame::Ready(texture)) = &playing {
        if ui.is_rect_visible(rect) {
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && let Some(path) = &media.path
        {
            actions.push(Action::OpenFile(path.clone()));
        }
        return size.x;
    }
    let status = media
        .path
        .as_ref()
        .filter(|_| !gif)
        .and_then(|_| view.video.status(&message.id));
    if ui.is_rect_visible(rect) {
        if status.is_some() {
            view.video.saw(&message.id);
        }
        match status.as_ref().and_then(|status| status.frame.as_ref()) {
            Some(frame) => {
                ui.painter().rect_filled(rect, radius, Color32::BLACK);
                paint_texture(
                    ui,
                    fit_within(frame.size_vec2(), rect),
                    frame.id(),
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    radius,
                );
            }
            None => match (thumbnail, &media.path) {
                (Some(thumbnail), _) => {
                    // Registering the poster decodes it, so it waits for the row to show.
                    let uri = thumbnail_uri(ui.ctx(), &message.chat, &message.id, thumbnail);
                    egui::Image::new(uri)
                        .fit_to_exact_size(size)
                        .corner_radius(radius)
                        .paint_at(ui, rect);
                }
                (None, Some(path)) => {
                    ui.painter().rect_filled(rect, radius, Color32::BLACK);
                    if let animation::Frame::Ready(texture) =
                        animation::frame(ui, path, rect, false)
                    {
                        paint_texture(
                            ui,
                            fit_within(texture.size_vec2(), rect),
                            texture.id(),
                            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            radius,
                        );
                    }
                }
                (None, None) => {}
            },
        }
        let state = status.as_ref().map(|status| status.state);
        if state != Some(State::Playing) {
            ui.painter()
                .rect_filled(rect, radius, Color32::from_black_alpha(40));
            let disc = Rect::from_center_size(rect.center(), Vec2::splat(48.0));
            ui.painter()
                .circle_filled(disc.center(), 24.0, Color32::from_black_alpha(140));
            match (&media.path, &media.state) {
                (Some(_), _)
                    if state == Some(State::Loading)
                        || matches!(playing, Some(animation::Frame::Pending)) =>
                {
                    theme::paint_spinner(ui, disc, 24.0, Color32::WHITE)
                }
                (Some(_), _) if gif => {
                    theme::paint_icon(ui, Icon::ExternalLink, disc, 22.0, Color32::WHITE)
                }
                (None, MediaState::Downloading) => {
                    theme::paint_spinner(ui, disc, 24.0, Color32::WHITE)
                }
                (None, MediaState::Failed(_)) => {
                    theme::paint_icon(ui, Icon::CircleAlert, disc, 22.0, palette.danger)
                }
                (Some(_), _) | (None, MediaState::Idle) => {
                    theme::paint_icon(ui, Icon::Play, disc, 22.0, Color32::WHITE)
                }
            }
        }
        match (&status, &media.path) {
            (Some(status), Some(path)) => {
                // Controls show while paused and while the pointer is over
                // the video, as in other players.
                if status.state == State::Paused
                    || (status.state == State::Playing && ui.rect_contains_pointer(rect))
                {
                    video_controls(
                        ui,
                        &VideoControls {
                            player: view.video,
                            locale: view.locale,
                            accent: view.palette.accent,
                            expanded: false,
                        },
                        &message.id,
                        path,
                        rect,
                        status,
                        actions,
                    );
                }
            }
            _ => {
                let mut label = Vec::new();
                if gif {
                    label.push("GIF".to_owned());
                }
                if let Some(seconds) = seconds {
                    label.push(crate::util::duration(seconds));
                }
                if media.path.is_none() {
                    label.push(crate::util::bytes(media.size));
                }
                if !label.is_empty() {
                    let galley = ui.painter().layout_no_wrap(
                        label.join(" · "),
                        theme::medium(11.5),
                        Color32::WHITE,
                    );
                    let chip = Rect::from_min_size(
                        pos2(rect.left() + 8.0, rect.bottom() - galley.size().y - 14.0),
                        galley.size() + vec2(12.0, 6.0),
                    );
                    ui.painter().rect_filled(
                        chip,
                        chip.height() / 2.0,
                        Color32::from_black_alpha(140),
                    );
                    ui.painter()
                        .galley(chip.min + vec2(6.0, 3.0), galley, Color32::WHITE);
                }
            }
        }
    }
    let auto = ui.is_rect_visible(rect)
        && media.path.is_none()
        && matches!(media.state, MediaState::Idle)
        && view.auto_download
        && media.is_within_download_limit();
    if auto {
        actions.push(Action::Download {
            manual: false,
            card: None,
            chat: view.chat.id.clone(),
            message: message.id.clone(),
        });
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        video_clicked(view, message, media, gif, auto, actions);
    }
    // The two clicks of a double-click play and pause between them; the
    // video then opens over the window as it was.
    if response.double_clicked()
        && !gif
        && let Some(path) = &media.path
    {
        actions.push(Action::ExpandVideo {
            message: message.id.clone(),
            path: path.clone(),
        });
    }
    size.x
}

/// What a click on a video does: a downloaded video plays or pauses, a GIF
/// that cannot play here opens in the system viewer, and a video still on
/// WhatsApp's servers downloads and then plays. The full-window viewer is a
/// separate way in, from the message's own menu, so a clip keeps playing in
/// its bubble here. `downloading` is set when this frame already asked for the
/// download.
fn video_clicked(
    view: &View<'_>,
    message: &Message,
    media: &Media,
    gif: bool,
    downloading: bool,
    actions: &mut Vec<Action>,
) {
    match &media.path {
        Some(path) if gif => actions.push(Action::OpenFile(path.clone())),
        Some(path) => actions.push(Action::PlayVideo {
            message: message.id.clone(),
            path: path.clone(),
        }),
        None => {
            if !downloading && !matches!(media.state, MediaState::Downloading) {
                actions.push(Action::Download {
                    manual: true,
                    card: None,
                    chat: view.chat.id.clone(),
                    message: message.id.clone(),
                });
            }
            if !gif {
                actions.push(Action::PlayVideoWhenDownloaded(message.id.clone()));
            }
        }
    }
}

/// What the video controls need besides the video: the bubble and the view
/// covering the window share them.
pub(crate) struct VideoControls<'a> {
    pub player: &'a crate::video::Player,
    pub locale: crate::i18n::Locale,
    pub accent: Color32,
    /// Whether the video covers the window; its button then puts it back.
    pub expanded: bool,
}

/// Play/pause, the time, a seek bar, a sound switch, and the switch between
/// the message and the whole window, along the bottom of a playing video.
pub(crate) fn video_controls(
    ui: &mut egui::Ui,
    controls: &VideoControls<'_>,
    message: &str,
    path: &Path,
    rect: Rect,
    status: &crate::video::Status,
    actions: &mut Vec<Action>,
) {
    let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 32.0), rect.max);
    ui.painter().rect_filled(
        bar,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 6,
            se: 6,
        },
        Color32::from_black_alpha(150),
    );
    let id = ui.id().with(("video-controls", message));
    let toggle = Rect::from_center_size(pos2(bar.left() + 18.0, bar.center().y), Vec2::splat(26.0));
    let playing = status.state == crate::video::State::Playing;
    theme::paint_icon(
        ui,
        if playing { Icon::Pause } else { Icon::Play },
        toggle,
        16.0,
        Color32::WHITE,
    );
    let tooltip = if playing {
        crate::i18n::gettext(controls.locale, "Pause")
    } else {
        crate::i18n::gettext(controls.locale, "Play")
    };
    if ui
        .interact(toggle, id.with("toggle"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip.as_ref())
        .clicked()
    {
        actions.push(Action::PlayVideo {
            message: message.to_owned(),
            path: path.to_owned(),
        });
    }
    let window =
        Rect::from_center_size(pos2(bar.right() - 18.0, bar.center().y), Vec2::splat(26.0));
    theme::paint_icon(
        ui,
        if controls.expanded {
            Icon::Minimize
        } else {
            Icon::Maximize
        },
        window,
        16.0,
        Color32::WHITE,
    );
    let tooltip = if controls.expanded {
        crate::i18n::gettext(controls.locale, "Back to the message (Esc)")
    } else {
        crate::i18n::gettext(controls.locale, "Fill the window")
    };
    if ui
        .interact(window, id.with("window"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip.as_ref())
        .clicked()
    {
        actions.push(if controls.expanded {
            Action::CloseImagePreview
        } else {
            Action::ExpandVideo {
                message: message.to_owned(),
                path: path.to_owned(),
            }
        });
    }
    let sound = Rect::from_center_size(
        pos2(window.left() - 15.0, bar.center().y),
        Vec2::splat(26.0),
    );
    let muted = controls.player.muted();
    theme::paint_icon(
        ui,
        if muted { Icon::VolumeX } else { Icon::Volume2 },
        sound,
        16.0,
        Color32::WHITE,
    );
    let tooltip = if muted {
        crate::i18n::gettext(controls.locale, "Unmute")
    } else {
        crate::i18n::gettext(controls.locale, "Mute")
    };
    if ui
        .interact(sound, id.with("sound"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip.as_ref())
        .clicked()
    {
        actions.push(Action::ToggleVideoSound);
    }
    // The playback speed, cycled by a click as the voice message chip is.
    let speed_label = crate::audio::speed_label(controls.player.speed());
    let speed_galley = ui.painter().layout_no_wrap(
        speed_label,
        theme::tabular(fastframe_fonts::Weight::Medium, 11.5),
        Color32::WHITE,
    );
    let speed = Rect::from_center_size(
        pos2(
            sound.left() - 4.0 - (speed_galley.size().x + 10.0) / 2.0,
            bar.center().y,
        ),
        vec2(speed_galley.size().x + 10.0, 18.0),
    );
    ui.painter()
        .rect_filled(speed, 9.0, Color32::from_white_alpha(40));
    ui.painter().galley(
        speed.center() - speed_galley.size() / 2.0,
        speed_galley,
        Color32::WHITE,
    );
    if ui
        .interact(speed, id.with("speed"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        actions.push(Action::CycleVideoSpeed);
    }
    let time = format!(
        "{} / {}",
        crate::util::duration(status.position.as_secs() as u32),
        crate::util::duration(status.total.as_secs() as u32)
    );
    let galley = ui.painter().layout_no_wrap(
        time,
        theme::tabular(fastframe_fonts::Weight::Medium, 11.5),
        Color32::WHITE,
    );
    let text = pos2(toggle.right() + 4.0, bar.center().y - galley.size().y / 2.0);
    let track = Rect::from_min_max(
        pos2(text.x + galley.size().x + 10.0, bar.center().y - 8.0),
        pos2(speed.left() - 6.0, bar.center().y + 8.0),
    );
    ui.painter().galley(text, galley, Color32::WHITE);
    if track.width() < 12.0 {
        return;
    }
    let response = ui
        .interact(track, id.with("seek"), Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let pointed = response
        .interact_pointer_pos()
        .map(|pointer| ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0));
    // Seeking restarts the decoder, so a drag seeks once, where it ends.
    let fraction = match pointed {
        Some(fraction) if response.dragged() => fraction,
        _ => status.fraction(),
    };
    let line = Rect::from_center_size(track.center(), vec2(track.width(), 3.0));
    ui.painter()
        .rect_filled(line, 1.5, Color32::from_white_alpha(90));
    let played = pos2(line.left() + fraction * line.width(), line.center().y);
    ui.painter().rect_filled(
        Rect::from_min_max(line.min, pos2(played.x, line.bottom())),
        1.5,
        controls.accent,
    );
    ui.painter().circle_filled(played, 5.0, controls.accent);
    if (response.clicked() || response.drag_stopped())
        && let Some(fraction) = pointed
    {
        actions.push(Action::SeekVideo {
            message: message.to_owned(),
            fraction,
        });
    }
}

/// Side of a round video message.
const NOTE_SIDE: f32 = 220.0;

/// Draws a round video message (PTV) as a circle that plays in place when
/// clicked, with a ring for the progress. Returns its width.
fn video_note(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    media: &Media,
    seconds: Option<u32>,
    actions: &mut Vec<Action>,
) -> f32 {
    use crate::video::State;
    let palette = view.palette;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(NOTE_SIDE), Sense::click());
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            video_flight_id(&message.chat, &message.id),
            rect.shrink(5.0),
        );
        data.insert_temp(
            crate::ui::image_preview::video_note_id(&message.chat, &message.id),
            true,
        );
    });
    let status = media
        .path
        .as_ref()
        .and_then(|_| view.video.status(&message.id));
    if ui.is_rect_visible(rect) {
        if status.is_some() {
            view.video.saw(&message.id);
        }
        let center = rect.center();
        // Leave room around the picture for the progress ring.
        let picture = rect.shrink(5.0);
        let frame = status
            .as_ref()
            .and_then(|status| status.frame.as_ref())
            .map(|frame| (frame.id(), frame.size_vec2()));
        let poster = || {
            let bytes = message.thumbnail.as_deref()?;
            let uri = thumbnail_uri(ui.ctx(), &message.chat, &message.id, bytes);
            match egui::Image::new(uri).load_for_size(ui.ctx(), picture.size()) {
                Ok(egui::load::TexturePoll::Ready { texture }) => Some((texture.id, texture.size)),
                _ => None,
            }
        };
        match frame.or_else(poster) {
            Some((texture, size)) => paint_texture(
                ui,
                picture,
                texture,
                crate::video::square_uv(size.x, size.y),
                picture.width() / 2.0,
            ),
            None => {
                ui.painter()
                    .circle_filled(center, picture.width() / 2.0, palette.surface);
            }
        }
        let state = status.as_ref().map(|status| status.state);
        let ring = NOTE_SIDE / 2.0 - 2.0;
        if let Some(status) = &status {
            ui.painter().circle_stroke(
                center,
                ring,
                Stroke::new(3.0, palette.secondary.gamma_multiply(0.35)),
            );
            ui.painter().add(egui::Shape::line(
                crate::video::arc(center, ring, status.fraction()),
                Stroke::new(3.0, palette.accent),
            ));
        }
        let hovered = ui.rect_contains_pointer(rect);
        let disc = Rect::from_center_size(center, Vec2::splat(48.0));
        let waiting = matches!(
            (&media.path, &media.state, state),
            (Some(_), _, Some(State::Loading)) | (None, MediaState::Downloading, _)
        );
        let icon = match (&media.path, &media.state, state) {
            _ if waiting => None,
            // Only a pause sign under the pointer covers a playing video.
            (Some(_), _, Some(State::Playing)) => hovered.then_some(Icon::Pause),
            (None, MediaState::Failed(_), _) => Some(Icon::CircleAlert),
            _ => Some(Icon::Play),
        };
        if state != Some(State::Playing) {
            ui.painter().circle_filled(
                center,
                picture.width() / 2.0,
                Color32::from_black_alpha(40),
            );
        }
        if waiting || icon.is_some() {
            ui.painter()
                .circle_filled(center, 24.0, Color32::from_black_alpha(140));
        }
        if waiting {
            theme::paint_spinner(ui, disc, 24.0, Color32::WHITE);
        } else if let Some(icon) = icon {
            let color = if icon == Icon::CircleAlert {
                palette.danger
            } else {
                Color32::WHITE
            };
            theme::paint_icon(ui, icon, disc, 22.0, color);
        }
        // The time while it plays, otherwise its length.
        let label = match &status {
            Some(status) => Some(crate::util::duration(status.position.as_secs() as u32)),
            None => seconds
                .map(crate::util::duration)
                .or_else(|| media.path.is_none().then(|| crate::util::bytes(media.size))),
        };
        if let Some(label) = label {
            let galley = ui.painter().layout_no_wrap(
                label,
                theme::tabular(fastframe_fonts::Weight::Medium, 11.5),
                Color32::WHITE,
            );
            let chip = Rect::from_center_size(
                pos2(center.x, picture.bottom() - galley.size().y / 2.0 - 16.0),
                galley.size() + vec2(12.0, 6.0),
            );
            ui.painter()
                .rect_filled(chip, chip.height() / 2.0, Color32::from_black_alpha(140));
            ui.painter()
                .galley(chip.min + vec2(6.0, 3.0), galley, Color32::WHITE);
        }
    }
    let auto = ui.is_rect_visible(rect)
        && media.path.is_none()
        && matches!(media.state, MediaState::Idle)
        && view.auto_download
        && media.is_within_download_limit();
    if auto {
        actions.push(Action::Download {
            manual: false,
            card: None,
            chat: view.chat.id.clone(),
            message: message.id.clone(),
        });
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        video_clicked(view, message, media, false, auto, actions);
    }
    NOTE_SIDE
}

/// Paints a texture into `rect` with rounded corners; a radius of half the
/// side makes a circle.
fn paint_texture(ui: &egui::Ui, rect: Rect, texture: egui::TextureId, uv: Rect, radius: f32) {
    ui.painter().add(
        egui::epaint::RectShape::filled(
            rect,
            CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8),
            Color32::WHITE,
        )
        .with_texture(texture, uv),
    );
}

/// The largest rect with the proportions of `size` centred in `rect`.
fn fit_within(size: Vec2, rect: Rect) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return rect;
    }
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    Rect::from_center_size(rect.center(), size * scale)
}

/// How wide the round icon of a file card is.
const ATTACHMENT_DISC: f32 = 36.0;
/// The least height of a file card's contents: its icon.
const ATTACHMENT_ROW: f32 = ATTACHMENT_DISC;
/// The space between a file card's icon, its text and its action.
const ATTACHMENT_GAP: f32 = 12.0;
/// The space between a file's name and the line under it.
const ATTACHMENT_LINE_GAP: f32 = 2.0;
/// How wide a file card's open or download action is.
const ATTACHMENT_ACTION: f32 = 20.0;

/// The room around a file card inside its bubble, and inside the card around
/// its contents. A Nothing bubble holds the card with the even rim it gives a
/// photo, and the card keeps the room a text bubble gives its text, so a file
/// lines up with the messages around it.
fn attachment_insets(palette: &Palette) -> (Vec2, Vec2) {
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
fn attachment_radius(palette: &Palette) -> f32 {
    if palette.bubbles == theme::BubbleStyle::Nothing {
        f32::from(widgets::NOTHING_BUBBLE_RADIUS) - f32::from(NOTHING_ATTACHMENT_RIM)
    } else {
        8.0
    }
}

/// The even rim of a Nothing bubble around a file card, as around a photo.
const NOTHING_ATTACHMENT_RIM: i8 = 6;

#[allow(clippy::too_many_arguments)]
fn attachment(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    media: &Media,
    icon: Icon,
    title: &str,
    detail: &str,
    width: f32,
    actions: &mut Vec<Action>,
) {
    let palette = view.palette;
    let (_, inset) = attachment_insets(&palette);
    let text_left = ATTACHMENT_DISC + ATTACHMENT_GAP;
    let text_width =
        (width - inset.x * 2.0 - text_left - ATTACHMENT_GAP - ATTACHMENT_ACTION).max(0.0);
    let name = widgets::line(ui, title, theme::medium(14.0), palette.text, text_width, 2);
    let detail = match &media.state {
        MediaState::Failed(error) => format!("{error}. Click to retry."),
        _ => detail.to_owned(),
    };
    let detail = widgets::line(
        ui,
        &detail,
        theme::regular(12.0),
        palette.secondary,
        text_width,
        1,
    );
    let lines = name.size().y + ATTACHMENT_LINE_GAP + detail.size().y;
    // The card is as tall as its icon or its two lines, whichever is more,
    // with the same room above and below.
    let (rect, _) = ui.allocate_exact_size(
        vec2(width, lines.max(ATTACHMENT_ROW) + inset.y * 2.0),
        Sense::hover(),
    );
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(
            rect,
            attachment_radius(&palette),
            palette.window.gamma_multiply(0.35),
        );
        let content = rect.shrink2(inset);
        let disc = pos2(content.left() + ATTACHMENT_DISC / 2.0, content.center().y);
        ui.painter().circle_filled(
            disc,
            ATTACHMENT_DISC / 2.0,
            palette.accent.gamma_multiply(0.25),
        );
        theme::paint_icon(
            ui,
            icon,
            Rect::from_center_size(disc, Vec2::splat(18.0)),
            18.0,
            palette.accent,
        );
        let top = content.center().y - lines / 2.0;
        name.paint(ui, pos2(content.left() + text_left, top), palette.text);
        detail.paint(
            ui,
            pos2(
                content.left() + text_left,
                top + name.size().y + ATTACHMENT_LINE_GAP,
            ),
            palette.secondary,
        );
        let action = Rect::from_center_size(
            pos2(
                content.right() - ATTACHMENT_ACTION / 2.0,
                content.center().y,
            ),
            Vec2::splat(ATTACHMENT_ACTION),
        );
        match (&media.path, &media.state) {
            (Some(_), _) => {
                theme::paint_icon(ui, Icon::ExternalLink, action, 18.0, palette.secondary);
            }
            (None, MediaState::Downloading) => {
                theme::paint_spinner(ui, action, 18.0, palette.accent);
                ui.ctx().request_repaint();
            }
            (None, _) => {
                theme::paint_icon(ui, Icon::Download, action, 18.0, palette.secondary);
            }
        }
    }
    let auto = ui.is_rect_visible(rect)
        && media.path.is_none()
        && matches!(media.state, MediaState::Idle)
        && view.auto_download
        && media.is_within_download_limit();
    if auto {
        actions.push(Action::Download {
            manual: false,
            card: None,
            chat: view.chat.id.clone(),
            message: message.id.clone(),
        });
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(bubble_id(&view.chat.id, &message.id).with("card"), rect);
    });
    let response = ui
        .interact(
            rect,
            ui.id().with(("attachment", &message.id)),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() && !auto {
        match &media.path {
            Some(path) => actions.push(Action::OpenFile(path.clone())),
            None if !matches!(media.state, MediaState::Downloading) => {
                actions.push(Action::Download {
                    manual: true,
                    card: None,
                    chat: view.chat.id.clone(),
                    message: message.id.clone(),
                })
            }
            None => {}
        }
    }
}

/// The space above a conversation's messages that keeps a short one at the
/// bottom of the view: what `natural`, the height the messages took last
/// frame, leaves of the viewport. Unmeasured, there is none.
fn bottom_filler(viewport: f32, natural: Option<f32>, spacing: f32) -> f32 {
    natural.map_or(0.0, |natural| (viewport - natural - spacing).max(0.0))
}

/// Draws the in-chat player with a waveform for voice notes or a track and music badge
/// for audio files. Playback, seeking and speed controls remain shared between both types.
#[allow(clippy::too_many_arguments)]
fn voice_player(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    media: &Media,
    seconds: Option<u32>,
    waveform: &[u8],
    width: f32,
    reserve: f32,
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    use crate::audio::State;
    let palette = view.palette;
    let status = view.player.status(&message.id);
    let button = 36.0;
    let bar_height = 30.0;
    let chip = 44.0;
    let voice_note = matches!(
        message.content,
        Content::Audio {
            voice_note: true,
            ..
        }
    );
    // The chip appears with the playable clip; the waveform takes its space
    // back while the audio is still downloading.
    let shows_chip = media.path.is_some();
    let wave_width = (width
        - button
        - 10.0
        - if shows_chip { chip + 10.0 } else { 0.0 }
        - if voice_note { 0.0 } else { button + 10.0 })
    .max(0.0);
    let bars: Vec<u8> = if !waveform.is_empty() {
        waveform.to_vec()
    } else if let Some(bars) = view.player.bars(&message.id) {
        bars.to_vec()
    } else {
        vec![12; crate::voice::BARS]
    };
    let fill = palette.accent.gamma_multiply(0.22);
    let hover = palette.accent.gamma_multiply(0.38);
    let waiting = |ui: &mut egui::Ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(button), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), button / 2.0, fill);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.centered_and_justified(|ui| {
                theme::spinner(ui, 18.0, palette.accent);
            });
        });
    };
    // Force left-to-right layout at the player's width inside own bubbles.
    let mut duration = None;
    let row = ui.allocate_ui_with_layout(
        vec2(width.max(0.0), button),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            match (&media.path, &media.state) {
                (None, MediaState::Downloading) => waiting(ui),
                (None, _) => {
                    if theme::circle_button(
                        ui,
                        Icon::Download,
                        button,
                        fill,
                        hover,
                        palette.accent,
                        "Download",
                    )
                    .clicked()
                    {
                        actions.push(Action::Download {
                            manual: true,
                            card: None,
                            chat: view.chat.id.clone(),
                            message: message.id.clone(),
                        });
                    }
                }
                (Some(path), _) => match status.state {
                    State::Loading => waiting(ui),
                    State::Playing | State::Paused | State::Idle => {
                        let (icon, tooltip) = if status.state == State::Playing {
                            (Icon::Pause, "Pause")
                        } else {
                            (Icon::Play, "Play")
                        };
                        if theme::circle_button(
                            ui,
                            icon,
                            button,
                            fill,
                            hover,
                            palette.accent,
                            tooltip,
                        )
                        .clicked()
                        {
                            actions.push(Action::PlayVoice {
                                message: message.id.clone(),
                                path: path.clone(),
                            });
                        }
                    }
                },
            }
            let mut wave_middle = None;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let (rect, response) =
                    ui.allocate_exact_size(vec2(wave_width, bar_height), Sense::click());
                wave_middle = Some(rect.center().y);
                let pitch = 3.0;
                let count = (rect.width() / pitch).floor() as usize;
                let fraction = if status.total > Duration::ZERO {
                    status.position.as_secs_f32() / status.total.as_secs_f32()
                } else {
                    0.0
                };
                let played_until = rect.left() + fraction * rect.width();
                let quiet = palette.secondary.gamma_multiply(0.7);
                let kind = if voice_note {
                    "voice-waveform"
                } else {
                    "audio-track"
                };
                ui.ctx().data_mut(|data| {
                    data.insert_temp(bubble_id(&view.chat.id, &message.id).with(kind), rect);
                });
                if !voice_note {
                    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
                    ui.painter().rect_filled(track, 2.0, quiet);
                    if fraction > 0.0 {
                        ui.painter().rect_filled(
                            Rect::from_min_max(track.min, pos2(played_until, track.bottom())),
                            2.0,
                            palette.accent,
                        );
                    }
                } else if count > 0 {
                    for index in 0..count {
                        let level = f32::from(bars[index * bars.len() / count]) / 100.0;
                        let height = (2.0 + level * (bar_height - 4.0)).max(2.0);
                        let x = rect.left() + index as f32 * pitch + 1.0;
                        let colour = if status.state != State::Idle && x <= played_until {
                            palette.accent
                        } else {
                            quiet
                        };
                        ui.painter().rect_filled(
                            Rect::from_center_size(
                                egui::pos2(x, rect.center().y),
                                vec2(2.0, height),
                            ),
                            1.0,
                            colour,
                        );
                    }
                }
                if (!voice_note || matches!(status.state, State::Playing | State::Paused))
                    && rect.width() >= 10.0
                {
                    let knob = played_until.clamp(rect.left() + 5.0, rect.right() - 5.0);
                    ui.painter().circle_filled(
                        egui::pos2(knob, rect.center().y),
                        5.0,
                        palette.accent,
                    );
                }
                if let Some(path) = &media.path {
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked()
                        && let Some(pointer) = response.interact_pointer_pos()
                    {
                        let fraction = if rect.width() > 0.0 {
                            ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        actions.push(Action::SeekVoice {
                            message: message.id.clone(),
                            path: path.clone(),
                            fraction,
                        });
                    }
                }
                // Show playback position while active, otherwise total duration.
                let shown = match status.state {
                    State::Playing | State::Paused => {
                        crate::util::duration(status.position.as_secs() as u32)
                    }
                    _ => seconds
                        .or_else(|| {
                            (status.total > Duration::ZERO).then_some(status.total.as_secs() as u32)
                        })
                        .map(crate::util::duration)
                        .unwrap_or_else(|| crate::util::bytes(media.size)),
                };
                let (text, font) = match &media.state {
                    MediaState::Failed(error) => {
                        (format!("{error}. Click to retry."), theme::regular(11.5))
                    }
                    _ => (
                        shown,
                        theme::tabular(fastframe_fonts::Weight::Regular, 11.5),
                    ),
                };
                duration = Some(theme::text(ui, text, font, palette.secondary).rect);
            });
            if !voice_note {
                let (slot, response) = ui.allocate_exact_size(Vec2::splat(button), Sense::hover());
                let at = Rect::from_center_size(
                    pos2(slot.center().x, wave_middle.unwrap_or(slot.center().y)),
                    Vec2::splat(button),
                );
                ui.painter().circle_filled(at.center(), button / 2.0, fill);
                theme::paint_icon(ui, Icon::Music, at, 20.0, palette.accent);
                let label = crate::i18n::gettext(view.locale, "Audio");
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label.as_ref())
                });
                ui.ctx().data_mut(|data| {
                    data.insert_temp(bubble_id(&view.chat.id, &message.id).with("audio-file"), at);
                });
            }
            // Speed chip, cycling 1x, 1.5x, and 2x like the phone. The
            // message menu lists every speed, including 1.25x and 1.75x.
            if shows_chip {
                let speed = view.voice_speed;
                // The label follows the click at once, faded until this
                // clip actually plays at that speed.
                let preparing = view.player.preparing_speed(&message.id);
                // Reserve the chip's place in the row, then draw it level
                // with the middle of the waveform rather than the whole row.
                let size = vec2(chip, 20.0);
                let (slot, _) = ui.allocate_exact_size(size, Sense::hover());
                let at = Rect::from_center_size(
                    egui::pos2(slot.center().x, wave_middle.unwrap_or(slot.center().y)),
                    size,
                );
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
                        speed_pill(ui, view, size, speed, speed > 1.0, preparing)
                    })
                    .inner;
                ui.ctx().data_mut(|data| {
                    data.insert_temp(speed_chip_id(&view.chat.id, &message.id), response.rect);
                });
                if response.clicked() {
                    actions.push(speed_action(
                        view.chat,
                        crate::audio::next_cycled_speed(speed),
                    ));
                }
                response.on_hover_text(if preparing {
                    "Preparing playback speed"
                } else {
                    "Playback speed. Right-click for every speed."
                });
            }
        },
    );
    let auto = media.path.is_none()
        && matches!(media.state, MediaState::Idle)
        && view.auto_download
        && media.is_within_download_limit();
    if auto {
        actions.push(Action::Download {
            manual: false,
            card: None,
            chat: view.chat.id.clone(),
            message: message.id.clone(),
        });
    }
    // The time and ticks share the duration's line, as on the phone. A long
    // error leaves them no room, so they keep a line of their own.
    let right = row.response.rect.right();
    duration
        .filter(|duration| duration.right() + 8.0 + reserve <= right)
        .map(|duration| {
            Rect::from_min_max(
                pos2(right - reserve, duration.center().y - 7.5),
                pos2(right, duration.center().y + 7.5),
            )
        })
}

/// Voice-recording controls and live waveform.
fn recording_strip(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let (elapsed, levels, listening) = match app.recording.as_ref() {
        Some(recorder) => (recorder.elapsed(), recorder.levels(), recorder.listening()),
        None => return,
    };
    // As tall as the one-line composer it replaces, so the field keeps its
    // height while recording.
    let line_height = ui
        .painter()
        .layout_no_wrap("x".to_owned(), theme::regular(BODY_SIZE), palette.text)
        .size()
        .y;
    let row_height = (line_height + COMPOSER_PADDING)
        .round()
        .max(COMPOSER_CONTROL);
    let button = row_height;
    let ctx = ui.ctx().clone();
    let (entry, loudness, slide) = recording_motion(&ctx, &levels);
    // As in WhatsApp: discard at the start, the light and the time, the
    // waveform across the rest, and send at the end. The row slides in from
    // the right as it appears; only its drawing moves, never its layout.
    let (row_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), row_height),
        Sense::hover(),
    );
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row_rect.translate(vec2(RECORDING_SLIDE * (1.0 - entry), 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row.set_opacity(entry);
    let ui = &mut row;
    {
        {
            ui.spacing_mut().item_spacing.x = 10.0;
            if theme::circle_button(
                ui,
                Icon::Trash,
                button,
                palette.surface,
                palette.surface_hover,
                palette.secondary,
                "Discard",
            )
            .clicked()
            {
                crate::haptics::tap(crate::haptics::Feel::Alignment);
                app.actions.push(Action::CancelRecording);
            }
            // The light breathes while recording; while the microphone wakes
            // it stays dim, and the time at zero.
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            let light = if listening {
                let (scale, opacity) = recording_breath(ctx.input(|input| input.time) as f32);
                (palette.danger.gamma_multiply(opacity), 5.0 * scale)
            } else {
                (palette.secondary.gamma_multiply(0.5), 5.0)
            };
            ui.painter().circle_filled(dot.center(), light.1, light.0);
            theme::text(
                ui,
                crate::util::duration(elapsed.as_secs() as u32),
                theme::tabular(fastframe_fonts::Weight::Medium, 14.0),
                palette.text,
            );
            // Recent audio levels, newest on the right against send.
            let spacing = ui.spacing().item_spacing.x;
            let wave_width = (ui.available_width() - button - spacing).max(0.0);
            let (rect, _) = ui.allocate_exact_size(vec2(wave_width, 28.0), Sense::hover());
            ui.ctx()
                .data_mut(|data| data.insert_temp(recording_wave_id(), rect));
            let pitch = 3.0;
            let count = (rect.width() / pitch).floor() as usize;
            let shown = &levels[levels.len().saturating_sub(count)..];
            // The newest bar slides in from the right by the share of its
            // interval not yet elapsed, so bars glide rather than step.
            let wave = ui.painter().with_clip_rect(rect.expand2(vec2(0.0, 4.0)));
            for (index, level) in shown.iter().enumerate() {
                let height = 2.0_f32 + (level * 4.0).min(1.0) * 24.0;
                let x = rect.right() - (shown.len() - index) as f32 * pitch + 1.0 + slide * pitch;
                wave.rect_filled(
                    Rect::from_center_size(egui::pos2(x, rect.center().y), vec2(2.0, height)),
                    1.0,
                    palette.accent,
                );
            }
            // A soft circle behind send swells with the voice.
            let glow_center = egui::pos2(ui.cursor().min.x + button / 2.0, rect.center().y);
            ui.painter().circle_filled(
                glow_center,
                button / 2.0 * (1.0 + 0.35 * loudness),
                palette.accent.gamma_multiply(0.25),
            );
            if theme::circle_button(
                ui,
                Icon::Send,
                button,
                palette.accent,
                palette.accent_hover,
                palette.on_accent,
                "Send",
            )
            .clicked()
            {
                crate::haptics::tap(crate::haptics::Feel::Alignment);
                app.actions.push(Action::SendRecording);
            }
        }
    }
    // The light and the level keep moving only while recording.
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
}

/// How far the strip starts to the right of its place, in points.
const RECORDING_SLIDE: f32 = 12.0;
/// Seconds one pass of the recording light's breath takes.
const RECORDING_BREATH: f32 = 1.2;
/// Seconds the level circle takes to follow the voice (the time constant).
const RECORDING_LEVEL_TAU: f32 = 0.08;
/// Seconds between two recorded levels.
const RECORDING_LEVEL_STEP: f32 = 0.05;

/// The recording light's size and opacity at `time` seconds: a sine loop
/// between 0.85 and 1.0 of its size and 0.6 and 1.0 of its opacity.
fn recording_breath(time: f32) -> (f32, f32) {
    let wave = 0.5 + 0.5 * (std::f32::consts::TAU * time / RECORDING_BREATH).sin();
    (0.85 + 0.15 * wave, 0.6 + 0.4 * wave)
}

/// Moves `value` toward `target` over `dt` seconds with an exponential ease
/// of time constant `tau`.
fn ease_level(value: f32, target: f32, dt: f32, tau: f32) -> f32 {
    value + (target - value) * (1.0 - (-dt.max(0.0) / tau).exp())
}

/// The strip's motion state: how far it has slid in (0..1), the smoothed
/// loudness (0..1) and the share of the newest bar's slide still to go
/// (1..0). The first frame of a recording starts the entry and taps once.
fn recording_motion(ctx: &egui::Context, levels: &[f32]) -> (f32, f32, f32) {
    #[derive(Clone, Copy)]
    struct State {
        started: f64,
        seen: f64,
        loudness: f32,
        count: usize,
        changed: f64,
    }
    let id = egui::Id::new("recording-motion");
    let now = ctx.input(|input| input.time);
    let still = crate::motion::screen_reader_attached(ctx);
    let previous: Option<State> = ctx.data(|data| data.get_temp(id));
    // A gap of more than half a second means a new recording began.
    let mut state = match previous {
        Some(state) if now - state.seen < 0.5 => state,
        _ => {
            crate::haptics::tap(crate::haptics::Feel::Generic);
            State {
                started: now,
                seen: now,
                loudness: 0.0,
                count: levels.len(),
                changed: now,
            }
        }
    };
    let dt = (now - state.seen) as f32;
    let target = levels.last().map_or(0.0, |level| (level * 4.0).min(1.0));
    state.loudness = if still {
        target
    } else {
        ease_level(state.loudness, target, dt, RECORDING_LEVEL_TAU)
    };
    if levels.len() != state.count {
        state.count = levels.len();
        state.changed = now;
    }
    state.seen = now;
    let entry = if still {
        1.0
    } else {
        theme::ease_out((now - state.started) as f32 / theme::MOTION_PANEL)
    };
    let slide = if still {
        0.0
    } else {
        1.0 - theme::ease_out((now - state.changed) as f32 / RECORDING_LEVEL_STEP)
    };
    ctx.data_mut(|data| data.insert_temp(id, state));
    (entry, state.loudness, slide)
}

/// Where the recorder's waveform was drawn, for layout tests.
pub(crate) fn recording_wave_id() -> egui::Id {
    egui::Id::new("recording-wave")
}

/// A selecting row's check box: an outline, ticked in the accent as `tick`
/// goes from 0 to 1. The tick's fill is drawn at `scale` of the box, for its
/// pop, and the whole box at `opacity`, as the selection comes and goes.
fn select_box(ui: &egui::Ui, palette: &Palette, rect: Rect, tick: f32, scale: f32, opacity: f32) {
    if opacity <= 0.0 {
        return;
    }
    let tick = tick.clamp(0.0, 1.0);
    // Under a fill on its way in or out; a full one covers it.
    if tick < 1.0 {
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.5, palette.secondary.gamma_multiply(opacity)),
            egui::StrokeKind::Inside,
        );
    }
    if tick > 0.0 {
        let alpha = tick * opacity;
        let scale = scale.max(0.0);
        let filled = Rect::from_center_size(rect.center(), rect.size() * scale);
        ui.painter()
            .rect_filled(filled, 4.0 * scale, palette.accent.gamma_multiply(alpha));
        theme::paint_icon(
            ui,
            Icon::Check,
            filled,
            14.0 * scale,
            palette.on_accent.gamma_multiply(alpha),
        );
    }
}

/// How the check boxes, the selected rows' bands and the selection bar slide
/// as a selection starts and ends.
const SELECT_SLIDE: crate::motion::Spring = crate::motion::Spring::new(0.4, 0.15);
/// Seconds the selection's check boxes, bands and bar take to fade in or out.
const SELECT_FADE: f32 = 0.18;
/// A check box ticked pops from this share of its size on [`SELECT_POP`].
const SELECT_POP_FROM: f32 = 0.6;
/// The spring a ticked check box pops on.
const SELECT_POP: crate::motion::Spring = crate::motion::Spring::new(0.3, 0.25);
/// Seconds a row's band and tick take to fade as it joins or leaves the
/// selection.
const SELECT_TICK_FADE: f32 = 0.12;
/// How far below its place the selection bar rises from, and sinks to as
/// it leaves.
const SELECT_BAR_RISE: f32 = 14.0;

/// Where a chat's selection furniture (the check column, the selected rows'
/// bands and the bar over the composer) stands as a selection starts or
/// ends. Kept per chat in the context's memory and brought up to date once
/// a pass by [`selection_motion`]; every change of course starts from where
/// it is.
#[derive(Clone, Default)]
struct SelectionMotion {
    /// Whether the chat is selecting.
    on: bool,
    /// How far in the furniture has slid: 0 out, 1 in, a little past either
    /// on its spring. With its speed, per second.
    slide: f32,
    velocity: f32,
    /// How opaque the furniture is, and how opaque it was when it last
    /// turned, and when that was.
    opacity: f32,
    fade_from: f32,
    turned: f64,
    /// When this was last brought up to date, and the pass it was.
    time: f64,
    pass: Option<u64>,
    /// The messages ticked when last seen: while selecting, those ticked;
    /// once it ends, those to draw ticked on their way out.
    ticked: Vec<String>,
    /// When each message last joined (`true`) or left the selection, and
    /// how far its tick was in then, for its pop and its band's fade.
    marks: HashMap<String, (f64, bool, f32)>,
}

impl SelectionMotion {
    fn id(chat: &str) -> egui::Id {
        egui::Id::new(("selection-motion", chat))
    }

    fn target(&self) -> f32 {
        if self.on { 1.0 } else { 0.0 }
    }

    /// How opaque the furniture is at `now`, easing from where it turned.
    fn opacity_at(&self, now: f64) -> f32 {
        let t = ((now - self.turned) as f32 / SELECT_FADE).clamp(0.0, 1.0);
        if t >= 1.0 {
            return self.target();
        }
        egui::lerp(self.fade_from..=self.target(), theme::ease_out(t))
    }

    /// Whether the check column, bands or bar are still to be drawn while
    /// no selection is open: the selection is leaving.
    fn leaving(&self) -> bool {
        !self.on && (self.opacity > 0.0 || self.slide.abs() > 1e-3)
    }

    /// How far the row of a message the check column moves is drawn from
    /// where it is laid out: a row of the other side's slides with the
    /// column, an own row on the right stays where it is.
    fn row_shift(&self, from_me: bool) -> f32 {
        if from_me {
            0.0
        } else {
            SELECT_GUTTER * (self.slide - self.target())
        }
    }

    /// Where the check box for a row whose column starts at `left` is
    /// centred, level with `y`: in from the left edge as it slides.
    fn check_rect(&self, left: f32, y: f32) -> Rect {
        let x = left + SELECT_BOX / 2.0 - SELECT_GUTTER * (1.0 - self.slide);
        Rect::from_center_size(pos2(x, y), Vec2::splat(SELECT_BOX))
    }

    /// How far a message's tick and band are in at `now`, 0 to 1, when it
    /// is `ticked` or not.
    fn tick(&self, id: &str, ticked: bool, now: f64) -> f32 {
        let target = if ticked { 1.0 } else { 0.0 };
        match self.marks.get(id) {
            Some(&(at, joined, from)) if joined == ticked => {
                let t = ((now - at) as f32 / SELECT_TICK_FADE).clamp(0.0, 1.0);
                if t >= 1.0 {
                    target
                } else {
                    egui::lerp(from..=target, theme::ease_out(t))
                }
            }
            _ => target,
        }
    }

    /// The size of a message's tick at `now`, as a share of its box: a
    /// message that just joined pops in from [`SELECT_POP_FROM`].
    fn pop(&self, id: &str, now: f64) -> f32 {
        match self.marks.get(id) {
            Some(&(at, true, _)) => {
                SELECT_POP_FROM + (1.0 - SELECT_POP_FROM) * SELECT_POP.value(now - at)
            }
            _ => 1.0,
        }
    }

    /// Whether anything is still moving at `now`.
    fn moving(&self, now: f64) -> bool {
        (self.slide - self.target()).abs() > 1e-3
            || self.velocity.abs() > 1e-2
            || (self.opacity - self.target()).abs() > 1e-4
            || self.marks.values().any(|&(at, joined, _)| {
                (now - at) < f64::from(SELECT_TICK_FADE)
                    || (joined && !SELECT_POP.settled(now - at))
            })
    }
}

/// Whether the selection's furniture goes to its place at once: with a
/// screen reader attached, and while the window is in the background.
fn selection_calm(ctx: &egui::Context, focused: bool) -> bool {
    !focused || crate::motion::screen_reader_attached(ctx) || crate::motion::window_resting(ctx)
}

/// Brings `chat`'s selection motion up to `selected` (`None` when it is
/// not selecting) and returns it. The composer and the transcript both ask
/// in a pass; the second finds it done. A chat not drawn in the pass before
/// (another was open, or the window was away) takes what changed meanwhile
/// at once, as does a `calm` one.
fn selection_motion(
    ctx: &egui::Context,
    chat: &str,
    selected: Option<&[String]>,
    calm: bool,
) -> SelectionMotion {
    let id = SelectionMotion::id(chat);
    let pass = ctx.cumulative_pass_nr();
    let now = ctx.input(|input| input.time);
    let mut motion = ctx
        .data(|data| data.get_temp::<SelectionMotion>(id))
        .unwrap_or_default();
    if motion.pass == Some(pass) {
        return motion;
    }
    let stale = motion.pass.is_none_or(|last| last + 1 < pass);
    let on = selected.is_some();
    let instant = calm || stale;
    if instant {
        motion.on = on;
        motion.slide = motion.target();
        motion.velocity = 0.0;
        motion.fade_from = motion.target();
        motion.turned = f64::NEG_INFINITY;
        motion.marks.clear();
        if !on {
            motion.ticked.clear();
        }
    } else if on != motion.on {
        // Turned this pass: from where it is, starting now.
        motion.fade_from = motion.opacity_at(now);
        motion.turned = now;
        motion.on = on;
    } else {
        let dt = (now - motion.time).max(0.0);
        let (offset, velocity) =
            SELECT_SLIDE.advance(motion.slide - motion.target(), motion.velocity, dt);
        if offset.abs() <= 1e-3 && velocity.abs() <= 1e-2 {
            motion.slide = motion.target();
            motion.velocity = 0.0;
        } else {
            motion.slide = motion.target() + offset;
            motion.velocity = velocity;
        }
    }
    motion.opacity = motion.opacity_at(now);
    if let Some(selected) = selected
        && motion.ticked.as_slice() != selected
    {
        if !instant {
            let now_ticked: HashSet<&str> = selected.iter().map(String::as_str).collect();
            let was_ticked: HashSet<&str> = motion.ticked.iter().map(String::as_str).collect();
            let mut changes = Vec::new();
            for joined in now_ticked.difference(&was_ticked) {
                changes.push(((*joined).to_owned(), true));
            }
            for left in was_ticked.difference(&now_ticked) {
                changes.push(((*left).to_owned(), false));
            }
            for (message, joined) in changes {
                let from = motion.tick(&message, !joined, now);
                motion.marks.insert(message, (now, joined, from));
            }
        }
        motion.ticked = selected.to_vec();
    }
    motion.marks.retain(|_, &mut (at, _, _)| now - at < 1.0);
    // Once a selection has left, what it held is forgotten, so the next one
    // starts from nothing ticked.
    if !motion.on && !motion.leaving() {
        motion.ticked.clear();
        motion.marks.clear();
    }
    motion.time = now;
    motion.pass = Some(pass);
    if !instant && motion.moving(now) {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(id, motion.clone()));
    motion
}

/// What the composer, or the selection bar when `bar`, drew in its last
/// pass, to fade where it was once the other has taken its place.
#[derive(Clone)]
struct KeptBar {
    chat: String,
    shapes: std::sync::Arc<Vec<egui::epaint::ClippedShape>>,
}

fn kept_bar_id(bar: bool) -> egui::Id {
    egui::Id::new(("selection-swap", bar))
}

/// Keeps what `layer` holds from `start` on, drawn by `chat`'s composer (or
/// its selection bar when `bar`), for [`paint_kept_bar`]. Frosted glass is a
/// paint callback that cannot fade, so the copy leaves it out.
fn keep_bar(
    ctx: &egui::Context,
    layer: egui::LayerId,
    chat: &str,
    bar: bool,
    start: egui::layers::ShapeIdx,
) {
    let shapes: Vec<egui::epaint::ClippedShape> = ctx.graphics(|graphics| {
        graphics.get(layer).map_or_else(Vec::new, |list| {
            list.all_entries()
                .skip(start.0)
                .filter(|clipped| {
                    !matches!(clipped.shape, egui::Shape::Callback(_) | egui::Shape::Noop)
                })
                .cloned()
                .collect()
        })
    });
    ctx.data_mut(|data| {
        data.insert_temp(
            kept_bar_id(bar),
            KeptBar {
                chat: chat.to_owned(),
                shapes: std::sync::Arc::new(shapes),
            },
        );
    });
}

/// Paints what `chat`'s composer (or its selection bar when `bar`) last
/// drew, at `opacity` and `drop` points below where it was. Only paint: it
/// takes no clicks or keys.
fn paint_kept_bar(
    ctx: &egui::Context,
    layer: egui::LayerId,
    chat: &str,
    bar: bool,
    opacity: f32,
    drop: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    let Some(kept) = ctx
        .data(|data| data.get_temp::<KeptBar>(kept_bar_id(bar)))
        .filter(|kept| kept.chat == chat)
    else {
        return;
    };
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for clipped in kept.shapes.iter() {
            let mut shape = clipped.shape.clone();
            fade_shape(&mut shape, opacity);
            if drop != 0.0 {
                shape.translate(vec2(0.0, drop));
            }
            list.add(clipped.clip_rect, shape);
        }
    });
}

/// Fades what `layer` holds from `start` on to `opacity` and moves it `drop`
/// points down: the selection bar or the composer on its way in.
fn settle_bar(
    ctx: &egui::Context,
    layer: egui::LayerId,
    start: egui::layers::ShapeIdx,
    opacity: f32,
    drop: f32,
) {
    if opacity >= 1.0 && drop == 0.0 {
        return;
    }
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in start.0..list.next_idx().0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                fade_shape(&mut clipped.shape, opacity);
                if drop != 0.0 {
                    clipped.shape.translate(vec2(0.0, drop));
                }
            });
        }
    });
}

/// Multiplies every colour of `shape` by `opacity`.
fn fade_shape(shape: &mut egui::Shape, opacity: f32) {
    if opacity < 1.0 {
        let opacity = opacity.max(0.0);
        egui::epaint::shape_transform::adjust_colors(shape, move |colour| {
            if *colour != Color32::PLACEHOLDER {
                *colour = colour.gamma_multiply(opacity);
            }
        });
    }
}

/// Replaces the composer while messages are selected.
fn selection_bar(app: &mut App, ui: &mut egui::Ui, chat: &str, selected: &[String]) {
    let palette = app.palette;
    let locale = app.locale;
    ui.horizontal(|ui| {
        if theme::icon_button(
            ui,
            Icon::X,
            18.0,
            palette.secondary,
            palette.text,
            &crate::i18n::gettext(locale, "Cancel selection"),
        )
        .clicked()
            || ui.input(|input| input.key_pressed(Key::Escape))
        {
            app.actions.push(Action::CancelSelection);
        }
        let count = crate::i18n::ngettext(
            locale,
            "{count} selected",
            "{count} selected",
            selected.len() as u32,
        )
        .replace("{count}", &selected.len().to_string());
        theme::text(ui, &count, theme::medium(14.5), palette.text);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // The selection stays open with nothing in it, as its check
            // boxes do; there is nothing to forward until one is ticked.
            let forward = ui
                .add_enabled_ui(!selected.is_empty(), |ui| {
                    theme::pill_button(
                        ui,
                        &palette,
                        &crate::i18n::gettext(locale, "Forward…"),
                        true,
                    )
                })
                .inner;
            if forward.clicked() {
                app.actions.push(Action::ShowDialog(Dialog::Forward {
                    chat: chat.to_owned(),
                    messages: selected.to_vec(),
                }));
            }
            let delete = ui
                .add_enabled_ui(!selected.is_empty(), |ui| {
                    theme::pill_button(
                        ui,
                        &palette,
                        &crate::i18n::gettext(locale, "Delete…"),
                        false,
                    )
                })
                .inner;
            // Demo/test: hold this menu open for a screenshot.
            #[cfg(any(test, feature = "demo"))]
            if app.open_selection_delete_menu {
                egui::Popup::open_id(ui.ctx(), delete.id.with("popup"));
            }
            let can_revoke = app.conversations.get(chat).is_some_and(|conversation| {
                selection_can_revoke(&conversation.messages, selected, crate::util::now())
            });
            let labels = [
                crate::i18n::gettext(app.locale, "Delete for everyone"),
                crate::i18n::gettext(app.locale, "Delete for me"),
            ];
            let width = widgets::menu_width(ui, &[&*labels[0], &*labels[1]], true);
            let mut actions = Vec::new();
            egui::Popup::menu(&delete)
                .width(width)
                .frame(widgets::menu_frame(&palette))
                .show_frosted(ui.ctx(), |ui| {
                    selection_delete_items(
                        ui,
                        &palette,
                        app.locale,
                        chat,
                        can_revoke,
                        selected,
                        &mut actions,
                    );
                });
            app.actions.extend(actions);
        });
    });
}

/// The file name to suggest when saving an attachment: the sender's name for
/// documents, the cached file's name otherwise. Path separators are dropped so
/// a crafted name cannot point the dialog somewhere else.
fn attachment_name(content: &Content, path: &Path) -> String {
    let name = match content {
        Content::Document { file_name, .. } if !file_name.trim().is_empty() => file_name.clone(),
        _ => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment".to_owned()),
    };
    name.replace(['/', '\\'], "_")
}

/// Whether a conversation has visible content. Used by tests.
#[allow(dead_code)]
pub fn has_messages(conversation: &Conversation) -> bool {
    !conversation.messages.is_empty()
}

#[allow(dead_code)]
fn status_label(status: Delivery) -> &'static str {
    match status {
        Delivery::None => "",
        Delivery::Pending => "sending",
        Delivery::Sent => "sent",
        Delivery::Delivered => "delivered",
        Delivery::Read => "read",
        Delivery::Played => "played",
        Delivery::Failed => "failed",
    }
}

#[allow(dead_code)]
fn chat_of(chat: &ChatId) -> &str {
    chat
}

/// How long an upload takes to rise into place when it is sent.
const UPLOAD_ENTRANCE: f32 = theme::MOTION_PANEL;
/// How far below its place an upload starts its entrance.
const UPLOAD_RISE: f32 = 14.0;
/// The widest an upload's picture is drawn.
const UPLOAD_PICTURE_WIDTH: f32 = 260.0;
/// The progress ring over a picture: its radius and stroke.
const UPLOAD_RING: f32 = 22.0;
const UPLOAD_RING_STROKE: f32 = 3.0;

/// Attachments on their way, as your own messages at the end of the chat:
/// a picture with a progress ring over it, or a file card whose icon is
/// ringed by its progress, each rising into place when it is sent and
/// replaced by its message once that is in the archive.
fn upload_rows(ui: &mut egui::Ui, palette: &Palette, uploads: &[crate::model::Upload]) {
    if uploads.is_empty() {
        return;
    }
    // The ring turns while anything is on its way.
    ui.ctx().request_repaint();
    let cap = (ui.available_width() * WHATSAPP_WIDTH_SHARE).min(CARD_WIDTH);
    for upload in uploads {
        let id = ui.id().with(("upload", upload.id));
        let shown = ui.ctx().animate_bool_with_time(id, true, UPLOAD_ENTRANCE);
        // Eased out, so it settles softly.
        let eased = theme::ease_out(shown);
        let fraction = upload.fraction().map(|fraction| {
            ui.ctx()
                .animate_value_with_time(id.with("progress"), fraction, theme::MOTION_PANEL)
        });
        let picture = upload.preview.as_ref().filter(|_| {
            matches!(
                upload.kind,
                crate::model::UploadKind::Image | crate::model::UploadKind::Video
            )
        });
        ui.add_space(RUN_GAP);
        if upload.kind == crate::model::UploadKind::Sticker
            && let Some(crate::model::UploadPreview::File(path)) = &upload.preview
        {
            upload_sticker(ui, palette, upload.id, path, eased);
            continue;
        }
        match picture {
            Some(preview) => upload_picture(ui, palette, preview, fraction, cap, eased),
            None => upload_card(ui, palette, upload, fraction, cap, eased),
        }
    }
}

/// How high, at most, a sticker flying from the picker arcs over its
/// straight path.
const STICKER_ARC: f32 = 64.0;
/// How long a sticker takes to fly from the picker, as the drag-carry
/// flights do.
const STICKER_FLIGHT: f32 = theme::MOTION_SCREEN;
/// The side of the "sending" chip under a sticker on its way.
const STICKER_CHIP: f32 = 22.0;
/// How much a sticker on its way is faded, until its message arrives.
const STICKER_WAITING_FADE: f32 = 0.25;

/// A sticker flying from its picker tile to its place in the chat.
#[derive(Clone)]
struct StickerFlight {
    upload: crate::model::UploadId,
    /// The tile it left, in screen coordinates.
    from: Rect,
    start: f64,
    /// Where it lands and what it shows, as its row last laid it out; it
    /// flies nowhere until the row has.
    to: Option<(Rect, egui::TextureId)>,
}

fn flights_id() -> egui::Id {
    egui::Id::new("sticker-flights")
}

/// Sends the sticker of `upload` flying from `from`, its tile in the picker,
/// to its place at the end of the chat. Without motion it is simply there.
pub fn launch_sticker(ctx: &egui::Context, upload: crate::model::UploadId, from: Rect) {
    if crate::motion::screen_reader_attached(ctx) {
        return;
    }
    let start = ctx.input(|input| input.time);
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<StickerFlight>>(flights_id())
            .push(StickerFlight {
                upload,
                from,
                start,
                to: None,
            });
    });
    ctx.request_repaint();
}

/// How far a flight that began at `start` has got, from 0 to 1.
fn flight_progress(start: f64, now: f64) -> f32 {
    ((now - start) / f64::from(STICKER_FLIGHT)).clamp(0.0, 1.0) as f32
}

/// Where a sticker flying from `from` to `to` is, `t` of the way: eased
/// out, growing to its size, along a low arc.
fn flight_rect(from: Rect, to: Rect, t: f32) -> Rect {
    let eased = theme::ease_out(t.clamp(0.0, 1.0));
    let center = from.center().lerp(to.center(), eased);
    let size = from.size() + (to.size() - from.size()) * eased;
    let arc = (from.center().distance(to.center()) * 0.25).min(STICKER_ARC);
    let lift = arc * (std::f32::consts::PI * eased).sin();
    Rect::from_center_size(center - vec2(0.0, lift), size)
}

/// Draws the stickers in flight over everything, after the chat has laid
/// out where they land, and forgets the ones that have.
pub fn paint_sticker_flights(ctx: &egui::Context) {
    let Some(mut flights) = ctx.data(|data| data.get_temp::<Vec<StickerFlight>>(flights_id()))
    else {
        return;
    };
    let now = ctx.input(|input| input.time);
    flights.retain(|flight| flight_progress(flight.start, now) < 1.0);
    for flight in &flights {
        let Some((to, texture)) = flight.to else {
            continue;
        };
        let t = flight_progress(flight.start, now);
        let rect = flight_rect(flight.from, to, t);
        // It settles to the faded look of a sticker still on its way.
        let tint = Color32::WHITE.gamma_multiply(1.0 - STICKER_WAITING_FADE * theme::ease_out(t));
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            flights_id().with(flight.upload),
        ))
        .image(
            texture,
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            tint,
        );
    }
    if flights.is_empty() {
        ctx.data_mut(|data| data.remove::<Vec<StickerFlight>>(flights_id()));
    } else {
        ctx.data_mut(|data| data.insert_temp(flights_id(), flights));
        ctx.request_repaint();
    }
}

/// The sticker's texture and natural size: an animation's first frame, or
/// the picture.
fn sticker_texture(ui: &egui::Ui, path: &Path, rect: Rect) -> Option<(egui::TextureId, Vec2)> {
    if crate::ui::picker::moves(ui.ctx(), path) {
        return match animation::frame(ui, path, rect, false) {
            animation::Frame::Ready(texture) => Some((texture.id(), texture.size_vec2())),
            _ => None,
        };
    }
    let poll = widgets::fitted_sticker(ui, path, rect.size())
        .load_for_size(ui.ctx(), rect.size())
        .ok()?;
    Some((poll.texture_id()?, poll.size()?))
}

/// A sticker on its way: drawn where its message will be, at the right edge
/// without a bubble, faded a little with a turning ring beside it. One sent
/// from the picker flies there first, and rests once it lands.
fn upload_sticker(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: crate::model::UploadId,
    path: &Path,
    eased: f32,
) {
    let square = Vec2::splat(STICKER_SIDE);
    let texture = sticker_texture(ui, path, Rect::from_min_size(ui.cursor().min, square));
    let size = texture.map_or(square, |(_, natural)| fit_sticker(natural.x, natural.y));
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let rest = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size);
    let now = ui.input(|input| input.time);
    // The flight aims at the sticker's place, or at the bottom of the view
    // while that is still below it.
    let clip = ui.clip_rect();
    let target = rest.translate(vec2(0.0, (clip.bottom() - rest.bottom()).min(0.0)));
    let flying = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<StickerFlight>>(flights_id())
            .iter_mut()
            .find(|flight| flight.upload == upload && flight_progress(flight.start, now) < 1.0)
            .map(|flight| {
                flight.to = texture.map(|(texture, _)| (target, texture));
            })
            .is_some()
    });
    if flying {
        return;
    }
    // One that did not fly rises into place as other uploads do.
    let rect = rest.translate(vec2(0.0, UPLOAD_RISE * (1.0 - eased)));
    let painter = ui.painter_at(row.expand(UPLOAD_RISE));
    match texture {
        Some((texture, _)) => {
            painter.image(
                texture,
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE.gamma_multiply((1.0 - STICKER_WAITING_FADE) * eased),
            );
        }
        None => {
            painter.rect_filled(rect, theme::RADIUS, palette.surface.gamma_multiply(eased));
            theme::paint_icon(
                ui,
                Icon::Sticker,
                Rect::from_center_size(rect.center(), Vec2::splat(28.0)),
                28.0,
                palette.secondary.gamma_multiply(eased),
            );
        }
    }
    // Waiting for the phone's servers, so a ring turns without a number.
    let chip = Rect::from_min_size(
        rect.right_bottom() - Vec2::splat(STICKER_CHIP + 4.0),
        Vec2::splat(STICKER_CHIP),
    );
    painter.circle_filled(
        chip.center(),
        STICKER_CHIP / 2.0,
        Color32::from_black_alpha((120.0 * eased) as u8),
    );
    progress_ring(
        ui,
        chip.center(),
        STICKER_CHIP / 2.0 - 5.0,
        None,
        Color32::WHITE.gamma_multiply(eased),
    );
}

/// The bubble of an upload, at the right edge, `eased` of the way into place.
fn upload_bubble(ui: &mut egui::Ui, palette: &Palette, size: Vec2, eased: f32) -> (Rect, f32) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let rect = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size)
        .translate(vec2(0.0, UPLOAD_RISE * (1.0 - eased)));
    let fill = palette.bubble_out.gamma_multiply(eased);
    if palette.bubbles == theme::BubbleStyle::Nothing {
        ui.painter()
            .add(widgets::grouped_bubble_shape(rect, fill, true, true, true));
    } else {
        ui.painter()
            .rect_filled(rect, f32::from(widgets::BUBBLE_RADIUS), fill);
    }
    (rect, eased)
}

/// A picture on its way: the picture itself, dimmed, under its progress.
fn upload_picture(
    ui: &mut egui::Ui,
    palette: &Palette,
    preview: &crate::model::UploadPreview,
    fraction: Option<f32>,
    cap: f32,
    eased: f32,
) {
    let width = UPLOAD_PICTURE_WIDTH.min(cap);
    let (texture, natural) = match preview {
        crate::model::UploadPreview::Pixels {
            width: w,
            height: h,
            texture,
            ..
        } => (
            texture.as_ref().map(egui::TextureHandle::id),
            Some(vec2(*w as f32, *h as f32)),
        ),
        crate::model::UploadPreview::File(path) => {
            match widgets::fitted_image(ui, path, Vec2::splat(width))
                .load_for_size(ui.ctx(), Vec2::splat(width))
            {
                Ok(poll) => (poll.texture_id(), poll.size()),
                Err(_) => (None, None),
            }
        }
    };
    let aspect = natural
        .filter(|size| size.x > 0.0 && size.y > 0.0)
        .map_or(0.75, |size| size.y / size.x);
    let margin = 4.0;
    let picture_size = vec2(width, (width * aspect).clamp(120.0, 320.0));
    let (bubble, eased) =
        upload_bubble(ui, palette, picture_size + Vec2::splat(margin * 2.0), eased);
    let picture = bubble.shrink(margin);
    let radius = if palette.bubbles == theme::BubbleStyle::Nothing {
        12.0
    } else {
        6.0
    };
    let painter = ui.painter_at(bubble);
    match texture {
        Some(texture) => {
            // Cover the frame, cropping the longer side.
            let uv = natural.map_or(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), |size| {
                cover_uv(size, picture.size())
            });
            painter.add(
                egui::epaint::RectShape::filled(
                    picture,
                    CornerRadius::same(radius as u8),
                    Color32::WHITE.gamma_multiply(eased),
                )
                .with_texture(texture, uv),
            );
        }
        None => {
            painter.rect_filled(picture, radius, palette.surface);
            theme::paint_icon(
                ui,
                Icon::Image,
                Rect::from_center_size(picture.center(), Vec2::splat(28.0)),
                28.0,
                palette.secondary,
            );
        }
    }
    painter.rect_filled(
        picture,
        radius,
        Color32::from_black_alpha((90.0 * eased) as u8),
    );
    let ring = Rect::from_center_size(picture.center(), Vec2::splat(UPLOAD_RING * 2.0 + 12.0));
    painter.circle_filled(
        ring.center(),
        UPLOAD_RING + 6.0,
        Color32::from_black_alpha((110.0 * eased) as u8),
    );
    progress_ring(
        ui,
        ring.center(),
        UPLOAD_RING,
        fraction,
        Color32::WHITE.gamma_multiply(eased),
    );
    theme::paint_icon(
        ui,
        Icon::ArrowUp,
        Rect::from_center_size(ring.center(), Vec2::splat(18.0)),
        18.0,
        Color32::WHITE.gamma_multiply(eased),
    );
    if let Some(fraction) = fraction {
        let label = format!("{:.0}%", fraction * 100.0);
        let galley = painter.layout_no_wrap(label, theme::medium(12.0), Color32::WHITE);
        let chip = Rect::from_min_size(
            picture.left_bottom() + vec2(8.0, -8.0 - galley.size().y - 6.0),
            galley.size() + vec2(14.0, 6.0),
        );
        painter.rect_filled(
            chip,
            chip.height() / 2.0,
            Color32::from_black_alpha((130.0 * eased) as u8),
        );
        painter.galley(
            chip.min + vec2(7.0, 3.0),
            galley,
            Color32::WHITE.gamma_multiply(eased),
        );
    }
}

/// The part of a picture of `natural` size that covers a frame of `frame`
/// size, centred.
fn cover_uv(natural: Vec2, frame: Vec2) -> Rect {
    let scale = (frame.x / natural.x).max(frame.y / natural.y);
    let shown = frame / (natural * scale);
    let min = (Vec2::splat(1.0) - shown) / 2.0;
    Rect::from_min_size(pos2(min.x, min.y), shown)
}

/// A file on its way, laid out as its message will be: the file card with
/// its icon ringed by the progress, its name, and how much has gone.
fn upload_card(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: &crate::model::Upload,
    fraction: Option<f32>,
    cap: f32,
    eased: f32,
) {
    let colours = palette.on_bubble(true);
    let (margin, inset) = attachment_insets(palette);
    let card = vec2(cap - margin.x * 2.0, ATTACHMENT_ROW);
    let (bubble, eased) = upload_bubble(ui, palette, card + inset * 2.0 + margin * 2.0, eased);
    let card = Rect::from_min_size(bubble.min + margin, card + inset * 2.0);
    let painter = ui.painter_at(bubble);
    painter.rect_filled(
        card,
        attachment_radius(palette),
        palette.window.gamma_multiply(0.35 * eased),
    );
    let content = card.shrink2(inset);
    let disc = pos2(content.left() + ATTACHMENT_DISC / 2.0, content.center().y);
    painter.circle_filled(
        disc,
        ATTACHMENT_DISC / 2.0,
        colours.accent.gamma_multiply(0.25 * eased),
    );
    progress_ring(
        ui,
        disc,
        ATTACHMENT_DISC / 2.0 - 1.5,
        fraction,
        colours.accent.gamma_multiply(eased),
    );
    let icon = match upload.kind {
        crate::model::UploadKind::Image => Icon::Image,
        crate::model::UploadKind::Video => Icon::Video,
        crate::model::UploadKind::Audio => Icon::Music,
        crate::model::UploadKind::Document => Icon::FileText,
        crate::model::UploadKind::Sticker => Icon::Sticker,
    };
    theme::paint_icon(
        ui,
        icon,
        Rect::from_center_size(disc, Vec2::splat(18.0)),
        18.0,
        colours.accent.gamma_multiply(eased),
    );
    let text_left = disc.x + ATTACHMENT_DISC / 2.0 + 12.0;
    let text_width = (content.right() - text_left).max(0.0);
    let name = widgets::line(
        ui,
        if upload.name.is_empty() {
            "Photo"
        } else {
            &upload.name
        },
        theme::medium(14.0),
        colours.text,
        text_width,
        1,
    );
    let detail = match fraction {
        Some(fraction) if fraction >= 1.0 => "Sending…".to_owned(),
        Some(fraction) => format!(
            "{} of {} · {:.0}%",
            crate::util::bytes((fraction as f64 * upload.total as f64) as u64),
            crate::util::bytes(upload.total),
            fraction * 100.0
        ),
        None => "Preparing…".to_owned(),
    };
    let detail = painter.layout_no_wrap(detail, theme::regular(12.0), colours.secondary);
    let gap = 2.0;
    let top = content.center().y - (name.size().y + gap + detail.size().y) / 2.0;
    name.paint(ui, pos2(text_left, top), colours.text.gamma_multiply(eased));
    painter.galley(
        pos2(text_left, top + name.size().y + gap),
        detail,
        colours.secondary.gamma_multiply(eased),
    );
}

/// A ring of `fraction` round from the top, clockwise, over a faint track;
/// without one, a quarter turning, as nothing is known yet.
fn progress_ring(
    ui: &egui::Ui,
    center: egui::Pos2,
    radius: f32,
    fraction: Option<f32>,
    color: Color32,
) {
    let painter = ui.painter();
    let stroke = UPLOAD_RING_STROKE.min(radius / 5.0).max(2.0);
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(stroke, color.gamma_multiply(0.25)),
    );
    let time = ui.input(|input| input.time) as f32;
    let (start, sweep) = match fraction {
        Some(fraction) => (
            -std::f32::consts::FRAC_PI_2,
            fraction.max(0.02) * std::f32::consts::TAU,
        ),
        None => (time * 4.0, std::f32::consts::FRAC_PI_2),
    };
    let steps = ((sweep / std::f32::consts::TAU) * 64.0).ceil().max(2.0) as usize;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|step| {
            let angle = start + sweep * step as f32 / steps as f32;
            center + Vec2::angled(angle) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(stroke, color)));
}

#[cfg(test)]
mod tests {
    #[test]
    fn recording_breath_stays_in_range_and_loops() {
        for step in 0..240 {
            let time = step as f32 * 0.01;
            let (scale, opacity) = recording_breath(time);
            assert!((0.85..=1.0).contains(&scale));
            assert!((0.6..=1.0).contains(&opacity));
            let (again, _) = recording_breath(time + RECORDING_BREATH);
            assert!((scale - again).abs() < 1e-3);
        }
    }

    #[test]
    fn level_ease_follows_target_without_overshoot() {
        let mut value = 0.0;
        for _ in 0..10 {
            let next = ease_level(value, 1.0, 0.016, RECORDING_LEVEL_TAU);
            assert!(next > value && next <= 1.0);
            value = next;
        }
        assert!(
            (ease_level(0.0, 1.0, RECORDING_LEVEL_TAU, RECORDING_LEVEL_TAU) - 0.632).abs() < 0.01
        );
        assert_eq!(ease_level(0.3, 1.0, 0.0, RECORDING_LEVEL_TAU), 0.3);
    }

    #[test]
    fn jump_plan_glides_near_targets_and_cuts_far_ones_short() {
        assert_eq!(jump_plan(500.0, 400.0), (0.0, 500.0));
        assert_eq!(jump_plan(-800.0, 400.0), (0.0, -800.0));
        assert_eq!(jump_plan(5000.0, 400.0), (4600.0, 400.0));
        assert_eq!(jump_plan(-5000.0, 400.0), (-4600.0, -400.0));
    }

    use super::*;

    #[test]
    fn a_sticker_flight_leaves_its_tile_and_lands_in_place() {
        let tile = Rect::from_min_size(pos2(40.0, 600.0), Vec2::splat(80.0));
        let place = Rect::from_min_size(pos2(700.0, 400.0), Vec2::splat(STICKER_SIDE));
        let near = |a: Rect, b: Rect| a.min.distance(b.min) < 0.01 && a.max.distance(b.max) < 0.01;
        assert!(near(flight_rect(tile, place, 0.0), tile));
        assert!(near(flight_rect(tile, place, 1.0), place));
        // It arcs above the straight line between them on its way.
        let middle = flight_rect(tile, place, 0.5);
        let straight = tile.center().lerp(place.center(), theme::ease_out(0.5));
        assert!(middle.center().y < straight.y);
        assert_eq!(
            flight_progress(1.0, 1.0 + f64::from(STICKER_FLIGHT) * 2.0),
            1.0
        );
    }

    #[test]
    fn quick_reactions_spring_in_one_after_another() {
        assert_eq!(quick_reaction_progress(0.0, 0), 0.0);
        // The third waits for its turn, the first has begun by then.
        assert_eq!(quick_reaction_progress(0.05, 2), 0.0);
        assert!(quick_reaction_progress(0.05, 0) > 0.0);
        assert_eq!(quick_reaction_progress(1.0, 5), 1.0);
        assert!(quick_reaction_progress(0.1, 0) > quick_reaction_progress(0.1, 3));
    }

    #[test]
    fn a_new_row_lifts_the_transcript_in_a_wave() {
        let glide = Glide {
            newest: None,
            height: 0.0,
            at_bottom: true,
            lifts: vec![(0.0, 40.0)],
        };
        // Every row starts on its old line.
        for below in 0..=WAVE_ROWS + 2 {
            assert_eq!(glide.shift(0.0, below), 40.0);
        }
        // The newest row is on its way before the rows above it start.
        assert!(glide.shift(0.05, 0) < 39.0);
        assert!(glide.shift(0.05, 0) < glide.shift(0.05, 2));
        assert_eq!(glide.shift(0.03, 2), 40.0);
        // Rows past the cap all wait the same.
        assert_eq!(glide.shift(0.2, WAVE_ROWS), glide.shift(0.2, WAVE_ROWS + 5));
        // On the send's own spring: past its place a little, then on it.
        let lowest = (0..200)
            .map(|step| glide.shift(f64::from(step) / 200.0, 0))
            .fold(f32::MAX, f32::min);
        assert!(lowest < 0.0 && lowest > -2.0, "overshoot {lowest}");
        let rest = SEND_RISE.settle_time() + WAVE_CAP;
        assert!(glide.moving(rest - 0.05));
        assert!(!glide.moving(rest + 0.001));
        for below in 0..=WAVE_ROWS {
            assert_eq!(glide.shift(rest + 0.001, below), 0.0);
        }
    }

    #[test]
    fn the_wave_waits_one_step_a_row_up_to_its_cap() {
        assert_eq!(wave_delay(0), 0.0);
        assert!((wave_delay(1) - WAVE_STEP).abs() < 1e-12);
        assert!((wave_delay(WAVE_ROWS) - WAVE_CAP).abs() < 1e-12);
        assert_eq!(wave_delay(WAVE_ROWS + 30), wave_delay(WAVE_ROWS));
        assert!(wave_delay(usize::MAX) <= 0.09 + 1e-12);
    }

    #[test]
    fn a_lift_mid_glide_adds_to_what_is_left() {
        let mut glide = Glide {
            newest: None,
            height: 0.0,
            at_bottom: true,
            lifts: vec![(0.0, 40.0)],
        };
        let before = glide.shift(0.1, 3);
        glide.lifts.push((0.1, 30.0));
        // The second lift holds the list a row lower; the first carries on.
        assert!((glide.shift(0.1, 3) - (before + 30.0)).abs() < 1e-4);
        assert!(glide.moving(0.1 + SEND_RISE.settle_time()));
    }

    #[test]
    fn a_received_bubble_pops_from_six_tenths_to_its_size() {
        assert_eq!(pop_scale(0.0), POP_FROM_SCALE);
        assert_eq!(pop_scale(POP.settle_time() + 0.01), 1.0);
        let peak = (0..200)
            .map(|step| pop_scale(f64::from(step) / 200.0))
            .fold(0.0, f32::max);
        assert!(peak > 1.0 && peak < 1.03, "a little past its size: {peak}");
        // Seen early, and opaque well before it has grown.
        assert_eq!(pop_opacity(0.0), 0.0);
        assert_eq!(pop_opacity(POP_FADE_SECONDS), 1.0);
        assert!(pop_scale(POP_FADE_SECONDS) < 0.95);
    }

    #[test]
    fn a_sent_bubble_rises_with_bounce_and_slides_across_without() {
        let across: Vec<f32> = (0..=200)
            .map(|step| SEND_ACROSS.value(f64::from(step) / 200.0))
            .collect();
        assert!(across.windows(2).all(|pair| pair[1] >= pair[0]));
        assert!(across.iter().all(|value| *value <= 1.0));
        let rise_peak = (0..=200)
            .map(|step| SEND_RISE.value(f64::from(step) / 200.0))
            .fold(0.0, f32::max);
        assert!(rise_peak > 1.0, "{rise_peak}");
        assert!(!send_settled(0.3));
        assert!(send_settled(
            SEND_RISE.settle_time().max(SEND_ACROSS.settle_time()) + 0.001
        ));
    }

    #[test]
    fn the_typing_bubble_breathes_slightly_and_slowly() {
        assert_eq!(breath_scale(0.0), 1.0);
        let top = breath_scale(BREATH_SECONDS / 2.0);
        assert!((top - (1.0 + BREATH_SCALE)).abs() < 1e-5, "{top}");
        assert!((breath_scale(BREATH_SECONDS) - 1.0).abs() < 1e-5);
        for step in 0..100 {
            let scale = breath_scale(f64::from(step) * 0.05);
            assert!((1.0..=1.0 + BREATH_SCALE + 1e-5).contains(&scale));
        }
        assert!((1.5..=2.0).contains(&BREATH_SECONDS));
    }

    #[test]
    fn the_typing_bubble_counts_as_up_while_every_pass_draws_it() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(60.0, 36.0));
        let first = TypingSeen::after(None, rect, 1.0, 5);
        assert_eq!(first.since, 1.0);
        // The next pass, however late a resting window draws it.
        let next = TypingSeen::after(Some(first), rect, 9.0, 6);
        assert_eq!((next.since, next.drawn), (1.0, 9.0));
        // A discarded pass draws it again in the same one.
        assert_eq!(TypingSeen::after(Some(next), rect, 9.0, 6).since, 1.0);
        // A pass without it ends the appearance: the next one pops again.
        assert_eq!(TypingSeen::after(Some(next), rect, 9.5, 8).since, 9.5);
    }

    #[test]
    fn the_field_veil_recolours_the_backdrop_and_body_text_only() {
        let bubble = Rect::from_min_size(pos2(100.0, 100.0), vec2(120.0, 40.0));
        let (own_fill, own_text) = (Color32::from_rgb(0, 120, 255), Color32::WHITE);
        let (field, ink) = (
            Color32::from_rgb(40, 40, 40),
            Color32::from_rgb(230, 230, 230),
        );
        let scrim = egui::Shape::Rect(
            egui::epaint::RectShape::filled(bubble.expand(12.0), 18.0, Color32::BLACK)
                .with_blur_width(24.0),
        );
        let backdrop = egui::Shape::rect_filled(bubble, 18.0, own_fill);
        let tick = egui::Shape::line_segment(
            [pos2(200.0, 130.0), pos2(205.0, 135.0)],
            Stroke::new(1.0, own_text),
        );
        let veil = field_veil(
            &[scrim, backdrop, tick],
            bubble,
            (field, ink),
            own_text,
            0.5,
        );
        assert_eq!(veil.len(), 1, "only the backdrop: {veil:?}");
        let egui::Shape::Rect(rect) = &veil[0] else {
            panic!("{veil:?}");
        };
        assert_eq!(rect.fill, field.gamma_multiply(0.5));
        assert_eq!(rect.rect, bubble);
    }

    #[test]
    fn cubic_bezier_runs_from_zero_to_one_and_never_turns_back() {
        let curves = [(0.23, 1.0, 0.32, 1.0), (0.42, 0.0, 0.58, 1.0)];
        for (x1, y1, x2, y2) in curves {
            assert!(cubic_bezier(0.0, x1, y1, x2, y2).abs() < 1e-4);
            assert!((cubic_bezier(1.0, x1, y1, x2, y2) - 1.0).abs() < 1e-4);
            let mut last = 0.0;
            for step in 1..=100 {
                let value = cubic_bezier(step as f32 / 100.0, x1, y1, x2, y2);
                assert!(value >= last - 1e-4, "{value} after {last}");
                last = value;
            }
        }
        // Out of range clamps to the ends.
        assert!(cubic_bezier(-1.0, 0.23, 1.0, 0.32, 1.0).abs() < 1e-4);
        assert!((cubic_bezier(2.0, 0.23, 1.0, 0.32, 1.0) - 1.0).abs() < 1e-4);
        assert!((ease_out_strong(0.2) - 0.682).abs() < 0.01);
    }
    /// A row that was on screen while the reader was following the tail keeps
    /// most of its place on the frame a send lands, then finishes the move
    /// over the glide. An instant stick-to-bottom would already be at the end.
    #[test]
    fn a_new_message_eases_the_open_transcript_up() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::app::App::headless(
            crate::paths::AppDirs::under(root.path()),
            crate::settings::Settings::default(),
        )
        .0;
        app.link = crate::backend::LinkStatus::Connected;
        app.window_focused = true;
        let chat = crate::model::Chat::new("123@s.whatsapp.net".into(), "Alice".into());
        app.chats.push(chat.clone());
        app.open_chat = Some(chat.id.clone());
        app.scroll_to_bottom = true;
        let mut conversation = crate::app::Conversation::default();
        for index in 0..40 {
            let text = if index == 38 { "keep-me" } else { "older line" };
            conversation.messages.push(sample_message(
                &chat.id,
                &format!("m{index}"),
                text,
                1_700_000_000 + index as i64,
            ));
        }
        conversation.complete = true;
        app.conversations.insert(chat.id.clone(), conversation);

        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        let frame = |app: &mut crate::app::App, time: f64| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            output.textures_delta.clear();
            output.shapes
        };
        for time in [0.0, 0.05, 0.1, 0.2] {
            frame(&mut app, time);
        }
        let settled = frame(&mut app, 0.3);
        let y0 = text_top(&settled, "keep-me").expect("the anchor row is on screen");
        let metrics = || ctx.data(|data| data.get_temp::<(f32, f32)>(scroll_metrics_id(&chat.id)));
        let gap_of = |glide: &Option<Glide>, metrics: Option<(f32, f32)>| match (glide, metrics) {
            (Some(glide), Some((offset, view))) => glide.height - offset - view,
            _ => f32::NAN,
        };
        let gap_settled = gap_of(
            &ctx.data(|data| data.get_temp::<Glide>(Glide::id(&chat.id))),
            metrics(),
        );
        app.scroll_to_bottom = true;
        app.conversations
            .get_mut(&chat.id)
            .unwrap()
            .messages
            .push(sample_message(
                &chat.id,
                "sent",
                "just-sent",
                crate::util::now(),
            ));
        let arrived = frame(&mut app, 0.32);
        let y1 = text_top(&arrived, "keep-me").expect("the anchor stays on screen");
        let glide_arrived = ctx.data(|data| data.get_temp::<Glide>(Glide::id(&chat.id)));
        let gap_arrived = gap_of(&glide_arrived, metrics());
        let next = frame(&mut app, 0.34);
        let y_next = text_top(&next, "keep-me").expect("the anchor stays on the next frame");
        let glide_next = ctx.data(|data| data.get_temp::<Glide>(Glide::id(&chat.id)));
        let gap_next = gap_of(&glide_next, metrics());
        let done = frame(&mut app, 0.32 + SEND_RISE.settle_time() + WAVE_CAP + 0.05);
        let y2 = text_top(&done, "keep-me").expect("the anchor is on screen after the glide");
        let travelled = y0.0 - y2.0;
        let early = y0.0 - y1.0;
        // One frame after the row lands, a real ease still has most of the
        // distance left. An instant stick is already on the final line. The
        // anchor has two rows under it, so it waits its turn in the wave.
        let left = y_next.0 - y2.0;
        let describe = |glide: &Option<Glide>| {
            glide.as_ref().map_or("none".to_owned(), |glide| {
                format!(
                    "newest={:?} height={} at_bottom={} lifts={:?} shift_now={}",
                    glide.newest,
                    glide.height,
                    glide.at_bottom,
                    glide.lifts,
                    glide.shift(0.32, 2)
                )
            })
        };
        assert!(
            travelled > 8.0,
            "the tail moves up for the new row: y0={} y1={} y_next={} y2={} clip0={:?} clip1={:?} arrived={} next={}",
            y0.0,
            y1.0,
            y_next.0,
            y2.0,
            y0.1,
            y1.1,
            describe(&glide_arrived),
            describe(&glide_next)
        );
        assert!(
            early < travelled * 0.45 && left > travelled * 0.25,
            "the tail eases instead of jumping: early={early} left={left} of {travelled} (y0={} y1={} y_next={} y2={} clip0={:?} clip1={:?} gap_settled={gap_settled} gap_arrived={gap_arrived} gap_next={gap_next} arrived={} next={})",
            y0.0,
            y1.0,
            y_next.0,
            y2.0,
            y0.1,
            y1.1,
            describe(&glide_arrived),
            describe(&glide_next)
        );
    }

    /// A turn of the wheel over the open chat: the row under the pointer
    /// follows the scroll at once, a row far up the view lags behind it on
    /// its spring and catches up, and the bubble's click target moves with
    /// what is drawn. Once the rows rest the springs hold nothing.
    #[test]
    fn rows_far_from_the_pointer_lag_a_scroll_and_catch_up() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::app::App::headless(
            crate::paths::AppDirs::under(root.path()),
            crate::settings::Settings::default(),
        )
        .0;
        app.link = crate::backend::LinkStatus::Connected;
        app.window_focused = true;
        let chat = crate::model::Chat::new("123@s.whatsapp.net".into(), "Alice".into());
        app.chats.push(chat.clone());
        app.open_chat = Some(chat.id.clone());
        app.scroll_to_bottom = true;
        let mut conversation = crate::app::Conversation::default();
        for index in 0..60 {
            conversation.messages.push(sample_message(
                &chat.id,
                &format!("m{index}"),
                &format!("row-{index:02}"),
                1_700_000_000 + index as i64,
            ));
        }
        conversation.complete = true;
        app.conversations.insert(chat.id.clone(), conversation);

        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        // Mid-view, so the row under it is still on screen once the list has
        // scrolled down by the turn below.
        let pointer = pos2(400.0, 360.0);
        let frame = |app: &mut crate::app::App, time: f64, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            output.textures_delta.clear();
            output.shapes
        };
        let mut time = 0.0;
        for _ in 0..8 {
            time += 0.05;
            frame(&mut app, time, vec![egui::Event::PointerMoved(pointer)]);
        }
        app.scroll_to_bottom = false;
        time += 0.05;
        let rest = frame(&mut app, time, vec![egui::Event::PointerMoved(pointer)]);
        let tops = |shapes: &[egui::epaint::ClippedShape]| -> Vec<Option<f32>> {
            (0..60)
                .map(|index| text_top(shapes, &format!("row-{index:02}")).map(|(y, _)| y))
                .collect()
        };
        let before = tops(&rest);
        let on_screen = |y: &f32| (80.0..640.0).contains(y);
        // The row nearest the pointer and the one nearest the top of the view.
        let nearest = |target: f32| {
            (0..60)
                .filter(|&index| before[index].is_some_and(|y| on_screen(&y)))
                .min_by(|&a, &b| {
                    let distance = |index: usize| (before[index].unwrap() - target).abs();
                    distance(a).total_cmp(&distance(b))
                })
                .expect("rows on screen")
        };
        let near = nearest(pointer.y - 8.0);
        let far = nearest(90.0);
        assert!(near > far + 5, "near {near}, far {far}: {before:?}");
        let rect = |index: usize| {
            ctx.data(|data| {
                data.get_temp::<Rect>(bubble_id(&chat.id, &format!("m{index}")).with("rect"))
            })
            .expect("the bubble was drawn")
        };
        let far_target = rect(far).top() - before[far].unwrap();

        // Scroll up: the content moves down, by 120 points in all.
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 30.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        let mut lagged = 0.0_f32;
        for _ in 0..4 {
            time += 1.0 / 60.0;
            let shapes = frame(
                &mut app,
                time,
                vec![egui::Event::PointerMoved(pointer), wheel.clone()],
            );
            let now = tops(&shapes);
            if let (Some(near_now), Some(far_now)) = (now[near], now[far]) {
                let near_moved = near_now - before[near].unwrap();
                let far_moved = far_now - before[far].unwrap();
                lagged = lagged.max(near_moved - far_moved);
                // The click target is where the bubble is drawn.
                assert!(
                    (rect(far).top() - far_now - far_target).abs() < 0.5,
                    "target {:?} for text at {far_now}",
                    rect(far)
                );
            }
        }
        assert!(
            lagged > 2.0,
            "the far row lags the scroll by {lagged} points"
        );
        assert!(
            lagged <= super::super::scroll_springs::MAX_LAG + 1.0,
            "lag {lagged}"
        );
        // Without more scrolling the rows come to rest on their places.
        let mut moving = true;
        for _ in 0..240 {
            time += 1.0 / 60.0;
            frame(&mut app, time, vec![egui::Event::PointerMoved(pointer)]);
            moving = super::super::scroll_springs::Springs::load(&ctx, &chat.id).moving();
            if !moving {
                break;
            }
        }
        assert!(!moving, "the springs settle within four seconds");
        time += 1.0 / 60.0;
        let settled = tops(&frame(
            &mut app,
            time,
            vec![egui::Event::PointerMoved(pointer)],
        ));
        let on_view = |index: usize| {
            settled[index].unwrap_or_else(|| panic!("row {index} is drawn: {settled:?}"))
        };
        let scrolled = on_view(near) - before[near].unwrap();
        assert!(scrolled > 20.0, "the list scrolled by {scrolled}");
        assert!(
            (on_view(far) - before[far].unwrap() - scrolled).abs() < 0.5,
            "the far row is back on its place: {} vs {scrolled}",
            on_view(far) - before[far].unwrap()
        );
    }

    fn sample_message(chat: &str, id: &str, text: &str, timestamp: i64) -> crate::model::Message {
        crate::model::Message {
            id: id.to_owned(),
            chat: chat.to_owned(),
            sender: "123@s.whatsapp.net".into(),
            sender_name: Some("Alice".into()),
            from_me: false,
            timestamp,
            content: crate::model::Content::text(text),
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

    fn text_top(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<(f32, egui::Rect)> {
        fn walk(shape: &egui::Shape, clip: egui::Rect, needle: &str) -> Option<(f32, egui::Rect)> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text.contains(needle) => {
                    Some((text.pos.y, clip))
                }
                egui::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| walk(shape, clip, needle))
                }
                _ => None,
            }
        }
        shapes
            .iter()
            .find_map(|clipped| walk(&clipped.shape, clipped.clip_rect, needle))
    }

    #[test]
    fn a_wider_transcript_clip_still_slides() {
        let viewport = Rect::from_min_max(pos2(0.0, 10.0), pos2(400.0, 500.0));
        let wider = Rect::from_min_max(pos2(-20.0, 10.0), pos2(420.0, 500.0));
        assert!(slides_with_transcript(wider, viewport));
        let nested = Rect::from_min_max(pos2(40.0, 100.0), pos2(200.0, 160.0));
        assert!(slides_with_transcript(nested, viewport));
        let bar = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 600.0));
        assert!(!slides_with_transcript(bar, viewport));
        let text = egui::Shape::rect_filled(
            Rect::from_min_max(pos2(20.0, 200.0), pos2(180.0, 240.0)),
            0.0,
            Color32::WHITE,
        );
        assert!(shape_follows_transcript(&text, viewport));
        let handle = egui::Shape::rect_filled(
            Rect::from_min_max(pos2(390.0, 40.0), pos2(398.0, 120.0)),
            0.0,
            Color32::WHITE,
        );
        assert!(!shape_follows_transcript(&handle, viewport));
    }

    #[test]
    fn account_read_receipt_privacy_hides_read_ticks_only_in_outgoing_dms() {
        use crate::model::ChatKind;

        assert_eq!(
            display_delivery(Delivery::Read, true, ChatKind::Direct, true),
            Delivery::Delivered
        );
        assert_eq!(
            display_delivery(Delivery::Played, true, ChatKind::Direct, true),
            Delivery::Played
        );
        assert_eq!(
            display_delivery(Delivery::Read, true, ChatKind::Group, true),
            Delivery::Read
        );
        assert_eq!(
            display_delivery(Delivery::Read, false, ChatKind::Direct, true),
            Delivery::Read
        );
        assert_eq!(
            display_delivery(Delivery::Read, true, ChatKind::Direct, false),
            Delivery::Read
        );
    }

    #[test]
    fn account_privacy_preserves_non_read_states_and_broadcasts() {
        use crate::model::ChatKind;
        for status in [
            Delivery::None,
            Delivery::Pending,
            Delivery::Sent,
            Delivery::Delivered,
            Delivery::Played,
            Delivery::Failed,
        ] {
            assert_eq!(
                display_delivery(status, true, ChatKind::Direct, true),
                status
            );
        }
        assert_eq!(
            display_delivery(Delivery::Read, true, ChatKind::Broadcast, true),
            Delivery::Read
        );
    }

    #[test]
    fn batch_revoke_requires_every_selected_message_to_be_eligible() {
        let now = crate::util::now();
        let first = Message {
            id: "first".into(),
            chat: "synthetic-chat".into(),
            sender: "me".into(),
            sender_name: None,
            from_me: true,
            timestamp: now,
            history_order: None,
            content: Content::text("Synthetic message"),
            status: Delivery::Sent,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        };
        let mut second = first.clone();
        second.id = "second".into();
        let selected = vec!["first".into(), "second".into()];
        assert!(selection_can_revoke(
            &[first.clone(), second.clone()],
            &selected,
            now
        ));
        second.from_me = false;
        assert!(!selection_can_revoke(
            &[first.clone(), second.clone()],
            &selected,
            now
        ));
        second.from_me = true;
        second.timestamp = now - crate::app::REVOKE_WINDOW.as_secs() as i64 - 1;
        assert!(!selection_can_revoke(
            &[first.clone(), second.clone()],
            &selected,
            now
        ));
        second.timestamp = now;
        second.content = Content::Revoked;
        assert!(!selection_can_revoke(
            &[first.clone(), second],
            &selected,
            now
        ));
        assert!(!selection_can_revoke(&[first], &selected, now));
        assert!(!selection_can_revoke(&[], &[], now));
    }

    #[test]
    fn sender_pictures_show_in_groups_only() {
        let chat = |id: &str| Chat::new(id.into(), "Chat".into());
        assert!(shows_sender_pictures(&chat("120363012345678901@g.us")));
        assert!(!shows_sender_pictures(&chat("393331234567@s.whatsapp.net")));
        assert!(!shows_sender_pictures(&chat(
            "120363055566677788@newsletter"
        )));
    }

    fn contact(name: &str, number: &str, account: Option<&str>) -> Option<SharedContact> {
        Some(SharedContact {
            name: name.to_owned(),
            number: number.to_owned(),
            account: account.map(str::to_owned),
            first_name: None,
        })
    }

    #[test]
    fn a_shared_contact_keeps_the_first_name_of_its_card() {
        let first = |card: &str| {
            shared_contact_details(card, "Fallback")
                .expect("a contact")
                .first_name
        };
        // A first name of two words stays whole, as the card has it (#314).
        assert_eq!(
            first("BEGIN:VCARD\nN:;My Dih;;;\nFN:My Dih\nTEL:+15550101234\nEND:VCARD").as_deref(),
            Some("My Dih")
        );
        assert_eq!(
            first("BEGIN:VCARD\nN:Evans;Mary;Ann;;\nTEL:+15550101234\nEND:VCARD").as_deref(),
            Some("Mary Ann")
        );
        assert_eq!(
            first("BEGIN:VCARD\nN:Smith;Ada\\;Jo;;\nTEL:+15550101234\nEND:VCARD").as_deref(),
            Some("Ada;Jo")
        );
        assert_eq!(
            first("BEGIN:VCARD\nN:山田;太郎;;;\nTEL:+15550101234\nEND:VCARD").as_deref(),
            Some("太郎")
        );
        // Without a structured first name nothing is known.
        assert_eq!(
            first("BEGIN:VCARD\nN:Evans;;;;\nTEL:+15550101234\nEND:VCARD"),
            None
        );
        assert_eq!(
            first("BEGIN:VCARD\nFN:Ada Lovelace\nTEL:+15550101234\nEND:VCARD"),
            None
        );
    }

    #[test]
    fn shared_contact_accepts_case_insensitive_vcard_property_names() {
        assert_eq!(
            shared_contact_details(
                "BEGIN:VCARD\nversion:3.0\nitem1.fn:Case-insensitive name\nitem1.tel;type=cell;waid=1234567:+1 234567\nEND:VCARD",
                "Fallback",
            ),
            contact("Case-insensitive name", "+1 234567", Some("1234567"))
        );
    }

    #[test]
    fn shared_contact_uses_first_valid_telephone_when_multiple_are_present() {
        assert_eq!(
            shared_contact_details(
                "BEGIN:VCARD\nFN:Two numbers\nTEL;TYPE=HOME:+1111111\nTEL;TYPE=CELL:+2222222\nEND:VCARD",
                "Fallback",
            ),
            contact("Two numbers", "+1111111", Some("1111111"))
        );
    }

    #[test]
    fn shared_contact_prefers_lowest_vcard_tel_preference() {
        assert_eq!(
            shared_contact_details(
                "BEGIN:VCARD\nFN:Preferred number\nTEL;TYPE=HOME:+1111111\nitem1.TEL;TYPE=CELL;PREF=2:+2222222\nTEL;TYPE=WORK;PREF=1:+3333333\nEND:VCARD",
                "Fallback",
            ),
            contact("Preferred number", "+3333333", Some("3333333"))
        );
    }

    #[test]
    fn shared_contact_prefers_vcard_name_and_reads_the_account_from_waid() {
        assert_eq!(
            shared_contact_details(
                "BEGIN:VCARD\nVERSION:3.0\nFN:Alice Example\nitem1.TEL;waid=15550101234:+1 (555) 010-1234\nEND:VCARD",
                "Contact from sender",
            ),
            contact("Alice Example", "+1 (555) 010-1234", Some("15550101234"))
        );
    }

    #[test]
    fn a_local_number_names_no_account_and_stays_readable() {
        // Without a country code the digits are not a WhatsApp account.
        assert_eq!(
            shared_contact_details(
                "BEGIN:VCARD\nFN:Local\nTEL;TYPE=CELL:0171 1234567\nEND:VCARD",
                "Fallback",
            ),
            contact("Local", "0171 1234567", None)
        );
    }

    #[test]
    fn shared_contact_without_vcard_name_uses_message_name_and_rejects_missing_phone() {
        assert_eq!(
            shared_contact_details("BEGIN:VCARD\nTEL:+1234567\nEND:VCARD", "Shared person"),
            contact("Shared person", "+1234567", Some("1234567"))
        );
        assert_eq!(
            shared_contact_details("BEGIN:VCARD\nFN:No phone\nEND:VCARD", "Fallback"),
            None
        );
    }

    #[test]
    fn shared_contact_details_use_the_first_vcard_when_multiple_contacts_are_shared() {
        assert_eq!(
            shared_contact_details(
                "BEGIN:VCARD\nFN:Alice\nTEL:+15550101234\nEND:VCARD\nBEGIN:VCARD\nFN:Bob\nTEL:+15550105678\nEND:VCARD",
                "Several contacts",
            ),
            contact("Alice", "+15550101234", Some("15550101234"))
        );
    }

    #[test]
    fn saved_attachments_suggest_a_plain_file_name() {
        let document = |file_name: &str| Content::Document {
            media: media(None, None),
            file_name: file_name.into(),
            caption: None,
            pages: None,
        };
        let cached = Path::new("/cache/media/abc123.pdf");
        assert_eq!(attachment_name(&document("Notes.pdf"), cached), "Notes.pdf");
        assert_eq!(
            attachment_name(&document("../../.bashrc"), cached),
            ".._.._.bashrc"
        );
        assert_eq!(attachment_name(&document("  "), cached), "abc123.pdf");
    }

    /// Builds synthetic attachment metadata for media-state and album regression tests.
    fn media(w: Option<u32>, h: Option<u32>) -> Media {
        Media {
            mime: "image/jpeg".into(),
            size: 1,
            width: w,
            height: h,
            album: None,
            path: None,
            state: MediaState::Idle,
        }
    }

    /// Builds a synthetic picture belonging to a specified protocol album.
    fn album_picture(id: &str, album: Option<&str>, sender: &str) -> Message {
        let mut attachment = media(Some(640), Some(480));
        attachment.album = album.map(str::to_owned);
        Message {
            id: id.into(),
            chat: "chat@example".into(),
            sender: sender.into(),
            sender_name: None,
            from_me: false,
            timestamp: 1,
            history_order: None,
            content: Content::Image {
                motion: None,
                caption: None,
                media: attachment,
            },
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    }

    /// Checks that consecutive compatible media sharing an album parent are displayed together.
    #[test]
    fn consecutive_items_with_the_same_protocol_album_are_grouped() {
        let messages = vec![
            album_picture("one", Some("album-1"), "alice"),
            album_picture("two", Some("album-1"), "alice"),
            album_picture("three", Some("album-2"), "alice"),
        ];
        assert_eq!(album_run(&messages, 0).map(<[Message]>::len), Some(2));
        assert!(album_run(&messages, 2).is_none());
    }

    /// A protocol album splits at the local day boundary even when its identity stays the same.
    #[test]
    fn album_runs_stop_at_the_local_calendar_day_boundary() {
        let (_, midnight) = crate::util::day_bounds(jiff::civil::date(2026, 9, 20)).unwrap();
        let mut messages: Vec<_> = ["one", "two", "three", "four"]
            .iter()
            .map(|id| album_picture(id, Some("album-1"), "alice"))
            .collect();
        for (message, offset) in messages.iter_mut().zip([-2, -1, 1, 2]) {
            message.timestamp = midnight + offset;
        }
        assert_eq!(album_run(&messages, 0).map(<[Message]>::len), Some(2));
        assert_eq!(album_run(&messages, 2).map(<[Message]>::len), Some(2));
        assert!(album_run(&messages[1..3], 0).is_none());
    }

    /// Checks that grouping never combines unrelated pictures or messages from different senders.
    #[test]
    fn unrelated_or_different_sender_pictures_are_not_grouped() {
        let unrelated = vec![
            album_picture("one", None, "alice"),
            album_picture("two", None, "alice"),
        ];
        assert!(album_run(&unrelated, 0).is_none());
        let senders = vec![
            album_picture("one", Some("album-1"), "alice"),
            album_picture("two", Some("album-1"), "bob"),
        ];
        assert!(album_run(&senders, 0).is_none());
    }

    fn message(content: Content) -> Message {
        Message {
            id: "m1".into(),
            chat: "peer@s.whatsapp.net".into(),
            sender: "me".into(),
            sender_name: None,
            from_me: true,
            timestamp: 0,
            content,
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
            history_order: None,
        }
    }

    #[test]
    fn picture_placeholder_matches_decoded_dimensions() {
        for limit in [200.0, PICTURE_WIDTH] {
            for (width, height) in [(900, 1200), (600, 1600), (1600, 900)] {
                assert_eq!(
                    frame_size(
                        &media(Some(width), Some(height)),
                        None,
                        limit,
                        PICTURE_HEIGHT
                    ),
                    fit_picture(width as f32, height as f32, limit, PICTURE_HEIGHT),
                    "decoding a {width}x{height} photo must not shift the transcript"
                );
            }
        }
    }

    #[test]
    fn reaction_affordance_action_targets_the_message_it_belongs_to() {
        let action = open_reaction_picker_action("chat@example", "message-42");
        assert!(matches!(
            action,
            Action::OpenReactionPicker { chat, message, beside_menu: false }
                if chat == "chat@example" && message == "message-42"
        ));
    }

    #[test]
    fn reaction_button_label_names_whose_message_it_reacts_to() {
        assert_eq!(reaction_button_label(true, "Ada"), "React to your message");
        assert_eq!(
            reaction_button_label(false, "Ada"),
            "React to Ada's message"
        );
    }

    #[test]
    fn reaction_affordance_sits_beside_the_bubble_without_clipping() {
        let bounds = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let incoming = Rect::from_min_max(pos2(20.0, 20.0), pos2(140.0, 80.0));
        let outgoing = Rect::from_min_max(pos2(260.0, 20.0), pos2(380.0, 80.0));

        let incoming_button = reaction_affordance_rect(incoming, bounds, false);
        let outgoing_button = reaction_affordance_rect(outgoing, bounds, true);

        assert!(incoming_button.left() > incoming.right());
        assert!(outgoing_button.right() < outgoing.left());
        assert!(bounds.contains_rect(incoming_button));
        assert!(bounds.contains_rect(outgoing_button));

        let edge = Rect::from_min_max(pos2(360.0, 260.0), pos2(398.0, 298.0));
        assert!(bounds.contains_rect(reaction_affordance_rect(edge, bounds, false)));
    }

    #[test]
    fn reaction_affordance_stays_visible_while_crossing_to_its_button() {
        let bubble = Rect::from_min_max(pos2(20.0, 20.0), pos2(140.0, 80.0));
        let button = Rect::from_min_max(pos2(146.0, 24.0), pos2(172.0, 50.0));

        assert!(reaction_affordance_visible(
            Some(bubble.center()),
            bubble,
            button
        ));
        assert!(reaction_affordance_visible(
            Some(button.center()),
            bubble,
            button
        ));
        assert!(!reaction_affordance_visible(
            Some(pos2(300.0, 200.0)),
            bubble,
            button
        ));
        assert!(!reaction_affordance_visible(None, bubble, button));
    }

    /// One pinned row, as the archive hands it over.
    fn pinned(chat: &str, id: &str) -> crate::archive::Pinned {
        crate::archive::Pinned {
            chat: chat.to_owned(),
            id: id.to_owned(),
            pinned_at: 1,
            expires_at: i64::MAX,
            text: format!("message {id}"),
            from_me: false,
            sent_at: 1,
            pinner: crate::archive::Pinner {
                by_me: true,
                sender: String::new(),
            },
        }
    }

    #[test]
    fn a_pin_notice_names_who_pinned_the_message() {
        let names = |id: &str, _: Option<&str>| id.split('@').next().unwrap_or(id).to_owned();
        let mut row = pinned("1@s.whatsapp.net", "m1");
        assert_eq!(
            pin_notice_label(crate::i18n::Locale::English, &row, &names),
            "You pinned a message"
        );

        row.pinner = crate::archive::Pinner {
            by_me: false,
            sender: "bob@s.whatsapp.net".to_owned(),
        };
        assert_eq!(
            pin_notice_label(crate::i18n::Locale::English, &row, &names),
            "bob pinned a message"
        );

        // An archive that does not say who pinned it still shows the pin.
        row.pinner.sender.clear();
        assert_eq!(
            pin_notice_label(crate::i18n::Locale::English, &row, &names),
            "A message was pinned"
        );
    }

    #[test]
    fn an_expired_pin_leaves_the_line_under_the_header() {
        let mut gone = pinned("1@s.whatsapp.net", "m2");
        gone.expires_at = 1_000;
        let mut live = pinned("1@s.whatsapp.net", "m1");
        live.expires_at = 2_000;

        let rows = live_pins(&[gone, live.clone()], 1_500);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "m1");
        assert!(
            live_pins(&[live], 2_000).is_empty(),
            "the moment it runs out, it leaves"
        );
    }

    #[test]
    fn an_open_pin_asks_for_a_frame_when_it_expires() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _) = App::headless(
            crate::paths::AppDirs::under(directory.path()),
            crate::settings::Settings::default(),
        );
        let chat = Chat::new("1@s.whatsapp.net".into(), "Ada".into());
        let mut row = pinned(&chat.id, "m1");
        row.expires_at = crate::util::now() + 30;
        app.chat_pins.insert(chat.id.clone(), vec![row]);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            pin_banner(&mut app, ui, &chat, false);
        });
        output.textures_delta.clear();
        let delay = output
            .viewport_output
            .values()
            .map(|viewport| viewport.repaint_delay)
            .min()
            .unwrap_or(std::time::Duration::MAX);
        assert!(
            delay.as_secs() <= 31,
            "the banner wakes when the pin runs out"
        );
    }

    #[test]
    fn a_pin_notice_sits_above_the_message_that_followed_its_pin() {
        // `chat_pins` hands the newest pin over first. A pin at 100 and one at
        // 500 belong above the messages at 200 and 600, one each.
        let mut newest_first = vec![
            pinned("1@s.whatsapp.net", "c"),
            pinned("1@s.whatsapp.net", "a"),
        ];
        newest_first[0].pinned_at = 500;
        newest_first[1].pinned_at = 100;

        let ordered = pin_notices_in_time_order(&newest_first);
        assert_eq!(
            ordered.iter().map(|row| row.pinned_at).collect::<Vec<_>>(),
            [100, 500],
            "the list goes back into time order before it is placed"
        );
        assert_eq!(notices_above(&ordered[..], 200), 1, "one notice above 200");
        assert_eq!(notices_above(&ordered[1..], 600), 1, "and one above 600");

        // Handed over unsorted, both would pile above the later message, which
        // is the bug this keeps out.
        assert_eq!(notices_above(&newest_first[..], 200), 0);
    }

    #[test]
    fn the_pin_line_shows_the_newest_pin_and_a_click_walks_the_rest() {
        let rows = vec![
            pinned("1@s.whatsapp.net", "newest"),
            pinned("1@s.whatsapp.net", "middle"),
            pinned("1@s.whatsapp.net", "oldest"),
        ];
        // Nothing shown yet, or the shown pin gone: the newest.
        assert_eq!(shown_pin(&rows, None), 0);
        assert_eq!(shown_pin(&rows, Some("unpinned")), 0);
        assert_eq!(shown_pin(&rows, Some("middle")), 1);
        // Each click moves on to the next older pin and wraps to the newest.
        assert_eq!(next_pin(3, 0), 1);
        assert_eq!(next_pin(3, 1), 2);
        assert_eq!(next_pin(3, 2), 0);
        // One pin is its own next pin.
        assert_eq!(next_pin(1, 0), 0);
    }

    #[test]
    fn the_pin_bar_has_a_segment_per_pin_inside_the_line() {
        for count in 1..=3 {
            let segments = pin_bar_segments(10.0, 28.0, count);
            assert_eq!(segments.len(), count);
            assert!((segments[0].0 - 10.0).abs() < 0.01);
            assert!((segments[count - 1].1 - 38.0).abs() < 0.01, "{segments:?}");
            for pair in segments.windows(2) {
                assert!(pair[1].0 > pair[0].1, "segments do not touch: {segments:?}");
            }
        }
    }

    #[test]
    fn a_click_on_the_pin_line_opens_the_pin_and_shows_the_next() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _) = App::headless(
            crate::paths::AppDirs::under(directory.path()),
            crate::settings::Settings::default(),
        );
        let chat = Chat::new("1@s.whatsapp.net".into(), "Ada".into());
        app.chat_pins.insert(
            chat.id.clone(),
            vec![pinned(&chat.id, "newest"), pinned(&chat.id, "older")],
        );
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(600.0, 400.0));
        // Clicks a second apart, so none of them counts as a double click.
        let clock = std::cell::Cell::new(0.0);
        let frame = |events: Vec<egui::Event>, app: &mut App, step: f64| {
            clock.set(clock.get() + step);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(clock.get()),
                    events,
                    ..Default::default()
                },
                |ui| pin_banner(app, ui, &chat, false),
            );
            output.textures_delta.clear();
        };
        let at = pos2(300.0, PIN_ROW_HEIGHT / 2.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // The line has to be laid out once before it can take a click.
        frame(vec![egui::Event::PointerMoved(at)], &mut app, 1.0);
        let mut opened = Vec::new();
        for _ in 0..3 {
            frame(
                vec![egui::Event::PointerMoved(at), press(true)],
                &mut app,
                1.0,
            );
            frame(vec![press(false)], &mut app, 0.05);
            for action in app.actions.drain(..) {
                if let Action::OpenMessage { message, .. } = action {
                    opened.push(message);
                }
            }
        }
        assert_eq!(opened, ["newest", "older", "newest"]);
    }

    #[test]
    fn the_footer_takes_a_slot_only_for_the_marks_on_the_bubble() {
        let ctx = egui::Context::default();
        let mut clock = 0.0;
        let mut plain = 0.0;
        let mut pinned = 0.0;
        let mut both = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 200.0))),
                ..Default::default()
            },
            |ui| {
                let message = crate::archive::tests::message("1@s.whatsapp.net", "m1", 0, false);
                clock = clock_width(ui, &message);
                plain = footer_width(ui, &message, false, false);
                pinned = footer_width(ui, &message, false, true);
                both = footer_width(ui, &message, true, true);
            },
        );
        output.textures_delta.clear();
        // Measured against the clock the reader's own format produces, not a
        // number that only holds on a 24-hour one: the same footer is a third
        // wider where the system shows "12:00 AM".
        assert!(
            (plain - clock).abs() < 0.01,
            "a message without a mark is just the clock, {plain} vs {clock}"
        );
        assert!(
            (pinned - clock - FOOTER_MARK_SLOT).abs() < 0.5,
            "a pin takes one slot, {pinned} vs {clock}"
        );
        assert!(
            (both - clock - FOOTER_MARK_SLOT * 2.0).abs() < 0.5,
            "a star and a pin take one slot each, {both} vs {clock}"
        );
    }

    #[test]
    fn the_footer_paints_a_pin_mark_when_the_message_is_pinned() {
        let ctx = egui::Context::default();
        with_still_view(|view| {
            let shapes = |pinned: bool| {
                let message = crate::archive::tests::message("1@s.whatsapp.net", "m1", 0, false);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(400.0, 200.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        footer(ui, view, &message, None, false, pinned, &mut Vec::new());
                    },
                );
                output.textures_delta.clear();
                output.shapes.len()
            };
            assert_eq!(
                shapes(true),
                shapes(false) + 1,
                "a pinned message paints one extra mark"
            );
        });
    }

    #[test]
    fn picture_frames_keep_their_shape_within_the_limit() {
        let landscape = frame_size(&media(Some(1600), Some(1200)), None, 340.0, PICTURE_HEIGHT);
        assert!((landscape.x - 340.0).abs() < 0.01);
        assert!((landscape.y - 255.0).abs() < 0.01);
        let tall = frame_size(&media(Some(600), Some(1200)), None, 340.0, PICTURE_HEIGHT);
        assert!(tall.y > 340.0 && tall.y <= PICTURE_HEIGHT);
        let exact = fit_picture(900.0, 1600.0, PICTURE_WIDTH, PICTURE_HEIGHT);
        assert!((exact.y - PICTURE_HEIGHT).abs() < 0.01);
        assert!(exact.x < PICTURE_WIDTH);
        let unknown = frame_size(&media(None, None), Some((16, 9)), 340.0, PICTURE_HEIGHT);
        assert!(unknown.x > unknown.y);
        let tiny = frame_size(&media(Some(40), Some(40)), None, 340.0, PICTURE_HEIGHT);
        assert!(tiny.x >= 120.0);
    }

    #[test]
    fn the_footer_reserves_a_slot_only_when_the_message_is_starred() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut clock = 0.0;
        let mut incoming = 0.0;
        let mut starred = 0.0;
        let mut own = 0.0;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 200.0))),
                ..Default::default()
            },
            |ui| {
                let from_them = crate::archive::tests::message("1@s.whatsapp.net", "m1", 0, false);
                let from_me = crate::archive::tests::message("1@s.whatsapp.net", "m1", 0, true);
                clock = clock_width(ui, &from_them);
                incoming = footer_width(ui, &from_them, false, false);
                starred = footer_width(ui, &from_them, true, false);
                own = footer_width(ui, &from_me, false, false);
            },
        );
        output.textures_delta.clear();
        // Measured against the clock the reader's own format produces, not a
        // number that only holds on a 24-hour one: the same footer is a third
        // wider where the system shows "12:00 AM".
        assert!(
            (incoming - clock).abs() < 0.01,
            "a plain incoming footer is just the clock, {incoming} vs {clock}"
        );
        assert!(
            (starred - clock - FOOTER_MARK_SLOT).abs() < 0.5,
            "a star adds one mark slot, {starred} vs {clock}"
        );
        assert!(
            (own - incoming - 19.0).abs() < 0.5,
            "our own messages add the delivery ticks, {own} vs {incoming}"
        );
    }

    /// Runs `check` with a still view of one plain chat, as a footer test
    /// needs one to paint into.
    fn with_still_view(check: impl FnOnce(&View<'_>)) {
        let directory = tempfile::tempdir().unwrap();
        let (app, _events) = App::headless(
            crate::paths::AppDirs::under(directory.path()),
            crate::settings::Settings::default(),
        );
        let chat = Chat::new("1@s.whatsapp.net".into(), "Ada".into());
        let names_or = |id: &str, hint: Option<&str>| app.display_name_or(id, hint);
        let mention_names = |id: &str| app.mention_name(id);
        let avatars = HashMap::new();
        let keyboard_navigation = std::cell::Cell::new(false);
        let view = View {
            palette: Palette::dark(),
            locale: app.locale,
            chat: &chat,
            me: None,
            auto_download: app.settings.auto_download,
            connected: false,
            poll_voting: &app.poll_voting,
            interactive_pending: &app.interactive_sending,
            anchor: None,
            open_menu: None,
            reaction: None,
            reaction_menu: false,
            reaction_emoji: &app.settings.reaction_emoji,
            menu: app.settings.message_menu,
            keyboard_navigation: &keyboard_navigation,
            names_or: &names_or,
            mention_names: &mention_names,
            avatars: &avatars,
            contacts: &app.contacts,
            now: 0,
            animate: false,
            player: &app.player,
            voice_speed: app.settings.voice_speed,
            time_inside: app.palette.bubbles != theme::BubbleStyle::Nothing
                || app.settings.always_show_time,
            video: &app.video,
            account_receipts_off: false,
            copy_rows: Some(app.copy_rows.as_ref()),
            swipes: app.settings.swipes,
            doodles: None,
            scrims: None,
            rows: None,
            selecting: false,
            selected: &[],
            motion_playing: None,
            starred: None,
            pins: None,
            pin_notices: &[],
        };
        check(&view);
    }

    #[test]
    fn the_footer_paints_a_star_mark_when_the_message_is_starred() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        with_still_view(|view| {
            let shapes = |starred: bool| {
                let message = crate::archive::tests::message("1@s.whatsapp.net", "m1", 0, false);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(400.0, 200.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        footer(ui, view, &message, None, starred, false, &mut Vec::new());
                    },
                );
                output.textures_delta.clear();
                output.shapes.len()
            };
            assert_eq!(shapes(true), shapes(false) + 1, "a star paints one mark");
        });
    }

    #[test]
    fn a_picture_without_a_caption_paints_the_marks_it_carries() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        with_still_view(|view| {
            let shapes = |starred: bool, pinned: bool| {
                let message = crate::archive::tests::message("1@s.whatsapp.net", "m1", 0, false);
                let picture = Rect::from_min_size(egui::Pos2::ZERO, vec2(240.0, 180.0));
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(400.0, 200.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        footer_over_picture(
                            ui,
                            view,
                            &message,
                            picture,
                            starred,
                            pinned,
                            &mut Vec::new(),
                        );
                    },
                );
                output.textures_delta.clear();
                output.shapes.len()
            };
            assert_eq!(
                shapes(true, false),
                shapes(false, false) + 1,
                "a star on a picture paints one mark"
            );
            assert_eq!(
                shapes(false, true),
                shapes(false, false) + 1,
                "a pin on a picture paints one mark"
            );
            assert_eq!(
                shapes(true, true),
                shapes(false, false) + 2,
                "both marks on a picture paint both"
            );
        });
    }

    #[test]
    fn the_star_mark_tint_is_not_the_secondary() {
        let palette = Palette::dark();
        assert_ne!(FOOTER_STAR, palette.secondary);
    }

    #[test]
    fn an_edited_bubble_keeps_its_size_until_the_text_outgrows_it() {
        // A short text keeps the body's drawn width, time beside it.
        let (width, inline) = inline_editor_width(40.0, 40.0, 110.0, 400.0, Some(50.0));
        assert_eq!(width, 110.0);
        assert!(inline);
        // Typing past it grows the bubble to the text and the time.
        let (width, inline) = inline_editor_width(150.0, 150.0, 110.0, 400.0, Some(50.0));
        assert_eq!(width, 150.0 + 8.0 + 50.0);
        assert!(inline);
        // Never past the widest bubble; the time moves under a full line.
        let (width, inline) = inline_editor_width(398.0, 398.0, 110.0, 400.0, Some(50.0));
        assert_eq!(width, 398.0);
        assert!(!inline);
        let (width, _) = inline_editor_width(420.0, 420.0, 110.0, 400.0, Some(50.0));
        assert_eq!(width, 400.0);
    }

    #[test]
    fn an_edit_opens_at_the_size_the_bubble_was_drawn() {
        // As `rich_body` sizes a line with the time beside it, and one with
        // the time on a row of its own: the first frame of the edit matches.
        let body = 40.0 + 8.0 + 50.0;
        assert_eq!(
            inline_editor_width(40.0, 40.0, body, 400.0, Some(50.0)),
            (body, true)
        );
        assert_eq!(
            inline_editor_width(360.0, 360.0, 360.0, 400.0, Some(50.0)),
            (360.0, false)
        );
        assert_eq!(
            inline_editor_width(380.0, 120.0, 380.0, 400.0, None),
            (380.0, false)
        );
    }

    /// Editing a sent message in its bubble draws the editor at the bubble's
    /// own size, on the layer above the scrim the rest of the chat recedes
    /// under, and Escape puts the row back once the scrim has faded.
    #[test]
    fn an_edit_in_place_keeps_its_bubble_and_lifts_it_over_the_chat() {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::app::App::headless(
            crate::paths::AppDirs::under(root.path()),
            crate::settings::Settings::default(),
        )
        .0;
        app.link = crate::backend::LinkStatus::Connected;
        app.window_focused = true;
        let chat = crate::model::Chat::new("123@s.whatsapp.net".into(), "Alice".into());
        app.chats.push(chat.clone());
        app.open_chat = Some(chat.id.clone());
        app.scroll_to_bottom = true;
        let now = crate::util::now();
        let text = "see you at the station";
        let mut conversation = crate::app::Conversation::default();
        conversation.messages.push(sample_message(
            &chat.id,
            "theirs",
            "where are you",
            now - 60,
        ));
        let mut own = sample_message(&chat.id, "own", text, now);
        own.from_me = true;
        conversation.messages.push(own);
        conversation.complete = true;
        app.conversations.insert(chat.id.clone(), conversation);

        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        let mut time = 0.0;
        let mut frame = |app: &mut crate::app::App, events: Vec<egui::Event>| {
            time += 1.0 / 60.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| show(app, ui),
            );
            output.textures_delta.clear();
        };
        for _ in 0..8 {
            frame(&mut app, Vec::new());
        }
        let bubble = bubble_id(&chat.id, "own");
        let rect = || {
            ctx.data(|data| data.get_temp::<Rect>(bubble.with("rect")))
                .expect("the bubble was drawn")
        };
        let shown = rect();
        let body = ctx
            .data(|data| data.get_temp::<Rect>(bubble.with("body")))
            .expect("the body was drawn");
        // What a double-click on it does.
        let slot = inline_edit_slot(&chat.id);
        ctx.data_mut(|data| {
            data.insert_temp(slot, ("own".to_owned(), text.to_owned()));
            data.insert_temp(slot.with("size"), body.size());
        });
        frame(&mut app, Vec::new());
        let editing = rect();
        assert!(
            (editing.min - shown.min).length() < 0.01
                && (editing.size() - shown.size()).length() < 0.01,
            "the bubble keeps its place and size: {shown:?} became {editing:?}"
        );
        let (_, lifted) = edit_focus_layers(&chat.id);
        let editor = bubble.with("inline-editor");
        assert_eq!(
            ctx.read_response(editor).map(|response| response.layer_id),
            Some(lifted),
            "the editor is drawn above the scrim"
        );
        assert_eq!(
            ctx.data(|data| data.get_temp::<String>(slot.with("focus"))),
            Some("own".to_owned())
        );
        frame(&mut app, Vec::new());
        assert!(ctx.memory(|memory| memory.has_focus(editor)));
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(
            ctx.data(|data| data.get_temp::<(String, String)>(slot))
                .is_none(),
            "Escape ends the edit"
        );
        for _ in 0..30 {
            frame(&mut app, Vec::new());
        }
        assert_eq!(
            ctx.data(|data| data.get_temp::<String>(slot.with("focus"))),
            None,
            "the scrim has gone"
        );
        assert!(
            ctx.read_response(bubble.with("body-text"))
                .is_some_and(|response| response.layer_id != lifted),
            "the row is back on the transcript"
        );
    }

    #[test]
    fn a_failed_footer_reserves_room_for_its_label() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut message = Message {
            id: "fixture".into(),
            chat: "1@s.whatsapp.net".into(),
            sender: "me@s.whatsapp.net".into(),
            sender_name: None,
            from_me: true,
            timestamp: 1000,
            content: Content::text("Fixture"),
            status: Delivery::Sent,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            history_order: None,
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        };
        let mut widths = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            widths.push(footer_width(ui, &message, false, false));
            message.status = Delivery::Failed;
            widths.push(footer_width(ui, &message, false, false));
            // Only our own messages can fail to send.
            message.from_me = false;
            widths.push(footer_width(ui, &message, false, false) + 19.0);
        });
        output.textures_delta.clear();
        assert!(widths[1] > widths[0] + 30.0, "{widths:?}");
        assert_eq!(widths[2], widths[0]);
    }

    #[test]
    fn a_bubble_narrower_than_its_time_row_lets_the_row_reach_left() {
        let bounds = egui::Rangef::new(0.0, 600.0);
        let wide = egui::Rangef::new(300.0, 500.0);
        assert_eq!(widen_to(wide, 60.0, bounds), wide, "a wide bubble is kept");
        let short = egui::Rangef::new(464.0, 500.0);
        let row = widen_to(short, 60.0, bounds);
        assert_eq!(row.max, 500.0, "still ends at the bubble's right edge");
        assert_eq!(row.span(), 60.0);
        let at_edge = egui::Rangef::new(0.0, 36.0);
        let row = widen_to(at_edge, 60.0, bounds);
        assert_eq!((row.min, row.max), (0.0, 60.0), "never past the view");
    }

    #[test]
    fn the_time_under_a_short_bubble_is_shown_whole() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut widths = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            widths.push(reference_timestamp_width(ui, "11:11 PM", true));
            widths.push(reference_timestamp_width(ui, "11:11 PM", false));
        });
        output.textures_delta.clear();
        assert!(widths[0] >= widths[1] + 18.0, "{widths:?}");
        assert!(widths[1] > 20.0, "{widths:?}");
    }

    #[test]
    fn visible_stickers_download_automatically_with_the_attachment_setting_off() {
        let mut sticker = media(Some(180), Some(180));
        assert!(auto_download_allowed(&sticker, true, false));
        assert!(!auto_download_allowed(&sticker, false, false));
        sticker.size = crate::model::ATTACHMENT_DOWNLOAD_LIMIT + 1;
        assert!(!auto_download_allowed(&sticker, true, false));
    }

    /// Every bubble's scrim ends up under every bubble: the second bubble's
    /// wash, painted after the first bubble, no longer veils its bottom.
    #[test]
    fn scrims_lie_under_every_bubble() {
        let ctx = egui::Context::default();
        let first = Rect::from_min_size(pos2(20.0, 20.0), vec2(160.0, 34.0));
        let second = first.translate(vec2(0.0, 38.0));
        let fill = Color32::from_rgb(0xd9, 0xfd, 0xd3);
        let wall = Color32::from_rgb(0xef, 0xea, 0xe2);
        let scrims = std::cell::RefCell::new(Vec::new());
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let slot = ui.painter().add(egui::Shape::Noop);
            for bubble in [first, second] {
                let scrim = ui.painter().add(egui::Shape::Noop);
                ui.painter().rect_filled(bubble, 8.0, fill);
                let shape = doodle_scrim(bubble, fill, Some((1.0, wall))).expect("a scrim");
                ui.painter().set(scrim, shape);
                scrims.borrow_mut().push((ui.layer_id(), scrim));
            }
            lay_scrims_under(ui.ctx(), ui.layer_id(), slot, &scrims.borrow());
        });
        output.textures_delta.clear();
        // In paint order: true for a scrim, false for a bubble.
        let mut order = Vec::new();
        let mut pending: Vec<&egui::Shape> = output
            .shapes
            .iter()
            .rev()
            .map(|clipped| &clipped.shape)
            .collect();
        while let Some(shape) = pending.pop() {
            match shape {
                egui::Shape::Vec(shapes) => pending.extend(shapes.iter().rev()),
                egui::Shape::Rect(rect) if rect.blur_width > 0.0 => order.push(true),
                egui::Shape::Rect(rect) if rect.fill == fill => order.push(false),
                _ => {}
            }
        }
        assert_eq!(order, [true, true, false, false]);
    }

    /// The typing dots animate at the display's rate, not a fixed timer.
    #[test]
    fn typing_dots_repaint_every_frame() {
        let ctx = egui::Context::default();
        let palette = crate::theme::Palette::dark();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| typing_dots(ui, &palette));
        output.textures_delta.clear();
        let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert_eq!(delay, std::time::Duration::ZERO);
    }

    /// With a screen reader attached the dots ask for a frame a step later,
    /// and each stepped frame draws exactly what an every-frame one would at
    /// that moment, so the wave keeps its speed and only skips frames.
    #[test]
    fn typing_dots_step_with_a_screen_reader_and_keep_their_phase() {
        let palette = crate::theme::Palette::dark();
        let draw = |reader: bool, time: f64| {
            let ctx = crate::motion::tests::reading(reader);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| typing_dots(ui, &palette),
            );
            output.textures_delta.clear();
            let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            let dots: Vec<_> = output
                .shapes
                .into_iter()
                .filter_map(|clipped| match clipped.shape {
                    egui::Shape::Circle(circle) => Some((circle.center, circle.fill)),
                    _ => None,
                })
                .collect();
            (delay, dots)
        };
        let mut seen = Vec::new();
        for step in 0..12 {
            let time = 5.0 + f64::from(step) * 0.1;
            let (plain_delay, plain) = draw(false, time);
            let (stepped_delay, stepped) = draw(true, time);
            assert_eq!(plain_delay, std::time::Duration::ZERO);
            assert!(
                stepped_delay > std::time::Duration::from_millis(75)
                    && stepped_delay <= crate::motion::STEP,
                "{stepped_delay:?}"
            );
            assert_eq!(stepped.len(), 3);
            assert_eq!(plain, stepped, "at {time}");
            seen.push(format!("{stepped:?}"));
        }
        // Over one 1.2 s wave the steps show it moving: only while all three
        // dots rest at the bottom do two steps look alike.
        seen.sort();
        seen.dedup();
        assert!(seen.len() >= 9, "{} different steps", seen.len());
    }

    #[test]
    fn composer_triggers_only_start_at_word_boundaries() {
        assert_eq!(standalone_trigger(":", 1, ':'), Some(0));
        assert_eq!(standalone_trigger("hello :", 7, ':'), Some(6));
        assert_eq!(standalone_trigger("hello @", 7, '@'), Some(6));
        assert_eq!(standalone_trigger("19:30", 3, ':'), None);
        assert_eq!(standalone_trigger("mail@example.com", 5, '@'), None);
    }

    #[test]
    fn mention_query_runs_from_the_at_to_the_cursor() {
        assert_eq!(active_mention("hello @mi", Some(6), 9), Some((9, "mi")));
        assert_eq!(active_mention("hello @mi\n", Some(6), 10), None);
        assert_eq!(active_mention("hello @mi", Some(99), 9), None);
    }

    #[test]
    fn emoji_query_ends_at_spaces_and_punctuation() {
        assert_eq!(active_emoji("hello :gri", Some(6), 10), Some((10, "gri")));
        assert_eq!(
            active_emoji("hello :gri_ning", Some(6), 15),
            Some((15, "gri_ning"))
        );
        assert_eq!(active_emoji("hello :gri ", Some(6), 11), None);
        assert_eq!(active_emoji("hello :gri!", Some(6), 11), None);
        assert_eq!(active_emoji("hello gri", Some(6), 9), None);
    }

    #[test]
    fn emoji_shortcodes_rank_ahead_of_name_matches() {
        let grinning = emojis::get("😀").expect("known emoji");
        assert_eq!(emoji_match_score(grinning, "grinning"), Some(0));
        assert_eq!(emoji_match_score(grinning, "grin"), Some(1));
        assert_eq!(emoji_match_score(grinning, "face"), Some(3));
        assert_eq!(emoji_match_score(grinning, "rocket"), None);
    }

    #[test]
    fn completion_offers_skin_tone_capable_emoji() {
        let directory = tempfile::tempdir().unwrap();
        let (app, _events) = App::headless(
            crate::paths::AppDirs::under(directory.path()),
            crate::settings::Settings::default(),
        );
        let offers = |query: &str, emoji: &str| {
            emoji_candidates(&app, query)
                .iter()
                .any(|suggestion| suggestion.emoji == emoji)
        };
        assert!(offers("pregnant", "🤰"));
        assert!(offers("thumbsup", "👍"));
    }
}

#[cfg(test)]
mod reaction_tests {
    use super::*;

    #[test]
    fn reaction_change_needs_an_earlier_tally() {
        let one = vec![("👍".to_owned(), 1)];
        let two = vec![("👍".to_owned(), 2)];
        assert_eq!(reaction_change(None, &one), ReactionChange::None);
        assert_eq!(reaction_change(Some(&one), &one), ReactionChange::None);
        assert_eq!(reaction_change(Some(&one), &two), ReactionChange::Changed);
        assert_eq!(reaction_change(Some(&[]), &one), ReactionChange::Arrived);
    }

    #[test]
    fn earlier_state_counts_only_from_the_frame_before() {
        assert_eq!(seen_last_frame(Some((9, 'a')), 10), Some('a'));
        assert_eq!(seen_last_frame(Some((10, 'a')), 10), Some('a'));
        assert_eq!(seen_last_frame(Some((5, 'a')), 10), None);
        assert_eq!(seen_last_frame::<char>(None, 10), None);
    }

    #[test]
    fn first_reaction_and_own_pick_are_live_changes() {
        let none: Vec<(String, u32)> = Vec::new();
        let mine = vec![("❤️".to_owned(), 1)];
        // A message drawn with no pill, then our pick landing, pops.
        assert_eq!(reaction_change(Some(&none), &mine), ReactionChange::Arrived);
        // A tally last drawn long ago is not a live change.
        let stale = seen_last_frame(Some((1, none.clone())), 100);
        assert_eq!(
            reaction_change(stale.as_deref(), &mine),
            ReactionChange::None
        );
    }

    #[test]
    fn status_advance_is_silent_on_first_sight_and_failure() {
        assert!(!status_advanced(None, Delivery::Sent));
        assert!(status_advanced(Some(Delivery::Pending), Delivery::Sent));
        assert!(status_advanced(Some(Delivery::Delivered), Delivery::Read));
        assert!(!status_advanced(Some(Delivery::Sent), Delivery::Sent));
        assert!(!status_advanced(Some(Delivery::Pending), Delivery::Failed));
    }

    use crate::model::{Delivery, Reaction};

    fn with_reactions(reactions: Vec<Reaction>) -> Message {
        Message {
            id: "m1".into(),
            chat: "a@s.whatsapp.net".into(),
            sender: "a@s.whatsapp.net".into(),
            sender_name: None,
            from_me: false,
            timestamp: 0,
            content: Content::text("hi"),
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions,
            history_order: None,
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    }

    fn reaction(from_me: bool, emoji: &str) -> Reaction {
        Reaction {
            sender: if from_me { "me" } else { "them" }.into(),
            from_me,
            emoji: emoji.into(),
        }
    }

    /// The tail goes to a run's last bubble, past the messages drawn without
    /// one after it, and only to one in the same run.
    #[test]
    fn the_tail_goes_to_the_last_bubble_of_a_run() {
        let photo = Message {
            id: "photo".into(),
            content: Content::Image {
                caption: None,
                media: crate::model::Media {
                    album: None,
                    mime: "image/jpeg".into(),
                    size: 1,
                    width: None,
                    height: None,
                    path: None,
                    state: crate::model::MediaState::Idle,
                },
                motion: None,
            },
            ..with_reactions(Vec::new())
        };
        let text = with_reactions(Vec::new());
        let mine = Message {
            from_me: true,
            ..with_reactions(Vec::new())
        };
        let messages = Palette::messages();
        // Text, then a lone photo in the same run: the text keeps the tail.
        let run = [text.clone(), photo.clone()];
        assert!(last_bubble_of_run(&messages, &run, 0));
        // Text, text: only the second.
        let run = [text.clone(), text.clone()];
        assert!(!last_bubble_of_run(&messages, &run, 0));
        assert!(last_bubble_of_run(&messages, &run, 1));
        // A photo, then text of the other side: the run ends, the text tails.
        let run = [text.clone(), photo.clone(), mine, text.clone()];
        assert!(last_bubble_of_run(&messages, &run, 0));
        // WhatsApp frames the photo, so there it is the last bubble.
        assert!(!last_bubble_of_run(
            &Palette::dark(),
            &[text.clone(), photo.clone()],
            0
        ));
    }

    /// The avatar Messages puts beside a run goes to its last message,
    /// bubble or not, and a run ends with another sender or another day.
    #[test]
    fn a_run_ends_with_its_last_message() {
        let from = |sender: &str, timestamp: i64| Message {
            sender: sender.into(),
            timestamp,
            ..with_reactions(Vec::new())
        };
        let run = [
            from("a", 0),
            from("a", 60),
            from("b", 120),
            from("b", 2 * 86_400),
        ];
        assert!(!last_of_run(&run, 0));
        assert!(last_of_run(&run, 1));
        // Another day starts another run, from the same sender too.
        assert!(last_of_run(&run, 2));
        assert!(last_of_run(&run, 3));
    }

    /// Messages draws a photo alone in its message without a bubble, with
    /// a bubble's corners; anything else in the message keeps the bubble.
    /// A quote on your own Messages balloon is white on its blue, in both
    /// appearances; on others' balloons and in WhatsApp's style it keeps the
    /// quoted sender's colour.
    #[test]
    fn quotes_on_own_messages_balloons_are_white() {
        let tint = Color32::from_rgb(0x80, 0x40, 0xc0);
        for palette in [Palette::messages(), Palette::messages_light()] {
            let own = quote_colours(&palette.on_bubble(true), true, || tint);
            assert_eq!(own.name, Color32::WHITE);
            assert_eq!(own.text, Color32::WHITE.gamma_multiply(0.6));
            assert_eq!(own.bar, Color32::WHITE.gamma_multiply(0.6));
            assert_eq!(own.fill, Color32::WHITE.gamma_multiply(0.15));
            let theirs = quote_colours(&palette.on_bubble(false), false, || tint);
            assert_eq!((theirs.name, theirs.bar), (tint, tint));
        }
        for palette in [Palette::dark(), Palette::light()] {
            let own = quote_colours(&palette.on_bubble(true), true, || tint);
            assert_eq!((own.name, own.bar), (tint, tint));
            assert_eq!(own.fill, palette.window.gamma_multiply(0.35));
        }
    }

    #[test]
    fn captioned_pictures_keep_an_even_rim_in_messages_and_whatsapp() {
        let mut photo = with_reactions(Vec::new());
        photo.content = Content::Image {
            caption: Some("Look".into()),
            media: crate::model::Media {
                album: None,
                mime: "image/jpeg".into(),
                size: 1,
                width: None,
                height: None,
                path: None,
                state: crate::model::MediaState::Idle,
            },
            motion: None,
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for palette in [Palette::messages(), Palette::dark()] {
                let margin = bubble_margin(&palette, &photo, false);
                let bleed = picture_bleed(ui, &palette, &photo, true);
                // Rim at the sides equals the rim above, which is the bubble's top margin.
                assert_eq!(margin.left + bleed.left, margin.top);
                assert_eq!(margin.right + bleed.right, margin.top);
                let bare = picture_bleed(ui, &palette, &photo, false);
                assert_eq!(margin.bottom + bare.bottom, margin.top);
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn only_lone_pictures_stand_without_a_bubble_in_messages() {
        let photo = |caption: Option<&str>| Message {
            content: Content::Image {
                caption: caption.map(Into::into),
                media: crate::model::Media {
                    album: None,
                    mime: "image/jpeg".into(),
                    size: 1,
                    width: None,
                    height: None,
                    path: None,
                    state: crate::model::MediaState::Idle,
                },
                motion: None,
            },
            ..with_reactions(Vec::new())
        };
        let messages = Palette::messages();
        assert!(bare_picture(&messages, &photo(None)));
        assert_eq!(
            picture_radius(&messages, &photo(None)),
            f32::from(widgets::MESSAGES_BUBBLE_RADIUS)
        );
        assert!(bare_picture(&Palette::messages_light(), &photo(None)));
        // WhatsApp's bubbles keep their frame around every picture.
        assert!(!bare_picture(&Palette::dark(), &photo(None)));
        assert_eq!(
            picture_radius(&Palette::dark(), &photo(None)),
            f32::from(widgets::BUBBLE_RADIUS) - 4.0
        );
        // A caption, a quote or the forwarded label keep the bubble.
        assert!(!bare_picture(&messages, &photo(Some("Look"))));
        let mut forwarded = photo(None);
        forwarded.forwarded = true;
        assert!(!bare_picture(&messages, &forwarded));
        let mut quoting = photo(None);
        quoting.quoted = Some(crate::model::Quoted {
            labelled: None,
            chat: None,
            id: "q".into(),
            sender: "a@s.whatsapp.net".into(),
            sender_name: None,
            summary: "hi".into(),
            mentions: Vec::new(),
        });
        assert!(!bare_picture(&messages, &quoting));
        assert!(!bare_picture(&messages, &with_reactions(Vec::new())));
    }

    #[test]
    fn only_our_own_reaction_counts_as_chosen() {
        let message = with_reactions(vec![reaction(false, "😂"), reaction(true, "❤️")]);
        assert_eq!(own_reaction(&message), Some("❤️"));
        assert_eq!(quick_reactions(&message, &[]), QUICK_REACTIONS.to_vec());
        assert_eq!(
            own_reaction(&with_reactions(vec![reaction(false, "😂")])),
            None
        );
    }

    #[test]
    fn quick_reactions_start_with_preferences_and_fill_with_unique_defaults() {
        let message = with_reactions(Vec::new());
        let preferred = vec![("🦀".into(), 5), ("👍".into(), 2), ("invalid".into(), 1)];
        let quick = quick_reactions(&message, &preferred);
        assert_eq!(&quick[..2], &["🦀", "👍"]);
        assert_eq!(quick.len(), 6);
        assert_eq!(quick.iter().filter(|&&emoji| emoji == "👍").count(), 1);
    }

    #[test]
    fn an_unusual_reaction_of_ours_joins_the_quick_row() {
        let message = with_reactions(vec![reaction(true, "🦀")]);
        let quick = quick_reactions(&message, &[]);
        assert_eq!(quick.len(), QUICK_REACTIONS.len() + 1);
        assert_eq!(quick.last(), Some(&"🦀"));
    }

    #[test]
    fn reaction_choice_toggles_or_replaces_our_emoji() {
        assert_eq!(reaction_choice(None, "🦀"), "🦀");
        assert_eq!(reaction_choice(Some("🦀"), "🦀"), "");
        assert_eq!(reaction_choice(Some("👍"), "🦀"), "🦀");
        assert_eq!(reaction_choice(Some("❤️"), "❤️"), "");
    }

    #[test]
    fn another_senders_custom_reaction_is_kept_on_the_message() {
        let message = with_reactions(vec![reaction(false, "🏆")]);
        assert_eq!(own_reaction(&message), None);
        assert_eq!(message.reactions[0].emoji, "🏆");
        assert!(!message.reactions[0].from_me);
        assert_eq!(quick_reactions(&message, &[]), QUICK_REACTIONS.to_vec());
    }

    /// A message as a list row draws its menu: starred or not, pinned or not.
    struct MenuRow {
        message: Message,
        marks: Marks,
        pinned: bool,
    }

    /// The accessible labels of the message menu for `row`, drawn in
    /// `locale` the way a right-click draws it.
    fn drawn_menu_labels(locale: crate::i18n::Locale, row: &MenuRow) -> Vec<String> {
        let message = &row.message;
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _) = App::headless(
            crate::paths::AppDirs::under(directory.path()),
            crate::settings::Settings {
                // Off by default, but still rows the menu's width allows for.
                message_menu: crate::settings::MessageMenu {
                    reply: true,
                    forward: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        app.locale = locale;
        if row.pinned {
            app.pins
                .insert(message.chat.clone(), HashSet::from([message.id.clone()]));
        }
        let chat = Chat::new(message.chat.clone(), "Ada".into());
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::theme::install(&ctx);
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            list_menu(&app, ui, &chat, message, row.marks, &mut actions);
        });
        output.textures_delta.clear();
        let tree = output
            .platform_output
            .accesskit_update
            .expect("accessibility tree");
        tree.nodes
            .iter()
            .filter_map(|(_, node)| node.label().map(str::to_owned))
            .collect()
    }

    /// The labels of the message menu made of words: quick reactions are emoji.
    fn drawn_menu_words(locale: crate::i18n::Locale, row: &MenuRow) -> Vec<String> {
        drawn_menu_labels(locale, row)
            .into_iter()
            .filter(|label| label.chars().any(|c| c.is_alphabetic()))
            .collect()
    }

    /// Rows whose menus between them show every entry a list row can: our
    /// own fresh delivered text, starred and pinned, photos with and without
    /// their file and one downloading, a video and a sticker.
    fn menu_fixtures(file: &Path) -> Vec<MenuRow> {
        use MediaState::{Downloading, Idle};
        let mut own = with_reactions(Vec::new());
        own.from_me = true;
        own.timestamp = crate::util::now();
        own.status = Delivery::Delivered;
        let path = Some(file.to_path_buf());
        let media = |mime: &str, path: Option<PathBuf>, state| Media {
            mime: mime.into(),
            size: 1,
            width: None,
            height: None,
            album: None,
            path,
            state,
        };
        let row = |content: Content| {
            let mut message = with_reactions(Vec::new());
            message.content = content;
            MenuRow {
                message,
                marks: Marks { starred: false },
                pinned: false,
            }
        };
        let photo = |path: Option<PathBuf>, state| {
            row(Content::Image {
                caption: None,
                media: media("image/jpeg", path, state),
                motion: None,
            })
        };
        vec![
            MenuRow {
                message: own,
                marks: Marks { starred: true },
                pinned: true,
            },
            photo(path.clone(), Idle),
            photo(None, Idle),
            photo(None, Downloading),
            row(Content::Video {
                caption: None,
                media: media("video/mp4", path.clone(), Idle),
                seconds: None,
                gif: false,
                note: false,
            }),
            row(Content::Sticker {
                media: media("image/webp", path, Idle),
                animated: false,
            }),
        ]
    }

    #[test]
    fn the_message_menu_speaks_the_interface_language() {
        // #552: the menu came out in English whatever the language.
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("photo.jpg");
        std::fs::write(&file, b"jpeg").unwrap();
        let mut labels = Vec::new();
        for row in menu_fixtures(&file) {
            let spanish = drawn_menu_words(crate::i18n::Locale::Spanish, &row);
            for english in drawn_menu_words(crate::i18n::Locale::English, &row) {
                assert!(!spanish.contains(&english), "{english} in {spanish:?}");
            }
            labels.extend(spanish);
        }
        for spanish in [
            "Copiar texto",
            "Eliminar",
            "Copiar imagen",
            "Mostrar en la carpeta",
            "Descargar",
        ] {
            assert!(
                labels.iter().any(|label| label == spanish),
                "{spanish} in {labels:?}"
            );
        }
    }

    #[test]
    fn every_message_menu_label_can_be_translated() {
        const TEMPLATE: &str = include_str!("../../assets/i18n/zapfast.pot");
        // Line by line: a Windows checkout may end the template's lines in CRLF.
        let translatable = |text: &str| {
            let msgid = format!("msgid \"{text}\"");
            TEMPLATE.lines().any(|line| line == msgid)
        };
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("photo.jpg");
        std::fs::write(&file, b"jpeg").unwrap();
        let mut labels: Vec<String> = menu_fixtures(&file)
            .iter()
            .flat_map(|row| drawn_menu_words(crate::i18n::Locale::English, row))
            .collect();
        // The menu is as wide as the longest label it measures.
        let measured = message_menu_labels(crate::i18n::Locale::English);
        for label in &labels {
            assert!(measured.contains(label), "menu width leaves out {label}");
        }
        // The fixtures reach every entry a list row's menu can show. A private
        // reply starts from the group on screen, so only the chat offers it.
        // Delete's choices are its submenu's, which sizes itself.
        let undrawn: Vec<&str> = measured
            .iter()
            .map(String::as_str)
            .filter(|label| !labels.iter().any(|drawn| drawn == label))
            .collect();
        assert_eq!(undrawn, ["Reply privately"]);
        // Hover texts are not in the accessibility tree.
        labels.extend(["React with any emoji".into(), "Remove your reaction".into()]);
        labels.sort();
        labels.dedup();
        let untranslatable: Vec<_> = labels.iter().filter(|label| !translatable(label)).collect();
        assert!(
            untranslatable.is_empty(),
            "not translatable: {untranslatable:?}"
        );
        for label in message_menu_labels(crate::i18n::Locale::English) {
            assert!(
                translatable(&label),
                "menu width uses untranslatable {label}"
            );
        }
    }

    #[test]
    fn the_selection_delete_menu_speaks_the_interface_language() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::theme::install(&ctx);
        let mut actions = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            selection_delete_items(
                ui,
                &crate::theme::Palette::dark(),
                crate::i18n::Locale::Spanish,
                "chat",
                true,
                &["message".to_owned()],
                &mut actions,
            );
        });
        output.textures_delta.clear();
        let tree = output
            .platform_output
            .accesskit_update
            .expect("accessibility tree");
        let labels: Vec<_> = tree
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .collect();
        assert!(labels.contains(&"Eliminar para todos"), "{labels:?}");
        assert!(labels.contains(&"Eliminar para mí"), "{labels:?}");
    }

    #[test]
    fn a_staged_picture_keeps_its_ratio_inside_the_bounds() {
        let bounds = vec2(440.0, 300.0);
        let wide = pending_media_size(vec2(1600.0, 900.0), bounds);
        assert!((wide.x / wide.y - 16.0 / 9.0).abs() < 0.01);
        assert!(wide.x <= bounds.x && wide.y <= bounds.y);
        let tall = pending_media_size(vec2(600.0, 1200.0), bounds);
        assert!((tall.y - bounds.y).abs() < 0.01 && (tall.x * 2.0 - tall.y).abs() < 0.01);
        // A tiny picture grows at most twice.
        assert_eq!(
            pending_media_size(vec2(100.0, 80.0), bounds),
            vec2(200.0, 160.0)
        );
    }

    #[test]
    fn the_message_menu_measures_star_and_unstar() {
        let labels = message_menu_labels(crate::i18n::Locale::German);
        assert!(labels.iter().any(|label| label == "Markierung entfernen"));
        assert!(labels.iter().any(|label| label == "Markieren"));
    }
}

/// A video's poster frame for the composer, read off the UI thread.
enum PendingPoster {
    Loading,
    Decoded(Option<egui::ColorImage>),
    Ready(egui::TextureHandle),
    Failed,
}

/// The poster of a staged video, started on first sight and picked up
/// when the decode finishes. `None` while it is still loading or when the
/// clip has no picture the app can decode.
fn pending_poster(ctx: &egui::Context, path: &std::path::Path) -> Option<(egui::TextureId, Vec2)> {
    let id = egui::Id::new(("pending-poster", path));
    let slot = ctx
        .data_mut(|data| data.get_temp::<std::sync::Arc<std::sync::Mutex<PendingPoster>>>(id))
        .unwrap_or_else(|| {
            let slot = std::sync::Arc::new(std::sync::Mutex::new(PendingPoster::Loading));
            ctx.data_mut(|data| data.insert_temp(id, slot.clone()));
            let (worker, ctx, path) = (slot.clone(), ctx.clone(), path.to_path_buf());
            std::thread::spawn(move || {
                let picture = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| crate::animation::poster(&bytes))
                    .and_then(|poster| poster.picture)
                    .map(|picture| {
                        let picture = image::DynamicImage::ImageRgb8(picture);
                        let picture = if picture.width().max(picture.height()) > 1024 {
                            picture.resize(1024, 1024, image::imageops::FilterType::Triangle)
                        } else {
                            picture
                        };
                        let rgba = picture.to_rgba8();
                        egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            &rgba,
                        )
                    });
                if let Ok(mut slot) = worker.lock() {
                    *slot = PendingPoster::Decoded(picture);
                }
                ctx.request_repaint();
            });
            slot
        });
    let mut state = slot.lock().ok()?;
    if let PendingPoster::Decoded(picture) = &mut *state {
        *state = match picture.take() {
            Some(image) => PendingPoster::Ready(ctx.load_texture(
                format!("pending-poster-{}", path.display()),
                image,
                egui::TextureOptions::LINEAR,
            )),
            None => PendingPoster::Failed,
        };
    }
    match &*state {
        PendingPoster::Ready(handle) => Some((handle.id(), handle.size_vec2())),
        _ => None,
    }
}

/// Lets go of the poster of every video no longer staged, here or parked with
/// another chat's draft. egui keeps temporary data for good, so its texture
/// would outlive the attachment.
fn forget_pending_posters(ctx: &egui::Context, app: &App) {
    let kept_id = egui::Id::new("pending-posters-kept");
    let kept = ctx
        .data_mut(|data| data.get_temp::<Vec<std::path::PathBuf>>(kept_id))
        .unwrap_or_default();
    let parked = app
        .accounts
        .iter()
        .flat_map(|account| account.attachment_drafts.values())
        .flat_map(|draft| draft.pending.iter());
    let current: Vec<std::path::PathBuf> = app
        .pending
        .iter()
        .chain(parked)
        .filter_map(|staged| match &staged.item {
            crate::app::Pending::File(path) if crate::image_preview::is_video(path) => {
                Some(path.clone())
            }
            _ => None,
        })
        .collect();
    if kept == current {
        return;
    }
    ctx.data_mut(|data| {
        for path in kept.iter().filter(|path| !current.contains(path)) {
            data.remove::<std::sync::Arc<std::sync::Mutex<PendingPoster>>>(egui::Id::new((
                "pending-poster",
                path.as_path(),
            )));
        }
        data.insert_temp(kept_id, current);
    });
}

/// Pixel size of a staged picture file, read once from its header.
fn pending_picture_size(ctx: &egui::Context, path: &std::path::Path) -> Option<Vec2> {
    let id = egui::Id::new(("pending-size", path));
    let known = ctx.data_mut(|data| data.get_temp::<Option<(u32, u32)>>(id));
    let size = known.unwrap_or_else(|| {
        let size = image::image_dimensions(path).ok();
        ctx.data_mut(|data| data.insert_temp(id, size));
        size
    });
    size.map(|(width, height)| vec2(width as f32, height as f32))
}

/// Largest size for a staged picture or clip: the native aspect ratio fitted
/// into `bounds`, enlarged at most twice, and never thinner than a tile.
fn pending_media_size(native: Vec2, bounds: Vec2) -> Vec2 {
    let native = vec2(native.x.max(1.0), native.y.max(1.0));
    let scale = (bounds.x / native.x).min(bounds.y / native.y).min(2.0);
    let fitted = native * scale;
    let floor = (PENDING_TILE / fitted.min_elem()).max(1.0);
    // A strip of a panorama keeps its ratio and may run past a tile's floor.
    let lifted = (fitted * floor).min(bounds.max(Vec2::splat(PENDING_TILE)));
    if lifted.x / lifted.y > 0.0 {
        lifted
    } else {
        fitted
    }
}

/// Side of the square a document or an undecoded attachment shows as.
const PENDING_TILE: f32 = 72.0;

/// Pending attachment tiles above the composer.
fn pending_strip(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let mut remove = None;
    let mut open = None;
    let reduce_motion = crate::motion::screen_reader_attached(ui.ctx());
    let animation_time = if reduce_motion {
        0.0
    } else {
        theme::MOTION_QUICK
    };
    // One attachment gets most of the panel, so a photo can be seen before it
    // goes out. Several share the width in pairs. Neither is a fixed size: the
    // picture's own ratio decides the shape inside these bounds.
    let spacing = 8.0;
    let available = ui.available_width();
    let count = app.pending.len();
    let screen_height = ui.ctx().content_rect().height();
    let bounds = if count == 1 {
        vec2(
            available.min(440.0),
            (screen_height * 0.38).clamp(160.0, 320.0),
        )
    } else {
        let side = ((available - spacing) / 2.0).clamp(120.0, 220.0);
        vec2(side, side)
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(spacing, spacing);
        for (index, item) in app.pending.iter_mut().enumerate() {
            // A picture can be cropped before it goes out. Anything else
            // cannot, and does not pretend to be clickable.
            let editable = match &item.item {
                crate::app::Pending::Picture { .. } => true,
                crate::app::Pending::File(path) => crate::app::Pending::is_picture_file(path),
                // A document goes as the file it is, so it is not edited here.
                crate::app::Pending::Document(_) => false,
            };
            let sense = if editable {
                Sense::click()
            } else {
                Sense::hover()
            };
            // A document shows as the file it arrives as, even a picture.
            let document = matches!(item.item, crate::app::Pending::Document(_));
            // What can be drawn as the picture itself, and how big it is.
            let ctx = ui.ctx().clone();
            let (poster, native) = match &item.item {
                crate::app::Pending::Picture { width, height, .. } => {
                    (None, Some(vec2(*width as f32, *height as f32)))
                }
                crate::app::Pending::File(path) if crate::app::Pending::is_picture_file(path) => {
                    (None, pending_picture_size(&ctx, path))
                }
                crate::app::Pending::File(path) if crate::image_preview::is_video(path) => {
                    let poster = pending_poster(&ctx, path);
                    // Until the frame arrives the clip holds a landscape slot.
                    let native = poster.map_or(vec2(16.0, 9.0), |(_, size)| size);
                    (poster, Some(native))
                }
                _ => (None, None),
            };
            let target = match native {
                Some(native) if !document => pending_media_size(native, bounds) + Vec2::splat(8.0),
                _ => Vec2::splat(PENDING_TILE),
            };
            let size = vec2(
                ctx.animate_value_with_time(
                    ui.id().with(("pending-w", item.id)),
                    target.x,
                    animation_time,
                ),
                ctx.animate_value_with_time(
                    ui.id().with(("pending-h", item.id)),
                    target.y,
                    animation_time,
                ),
            );
            if size != target {
                ctx.request_repaint();
            }
            let (rect, response) = ui.allocate_exact_size(size, sense);
            if ui.is_rect_visible(rect) {
                // Lifted off the chat like a bubble, with a bubble's corners;
                // a large picture takes the control radius so the corners
                // are in proportion to it.
                let radius = CornerRadius::same(if size.min_elem() > 120.0 {
                    theme::RADIUS_CONTROL
                } else {
                    widgets::BUBBLE_RADIUS
                });
                let inner_radius = if size.min_elem() > 120.0 {
                    theme::RADIUS_SMALL
                } else {
                    theme::RADIUS_SMALL.min(widgets::BUBBLE_RADIUS)
                };
                ui.painter()
                    .add(palette.bubble_shadow().as_shape(rect, radius));
                ui.painter().rect_filled(
                    rect.translate(vec2(0.0, -theme::RAISED_EDGE)),
                    radius,
                    palette.raised_edge(palette.surface),
                );
                ui.painter().rect_filled(rect, radius, palette.surface);
                let media = rect.shrink(4.0);
                match &mut item.item {
                    crate::app::Pending::Picture {
                        width,
                        height,
                        rgba,
                        texture,
                    } => {
                        let handle = texture.get_or_insert_with(|| {
                            // Limit thumbnails to the GPU's maximum texture size.
                            let image = if *width > 2048 || *height > 2048 {
                                let scale = 2048.0 / (*width).max(*height) as f32;
                                let (w, h) = (
                                    ((*width as f32 * scale) as u32).max(1),
                                    ((*height as f32 * scale) as u32).max(1),
                                );
                                // Borrowed: a copy of a 4K bitmap is 33 MB.
                                match image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
                                    *width as u32,
                                    *height as u32,
                                    rgba.as_slice(),
                                ) {
                                    Some(full) => {
                                        let small = image::imageops::resize(
                                            &full,
                                            w,
                                            h,
                                            image::imageops::FilterType::Triangle,
                                        );
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [w as usize, h as usize],
                                            &small,
                                        )
                                    }
                                    None => egui::ColorImage::example(),
                                }
                            } else {
                                egui::ColorImage::from_rgba_unmultiplied([*width, *height], rgba)
                            };
                            ui.ctx().load_texture(
                                format!("pending-picture-{index}"),
                                image,
                                egui::TextureOptions::LINEAR,
                            )
                        });
                        egui::Image::from_texture((handle.id(), media.size()))
                            .corner_radius(inner_radius)
                            .paint_at(ui, media);
                    }
                    crate::app::Pending::File(path) | crate::app::Pending::Document(path) => {
                        if !document && crate::app::Pending::is_picture_file(path) {
                            widgets::fitted_image(ui, path, media.size())
                                .fit_to_exact_size(media.size())
                                .corner_radius(inner_radius)
                                .paint_at(ui, media);
                        } else if !document && crate::image_preview::is_video(path) {
                            ui.painter().rect_filled(
                                media,
                                inner_radius,
                                palette.shadow.gamma_multiply(1.4),
                            );
                            if let Some((texture, _)) = poster {
                                egui::Image::from_texture((texture, media.size()))
                                    .corner_radius(inner_radius)
                                    .paint_at(ui, media);
                            }
                            // A clip says it is one, as it does in the chat.
                            let disc = Rect::from_center_size(media.center(), Vec2::splat(42.0));
                            ui.painter().circle_filled(
                                disc.center(),
                                21.0,
                                Color32::from_black_alpha(130),
                            );
                            theme::paint_icon(ui, Icon::Play, disc, 22.0, Color32::WHITE);
                        } else {
                            let icon = Rect::from_center_size(
                                rect.center() - vec2(0.0, 10.0),
                                Vec2::splat(24.0),
                            );
                            theme::paint_icon(ui, Icon::FileText, icon, 22.0, palette.secondary);
                            let name = path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let line = widgets::line(
                                ui,
                                &name,
                                theme::regular(10.5),
                                palette.text,
                                size.x - 8.0,
                                1,
                            );
                            line.paint(
                                ui,
                                egui::pos2(
                                    rect.center().x - line.size().x / 2.0,
                                    rect.bottom() - 18.0,
                                ),
                                palette.text,
                            );
                        }
                    }
                }
                if editable && response.hovered() {
                    // A picture that can be cropped says so, rather than
                    // looking like every other tile.
                    let dim = palette.shadow.gamma_multiply(1.4);
                    ui.painter().rect_filled(rect, radius, dim);
                    theme::paint_icon(
                        ui,
                        Icon::Pencil,
                        Rect::from_center_size(rect.center(), Vec2::splat(22.0)),
                        20.0,
                        palette.text,
                    );
                }
                // Remove button in the corner.
                let close =
                    Rect::from_center_size(rect.right_top() + vec2(-14.0, 14.0), Vec2::splat(22.0));
                let close_response =
                    ui.interact(close, ui.id().with(("unstage", item.id)), Sense::click());
                ui.painter()
                    .circle_filled(close.center(), 11.0, palette.overlay);
                theme::paint_icon(ui, Icon::X, close, 14.0, palette.text);
                if close_response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    remove = Some(index);
                } else if response.clicked() {
                    open = Some(item.id);
                }
            }
        }
    });
    if let Some(index) = remove {
        app.actions.push(Action::RemovePending(index));
    }
    if let Some(target) = open {
        app.actions.push(Action::EditPicture(target));
    }
    ui.add_space(4.0);
}

/// A photo's view-once choice stays beside its caption and Send, with the
/// same dashed-circle mark as on the phone. Its state is exposed to readers
/// and keyboard users as a checkbox, even though it is drawn as an icon.
fn view_once_button(app: &App, ui: &mut egui::Ui) -> egui::Response {
    let palette = app.palette;
    let selected = app.pending_view_once;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(COMPOSER_CONTROL), Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, COMPOSER_CONTROL / 2.0);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            ui.is_enabled(),
            selected,
            crate::i18n::gettext(app.locale, "View once"),
        )
    });
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || response.has_focus();
        let fill = if selected {
            if hovered {
                palette.accent_hover
            } else {
                palette.accent
            }
        } else if hovered {
            palette.surface_hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .circle_filled(rect.center(), COMPOSER_CONTROL / 2.0, fill);
        let color = if selected {
            palette.on_accent
        } else {
            palette.secondary
        };
        theme::paint_icon(ui, Icon::ViewOnce, rect, 24.0, color);
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(crate::i18n::gettext(
            app.locale,
            "The recipient can open this photo only once, on their phone.",
        ))
}
