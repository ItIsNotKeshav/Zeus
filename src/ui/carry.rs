//! Carrying messages to another chat: a message picked up in the open chat
//! and dropped on a chat in the list (or on its avatar in the rail) is
//! forwarded there, as the Forward dialog would with that one chat ticked.
//!
//! A message is picked up by dragging it from anywhere on its bubble that is
//! not text (a photo, a video, a file card, a sticker, the bubble's own
//! padding), or by holding the pointer still on it for [`HOLD`], text
//! included. A drag that starts on text still selects text, and one beside
//! the bubble still sweeps messages into a selection. A message in a
//! selection carries the whole selection with it.
//!
//! What follows the pointer is the bubble itself: the shapes it painted the
//! frame before it was lifted, scaled down to a card under a soft shadow,
//! the spot it was grabbed by kept under the pointer. The bubble left behind
//! fades while it is away. A chat that can take the messages lights up under
//! the pointer and the card shrinks toward it with a "Forward to" label; one
//! that cannot (read-only, a channel we may not post in) shows a refusing
//! cursor. Dropped on a chat, the card falls into its avatar; dropped
//! anywhere else, or abandoned with Escape, it glides back into its bubble.
//!
//! Carried out of the window with downloaded files on Windows, the files go
//! on to the system's own drag and drop ([`crate::drag_out`]), so they can be
//! dropped in Explorer or another app.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2, pos2, vec2};

use crate::app::App;
use crate::model::{Action, ChatId, Content, Message};
use crate::theme::{self, Icon, Palette};

/// How long the pointer rests on a message before it lifts, text included.
pub const HOLD: f64 = 0.35;
/// How far the pointer may wander while held before it is a drag of
/// something else (egui's own click distance).
const STILL: f32 = 6.0;
/// The card's largest size while carried, in points.
const CARD: Vec2 = vec2(320.0, 240.0);
/// How small a card may become to fit [`CARD`]: a long message is clipped
/// below that rather than shrunk to an unreadable stamp.
const SMALLEST: f32 = 0.45;
/// The card's size over a chat that takes it, relative to its carried size.
const OVER_TARGET: f32 = 0.9;
/// How faint the bubble left behind is while its card is away.
const GHOST: f32 = 0.3;
const LIFT_SECONDS: f64 = theme::MOTION_PANEL as f64;
const LAND_SECONDS: f64 = theme::MOTION_SCREEN as f64;
const RETURN_SECONDS: f64 = theme::MOTION_SCREEN as f64;

/// Set while a card is lifted, for the selection leash's input hook, which
/// runs before the frame and must let the pointer leave the message view.
static LIFTED: AtomicBool = AtomicBool::new(false);

/// Whether a card is lifted and following the pointer.
pub fn lifted() -> bool {
    LIFTED.load(Ordering::Relaxed)
}

fn carry_id() -> egui::Id {
    egui::Id::new("carry")
}

fn targets_id() -> egui::Id {
    egui::Id::new("carry-targets")
}

fn glow_id() -> egui::Id {
    egui::Id::new("carry-glow")
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// The button went down on the bubble; not yet a carry.
    Pressed,
    /// Following the pointer, from `at`.
    Lifted { at: f64 },
    /// Dropped on a chat: falling into its avatar at `to`.
    Landing { at: f64, to: Pos2 },
    /// Dropped elsewhere or abandoned: gliding back into the bubble.
    Returning { at: f64 },
    /// Handed to the system's drag and drop; nothing is drawn.
    Gone,
}

/// One pickup, from the press on a bubble to the end of its animation.
#[derive(Clone)]
struct Carry {
    chat: ChatId,
    /// The ids carried, in the chat's order.
    messages: Vec<String>,
    /// The message (or an album's first member) whose bubble was grabbed.
    anchor: String,
    /// The bubble, where it was painted when grabbed.
    origin: Rect,
    /// The bubble's laid-out rect, which the pointer is tested against.
    laid: Rect,
    /// What the bubble painted, in screen coordinates.
    shapes: Arc<Vec<egui::Shape>>,
    /// The bubble's fill, for the cards stacked behind a selection.
    fill: Color32,
    /// Where the bubble's shapes were painted this pass, until it lifts:
    /// they are read at the end of the pass, once the transcript has moved
    /// them where they show.
    painted: Option<Painted>,
    /// Downloaded files among the messages, for dragging out of the window.
    files: Vec<PathBuf>,
    /// The spot on the bubble it was grabbed by, which stays under the
    /// pointer; it moves with the bubble until the bubble lifts.
    press: Pos2,
    /// Where the button went down, and when.
    down: Pos2,
    pressed_at: f64,
    phase: Phase,
    /// The card's grab point, trailing the pointer a little.
    at: Pos2,
    /// The card's scale and its opacity when it was let go.
    scale: f32,
    /// When the card last moved, to step `at` by the time between frames.
    moved: f64,
}

#[derive(Clone, Copy)]
struct Painted {
    layer: egui::LayerId,
    start: usize,
    end: usize,
    pass: u64,
}

/// A chat that can be dropped on, as its row registered it this frame.
#[derive(Clone)]
struct Target {
    rect: Rect,
    chat: ChatId,
    title: String,
    /// Where the card lands: the chat's avatar.
    avatar: Pos2,
    /// Whether the messages may be sent there.
    accepts: bool,
}

fn load(ctx: &egui::Context) -> Option<Carry> {
    ctx.data(|data| data.get_temp::<Carry>(carry_id()))
}

fn store(ctx: &egui::Context, carry: Option<Carry>) {
    LIFTED.store(
        carry
            .as_ref()
            .is_some_and(|carry| matches!(carry.phase, Phase::Lifted { .. })),
        Ordering::Relaxed,
    );
    ctx.data_mut(|data| match carry {
        Some(carry) => {
            data.insert_temp(carry_id(), carry);
        }
        None => data.remove::<Carry>(carry_id()),
    });
}

/// Whether drop targets should register this frame: a card is in the air.
pub fn carrying(ctx: &egui::Context) -> bool {
    load(ctx).is_some_and(|carry| !matches!(carry.phase, Phase::Pressed | Phase::Gone))
}

/// How opaque a message's bubble is drawn: faint while its card is away,
/// coming back as the card returns. `None` draws it as usual.
pub fn ghost(ctx: &egui::Context, chat: &str, message: &str) -> Option<f32> {
    let carry = load(ctx)?;
    if carry.chat != chat || !carry.messages.iter().any(|id| id == message) {
        return None;
    }
    match carry.phase {
        Phase::Pressed | Phase::Gone => None,
        Phase::Lifted { .. } | Phase::Landing { .. } => Some(GHOST),
        Phase::Returning { at } => {
            let now = ctx.input(|input| input.time);
            let t = ((now - at) / RETURN_SECONDS) as f32;
            Some(egui::lerp(GHOST..=1.0, ease_in_out(t)))
        }
    }
}

/// Whether a message's content can be carried: what the Forward dialog
/// takes.
pub fn carriable(message: &Message) -> bool {
    crate::app::can_select(&message.content)
}

/// Watches a message's bubble for a press that may pick it up, and keeps
/// the picture of a pressed bubble current until it lifts. Called after the
/// row is drawn, with the layer's first shape of the row: `members` are the
/// album's messages, or the message alone, and `selected` the chat's
/// selection.
pub fn watch(
    ui: &egui::Ui,
    chat: &str,
    members: &[Message],
    selected: &[String],
    start: egui::layers::ShapeIdx,
    bubble: Rect,
    fill: Color32,
) {
    let Some(first) = members.first() else {
        return;
    };
    let ctx = ui.ctx();
    let current = load(ctx);
    let ours = current
        .as_ref()
        .is_some_and(|carry| carry.chat == chat && carry.anchor == first.id);
    let pressed = ui.input(|input| {
        input.pointer.press_origin().filter(|origin| {
            input.pointer.primary_pressed()
                && bubble.contains(*origin)
                && ui.clip_rect().contains(*origin)
        })
    });
    let pressed = pressed.filter(|origin| {
        // Not through a menu, a picker or a dialog above the chat.
        ctx.layer_id_at(*origin)
            .is_none_or(|layer| layer == ui.layer_id())
            && members.iter().all(carriable)
            && current
                .as_ref()
                .is_none_or(|carry| matches!(carry.phase, Phase::Pressed))
    });
    if pressed.is_none() && ours {
        if let Some(mut carry) = current.clone()
            && carry.phase != Phase::Pressed
            && carry.laid != bubble
        {
            // Carried while the chat scrolls: the way home moves with it.
            let delta = bubble.min - carry.laid.min;
            carry.press += delta;
            carry.origin = carry.origin.translate(delta);
            carry.laid = bubble;
            store(ctx, Some(carry));
        }
        if current
            .as_ref()
            .is_some_and(|carry| carry.phase != Phase::Pressed)
        {
            return;
        }
    }
    if pressed.is_none() && !ours {
        return;
    }
    let laid = bubble;
    let painted = Some(Painted {
        layer: ui.layer_id(),
        start: start.0,
        end: ctx.graphics(|graphics| {
            graphics
                .get(ui.layer_id())
                .map_or(start.0, |list| list.next_idx().0)
        }),
        pass: ctx.cumulative_pass_nr(),
    });
    let now = ui.input(|input| input.time);
    let carry = match (pressed, current) {
        (Some(origin), _) => {
            // A message in the selection carries the selection.
            let messages = if members.iter().any(|member| selected.contains(&member.id)) {
                selected.to_vec()
            } else {
                members.iter().map(|member| member.id.clone()).collect()
            };
            Carry {
                chat: chat.to_owned(),
                messages,
                anchor: first.id.clone(),
                origin: bubble,
                laid,
                shapes: Arc::default(),
                painted,
                fill,
                files: members.iter().filter_map(downloaded).collect(),
                press: origin,
                down: origin,
                pressed_at: now,
                phase: Phase::Pressed,
                at: origin,
                scale: 1.0,
                moved: now,
            }
        }
        (None, Some(mut carry)) => {
            carry.press += laid.min - carry.laid.min;
            carry.laid = laid;
            carry.painted = painted;
            carry
        }
        (None, None) => return,
    };
    store(ctx, Some(carry));
}

/// The downloaded file of a message, if it has one.
fn downloaded(message: &Message) -> Option<PathBuf> {
    match &message.content {
        Content::Image { media, .. }
        | Content::Video { media, .. }
        | Content::Audio { media, .. }
        | Content::Document { media, .. }
        | Content::Sticker { media, .. } => media.path.clone(),
        _ => None,
    }
}

/// Reads what the pressed bubble painted this pass, now that the
/// transcript has put it where it shows, leaving out what was drawn beside
/// it (the sender's picture, the reaction button, the time under it). Its
/// rect is the largest shape's: the bubble's backdrop, or a bare picture.
fn capture(ctx: &egui::Context, carry: &mut Carry) {
    let Some(painted) = carry.painted.take() else {
        return;
    };
    if painted.pass != ctx.cumulative_pass_nr() {
        return;
    }
    let shapes: Vec<egui::Shape> = ctx.graphics(|graphics| {
        graphics.get(painted.layer).map_or_else(Vec::new, |list| {
            list.all_entries()
                .skip(painted.start)
                .take(painted.end.saturating_sub(painted.start))
                .map(|clipped| clipped.shape.clone())
                .filter(|shape| shape.visual_bounding_rect().is_positive())
                .collect()
        })
    });
    let area = |rect: Rect| rect.width() * rect.height();
    let Some(rect) = shapes
        .iter()
        .map(egui::Shape::visual_bounding_rect)
        .filter(|rect| (rect.width() - carry.laid.width()).abs() < 24.0)
        .max_by(|a, b| area(*a).total_cmp(&area(*b)))
    else {
        return;
    };
    let within = rect.expand(10.0);
    carry.origin = rect;
    carry.shapes = Arc::new(
        shapes
            .into_iter()
            .filter(|shape| within.contains_rect(shape.visual_bounding_rect()))
            .collect(),
    );
}

/// A drag that egui began on the strip around a bubble: the bubble's own
/// padding and media lift it, instead of sweeping messages. Returns whether
/// it lifted.
pub fn lift_on_drag(ctx: &egui::Context, chat: &str, message: &str) -> bool {
    let Some(mut carry) = load(ctx) else {
        return false;
    };
    if carry.chat != chat
        || carry.anchor != message
        || carry.phase != Phase::Pressed
        || !carry.laid.contains(carry.down)
    {
        return false;
    }
    lift(ctx, &mut carry);
    store(ctx, Some(carry));
    true
}

fn lift(ctx: &egui::Context, carry: &mut Carry) {
    let now = ctx.input(|input| input.time);
    carry.phase = Phase::Lifted { at: now };
    carry.moved = now;
    // The card owns the drag from here: text under the press stops
    // selecting, and a selection's rows stop sweeping.
    ctx.set_dragged_id(carry_id());
    clear_text_selection(ctx);
}

fn clear_text_selection(ctx: &egui::Context) {
    if let Some(selection) = ctx.plugin_opt::<egui::text_selection::LabelSelectionState>() {
        selection.lock().clear_selection();
    }
}

/// Registers a chat's row as somewhere to drop the carried messages. Rows
/// call it while drawing, only while [`carrying`].
pub fn target(ui: &egui::Ui, rect: Rect, chat: &crate::model::Chat, title: &str, avatar: Pos2) {
    let visible = rect.intersect(ui.clip_rect());
    if !visible.is_positive() {
        return;
    }
    let target = Target {
        rect: visible,
        chat: chat.id.clone(),
        title: title.to_owned(),
        avatar,
        accepts: chat.can_send(),
    };
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<Target>>(targets_id())
            .push(target);
    });
}

/// Takes Escape from the rest of the window while a card is carried: it
/// sends the card back rather than closing the chat. Called before the
/// keyboard shortcuts.
pub fn begin(ctx: &egui::Context) {
    let Some(mut carry) = load(ctx) else {
        return;
    };
    if matches!(carry.phase, Phase::Lifted { .. })
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        let now = ctx.input(|input| input.time);
        carry.phase = Phase::Returning { at: now };
        ctx.stop_dragging();
        store(ctx, Some(carry));
    }
}

/// Moves the carry along a frame and draws it over everything: lifts a held
/// press, follows the pointer, lights the chat under it, and forwards on a
/// drop. Called last, once every row has registered.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let targets = ctx
        .data_mut(|data| data.remove_temp::<Vec<Target>>(targets_id()))
        .unwrap_or_default();
    #[cfg(any(test, feature = "demo"))]
    demo(app, ctx);
    let Some(mut carry) = load(ctx) else {
        fade_glow(app, ctx, None);
        return;
    };
    capture(ctx, &mut carry);
    let (now, pointer, down) = ctx.input(|input| {
        (
            input.time,
            input.pointer.latest_pos(),
            input.pointer.primary_down(),
        )
    });
    #[cfg(any(test, feature = "demo"))]
    let down = down || app.demo_carry.is_some();
    let reduce_motion = crate::motion::screen_reader_attached(ctx);
    match carry.phase {
        Phase::Pressed => {
            let moved = pointer.is_some_and(|pointer| pointer.distance(carry.down) > STILL);
            #[cfg(any(test, feature = "demo"))]
            let moved = moved && app.demo_carry.is_none();
            if !down || moved {
                // A click, or a drag of something else: text, a slider.
                store(ctx, None);
                return;
            }
            if now - carry.pressed_at < HOLD {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    HOLD - (now - carry.pressed_at),
                ));
                store(ctx, Some(carry));
                return;
            }
            lift(ctx, &mut carry);
        }
        Phase::Lifted { .. } => {
            // The selection under a long press keeps being cleared: its label
            // still holds the button and would follow the pointer.
            clear_text_selection(ctx);
        }
        Phase::Gone => {
            store(ctx, None);
            return;
        }
        Phase::Landing { at, .. } if now - at >= LAND_SECONDS || reduce_motion => {
            store(ctx, None);
            fade_glow(app, ctx, None);
            return;
        }
        Phase::Returning { at } if now - at >= RETURN_SECONDS || reduce_motion => {
            store(ctx, None);
            fade_glow(app, ctx, None);
            return;
        }
        Phase::Landing { .. } | Phase::Returning { .. } => {}
    }
    let pointer = pointer.unwrap_or(carry.at);
    let over = targets
        .iter()
        .find(|target| target.rect.contains(pointer) && target.chat != carry.chat);
    if let Phase::Lifted { .. } = carry.phase {
        let screen = ctx.content_rect();
        if !down {
            // Let go: on a chat that takes it, forward; anywhere else, back.
            match over.filter(|target| target.accepts) {
                Some(target) => {
                    app.actions.push(Action::ForwardDropped {
                        from_chat: carry.chat.clone(),
                        messages: carry.messages.clone(),
                        to_chat: target.chat.clone(),
                    });
                    carry.phase = Phase::Landing {
                        at: now,
                        to: target.avatar,
                    };
                }
                None => carry.phase = Phase::Returning { at: now },
            }
        } else if !screen.contains(pointer)
            && !carry.files.is_empty()
            && crate::drag_out::supported()
        {
            // Out of the window with files: the system carries them on.
            crate::drag_out::request(carry.files.clone());
            carry.phase = Phase::Gone;
            ctx.stop_dragging();
            store(ctx, Some(carry));
            fade_glow(app, ctx, None);
            ctx.request_repaint();
            return;
        } else {
            ctx.set_cursor_icon(match over {
                Some(target) if !target.accepts => egui::CursorIcon::NotAllowed,
                _ => egui::CursorIcon::Grabbing,
            });
        }
    }
    let lit = match carry.phase {
        Phase::Lifted { .. } => over,
        Phase::Landing { to, .. } => targets.iter().find(|target| target.avatar == to),
        _ => None,
    };
    fade_glow(app, ctx, lit);
    paint(app, ctx, &mut carry, pointer, over, now, reduce_motion);
    store(ctx, Some(carry));
    ctx.request_repaint();
}

/// The light on the chat under the card: it slides from row to row and
/// fades in and out.
#[derive(Clone, Copy)]
struct Glow {
    rect: Rect,
    accepts: bool,
}

fn fade_glow(app: &App, ctx: &egui::Context, lit: Option<&Target>) {
    let now = lit.map(|target| Glow {
        rect: target.rect,
        accepts: target.accepts,
    });
    let previous = ctx.data(|data| data.get_temp::<Glow>(glow_id()));
    let strength = ctx.animate_bool_with_time_and_easing(
        glow_id(),
        now.is_some(),
        theme::MOTION_QUICK,
        theme::ease_out,
    );
    let Some(target) = now.or(previous) else {
        return;
    };
    // Slide toward the row under the pointer rather than jump to it.
    let rect = match (previous, now) {
        (Some(previous), Some(now)) if previous.rect != now.rect => {
            let step = 1.0 - (-ctx.input(|input| input.stable_dt) * 30.0).exp();
            let rect = Rect::from_min_max(
                previous.rect.min.lerp(now.rect.min, step),
                previous.rect.max.lerp(now.rect.max, step),
            );
            ctx.request_repaint();
            rect
        }
        _ => target.rect,
    };
    let glow = Glow {
        rect,
        accepts: target.accepts,
    };
    if strength <= 0.0 {
        ctx.data_mut(|data| data.remove::<Glow>(glow_id()));
        return;
    }
    ctx.data_mut(|data| data.insert_temp(glow_id(), glow));
    let palette = app.palette;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("carry-glow-layer"),
    ));
    let card = glow.rect.shrink2(vec2(6.0, 2.0));
    let radius = CornerRadius::same(theme::RADIUS + 2);
    let colour = if glow.accepts {
        palette.accent
    } else {
        palette.dim
    };
    painter.rect(
        card,
        radius,
        colour.gamma_multiply(0.16 * strength),
        Stroke::new(1.5, colour.gamma_multiply(0.85 * strength)),
        egui::StrokeKind::Inside,
    );
}

/// Draws the card at the pointer, or on its way into a chat or back home.
fn paint(
    app: &App,
    ctx: &egui::Context,
    carry: &mut Carry,
    pointer: Pos2,
    over: Option<&Target>,
    now: f64,
    reduce_motion: bool,
) {
    let palette = app.palette;
    let size = carry.origin.size();
    let rest = (CARD.x / size.x).min(CARD.y / size.y).clamp(SMALLEST, 1.0) * 0.94;
    let accepting = over.is_some_and(|target| target.accepts);
    let shrink = ctx.animate_bool_with_time_and_easing(
        carry_id().with("over"),
        accepting && matches!(carry.phase, Phase::Lifted { .. }),
        theme::MOTION_QUICK,
        theme::ease_out,
    );
    let mut opacity = 1.0;
    let (at, scale) = match carry.phase {
        Phase::Lifted { at } => {
            let lift = if reduce_motion {
                1.0
            } else {
                ease_out(((now - at) / LIFT_SECONDS) as f32)
            };
            // The grab point trails the pointer by a few frames, so the card
            // feels carried rather than glued on.
            let dt = (now - carry.moved).clamp(0.0, 0.1) as f32;
            let step = if reduce_motion {
                1.0
            } else {
                1.0 - (-dt * 28.0).exp()
            };
            carry.at = carry.at.lerp(pointer, step);
            carry.moved = now;
            carry.scale = egui::lerp(1.0..=rest, lift) * egui::lerp(1.0..=OVER_TARGET, shrink);
            (carry.at, carry.scale)
        }
        Phase::Landing { at, to } => {
            let t = ease_in_out(((now - at) / LAND_SECONDS) as f32);
            opacity = 1.0 - t * t;
            // The grab point heads for the avatar; the card's middle should
            // end there, so aim the grab point where that puts it.
            let middle = carry.origin.center();
            let end = to + (carry.press - middle) * carry.scale * 0.15;
            (
                carry.at.lerp(end, t),
                carry.scale * egui::lerp(1.0..=0.15, t),
            )
        }
        Phase::Returning { at } => {
            let t = ease_in_out(((now - at) / RETURN_SECONDS) as f32);
            opacity = if t < 0.75 {
                1.0
            } else {
                1.0 - (t - 0.75) / 0.25
            };
            (
                carry.at.lerp(carry.press, t),
                egui::lerp(carry.scale..=1.0, t),
            )
        }
        Phase::Pressed | Phase::Gone => return,
    };
    let transform = egui::emath::TSTransform::from_translation(at.to_vec2())
        * egui::emath::TSTransform::from_scaling(scale)
        * egui::emath::TSTransform::from_translation(-carry.press.to_vec2());
    let card = transform.mul_rect(carry.origin);
    let layer = egui::LayerId::new(egui::Order::Tooltip, carry_id());
    let mut ui = egui::Ui::new(
        ctx.clone(),
        carry_id().with("ui"),
        egui::UiBuilder::new()
            .layer_id(layer)
            .max_rect(ctx.content_rect()),
    );
    ui.set_opacity(opacity);
    let painter = ui.painter().clone();
    let lifting = match carry.phase {
        Phase::Lifted { at } if !reduce_motion => ease_out(((now - at) / LIFT_SECONDS) as f32),
        _ => 1.0,
    };
    let radius = (16.0 * scale).min(card.height() / 2.0);
    // A selection carries a stack: two more cards peek out behind.
    let count = carry.messages.len();
    if count > 1 {
        for step in [2.0, 1.0] {
            let offset = vec2(5.0, 5.0) * step * lifting;
            painter.rect(
                card.translate(offset),
                radius,
                carry.fill.gamma_multiply(0.85 - 0.2 * step),
                Stroke::new(1.0, palette.outline),
                egui::StrokeKind::Inside,
            );
        }
    }
    painter.add(
        egui::epaint::Shadow {
            offset: [0, (10.0 * lifting) as i8],
            blur: (28.0 * lifting) as u8,
            spread: 0,
            color: Color32::from_black_alpha(
                (if palette.dark { 150.0 } else { 70.0 } * lifting) as u8,
            ),
        }
        .as_shape(card, radius),
    );
    // Glass bubbles let what is under them show through; the card is solid,
    // so the rows it passes over do not read through it.
    if carry.fill.a() > 0 {
        painter.rect_filled(card, radius, palette.window);
    }
    // A long message is cut at the card's height rather than shrunk further.
    let clip = Rect::from_min_size(card.min, vec2(card.width(), card.height().min(CARD.y)))
        .expand(2.0)
        .intersect(ctx.content_rect());
    let clipped = painter.with_clip_rect(clip);
    for shape in carry.shapes.iter() {
        let mut shape = shape.clone();
        shape.transform(transform);
        clipped.add(shape);
    }
    if count > 1 {
        let badge = pos2(card.right() - 2.0, card.top() + 2.0);
        painter.circle(
            badge,
            12.0,
            palette.accent,
            Stroke::new(2.0, palette.window),
        );
        painter.text(
            badge,
            Align2::CENTER_CENTER,
            count.to_string(),
            theme::bold(12.0),
            palette.on_accent,
        );
    }
    if let (Phase::Lifted { .. }, Some(target)) = (carry.phase, over) {
        label(&ui, &palette, card, target);
    }
}

/// The pill under the card naming where it goes, or that it cannot.
fn label(ui: &egui::Ui, palette: &Palette, card: Rect, target: &Target) {
    let painter = ui.painter();
    let (icon, text, colour) = if target.accepts {
        (
            Icon::Forward,
            format!("Forward to {}", target.title),
            palette.text,
        )
    } else {
        (
            Icon::Ban,
            format!("{} is read-only", target.title),
            palette.secondary,
        )
    };
    let galley = painter.layout(text, theme::semibold(13.0), colour, 260.0);
    let height = 30.0;
    let width = 14.0 + 16.0 + 6.0 + galley.size().x + 14.0;
    let screen = ui.ctx().content_rect();
    let left = (card.center().x - width / 2.0).clamp(
        screen.left() + 8.0,
        (screen.right() - width - 8.0).max(screen.left()),
    );
    let pill = Rect::from_min_size(pos2(left, card.bottom() + 10.0), vec2(width, height));
    painter.add(palette.float_shadow().as_shape(pill, height / 2.0));
    painter.rect(
        pill,
        height / 2.0,
        palette.overlay,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
    let icon_rect = Rect::from_center_size(
        pos2(pill.left() + 14.0 + 8.0, pill.center().y),
        Vec2::splat(16.0),
    );
    icon.image(
        if target.accepts {
            palette.accent
        } else {
            palette.dim
        },
        16.0,
    )
    .paint_at(ui, icon_rect);
    painter.galley(
        pos2(
            icon_rect.right() + 6.0,
            pill.center().y - galley.size().y / 2.0,
        ),
        galley,
        colour,
    );
}

fn ease_out(t: f32) -> f32 {
    theme::ease_out(t)
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Demo and tests: a carry held over the chat the fake pointer rests on,
/// for screenshots of what moving pictures cannot show.
#[cfg(any(test, feature = "demo"))]
fn demo(app: &mut App, ctx: &egui::Context) {
    let Some(message) = app.demo_carry.clone() else {
        return;
    };
    let Some(chat) = app.open_chat.clone() else {
        return;
    };
    let current = load(ctx);
    let bubble = ctx.data(|data| {
        data.get_temp::<Rect>(super::conversation::bubble_id(&chat, &message).with("rect"))
    });
    match (current, bubble) {
        (None, Some(bubble)) => {
            // Pressed first, so the bubble paints itself into the card.
            let now = ctx.input(|input| input.time);
            let press = bubble.center();
            store(
                ctx,
                Some(Carry {
                    chat,
                    messages: vec![message.clone()],
                    anchor: message,
                    origin: bubble,
                    laid: bubble,
                    shapes: Arc::default(),
                    painted: None,
                    fill: app.palette.bubble_in,
                    files: Vec::new(),
                    press,
                    down: press,
                    pressed_at: now,
                    phase: Phase::Pressed,
                    at: press,
                    scale: 1.0,
                    moved: now,
                }),
            );
            ctx.request_repaint();
        }
        (Some(mut carry), _) if carry.phase == Phase::Pressed && !carry.shapes.is_empty() => {
            carry.press = carry.origin.left_center() + vec2(36.0, 0.0);
            carry.phase = Phase::Lifted {
                at: ctx.input(|input| input.time) - LIFT_SECONDS,
            };
            carry.at = ctx
                .input(|input| input.pointer.latest_pos())
                .unwrap_or(carry.at);
            store(ctx, Some(carry));
        }
        (Some(mut carry), _) => {
            // Held where the fake pointer is, fully lifted.
            if let Some(pointer) = ctx.input(|input| input.pointer.latest_pos()) {
                carry.at = pointer;
            }
            if let Phase::Lifted { .. } = carry.phase {
                carry.phase = Phase::Lifted {
                    at: ctx.input(|input| input.time) - LIFT_SECONDS,
                };
            }
            store(ctx, Some(carry));
        }
        _ => ctx.request_repaint(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_starts_and_ends_in_place() {
        for ease in [ease_out, ease_in_out] {
            assert_eq!(ease(0.0), 0.0);
            assert!((ease(1.0) - 1.0).abs() < 1e-6);
            assert!((ease(2.0) - 1.0).abs() < 1e-6);
            assert!(ease(0.5) > 0.0 && ease(0.5) < 1.0);
        }
    }
}
