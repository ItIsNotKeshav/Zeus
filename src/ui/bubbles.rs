//! Chat bubbles (see `app::bubbles`): a borderless, see-through window that
//! floats above every other one and holds a column of round chat pictures.
//! Clicking one opens its chat in a small card beside the column, growing out
//! of the bubble with the theme's motion, and clicking it again (or Escape)
//! folds the card back into it. Dragging a bubble, or the open card by its
//! header, moves the whole column about the screen; the card opens toward
//! the middle of the screen.
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
/// How long a bubble's glow breathes after a message arrives, before it
/// settles to a steady light: three breaths. The window is not redrawn for
/// it after that.
const GLOW_BREATHING: f64 = 3.6;
/// One breath of the glow, in seconds.
const GLOW_BREATH: f64 = 1.2;
/// How far the glow reaches past the bubble: inside [`PAD`].
const GLOW_REACH: f32 = 9.0;
const _: () = assert!(GLOW_REACH < PAD);
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
    // See-through only where the main window is: its GL config then has an
    // alpha channel. Elsewhere the window is a solid tile in the theme's
    // colour, so no platform shows black where the circles are not.
    let see_through = crate::backdrop::window_is_transparent();
    let title = crate::i18n::gettext(app.locale, "Zeus bubbles");
    let mut builder = egui::ViewportBuilder::default()
        .with_title(title.as_ref())
        .with_inner_size(size)
        .with_decorations(false)
        .with_transparent(see_through)
        .with_resizable(false)
        .with_taskbar(false)
        .with_has_shadow(!see_through)
        .with_always_on_top();
    if let Some(spawn) = placement.spawn {
        builder = builder.with_position(spawn);
    }
    // On the desktop it was put on, not following into other Spaces or
    // over an app in full screen.
    crate::backdrop::keep_on_its_desktop(title.as_ref());
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
        let region = contents(app, ui);
        // The picker and menus leave this window the way they came, too.
        super::closing::paint(ui.ctx());
        if see_through {
            // Windows 11 frames even a borderless window with a hairline and
            // rounds its corners, and composes the parts it paints nothing
            // on as a faint sheet: the window is cut down to what it shows.
            crate::backdrop::strip_window_frame(title.as_ref());
            crate::backdrop::shape_window(title.as_ref(), &region, ui.ctx().pixels_per_point());
        }
    });
    // Only what names its own chat or message leaves the window, and what
    // the picker over it asked for.
    let mut index = 0;
    app.actions.retain(|action| {
        let keep = index < before || App::bubble_allows(action);
        index += 1;
        keep
    });
    app.actions.append(&mut app.bubbles.picker_actions);
    // A picture clicked in the card is too small to see there: it opens in
    // the main window's viewer, large, with the album to step through. The
    // card folds back into its bubble so it does not float over the viewer,
    // and the main window comes forward, unminimised.
    let previewing = app.actions[before.min(app.actions.len())..]
        .iter()
        .any(|action| {
            matches!(
                action,
                Action::PreviewImage { .. } | Action::PreviewSticker(_)
            )
        });
    if previewing {
        app.actions.push(Action::CollapseBubbles);
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::Minimized(false),
        );
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }
}

/// Draws the window's bubbles and card, and returns what it painted: the
/// shapes (rects and their corner radius) the window is cut down to.
fn contents(app: &mut App, ui: &mut egui::Ui) -> Vec<(Rect, f32)> {
    let ctx = ui.ctx().clone();
    let mut region = Vec::new();
    if !crate::backdrop::window_is_transparent() {
        let palette = app.palette;
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, palette.window.to_opaque());
    }
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
    // A bubble that went (closed, or let go for a fourth) leaves nothing
    // behind: one that comes back, as a chat's new message brings it, grows
    // in again from nothing.
    let listed_id = egui::Id::new("bubble-chats");
    let listed = ctx
        .data(|data| data.get_temp::<Vec<ChatId>>(listed_id))
        .unwrap_or_default();
    for gone in listed.iter().filter(|chat| !chats.contains(chat)) {
        let id = egui::Id::new(("bubble", gone.as_str()));
        ctx.data_mut(|data| data.remove::<bool>(id.with("seen")));
        crate::motion::forget(&ctx, id.with("in"));
        crate::motion::forget(&ctx, id.with("y"));
    }
    if listed != chats {
        ctx.data_mut(|data| data.insert_temp(listed_id, chats.clone()));
    }
    let slide = crate::motion::Spring::settling_in(f64::from(theme::MOTION_PANEL));
    for (index, chat) in chats.iter().enumerate() {
        // The column closes up, or makes room, on the theme's spring when a
        // bubble comes or goes; a new one takes its place at once.
        let y = crate::motion::spring_to(
            &ctx,
            egui::Id::new(("bubble", chat.as_str())).with("y"),
            index as f32 * (BUBBLE + SPACING),
            slide,
        );
        let centre = pos2(column_x + BUBBLE / 2.0, area.top() + PAD + BUBBLE / 2.0 + y);
        if shown.as_deref() == Some(chat.as_str()) {
            pivot = Some(centre);
        }
        bubble(
            app,
            ui,
            chat,
            centre,
            expanded.as_deref() == Some(chat.as_str()),
            &mut region,
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
        region.push(card_view(app, ui, &known, card, pivot, reveal));
        picker(
            app,
            &ctx,
            &chat,
            card,
            expanded.as_deref() == Some(chat.as_str()),
        );
        // Read while it is open in front of the reader.
        if reveal >= 1.0 && known.unread > 0 && ctx.input(|input| input.focused) {
            app.actions.push(Action::MarkRead(chat));
        }
    }
    if app.bubbles.picker.is_some() && (app.picker.is_none() || app.bubbles.picker != expanded) {
        // Closed, or its bubble folded away: it goes with it.
        app.picker = None;
        app.bubbles.picker = None;
    }
    // Messages carried out of the card, and the card carrying them.
    super::carry::begin(&ctx);
    super::carry::show(app, &ctx);
    region.extend(super::carry::painted(&ctx).map(|rect| (rect.expand(16.0), 18.0)));
    // Tooltips, menus and anything else floating over the window.
    let floating: Vec<Rect> = ctx.memory(|memory| {
        memory
            .areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|layer| {
                matches!(
                    layer.order,
                    egui::Order::Middle | egui::Order::Foreground | egui::Order::Tooltip
                )
            })
            .filter_map(|layer| memory.area_rect(layer.id))
            .collect()
    });
    region.extend(floating.into_iter().map(|rect| (rect.expand(2.0), 12.0)));
    region
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

/// How strong a bubble's glow is, 0 to 1, `since` seconds after its unread
/// count last rose: it breathes between dim and full for
/// [`GLOW_BREATHING`], then rests at a steady light. `still` (a screen
/// reader is attached) skips the breathing.
fn glow_strength(since: f64, still: bool) -> f32 {
    const REST: f32 = 0.6;
    if still || since >= GLOW_BREATHING {
        return REST;
    }
    // Starts and ends at the resting light, so settling shows no step.
    let wave = (since / GLOW_BREATH * std::f64::consts::TAU).sin() as f32;
    (REST + 0.4 * wave).clamp(0.2, 1.0)
}

/// The glow of a bubble whose chat has unread messages, in the accent,
/// around `shown`: it breathes when a message arrives and then stays lit
/// until the chat is read. Returns nothing to draw once `unread` is 0.
fn paint_glow(
    ctx: &egui::Context,
    painter: &egui::Painter,
    id: egui::Id,
    shown: Rect,
    unread: u32,
    accent: egui::Color32,
) {
    let now = ctx.input(|input| input.time);
    let key = id.with("glow");
    // The count it last saw, and when that count last rose.
    let (seen, since) = ctx
        .data(|data| data.get_temp::<(u32, f64)>(key))
        .unwrap_or((0, now));
    let since = if unread > seen { now } else { since };
    ctx.data_mut(|data| data.insert_temp(key, (unread, since)));
    let lit = theme::animate(ctx, key.with("lit"), unread > 0, theme::MOTION_PANEL);
    if lit <= 0.0 {
        return;
    }
    let still = crate::motion::screen_reader_attached(ctx);
    let elapsed = now - since;
    if unread > 0 && !still && elapsed < GLOW_BREATHING {
        ctx.request_repaint();
    }
    let strength = glow_strength(elapsed, still) * lit;
    // Soft rings from the bubble's edge outward, each fainter than the last.
    const RINGS: usize = 6;
    let radius = shown.width() / 2.0;
    for ring in 0..RINGS {
        let t = (ring as f32 + 0.5) / RINGS as f32;
        let alpha = strength * (1.0 - t).powi(2) * 0.55;
        painter.circle_stroke(
            shown.center(),
            radius + t * GLOW_REACH,
            Stroke::new(
                GLOW_REACH / RINGS as f32 + 0.5,
                accent.gamma_multiply(alpha),
            ),
        );
    }
}

/// One bubble: the chat's picture on a shadow, growing a little under the
/// pointer, ringed in the accent while its card is open, with the unread
/// count on it and a close button that shows on hover. It glows in the
/// accent while its chat has unread messages.
fn bubble(
    app: &mut App,
    ui: &mut egui::Ui,
    chat: &str,
    centre: Pos2,
    open: bool,
    region: &mut Vec<(Rect, f32)>,
) {
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
    // Messages carried over it from another window light it up too.
    let landing = super::carry::over_chat(&ctx).as_deref() == Some(chat);
    let ring = theme::animate(&ctx, id.with("ring"), open || landing, theme::MOTION_QUICK);
    let Some(known) = app.chat(chat).cloned() else {
        return;
    };
    if super::carry::carrying(&ctx) {
        super::carry::target(ui, rect, &known, &app.chat_title(&known), centre);
    }
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
    // The picture, its ring and its shadow's edge.
    region.push((shown.expand(6.0), shown.width() / 2.0 + 6.0));
    let mut layer = ui.new_child(egui::UiBuilder::new().max_rect(shown));
    layer.set_opacity(arrive);
    let painter = layer.painter().clone();
    painter.add(palette.float_shadow().as_shape(shown, shown.width() / 2.0));
    paint_glow(&ctx, &painter, id, shown, known.unread, palette.accent);
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
        region.push((badge.expand(3.0), 13.0));
        painter.rect(
            badge,
            10.0,
            palette.accent_fill,
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
        region.push((close.expand(2.0), 12.0));
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
fn card_view(
    app: &mut App,
    ui: &mut egui::Ui,
    chat: &Chat,
    card: Rect,
    pivot: Pos2,
    reveal: f32,
) -> (Rect, f32) {
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
        card_header(app, ui, chat, header);
        // The messages run on to the card's foot, under the reply area's
        // glass, and keep room at their end for it. They stop short of the
        // very bottom, where the card's corners curve in, so nothing pokes
        // past them.
        let room = composer_room(app, ui.ctx(), chat);
        let middle = Rect::from_min_max(
            pos2(card.left(), header.bottom()),
            pos2(card.right(), card.bottom() - CARD_RADIUS / 3.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(middle), |ui| {
            ui.set_clip_rect(middle.intersect(ui.clip_rect()));
            conversation::bubble_transcript(
                app,
                ui,
                chat,
                room - (card.bottom() - middle.bottom()),
            );
        });
        composer(app, ui, chat, card, reveal >= 1.0);
        painter.rect_stroke(
            card,
            CARD_RADIUS,
            Stroke::new(1.25, palette.outline),
            egui::StrokeKind::Inside,
        );
        // Messages carried here from the main window or another bubble.
        if super::carry::carrying(ui.ctx()) {
            super::carry::target(ui, card, chat, &app.chat_title(chat), pivot);
        }
    });
    // Where the card shows this frame, grown a point for its soft edge.
    (
        transform.mul_rect(card).expand(1.0),
        CARD_RADIUS * scale + 1.0,
    )
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
    // Only the cross: the bubble itself folds the card away when clicked.
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
    // The rest of the header is the open card's handle, as a window's title
    // bar is: the bubble beside it is a small thing to find and hold. It
    // stops short of the buttons, so a press on one is never a drag.
    let handle = ui.interact(
        Rect::from_min_max(
            rect.min,
            pos2(buttons.min_rect().left() - 4.0, rect.bottom()),
        ),
        egui::Id::new(("bubble-card-handle", chat.id.as_str())),
        Sense::drag(),
    );
    if handle.drag_started() {
        // The platform moves the window with the pointer from here.
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if handle.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    let room = (buttons.min_rect().left() - avatar.right() - 18.0).max(40.0);
    let name = super::widgets::line(ui, &title, theme::semibold(15.0), palette.text, room, 1);
    name.paint(
        ui,
        pos2(avatar.right() + 10.0, rect.center().y - name.size().y / 2.0),
        palette.text,
    );
}

/// Between the card's edges and the reply field, as the chat's composer
/// keeps from its card.
const MARGIN: f32 = conversation::COMPOSER_MARGIN as f32;
/// The reply field: one line with its controls, rounded at the ends.
const FIELD: f32 = 44.0;
/// How many lines the field grows to before its text scrolls.
const FIELD_LINES: f32 = 5.0;
/// Space the send button keeps inside the field's rounded end.
const SEND_CLEARANCE: f32 = 4.0;
/// Inside the attachment tray, round its tiles. With the tray's radius it
/// keeps the tiles' corners parallel to its own.
const TRAY_PAD: f32 = 8.0;
/// Between the tray and the field.
const TRAY_GAP: f32 = 8.0;
/// A picture's tile when several things are attached.
const TILE: f32 = 72.0;
/// Between two tiles.
const TILE_GAP: f32 = 8.0;
/// The tallest a lone picture or clip is previewed.
const PREVIEW_HEIGHT: f32 = 168.0;
/// How far below its place the tray starts as it rises out of the field.
const TRAY_RISE: f32 = 14.0;

/// What one attachment's tile shows.
#[derive(Clone)]
enum Face {
    /// A pasted picture, or a clip's first frame.
    Texture(egui::TextureHandle),
    /// A picture file, read through the image cache.
    Photo(std::path::PathBuf),
    /// A clip whose first frame has not been read yet.
    Clip,
    /// Anything else: a card with its name, type and size.
    File(std::path::PathBuf),
}

/// One attachment as the tray draws it.
#[derive(Clone)]
struct Tile {
    /// Which attachment it is, the same from frame to frame while it stays.
    key: egui::Id,
    face: Face,
    /// The picture's own size, for what has a picture.
    native: Option<Vec2>,
    /// Whether it is a clip, marked with a play button.
    play: bool,
}

impl Tile {
    fn is_file(&self) -> bool {
        matches!(self.face, Face::File(_))
    }
}

/// The tray as it was drawn last frame, kept so a tile taken away can fade
/// where it was and the tray can close with what it showed.
#[derive(Clone, Default)]
struct Tray {
    /// Every tile drawn, with its place relative to the row's top left.
    tiles: Vec<(Tile, Rect)>,
    /// Tiles taken away, with when they went.
    leaving: Vec<(Tile, Rect, f64)>,
    /// How tall the row of tiles was.
    row: f32,
}

fn tray_id(chat: &str) -> egui::Id {
    egui::Id::new(("bubble-tray", chat))
}

/// A pasted picture made small enough for a tile: never larger than the
/// GPU takes, nor than the card can show.
fn tile_image(width: usize, height: usize, rgba: &[u8]) -> egui::ColorImage {
    const MOST: usize = 1024;
    if width.max(height) <= MOST {
        return egui::ColorImage::from_rgba_unmultiplied([width, height], rgba);
    }
    let scale = MOST as f32 / width.max(height) as f32;
    let (small_width, small_height) = (
        ((width as f32 * scale) as u32).max(1),
        ((height as f32 * scale) as u32).max(1),
    );
    // Borrowed: a copy of a 4K bitmap is 33 MB.
    match image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(width as u32, height as u32, rgba)
    {
        Some(full) => {
            let small = image::imageops::resize(
                &full,
                small_width,
                small_height,
                image::imageops::FilterType::Triangle,
            );
            egui::ColorImage::from_rgba_unmultiplied(
                [small_width as usize, small_height as usize],
                &small,
            )
        }
        None => egui::ColorImage::example(),
    }
}

/// What `chat`'s bubble has attached, as tiles, in order.
fn staged_tiles(app: &mut App, ctx: &egui::Context, chat: &str) -> Vec<Tile> {
    let Some(bubble) = app.bubbles.get_mut(chat) else {
        return Vec::new();
    };
    let mut seen: Vec<egui::Id> = Vec::new();
    bubble
        .staged
        .iter_mut()
        .map(|item| {
            let (base, face, native, play) = match item {
                Pending::Picture {
                    width,
                    height,
                    rgba,
                    texture,
                } => {
                    let base = egui::Id::new(("picture", std::sync::Arc::as_ptr(rgba) as usize));
                    let handle = texture
                        .get_or_insert_with(|| {
                            ctx.load_texture(
                                format!("bubble-staged-{base:?}"),
                                tile_image(*width, *height, rgba.as_slice()),
                                egui::TextureOptions::LINEAR,
                            )
                        })
                        .clone();
                    let native = vec2(*width as f32, *height as f32);
                    (base, Face::Texture(handle), Some(native), false)
                }
                // A file sent as a document shows as the file it is, even a
                // picture.
                Pending::Document(path) => (
                    egui::Id::new(("document", path.as_path())),
                    Face::File(path.clone()),
                    None,
                    false,
                ),
                Pending::File(path) => {
                    let base = egui::Id::new(("file", path.as_path()));
                    if Pending::is_picture_file(path) {
                        let native = conversation::pending_picture_size(ctx, path);
                        (base, Face::Photo(path.clone()), native, false)
                    } else if crate::image_preview::is_video(path) {
                        match conversation::pending_poster(ctx, path) {
                            Some(poster) => {
                                let native = poster.size_vec2();
                                (base, Face::Texture(poster), Some(native), true)
                            }
                            // Until its frame arrives the clip holds a
                            // landscape slot.
                            None => (base, Face::Clip, Some(vec2(16.0, 9.0)), true),
                        }
                    } else {
                        (base, Face::File(path.clone()), None, false)
                    }
                }
            };
            // The same file attached twice is two tiles.
            let nth = seen.iter().filter(|known| **known == base).count();
            seen.push(base);
            Tile {
                key: base.with(nth),
                face,
                native,
                play,
            }
        })
        .collect()
}

/// Where each tile lies in a row `room` wide, from the row's top left, and
/// how tall the row is. One picture or clip alone is previewed large, in its
/// own shape; one file alone is a card across the row; several share it.
fn lay_out_tiles(tiles: &[Tile], room: f32) -> (Vec<Rect>, f32) {
    let sizes: Vec<Vec2> = match tiles {
        [lone] if !lone.is_file() => {
            let native = lone
                .native
                .unwrap_or(Vec2::splat(1.0))
                .max(Vec2::splat(1.0));
            let scale = (room / native.x).min(PREVIEW_HEIGHT / native.y);
            // Never thinner than a tile: a panorama shows whole on it.
            let fitted = native * scale;
            let size = vec2(
                fitted.x.clamp(TILE.min(room), room),
                fitted.y.clamp(TILE, PREVIEW_HEIGHT),
            );
            vec![size.round()]
        }
        [_] => vec![vec2(room, conversation::FILE_CARD_HEIGHT)],
        _ => tiles
            .iter()
            .map(|tile| {
                if tile.is_file() {
                    vec2(conversation::FILE_CARD_WIDTH.min(room), TILE)
                } else {
                    Vec2::splat(TILE)
                }
            })
            .collect(),
    };
    let row = sizes.iter().map(|size| size.y).fold(0.0, f32::max);
    let mut left = 0.0;
    let places = sizes
        .into_iter()
        .map(|size| {
            let place = Rect::from_min_size(pos2(left, (row - size.y) / 2.0), size);
            left += size.x + TILE_GAP;
            place
        })
        .collect();
    (places, row)
}

/// The part of a picture `size` large that covers a tile shaped like
/// `frame`, from its middle.
fn cover_uv(size: Vec2, frame: Vec2) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 || frame.x <= 0.0 || frame.y <= 0.0 {
        return Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    }
    let (picture, wanted) = (size.x / size.y, frame.x / frame.y);
    let (across, down) = if picture > wanted {
        (wanted / picture, 1.0)
    } else {
        (1.0, picture / wanted)
    };
    Rect::from_center_size(pos2(0.5, 0.5), vec2(across, down))
}

/// Paints one tile's face in `rect`. A lone picture shows whole, in its
/// own shape; a tile in a row is covered by its picture from the middle.
fn paint_tile(
    ui: &egui::Ui,
    palette: &crate::theme::Palette,
    tile: &Tile,
    rect: Rect,
    whole: bool,
) {
    let radius = f32::from(theme::RADIUS_CONTROL);
    let painter = ui.painter();
    let base = if palette.vibrant() {
        palette.text.gamma_multiply(0.08)
    } else {
        palette.surface
    };
    painter.rect_filled(rect, radius, base);
    let picture = |native: Vec2| {
        if whole {
            (
                conversation::contained(native, rect),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            )
        } else {
            (rect, cover_uv(native, rect.size()))
        }
    };
    match &tile.face {
        Face::Texture(texture) => {
            let (shown, uv) = picture(texture.size_vec2());
            egui::Image::new((texture.id(), shown.size()))
                .uv(uv)
                .corner_radius(radius)
                .maintain_aspect_ratio(false)
                .paint_at(ui, shown);
        }
        Face::Photo(path) => {
            let native = tile.native.unwrap_or(rect.size());
            let (shown, uv) = picture(native);
            let uri = crate::image_cache::fit_uri(ui.ctx(), path, shown.size().max(rect.size()));
            egui::Image::new(uri)
                .uv(uv)
                .corner_radius(radius)
                .maintain_aspect_ratio(false)
                .paint_at(ui, shown);
        }
        Face::Clip => {
            painter.rect_filled(rect, radius, palette.shadow.gamma_multiply(1.4));
        }
        Face::File(path) => {
            conversation::paint_file_card(ui, palette, rect, path);
        }
    }
    if tile.play {
        let disc = (rect.size().min_elem() * 0.32).clamp(22.0, 40.0);
        painter.circle_filled(
            rect.center(),
            disc / 2.0,
            egui::Color32::from_black_alpha(140),
        );
        theme::paint_icon(
            ui,
            Icon::Play,
            Rect::from_center_size(rect.center(), Vec2::splat(disc * 0.5)),
            disc * 0.5,
            egui::Color32::WHITE,
        );
    }
}

/// How the reply field and the glass over the messages look: the chat's
/// composer's, so the bubble's card matches the window.
fn field_look(palette: &crate::theme::Palette) -> (egui::Color32, Stroke) {
    if palette.vibrant() {
        (
            palette.menu_glass(),
            Stroke::new(1.0, palette.text.gamma_multiply(0.16)),
        )
    } else {
        (palette.bubble_in, Stroke::NONE)
    }
}

/// How far the reply area reaches up from the card's bottom: the field, the
/// tray of attachments over it as far as it has opened, and the margins.
/// The messages scroll on under it, so they keep this much room at the end.
fn composer_room(app: &App, ctx: &egui::Context, chat: &Chat) -> f32 {
    if !chat.can_send() {
        return 44.0;
    }
    let open = tray_open(app, ctx, chat.id.as_str());
    let row = ctx
        .data(|data| data.get_temp::<Tray>(tray_id(&chat.id)))
        .map_or(0.0, |tray| tray.row);
    MARGIN + field_height(ctx, &chat.id) + open * (row + 2.0 * TRAY_PAD + TRAY_GAP) + 6.0
}

/// How tall the bubble's field is now: one line's [`FIELD`], growing with
/// the lines typed up to [`FIELD_LINES`] (see [`composer`]).
fn field_height(ctx: &egui::Context, chat: &str) -> f32 {
    ctx.data(|data| data.get_temp::<f32>(field_height_id(chat)))
        .unwrap_or(FIELD)
}

fn field_height_id(chat: &str) -> egui::Id {
    egui::Id::new(("bubble-field-height", chat))
}

/// How far the tray has opened out of the field, 0 to 1.
fn tray_open(app: &App, ctx: &egui::Context, chat: &str) -> f32 {
    let staged = app
        .bubbles
        .get(chat)
        .is_some_and(|bubble| !bubble.staged.is_empty());
    theme::animate(
        ctx,
        tray_id(chat).with("open"),
        staged,
        if staged {
            theme::MOTION_PANEL
        } else {
            theme::MOTION_PANEL * theme::CLOSE
        },
    )
}

/// The reply area, floating over the bottom of the card's messages on
/// glass, as the chat's composer does: the tray of what is attached, then
/// the field with the attach button, the text and the send button. A paste
/// of a picture or of files, or files dropped on the window, are attached;
/// Enter sends.
fn composer(app: &mut App, ui: &mut egui::Ui, chat: &Chat, card: Rect, settled: bool) {
    let palette = app.palette;
    let locale = app.locale;
    if !chat.can_send() {
        let footer = Rect::from_min_max(pos2(card.left(), card.bottom() - 44.0), card.max);
        let round = CARD_RADIUS as u8;
        ui.painter().rect_filled(
            footer,
            egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: round,
                se: round,
            },
            palette.panel.to_opaque(),
        );
        ui.painter().text(
            footer.center(),
            egui::Align2::CENTER_CENTER,
            crate::i18n::gettext(locale, "You can't send messages here"),
            theme::regular(13.0),
            palette.secondary,
        );
        return;
    }
    let ctx = ui.ctx().clone();
    app.take_bubble_input(&ctx, &chat.id);
    let text_id = egui::Id::new(("bubble-text", chat.id.as_str()));
    // The field grows a line at a time with what is typed, eased, up to
    // FIELD_LINES; past that its text scrolls. It is measured at the width
    // the text had last frame, which only changes with the card.
    let font = theme::regular(14.5);
    let format = egui::TextFormat::simple(font.clone(), palette.text);
    let line = ctx.fonts_mut(|fonts| fonts.row_height(&font));
    let inset = (FIELD - line) / 2.0;
    let wrap = ctx
        .data(|data| data.get_temp::<f32>(text_id.with("width")))
        .unwrap_or(CARD.x - 2.0 * MARGIN - 2.0 * FIELD - 16.0);
    let typed = app
        .bubbles
        .get(&chat.id)
        .map(|bubble| bubble.text.clone())
        .unwrap_or_default();
    let lines_height = if typed.contains('\n') || typed.len() > 20 {
        crate::bidi::layout_editor(ui, &typed, &format, wrap, true)
            .0
            .size()
            .y
    } else {
        line
    };
    let target = (lines_height.min(line * FIELD_LINES) + 2.0 * inset).max(FIELD);
    let height = crate::motion::spring_to(
        &ctx,
        field_height_id(&chat.id).with("spring"),
        target,
        crate::motion::Spring::settling_in(f64::from(theme::MOTION_QUICK)),
    );
    ctx.data_mut(|data| data.insert_temp(field_height_id(&chat.id), height));
    let field = Rect::from_min_max(
        pos2(card.left() + MARGIN, card.bottom() - MARGIN - height),
        pos2(card.right() - MARGIN, card.bottom() - MARGIN),
    );
    let staged_count = tray(app, ui, chat, field);
    let (fill, edge) = field_look(&palette);
    if palette.vibrant() {
        ui.painter()
            .add(crate::ui::glass::rounded(field, FIELD / 2.0));
    }
    ui.painter()
        .rect(field, FIELD / 2.0, fill, edge, egui::StrokeKind::Inside);
    // The plus sits in the field's rounded left end, as the chat's does, and
    // the picker's button close beside it, as a pair.
    let mut tools = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(field.left() + 6.0, field.bottom() - FIELD),
                pos2(field.left() + 2.0 * FIELD, field.bottom()),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    tools.spacing_mut().item_spacing.x = 0.0;
    let attach = theme::icon_button(
        &mut tools,
        Icon::Plus,
        20.0,
        palette.secondary,
        palette.text,
        &crate::i18n::gettext(locale, "Attach"),
    );
    if attach.clicked() {
        app.actions.push(Action::BubbleAttach(chat.id.clone()));
    }
    let picking = app.bubbles.picker.as_deref() == Some(chat.id.as_str()) && app.picker.is_some();
    let (mode_icon, mode_label) = match app.settings.picker_mode {
        crate::model::PickerTab::Emoji => (Icon::Smile, "Emoji, GIFs, and stickers"),
        crate::model::PickerTab::Gifs => (Icon::Gif, "GIFs, emoji, and stickers"),
        crate::model::PickerTab::Stickers => (Icon::Sticker, "Stickers, emoji, and GIFs"),
    };
    let smile = theme::icon_button(
        &mut tools,
        mode_icon,
        20.0,
        if picking {
            palette.accent
        } else {
            palette.secondary
        },
        palette.text,
        &crate::i18n::gettext(locale, mode_label),
    );
    // Where the picker grows from, and the button that toggles it.
    ctx.data_mut(|data| data.insert_temp(picker_anchor_id(), smile.rect));
    if smile.clicked() {
        app.actions.push(Action::BubblePicker(chat.id.clone()));
    }
    let send = Rect::from_center_size(
        pos2(field.right() - FIELD / 2.0, field.bottom() - FIELD / 2.0),
        Vec2::splat(FIELD - 2.0 * SEND_CLEARANCE),
    );
    // The hint says what the text becomes, as the chat's does.
    let hint = if staged_count > 0 {
        crate::i18n::gettext(locale, "Add a caption")
    } else {
        crate::i18n::gettext(locale, "Type a message")
    }
    .into_owned();
    // A `:` word being typed offers emoji, as the chat's composer does. Its
    // keys are taken before the field sees them: Enter picks rather than
    // sends, and the arrows move through the list.
    let suggesting = emoji_query(app, &ctx, &chat.id, text_id)
        .map(|(start, end, query)| {
            let candidates = conversation::emoji_candidates(app, &query);
            (start, end, candidates)
        })
        .filter(|(_, _, candidates)| !candidates.is_empty());
    let selected_id = text_id.with("emoji-selected");
    let mut selected = ctx
        .data(|data| data.get_temp::<usize>(selected_id))
        .unwrap_or(0);
    let mut picked = None;
    if let Some((_, _, candidates)) = &suggesting {
        if conversation::take_plain_key(ui, egui::Key::ArrowDown) {
            selected = (selected + 1) % candidates.len();
        }
        if conversation::take_plain_key(ui, egui::Key::ArrowUp) {
            selected = (selected + candidates.len() - 1) % candidates.len();
        }
        selected = selected.min(candidates.len() - 1);
        if conversation::take_plain_key(ui, egui::Key::Enter)
            || conversation::take_plain_key(ui, egui::Key::Tab)
        {
            picked = Some(selected);
        }
    } else {
        selected = 0;
    }
    // Enter sends and Shift+Enter starts a line, or, with Settings' Enter
    // to send off, Enter starts a line and Ctrl/Cmd+Enter sends, as in the
    // chat's composer.
    let enter_sends = app.settings.enter_sends;
    let focused = ctx.memory(|memory| memory.has_focus(text_id));
    let enter = focused
        && ui.input_mut(|input| {
            if enter_sends {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
            } else {
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
            }
        });
    let mut ready = staged_count > 0;
    if let Some(bubble) = app.bubbles.get_mut(&chat.id) {
        // The text between the field's controls, never drawn past them; its
        // last line sits where a single one would.
        let text = Rect::from_min_max(
            pos2(smile.rect.right() + 4.0, field.top() + inset),
            pos2(send.left() - 8.0, field.bottom() - inset),
        );
        ctx.data_mut(|data| data.insert_temp(text_id.with("width"), text.width()));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(text));
        child.set_clip_rect(
            Rect::from_x_y_ranges(text.x_range(), field.y_range().shrink(2.0))
                .intersect(child.clip_rect()),
        );
        let scroll = egui::ScrollArea::vertical()
            .id_salt(text_id.with("scroll"))
            .max_height(text.height())
            .auto_shrink([false, true])
            .stick_to_bottom(true)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
        let inner = scroll.show(&mut child, |ui| {
            // Emoji keep their place in the text as see-through glyphs, and
            // their colour pictures are painted over them, as in the chat's.
            let mut clusters: Vec<(usize, usize, String)> = Vec::new();
            let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
                let (galley, found) =
                    crate::bidi::layout_editor(ui, buffer.as_str(), &format, wrap, true);
                clusters = found;
                galley
            };
            let output = egui::TextEdit::multiline(&mut bubble.text)
                .id(text_id)
                .hint_text(
                    egui::RichText::new(hint)
                        .color(palette.dim)
                        .font(font.clone()),
                )
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font)
                .text_color(palette.text)
                .desired_rows(1)
                .desired_width(text.width())
                .return_key(Some(egui::KeyboardShortcut::new(
                    if enter_sends {
                        egui::Modifiers::SHIFT
                    } else {
                        egui::Modifiers::NONE
                    },
                    egui::Key::Enter,
                )))
                .layouter(&mut layouter)
                .show(ui);
            for (start, length, cluster) in &clusters {
                let Some(bounds) = crate::bidi::char_bounds(&output.galley, *start, start + length)
                else {
                    continue;
                };
                // An emoji split across rows is left to its glyphs.
                if bounds.height() > line * 1.5 {
                    continue;
                }
                crate::emoji::paint_cluster(
                    ui,
                    cluster,
                    bounds.translate(output.galley_pos.to_vec2()),
                );
            }
            output.response.response
        });
        let response = inner.inner;
        // Once the card has settled: a field on its way in takes nothing.
        if settled && std::mem::take(&mut bubble.focus) {
            response.request_focus();
        }
        ready |= !bubble.text.trim().is_empty();
    }
    if let Some((start, end, candidates)) = &suggesting {
        let above = ctx
            .data(|data| data.get_temp::<f32>(tray_id(&chat.id).with("top")))
            .unwrap_or(field.top());
        if let Some(index) = emoji_list(
            ui,
            &palette,
            candidates,
            selected,
            field,
            above.min(field.top()),
        ) {
            picked = Some(index);
        }
        if let Some(index) = picked
            && let Some(candidate) = candidates.get(index)
            && let Some(bubble) = app.bubbles.get_mut(&chat.id)
        {
            // The `:word` becomes the emoji and a space, the caret after it.
            let replacement = format!("{} ", candidate.emoji);
            bubble.text.replace_range(*start..*end, &replacement);
            let caret = bubble.text[..*start + replacement.len()].chars().count();
            let mut state = egui::TextEdit::load_state(&ctx, text_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(caret),
                )));
            state.store(&ctx, text_id);
            bubble.focus = true;
            selected = 0;
        }
    }
    if suggesting.is_none() {
        theme::forget_motion(&ctx, emoji_list_id());
    }
    ctx.data_mut(|data| data.insert_temp(selected_id, selected));
    // The send button, as the chat's: lit once there is something to send.
    let (idle, idle_hover, idle_icon) = if palette.vibrant() {
        (
            palette.text.gamma_multiply(0.12),
            palette.text.gamma_multiply(0.2),
            palette.dim,
        )
    } else {
        (palette.surface, palette.surface_hover, palette.dim)
    };
    let lit = theme::animate(
        ui.ctx(),
        egui::Id::new(("bubble-send", chat.id.as_str())),
        ready,
        theme::MOTION_QUICK,
    );
    let mut button = ui.new_child(egui::UiBuilder::new().max_rect(send));
    let pressed = theme::circle_button_morph(
        &mut button,
        ("bubble-send", chat.id.as_str()),
        Icon::Send,
        send.width(),
        idle.lerp_to_gamma(palette.accent_fill, lit),
        idle_hover.lerp_to_gamma(palette.accent_hover, lit),
        idle_icon.lerp_to_gamma(palette.on_accent, lit),
        &crate::i18n::gettext(locale, "Send"),
    );
    if (enter || pressed.clicked()) && ready {
        fly_staged(app, ui.ctx(), &chat.id);
        app.actions.push(Action::BubbleSend(chat.id.clone()));
    }
}

/// The composer's picker, over `chat`'s card while its button opened it:
/// kept to the card, growing from that button. What it picks goes to the
/// bubble: an emoji into its field, a sticker or GIF to its chat.
fn picker(app: &mut App, ctx: &egui::Context, chat: &str, card: Rect, open: bool) {
    if !open || app.bubbles.picker.as_deref() != Some(chat) || app.picker.is_none() {
        return;
    }
    let Some(anchor) = ctx.data(|data| data.get_temp::<Rect>(picker_anchor_id())) else {
        return;
    };
    let main_anchor = app.picker_anchor.replace(anchor);
    let mark = app.actions.len();
    ctx.data_mut(|data| data.insert_temp(super::picker::bounds_id(), card));
    super::picker::show(app, ctx);
    ctx.data_mut(|data| data.remove::<Rect>(super::picker::bounds_id()));
    app.picker_anchor = main_anchor;
    let asked: Vec<Action> = app.actions.drain(mark..).collect();
    for action in asked {
        let action = match action {
            Action::InsertEmoji(text) => Action::BubbleInsert {
                chat: chat.to_owned(),
                text,
            },
            Action::SendSticker(path) => Action::BubbleSticker {
                chat: chat.to_owned(),
                path,
            },
            Action::SendGif(gif) => Action::BubbleGif {
                chat: chat.to_owned(),
                gif,
            },
            other => other,
        };
        app.bubbles.picker_actions.push(action);
    }
}

/// Where the bubble's picker button is, for the picker to grow from.
fn picker_anchor_id() -> egui::Id {
    egui::Id::new("bubble-picker-anchor")
}

fn emoji_list_id() -> egui::Id {
    egui::Id::new("bubble-emoji-list")
}

/// The `:word` being typed at the bubble field's caret: where its `:` and
/// the caret are in the text, in bytes, and the word. A `:` starts one only
/// at the start of the text or after a space, so a time or a smiley like
/// ":)" is left alone.
fn emoji_query(
    app: &App,
    ctx: &egui::Context,
    chat: &str,
    field: egui::Id,
) -> Option<(usize, usize, String)> {
    let text = &app.bubbles.get(chat)?.text;
    if !ctx.memory(|memory| memory.has_focus(field)) {
        return None;
    }
    let caret = egui::TextEdit::load_state(ctx, field)
        .and_then(|state| state.cursor.char_range())
        .map_or_else(|| text.chars().count(), |range| range.primary.index.0);
    let end = text
        .char_indices()
        .nth(caret)
        .map_or(text.len(), |(at, _)| at);
    let before = &text[..end];
    let colon = before.rfind(':')?;
    let starts_word = before[..colon]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace);
    let query = &before[colon + 1..];
    let word = !query.is_empty()
        && query
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | '+'));
    (starts_word && word).then(|| (colon, end, query.to_owned()))
}

/// The emoji a `:word` matches, on glass over the field and anything over
/// it (`above`), growing up out of it. Returns the row clicked.
fn emoji_list(
    ui: &mut egui::Ui,
    palette: &crate::theme::Palette,
    candidates: &[conversation::EmojiSuggestion],
    selected: usize,
    field: Rect,
    above: f32,
) -> Option<usize> {
    const ROW: f32 = 36.0;
    let ctx = ui.ctx().clone();
    let shown = theme::appear(&ctx, emoji_list_id(), theme::MOTION_QUICK);
    let height = candidates.len() as f32 * ROW + 12.0;
    let list = Rect::from_min_max(
        pos2(field.left(), above - TRAY_GAP - height),
        pos2(field.right(), above - TRAY_GAP),
    )
    .translate(vec2(0.0, (1.0 - theme::ease_out(shown)) * TRAY_RISE));
    let radius = f32::from(theme::RADIUS_MENU);
    if palette.vibrant() {
        ui.painter()
            .add(crate::ui::glass::veil(list, radius, shown));
    }
    let mut scope = ui.new_child(egui::UiBuilder::new().max_rect(list));
    scope.multiply_opacity(shown);
    scope.painter().rect(
        list,
        radius,
        palette.menu_glass(),
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
    let mut clicked = None;
    for (index, candidate) in candidates.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(list.left() + 6.0, list.top() + 6.0 + index as f32 * ROW),
            vec2(list.width() - 12.0, ROW),
        );
        let response = scope.interact(
            row,
            egui::Id::new(("bubble-emoji-row", index)),
            Sense::click(),
        );
        if index == selected || response.hovered() {
            scope.painter().rect_filled(
                row,
                f32::from(theme::RADIUS_CONTROL),
                if index == selected {
                    palette.accent.gamma_multiply(0.18)
                } else {
                    palette.surface_hover
                },
            );
        }
        let glyph = Rect::from_min_size(
            pos2(row.left() + 8.0, row.center().y - 11.0),
            Vec2::splat(22.0),
        );
        crate::emoji::paint_cluster(&scope, candidate.emoji, glyph);
        let code = super::widgets::line(
            &scope,
            &candidate.shortcode,
            theme::medium(theme::TEXT_CONTROL),
            palette.text,
            row.width() - 48.0,
            1,
        );
        code.paint(
            &scope,
            pos2(row.left() + 40.0, row.center().y - code.size().y / 2.0),
            palette.text,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            clicked = Some(index);
        }
    }
    clicked
}

/// The tray of what is attached, over `field`: it rises out of the field as
/// the first thing is attached and sinks back once they are sent or taken
/// away. While it is open a tile added pops in, one taken away fades and
/// shrinks where it was, and the rest slide into their new places. Returns
/// how many things are attached.
fn tray(app: &mut App, ui: &mut egui::Ui, chat: &Chat, field: Rect) -> usize {
    let palette = app.palette;
    let ctx = ui.ctx().clone();
    let id = tray_id(&chat.id);
    let now = ctx.input(|input| input.time);
    let tiles = staged_tiles(app, &ctx, &chat.id);
    let open = tray_open(app, &ctx, &chat.id);
    let mut kept = ctx
        .data(|data| data.get_temp::<Tray>(id))
        .unwrap_or_default();
    // Tiles sent last frame that fly into the transcript.
    let flown = ctx
        .data_mut(|data| data.remove_temp::<Vec<egui::Id>>(id.with("flown")))
        .unwrap_or_default();
    if open <= 0.0 && tiles.is_empty() {
        if !kept.tiles.is_empty() || !kept.leaving.is_empty() {
            // Lets go of the pictures once nothing shows them.
            ctx.data_mut(|data| data.remove::<Tray>(id));
        }
        ctx.data_mut(|data| {
            data.remove::<f32>(id.with("top"));
            data.remove::<Vec<(egui::Id, Rect)>>(id.with("faces"));
        });
        return 0;
    }
    let spring = crate::motion::Spring::settling_in(f64::from(theme::MOTION_PANEL));
    let room = (field.width() - 2.0 * TRAY_PAD).max(TILE);
    let (places, row) = lay_out_tiles(&tiles, room);
    // The row's height follows the tiles; while the tray closes it keeps
    // the height it had.
    let row = if tiles.is_empty() {
        kept.row
    } else {
        crate::motion::spring_to(&ctx, id.with("row"), row, spring)
    };
    // Tiles gone since the last frame fade where they were.
    for (tile, place) in &kept.tiles {
        if !tiles.iter().any(|staged| staged.key == tile.key) {
            // One that flies away leaves no ghost behind.
            if !flown.contains(&tile.key) {
                kept.leaving.push((tile.clone(), *place, now));
            }
            for part in ["x", "w", "h", "in"] {
                crate::motion::forget(&ctx, tile.key.with(part));
            }
        }
    }
    let leave = f64::from(theme::MOTION_PANEL);
    kept.leaving.retain(|(_, _, since)| now - since < leave);
    let eased = theme::ease_out(open);
    let height = row + 2.0 * TRAY_PAD;
    let tray = Rect::from_min_max(
        pos2(field.left(), field.top() - TRAY_GAP - height),
        pos2(field.right(), field.top() - TRAY_GAP),
    )
    .translate(vec2(0.0, (1.0 - eased) * TRAY_RISE));
    // Anything offered over the field (the emoji a `:word` matches) sits
    // above the tray.
    ctx.data_mut(|data| data.insert_temp(id.with("top"), tray.top()));
    let radius = f32::from(theme::RADIUS_CARD);
    let (fill, edge) = field_look(&palette);
    // The glass first, then everything on it, all fading in together.
    if palette.vibrant() {
        ui.painter().add(crate::ui::glass::veil(tray, radius, open));
    }
    let mut scope = ui.new_child(egui::UiBuilder::new().max_rect(tray));
    scope.multiply_opacity(open);
    scope
        .painter()
        .rect(tray, radius, fill, edge, egui::StrokeKind::Inside);
    let inner = tray.shrink(TRAY_PAD);
    let content_width = places.last().map_or(0.0, |place| place.right());
    let mut remove = None;
    let mut drawn = Vec::new();
    let mut faces: Vec<(egui::Id, Rect)> = Vec::new();
    let mut moving = !kept.leaving.is_empty();
    scope.set_clip_rect(tray.intersect(scope.clip_rect()));
    let mut row_ui = scope.new_child(egui::UiBuilder::new().max_rect(inner));
    egui::ScrollArea::horizontal()
        .id_salt(id.with("scroll"))
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(&mut row_ui, |ui| {
            let (row_rect, _) =
                ui.allocate_exact_size(vec2(content_width.max(inner.width()), row), Sense::hover());
            let origin = row_rect.min.to_vec2();
            // Those leaving first, under the rest, shrinking and fading.
            for (tile, place, since) in &kept.leaving {
                let gone = theme::ease_out(((now - since) / leave) as f32);
                let scale = 1.0 - (1.0 - theme::CLOSE_SCALE) * gone;
                let rect = Rect::from_center_size(place.center() + origin, place.size() * scale);
                let mut ghost = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                ghost.multiply_opacity(1.0 - gone);
                paint_tile(&ghost, &palette, tile, rect, kept.tiles.len() <= 1);
            }
            let whole = tiles.len() == 1;
            for (index, (tile, target)) in tiles.iter().zip(&places).enumerate() {
                // Each slides and grows to its place on the theme's spring,
                // and one new to the tray pops in from a little under its
                // size.
                let x = crate::motion::spring_to(&ctx, tile.key.with("x"), target.left(), spring);
                let w = crate::motion::spring_to(&ctx, tile.key.with("w"), target.width(), spring);
                let h = crate::motion::spring_to(&ctx, tile.key.with("h"), target.height(), spring);
                let shown = theme::appear(&ctx, tile.key.with("in"), theme::MOTION_QUICK);
                moving |= shown < 1.0
                    || (x - target.left()).abs() > 0.5
                    || (w - target.width()).abs() > 0.5;
                let place = Rect::from_min_size(pos2(x, (row - h) / 2.0), vec2(w, h));
                let scale = egui::lerp(0.92..=1.0, shown);
                let rect = Rect::from_center_size(place.center() + origin, place.size() * scale);
                let mut face = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                face.multiply_opacity(shown);
                paint_tile(&face, &palette, tile, rect, whole);
                // Its remove button, in the corner, clear of a rounded edge.
                let close =
                    Rect::from_center_size(rect.right_top() + vec2(-13.0, 13.0), Vec2::splat(20.0));
                let response = ui
                    .interact(close, id.with(("unstage", tile.key)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(crate::i18n::gettext(app.locale, "Remove").as_ref());
                let hovered = theme::animate(
                    &ctx,
                    response.id.with("hover"),
                    response.hovered(),
                    theme::MOTION_QUICK,
                );
                face.painter().circle(
                    close.center(),
                    10.0,
                    palette
                        .overlay
                        .to_opaque()
                        .lerp_to_gamma(palette.surface_hover.to_opaque(), hovered),
                    Stroke::new(1.0, palette.outline),
                );
                theme::paint_icon(&face, Icon::X, close.shrink(5.0), 10.0, palette.text);
                if response.clicked() {
                    remove = Some(index);
                }
                drawn.push((tile.clone(), place));
                faces.push((tile.key, rect));
            }
        });
    if moving {
        crate::motion::request_frame(&ctx);
    }
    if let Some(index) = remove {
        app.actions.push(Action::BubbleUnstage {
            chat: chat.id.clone(),
            index,
        });
    }
    let count = tiles.len();
    ctx.data_mut(|data| {
        data.insert_temp(id.with("faces"), faces);
        data.insert_temp(
            id,
            Tray {
                tiles: drawn,
                leaving: kept.leaving,
                row,
            },
        )
    });
    count
}

/// Sets what `chat`'s bubble has attached flying from its tiles into the
/// card's transcript once it is sent, each with the face drawn this frame.
/// While motion is reduced, or the window is not in front, each appears in
/// its place instead.
fn fly_staged(app: &mut App, ctx: &egui::Context, chat: &str) {
    let id = tray_id(chat);
    let focused = ctx.input(|input| input.viewport().focused.unwrap_or(false));
    let faces = ctx
        .data(|data| data.get_temp::<Vec<(egui::Id, Rect)>>(id.with("faces")))
        .unwrap_or_default();
    let staged = app.bubbles.get(chat).map_or(0, |bubble| bubble.staged.len());
    if !focused || crate::motion::reduced(ctx) || faces.is_empty() || faces.len() != staged {
        return;
    }
    let tiles: Vec<(crate::app::AttachmentId, Rect)> = faces
        .iter()
        .map(|&(_, face)| (app.next_attachment_id(), face))
        .collect();
    if let Some(bubble) = app.bubbles.get_mut(chat) {
        bubble.flights = tiles.iter().map(|&(staged, _)| staged).collect();
    }
    conversation::start_flights(ctx, chat, &tiles);
    let flown: Vec<egui::Id> = faces.iter().map(|&(key, _)| key).collect();
    ctx.data_mut(|data| data.insert_temp(id.with("flown"), flown));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Drop(std::path::PathBuf);
    impl egui::DroppedFile for Drop {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    /// The glow breathes when a message arrives and then rests, with no
    /// step where it settles, and stays inside the window's padding.
    #[test]
    fn the_glow_breathes_then_rests() {
        let rest = glow_strength(GLOW_BREATHING + 1.0, false);
        assert_eq!(glow_strength(0.0, false), rest, "it starts from rest");
        assert!((glow_strength(GLOW_BREATHING - 1e-6, false) - rest).abs() < 1e-3);
        let peak = glow_strength(GLOW_BREATH / 4.0, false);
        let trough = glow_strength(GLOW_BREATH * 3.0 / 4.0, false);
        assert!(peak > rest && trough < rest, "{peak} {rest} {trough}");
        assert_eq!(
            glow_strength(GLOW_BREATH / 4.0, true),
            rest,
            "still: no breathing"
        );
        let breaths = GLOW_BREATHING / GLOW_BREATH;
        assert!((breaths - breaths.round()).abs() < 1e-9, "whole breaths");
    }

    /// A bubble asks for frames only while its glow breathes: lit and
    /// resting, or with nothing unread, it leaves the window asleep.
    #[test]
    fn a_glowing_bubble_stops_asking_for_frames() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("glow-test");
        let repaints = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = repaints.clone();
        ctx.set_request_repaint_callback(move |info| {
            if info.delay.is_zero() {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
        let asked = |time: f64, unread: u32| {
            let raw = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            // Let the fade-in spring of an earlier frame finish first.
            let before = repaints.load(std::sync::atomic::Ordering::Relaxed);
            let mut output = ctx.run_ui(raw, |ui| {
                let shown = Rect::from_center_size(pos2(50.0, 50.0), Vec2::splat(BUBBLE));
                paint_glow(
                    ui.ctx(),
                    ui.painter(),
                    id,
                    shown,
                    unread,
                    egui::Color32::GREEN,
                );
            });
            output.textures_delta.clear();
            repaints.load(std::sync::atomic::Ordering::Relaxed) > before
        };
        // A new context asks for a frame or two of its own.
        asked(0.0, 0);
        asked(0.1, 0);
        assert!(!asked(0.2, 0), "nothing unread, nothing drawn");
        assert!(asked(1.0, 1), "a message arrived: it breathes");
        assert!(asked(2.0, 1));
        // Long past the breathing and the fade-in.
        asked(1.0 + GLOW_BREATHING + 5.0, 1);
        assert!(!asked(1.0 + GLOW_BREATHING + 6.0, 1), "lit and at rest");
        assert!(asked(1.0 + GLOW_BREATHING + 7.0, 2), "another message");
    }

    /// A bubble opens, draws its card, takes a dropped file and goes away.
    #[test]
    fn a_bubble_opens_into_a_card_and_takes_a_dropped_file() {
        let root = std::env::temp_dir().join(format!("zeus-bubbles-{}", std::process::id()));
        let (mut app, _events) = App::headless(
            crate::paths::AppDirs::under(&root),
            crate::settings::Settings::default(),
        );
        app.settings.chat_bubbles = true;
        let chat = "1@s.whatsapp.net".to_owned();
        app.chats
            .push(crate::model::Chat::new(chat.clone(), "Ada".into()));
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let run =
            |app: &mut App,
             time: f64,
             drops: Vec<std::sync::Arc<dyn egui::DroppedFile + Send + Sync>>| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 700.0))),
                        time: Some(time),
                        dropped_files: drops,
                        ..Default::default()
                    },
                    |ui| {
                        app.apply_actions(ui.ctx());
                        show(app, ui.ctx());
                    },
                );
                output.textures_delta.clear();
            };
        app.actions.push(Action::Bubble(chat.clone()));
        for frame in 0..8 {
            run(&mut app, frame as f64 * 0.05, Vec::new());
        }
        assert_eq!(app.bubbles.list.len(), 1);
        assert_eq!(app.bubbles.expanded.as_deref(), Some(chat.as_str()));
        assert!(
            ctx.data(|data| data.get_temp::<Placement>(placement_id()))
                .is_some_and(|placement| placement.open),
            "the window made room for the card"
        );
        let file = Drop(std::path::PathBuf::from("picture.png"));
        run(&mut app, 0.5, vec![std::sync::Arc::new(file)]);
        assert_eq!(app.bubbles.get(&chat).unwrap().staged.len(), 1);

        app.actions.push(Action::DismissBubble(chat));
        run(&mut app, 0.6, Vec::new());
        assert!(app.bubbles.is_empty());
    }

    /// Dragging the open card's header moves the window; its buttons still
    /// take their clicks.
    #[test]
    fn the_open_card_is_dragged_by_its_header() {
        let root = std::env::temp_dir().join(format!("zeus-bubble-drag-{}", std::process::id()));
        let (mut app, _events) = App::headless(
            crate::paths::AppDirs::under(&root),
            crate::settings::Settings::default(),
        );
        let chat = crate::model::Chat::new("1@s.whatsapp.net".into(), "Ada".into());
        app.chats.push(chat.clone());
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let header = Rect::from_min_size(pos2(20.0, 20.0), vec2(CARD.x, HEADER));
        // Runs one frame with `events`, and says whether the window was
        // asked to follow the pointer.
        let frame = |app: &mut App, time: f64, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 300.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| card_header(app, ui, &chat, header),
            );
            output.textures_delta.clear();
            output.viewport_output.values().any(|viewport| {
                viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::StartDrag))
            })
        };
        let press = |at: Pos2, pressed: bool| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // On the name, well left of the buttons.
        let name = pos2(header.left() + 120.0, header.center().y);
        frame(&mut app, 0.0, vec![egui::Event::PointerMoved(name)]);
        // Nothing else there takes a click, so it drags from the press, as
        // a title bar does.
        let pressed = frame(&mut app, 0.1, vec![press(name, true)]);
        let moved = egui::Event::PointerMoved(name + vec2(24.0, 10.0));
        let dragged = frame(&mut app, 0.2, vec![moved]);
        assert!(pressed || dragged, "the header drags the window");
        frame(&mut app, 0.3, vec![press(name + vec2(24.0, 10.0), false)]);
        assert!(app.actions.is_empty(), "a drag is not a click on anything");

        // A click on the rightmost button, Remove bubble, is still its own.
        let button = pos2(header.right() - 28.0, header.center().y);
        frame(&mut app, 1.0, vec![egui::Event::PointerMoved(button)]);
        assert!(!frame(&mut app, 1.1, vec![press(button, true)]));
        assert!(!frame(&mut app, 1.2, vec![press(button, false)]));
        assert!(
            matches!(app.actions.as_slice(), [Action::DismissBubble(id)] if *id == chat.id),
            "{:?}",
            app.actions.len()
        );
    }

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

    /// A picture covers its tile from the middle, undistorted.
    #[test]
    fn a_picture_is_cropped_to_its_tile_from_its_middle() {
        let square_uv = |size| cover_uv(size, Vec2::splat(TILE));
        let wide = square_uv(vec2(1600.0, 900.0));
        assert_eq!(wide.center(), pos2(0.5, 0.5));
        assert!((wide.width() - 900.0 / 1600.0).abs() < 1e-6 && wide.height() == 1.0);
        let tall = square_uv(vec2(900.0, 1600.0));
        assert!(tall.width() == 1.0 && (tall.height() - 900.0 / 1600.0).abs() < 1e-6);
        assert_eq!(square_uv(vec2(0.0, 0.0)).size(), vec2(1.0, 1.0));
    }
}
