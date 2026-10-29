//! Chat bubbles (see `app::bubbles`): a borderless, see-through window that
//! floats above every other one and holds a column of round chat pictures.
//! Clicking one opens its chat in a small card beside the column, growing out
//! of the bubble with the theme's motion, and clicking it again (or Escape)
//! folds the card back into it. Dragging a bubble moves the whole column
//! about the screen; the card opens toward the middle of the screen.
//!
//! The window is only as large as what it shows: the column alone while
//! every bubble is closed, the column and the card while one is open, so the
//! see-through part never sits over something the reader wants to click.
//! Where the platform cannot keep a window on top or place it (Wayland), the
//! window still works, wherever the compositor puts it.

use egui::{Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

use super::conversation;
use crate::app::{App, Pending};
use crate::model::{Action, Chat, ChatId};
use crate::theme::{self, Icon};

/// A bubble's diameter.
const BUBBLE: f32 = 56.0;
/// Between two bubbles of the column.
const SPACING: f32 = 12.0;
/// Around the column and the card: room for their shadows and for a bubble
/// growing under the pointer.
const PAD: f32 = 14.0;
/// The open chat's card.
const CARD: Vec2 = vec2(360.0, 520.0);
/// Between the column and the card.
const CARD_GAP: f32 = 10.0;
/// The card's corners, as the chat panel's.
const CARD_RADIUS: f32 = 24.0;
const HEADER: f32 = 56.0;
/// An attachment's thumbnail above the reply field.
const THUMB: f32 = 48.0;
/// How long after the window was moved or resized its position is read back
/// as where the column is: the platform reports the new frame a little late.
const SETTLE: f64 = 0.4;

fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("zeus-bubbles")
}

fn placement_id() -> egui::Id {
    egui::Id::new("bubbles-placement")
}

fn card_id() -> egui::Id {
    egui::Id::new("bubbles-card")
}

/// Where the window is, kept across frames.
#[derive(Clone, Copy, Debug)]
struct Placement {
    /// Where the window was first put. It never changes: the window's own
    /// description keeps it, and a change there would move the window.
    spawn: Option<Pos2>,
    /// The screen position of the column's window while every bubble is
    /// closed, when the platform reports one.
    column: Option<Pos2>,
    /// Whether the card opens to the column's left.
    card_left: bool,
    /// Whether the window is laid out with room for the card.
    open: bool,
    /// How many bubbles it was laid out for.
    count: usize,
    /// When it was last moved or resized by us.
    at: f64,
}

/// The window's size with `count` bubbles, closed or with the card open.
fn window_size(count: usize, open: bool) -> Vec2 {
    let count = count.max(1) as f32;
    let column = count * BUBBLE + (count - 1.0) * SPACING;
    if open {
        vec2(
            CARD.x + CARD_GAP + BUBBLE + 2.0 * PAD,
            column.max(CARD.y) + 2.0 * PAD,
        )
    } else {
        vec2(BUBBLE + 2.0 * PAD, column + 2.0 * PAD)
    }
}

/// How far the column sits from the window's left edge.
fn column_offset(placement: &Placement) -> f32 {
    if placement.open && placement.card_left {
        CARD.x + CARD_GAP
    } else {
        0.0
    }
}

/// Draws the bubbles, if there are any, in their window. Called with the
/// main window's frame, so they redraw with it.
pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.settings.chat_bubbles || app.bubbles.is_empty() {
        ctx.data_mut(|data| data.remove::<Placement>(placement_id()));
        return;
    }
    let count = app.bubbles.list.len();
    let first = ctx.data(|data| data.get_temp::<Placement>(placement_id()));
    let placement = first.unwrap_or_else(|| {
        // First shown near the top right of the main window, over it.
        let main = ctx.input(|input| input.viewport().outer_rect);
        let spawn = main.map(|main| {
            pos2(
                main.right() - window_size(count, false).x - 24.0,
                main.top() + 96.0,
            )
        });
        Placement {
            spawn,
            column: spawn,
            card_left: true,
            open: false,
            count,
            at: ctx.input(|input| input.time),
        }
    });
    ctx.data_mut(|data| data.insert_temp(placement_id(), placement));
    let size = window_size(placement.count, placement.open);
    let mut builder = egui::ViewportBuilder::default()
        .with_title(crate::i18n::gettext(app.locale, "Zeus bubbles"))
        .with_inner_size(size)
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_has_shadow(false)
        .with_always_on_top();
    if let Some(spawn) = placement.spawn {
        builder = builder.with_position(spawn);
    }
    let before = app.actions.len();
    ctx.show_viewport_immediate(viewport_id(), builder, |ui, class| {
        if class == egui::ViewportClass::Immediate
            && ui.ctx().input(|input| input.viewport().close_requested())
        {
            // Closing the window lets every bubble go.
            for bubble in &app.bubbles.list {
                app.actions.push(Action::DismissBubble(bubble.chat.clone()));
            }
            return;
        }
        contents(app, ui);
    });
    // Only what names its own chat or message leaves the window.
    let mut index = 0;
    app.actions.retain(|action| {
        let keep = index < before || App::bubble_allows(action);
        index += 1;
        keep
    });
}

fn contents(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|input| input.time);
    let mut placement = ctx
        .data(|data| data.get_temp::<Placement>(placement_id()))
        .expect("placed before the window is shown");
    // The card plays out after its bubble closes: the chat it showed stays
    // until it has gone.
    let expanded = app.bubbles.expanded.clone();
    if let Some(chat) = &expanded {
        ctx.data_mut(|data| data.insert_temp(card_id().with("chat"), chat.clone()));
    }
    let reveal = theme::animate(
        &ctx,
        card_id(),
        expanded.is_some(),
        if expanded.is_some() {
            theme::MOTION_PANEL
        } else {
            theme::MOTION_PANEL * theme::CLOSE
        },
    );
    let shown: Option<ChatId> = if reveal > 0.0 {
        expanded
            .clone()
            .or_else(|| ctx.data(|data| data.get_temp::<ChatId>(card_id().with("chat"))))
            .filter(|chat| app.bubbles.get(chat).is_some())
    } else {
        None
    };
    lay_out_window(
        &ctx,
        &mut placement,
        shown.is_some(),
        app.bubbles.list.len(),
        now,
    );
    ctx.data_mut(|data| data.insert_temp(placement_id(), placement));
    if expanded.is_some() && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.actions.push(Action::CollapseBubbles);
    }
    let area = ui.max_rect();
    let column_x = area.left() + PAD + column_offset(&placement);
    let mut pivot = None;
    let chats: Vec<ChatId> = app
        .bubbles
        .list
        .iter()
        .map(|bubble| bubble.chat.clone())
        .collect();
    for (index, chat) in chats.iter().enumerate() {
        let centre = pos2(
            column_x + BUBBLE / 2.0,
            area.top() + PAD + BUBBLE / 2.0 + index as f32 * (BUBBLE + SPACING),
        );
        if shown.as_deref() == Some(chat.as_str()) {
            pivot = Some(centre);
        }
        bubble(
            app,
            ui,
            chat,
            centre,
            expanded.as_deref() == Some(chat.as_str()),
        );
    }
    if let (Some(chat), Some(pivot)) = (shown, pivot)
        && let Some(known) = app.chat(&chat).cloned()
    {
        let left = if placement.card_left {
            area.left() + PAD
        } else {
            column_x + BUBBLE + CARD_GAP
        };
        let card = Rect::from_min_size(pos2(left, area.top() + PAD), CARD);
        card_view(app, ui, &known, card, pivot, reveal);
        // Read while it is open in front of the reader.
        if reveal >= 1.0 && known.unread > 0 && ctx.input(|input| input.focused) {
            app.actions.push(Action::MarkRead(chat));
        }
    }
}

/// Resizes and moves the window when the card opens or closes, or bubbles
/// come and go, and otherwise follows where the reader has dragged it.
fn lay_out_window(
    ctx: &egui::Context,
    placement: &mut Placement,
    open: bool,
    count: usize,
    now: f64,
) {
    let outer = ctx.input(|input| input.viewport().outer_rect);
    if open == placement.open && count == placement.count {
        // Settled: where the window is now is where the reader left it.
        if now - placement.at > SETTLE
            && let Some(outer) = outer
        {
            placement.column = Some(outer.min + vec2(column_offset(placement), 0.0));
        }
        return;
    }
    if open && !placement.open {
        // The card opens toward the middle of the screen.
        let monitor = ctx.input(|input| input.viewport().monitor_size);
        placement.card_left = match (placement.column, monitor) {
            (Some(column), Some(monitor)) => column.x + BUBBLE / 2.0 > monitor.x / 2.0,
            _ => true,
        };
    }
    placement.open = open;
    placement.count = count;
    placement.at = now;
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(window_size(count, open)));
    if let Some(column) = placement.column {
        let to = column - vec2(column_offset(placement), 0.0);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(to));
    }
}

/// One bubble: the chat's picture on a shadow, growing a little under the
/// pointer, ringed in the accent while its card is open, with the unread
/// count on it and a close button that shows on hover.
fn bubble(app: &mut App, ui: &mut egui::Ui, chat: &str, centre: Pos2, open: bool) {
    let ctx = ui.ctx().clone();
    let palette = app.palette;
    let id = egui::Id::new(("bubble", chat));
    // A new bubble grows in from nothing.
    let seen = id.with("seen");
    if ctx.data(|data| data.get_temp::<bool>(seen).is_none()) {
        ctx.data_mut(|data| data.insert_temp(seen, true));
        theme::animate(&ctx, id.with("in"), false, theme::MOTION_PANEL);
    }
    let arrive = theme::animate(&ctx, id.with("in"), true, theme::MOTION_PANEL);
    let rect = Rect::from_center_size(centre, Vec2::splat(BUBBLE));
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let hover = theme::animate(
        &ctx,
        id.with("hover"),
        response.hovered(),
        theme::MOTION_QUICK,
    );
    let ring = theme::animate(&ctx, id.with("ring"), open, theme::MOTION_QUICK);
    let Some(known) = app.chat(chat).cloned() else {
        return;
    };
    let title = app.chat_title(&known);
    if response.drag_started() {
        // The platform moves the window with the pointer from here.
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    } else if response.clicked() {
        app.actions.push(Action::ToggleBubble(chat.to_owned()));
    }
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(&title);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, open, &title));
    let scale = egui::lerp(0.6..=1.0, theme::ease_out(arrive)) * (1.0 + 0.06 * hover);
    let shown = Rect::from_center_size(centre, Vec2::splat(BUBBLE * scale));
    let mut layer = ui.new_child(egui::UiBuilder::new().max_rect(shown));
    layer.set_opacity(arrive);
    let painter = layer.painter().clone();
    painter.add(palette.float_shadow().as_shape(shown, shown.width() / 2.0));
    let picture = app.avatar(chat);
    super::widgets::paint_avatar(&layer, &palette, shown, &title, chat, picture.as_deref());
    if ring > 0.0 {
        painter.circle_stroke(
            shown.center(),
            shown.width() / 2.0 + 2.5,
            Stroke::new(2.5, palette.accent.gamma_multiply(ring)),
        );
    }
    if known.unread > 0 {
        let label = if known.unread > 99 {
            "99+".to_owned()
        } else {
            known.unread.to_string()
        };
        let galley = painter.layout_no_wrap(label, theme::bold(11.0), palette.on_accent);
        let size = vec2((galley.size().x + 10.0).max(20.0), 20.0);
        let badge = Rect::from_center_size(pos2(shown.right() - 6.0, shown.top() + 6.0), size);
        painter.rect(
            badge,
            10.0,
            palette.accent,
            Stroke::new(2.0, palette.window.to_opaque()),
            egui::StrokeKind::Outside,
        );
        painter.galley(
            badge.center() - galley.size() / 2.0,
            galley,
            palette.on_accent,
        );
    }
    // The close button, on the bubble's other top corner.
    let close = Rect::from_center_size(
        pos2(shown.left() + 7.0, shown.top() + 7.0),
        Vec2::splat(20.0),
    );
    let pressed = ui.interact(close, id.with("close"), Sense::click());
    let near = theme::animate(
        &ctx,
        id.with("close-near"),
        response.hovered() || pressed.hovered(),
        theme::MOTION_QUICK,
    );
    if near > 0.0 {
        painter.circle(
            close.center(),
            10.0,
            palette.overlay.to_opaque().gamma_multiply(near),
            Stroke::new(1.0, palette.outline.gamma_multiply(near)),
        );
        theme::paint_icon(
            &layer,
            Icon::X,
            close.shrink(5.0),
            10.0,
            palette.text.gamma_multiply(near),
        );
        let label = crate::i18n::gettext(app.locale, "Remove bubble");
        if pressed.on_hover_text(label.as_ref()).clicked() {
            app.actions.push(Action::DismissBubble(chat.to_owned()));
        }
    }
}

/// The open bubble's chat: a rounded card with the chat's name and buttons,
/// its transcript, and a reply field with what is attached. It grows out of
/// its bubble at `pivot` as `reveal` goes from 0 to 1 and folds back into it.
fn card_view(app: &mut App, ui: &mut egui::Ui, chat: &Chat, card: Rect, pivot: Pos2, reveal: f32) {
    let palette = app.palette;
    let eased = theme::ease_out(reveal);
    let scale = egui::lerp(0.88..=1.0, eased);
    let transform = egui::emath::TSTransform::from_translation(pivot.to_vec2())
        * egui::emath::TSTransform::from_scaling(scale)
        * egui::emath::TSTransform::from_translation(-pivot.to_vec2());
    let mut scope = ui.new_child(egui::UiBuilder::new().max_rect(card));
    scope.set_opacity(reveal);
    if reveal < 1.0 {
        // Nothing in a card on its way in or out takes a click.
        scope.disable();
    }
    scope.with_visual_transform(transform, |ui| {
        let painter = ui.painter().clone();
        painter.add(palette.float_shadow().as_shape(card, CARD_RADIUS));
        painter.rect(
            card,
            CARD_RADIUS,
            palette.window.to_opaque(),
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        let header = Rect::from_min_size(card.min, vec2(card.width(), HEADER));
        let reply = composer_height(app, chat);
        let footer = Rect::from_min_max(pos2(card.left(), card.bottom() - reply), card.max);
        let round = CARD_RADIUS as u8;
        painter.rect_filled(
            header,
            egui::CornerRadius {
                nw: round,
                ne: round,
                sw: 0,
                se: 0,
            },
            palette.panel.to_opaque(),
        );
        painter.rect_filled(
            footer,
            egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: round,
                se: round,
            },
            palette.panel.to_opaque(),
        );
        card_header(app, ui, chat, header);
        let middle = Rect::from_min_max(
            pos2(card.left(), header.bottom()),
            pos2(card.right(), footer.top()),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(middle), |ui| {
            ui.set_clip_rect(middle.intersect(ui.clip_rect()));
            conversation::bubble_transcript(app, ui, chat);
        });
        composer(app, ui, chat, footer, reveal >= 1.0);
        painter.rect_stroke(
            card,
            CARD_RADIUS,
            Stroke::new(1.25, palette.outline),
            egui::StrokeKind::Inside,
        );
    });
}

fn card_header(app: &mut App, ui: &mut egui::Ui, chat: &Chat, rect: Rect) {
    let palette = app.palette;
    let title = app.chat_title(chat);
    let inner = rect.shrink2(vec2(14.0, 10.0));
    let avatar = Rect::from_min_size(
        pos2(inner.left(), inner.center().y - 18.0),
        Vec2::splat(36.0),
    );
    let picture = app.avatar(&chat.id);
    super::widgets::paint_avatar(ui, &palette, avatar, &title, &chat.id, picture.as_deref());
    let mut buttons = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(avatar.right() + 10.0, inner.top()),
                inner.max,
            ))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    buttons.spacing_mut().item_spacing.x = 2.0;
    let locale = app.locale;
    let tr = |text: &'static str| crate::i18n::gettext(locale, text).into_owned();
    if theme::icon_button(
        &mut buttons,
        Icon::X,
        17.0,
        palette.secondary,
        palette.text,
        &tr("Remove bubble"),
    )
    .clicked()
    {
        app.actions.push(Action::DismissBubble(chat.id.clone()));
    }
    if theme::icon_button(
        &mut buttons,
        Icon::Maximize,
        17.0,
        palette.secondary,
        palette.text,
        &tr("Open in Zeus"),
    )
    .clicked()
    {
        app.actions.push(Action::DockBubble(chat.id.clone()));
    }
    if theme::icon_button(
        &mut buttons,
        Icon::Minimize,
        17.0,
        palette.secondary,
        palette.text,
        &tr("Close bubble"),
    )
    .clicked()
    {
        app.actions.push(Action::CollapseBubbles);
    }
    let room = (buttons.min_rect().left() - avatar.right() - 18.0).max(40.0);
    let name = super::widgets::line(ui, &title, theme::semibold(15.0), palette.text, room, 1);
    name.paint(
        ui,
        pos2(avatar.right() + 10.0, rect.center().y - name.size().y / 2.0),
        palette.text,
    );
}

/// How tall the reply area is: the field, and the thumbnails above it when
/// something is attached.
fn composer_height(app: &App, chat: &Chat) -> f32 {
    if !chat.can_send() {
        return 44.0;
    }
    let staged = app
        .bubbles
        .get(&chat.id)
        .is_some_and(|bubble| !bubble.staged.is_empty());
    64.0 + if staged { THUMB + 12.0 } else { 0.0 }
}

/// The reply area: what is attached, then the attach button, the field and
/// the send button. A paste of a picture or of files, or files dropped on
/// the window, are attached; Enter sends.
fn composer(app: &mut App, ui: &mut egui::Ui, chat: &Chat, rect: Rect, settled: bool) {
    let palette = app.palette;
    let locale = app.locale;
    if !chat.can_send() {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            crate::i18n::gettext(locale, "You can't send messages here"),
            theme::regular(13.0),
            palette.secondary,
        );
        return;
    }
    app.take_bubble_input(&ui.ctx().clone(), &chat.id);
    let inner = rect.shrink2(vec2(12.0, 12.0));
    let mut top = inner.top();
    let staged_count = app
        .bubbles
        .get(&chat.id)
        .map_or(0, |bubble| bubble.staged.len());
    if staged_count > 0 {
        let strip = Rect::from_min_size(pos2(inner.left(), top), vec2(inner.width(), THUMB));
        thumbnails(app, ui, chat, strip);
        top += THUMB + 12.0;
    }
    let row = Rect::from_min_max(pos2(inner.left(), top), pos2(inner.right(), top + 40.0));
    let attach = Rect::from_min_size(row.min, Vec2::splat(40.0));
    let send = Rect::from_min_size(pos2(row.right() - 40.0, row.top()), Vec2::splat(40.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(attach));
    if theme::icon_button(
        &mut child,
        Icon::Paperclip,
        19.0,
        palette.secondary,
        palette.text,
        &crate::i18n::gettext(locale, "Attach"),
    )
    .clicked()
    {
        app.actions.push(Action::BubbleAttach(chat.id.clone()));
    }
    let field = Rect::from_min_max(
        pos2(attach.right() + 6.0, row.top()),
        pos2(send.left() - 6.0, row.bottom()),
    );
    ui.painter().rect(
        field,
        20.0,
        palette.surface,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
    let hint = crate::i18n::gettext(locale, "Type a message").into_owned();
    let mut enter = false;
    let mut ready = staged_count > 0;
    if let Some(bubble) = app.bubbles.get_mut(&chat.id) {
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(field.shrink2(vec2(14.0, 0.0))));
        child.set_clip_rect(field.intersect(child.clip_rect()));
        let response = child
            .centered_and_justified(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut bubble.text)
                        .id(egui::Id::new(("bubble-text", chat.id.as_str())))
                        .hint_text(hint)
                        .frame(egui::Frame::NONE)
                        .font(theme::regular(14.5))
                        .desired_width(f32::INFINITY),
                )
            })
            .inner;
        // Once the card has settled: a field on its way in takes nothing.
        if settled && std::mem::take(&mut bubble.focus) {
            response.request_focus();
        }
        enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if enter {
            response.request_focus();
        }
        ready |= !bubble.text.trim().is_empty();
    }
    let lit = theme::animate(
        ui.ctx(),
        egui::Id::new(("bubble-send", chat.id.as_str())),
        ready,
        theme::MOTION_QUICK,
    );
    let pressed = ui.interact(
        send,
        egui::Id::new(("bubble-send-button", chat.id.as_str())),
        Sense::click(),
    );
    let fill = palette
        .surface
        .lerp_to_gamma(palette.accent, lit)
        .lerp_to_gamma(
            palette.accent_hover,
            if pressed.hovered() { lit } else { 0.0 },
        );
    ui.painter().circle_filled(send.center(), 20.0, fill);
    theme::paint_icon(
        ui,
        theme::send_icon(),
        Rect::from_center_size(send.center(), Vec2::splat(18.0)),
        18.0,
        palette.secondary.lerp_to_gamma(palette.on_accent, lit),
    );
    let pressed = pressed
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(crate::i18n::gettext(locale, "Send").as_ref());
    if (enter || pressed.clicked()) && ready {
        app.actions.push(Action::BubbleSend(chat.id.clone()));
    }
}

/// What is attached, as thumbnails with a remove button each.
fn thumbnails(app: &mut App, ui: &mut egui::Ui, chat: &Chat, strip: Rect) {
    let palette = app.palette;
    let ctx = ui.ctx().clone();
    let Some(bubble) = app.bubbles.get_mut(&chat.id) else {
        return;
    };
    let mut remove = None;
    for (index, item) in bubble.staged.iter_mut().enumerate() {
        let left = strip.left() + index as f32 * (THUMB + 8.0);
        if left + THUMB > strip.right() {
            break;
        }
        let rect = Rect::from_min_size(pos2(left, strip.top()), Vec2::splat(THUMB));
        let radius = 10.0;
        ui.painter().rect_filled(rect, radius, palette.surface);
        match item {
            Pending::Picture {
                width,
                height,
                rgba,
                texture,
            } => {
                let texture = texture.get_or_insert_with(|| {
                    ctx.load_texture(
                        format!("bubble-staged-{}-{index}", chat.id),
                        egui::ColorImage::from_rgba_unmultiplied([*width, *height], rgba),
                        egui::TextureOptions::LINEAR,
                    )
                });
                egui::Image::new(&*texture)
                    .corner_radius(radius)
                    .maintain_aspect_ratio(false)
                    .paint_at(ui, rect);
            }
            Pending::File(path) | Pending::Document(path) => {
                if crate::model::UploadKind::of(path, false) == crate::model::UploadKind::Image {
                    let uri = crate::image_cache::fit_uri(&ctx, path, Vec2::splat(THUMB));
                    egui::Image::new(uri)
                        .corner_radius(radius)
                        .paint_at(ui, rect);
                } else {
                    theme::paint_icon(
                        ui,
                        Icon::Paperclip,
                        Rect::from_center_size(rect.center(), Vec2::splat(20.0)),
                        20.0,
                        palette.secondary,
                    );
                }
            }
        }
        let close = Rect::from_center_size(rect.right_top() + vec2(-4.0, 4.0), Vec2::splat(18.0));
        let response = ui.interact(
            close,
            egui::Id::new(("bubble-unstage", chat.id.as_str(), index)),
            Sense::click(),
        );
        ui.painter().circle(
            close.center(),
            9.0,
            palette.overlay.to_opaque(),
            Stroke::new(1.0, palette.outline),
        );
        theme::paint_icon(ui, Icon::X, close.shrink(4.5), 9.0, palette.text);
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        app.actions.push(Action::BubbleUnstage {
            chat: chat.id.clone(),
            index,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_fits_the_column_or_the_card() {
        let one = window_size(1, false);
        assert_eq!(one, vec2(BUBBLE + 2.0 * PAD, BUBBLE + 2.0 * PAD));
        let three = window_size(3, false);
        assert_eq!(three.y, 3.0 * BUBBLE + 2.0 * SPACING + 2.0 * PAD);
        let open = window_size(3, true);
        assert!(open.x > CARD.x && open.y >= CARD.y + 2.0 * PAD);
    }

    #[test]
    fn the_column_stays_put_when_the_card_opens_to_its_left() {
        let mut placement = Placement {
            spawn: None,
            column: Some(pos2(1500.0, 200.0)),
            card_left: true,
            open: false,
            count: 2,
            at: 0.0,
        };
        assert_eq!(column_offset(&placement), 0.0);
        placement.open = true;
        assert_eq!(column_offset(&placement), CARD.x + CARD_GAP);
        placement.card_left = false;
        assert_eq!(column_offset(&placement), 0.0);
    }
}
