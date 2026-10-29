//! Split view and the pop-out window.
//!
//! The chat panel is one rounded card, or two side by side when a chat has
//! been dragged out of the list into it (or opened beside the open one from a
//! menu). Each pane is a card of its own; the one that is the open chat has an
//! accent outline, and a press anywhere in the other makes it the open chat.
//! A grip in the gap between them resizes them, and double-clicking it evens
//! them out.
//!
//! A pane arrives by growing out of the side it was dropped on, the open
//! chat's pane making way for it, and leaves the same way: while it does, the
//! chat that stays keeps drawing and the one that goes is a faint card with
//! its name, because the transcript of a pane that is shrinking away is not
//! worth laying out again for 200 milliseconds.
//!
//! A chat row is picked up by pressing on it and moving toward the chat
//! panel. A card shaped like the row's chat follows the pointer, the panel
//! lights up with where it would land (a half of it, or the pane it would
//! replace), and letting go there opens it; letting go elsewhere, or Escape,
//! glides the card back into its row. Moving up or down instead is the list
//! scrolling, as before.
//!
//! The pop-out is a window of its own for one chat: its transcript, a reply
//! field and a button that brings it back into the window. It floats above
//! other windows where the platform allows it.

use std::path::PathBuf;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

use super::chats;
use super::conversation::{self, Pane};
use crate::app::{App, Side, split as state};
use crate::model::{Action, Chat, ChatId, Page};
use crate::theme::{self, Icon, Palette};

/// The space between two panes: the gutter around a card.
pub(crate) const GAP: f32 = chats::CARD_GAP;
/// How far a pane that is arriving or leaving slides, in points.
const SLIDE: f32 = 14.0;

// -- the panes -------------------------------------------------------------

/// A pane on its way out of the panel: only its card is drawn, fading.
#[derive(Clone, Debug)]
pub(crate) struct Ghost {
    chat: ChatId,
    title: String,
    side: Side,
    ratio: f32,
}

fn ghost_id() -> egui::Id {
    egui::Id::new("split-ghost")
}

fn open_id() -> egui::Id {
    egui::Id::new("split-open")
}

/// Starts playing a pane that was closed out of the panel: it fades away
/// where it was while the pane that stays grows over the room.
pub(crate) fn leave(ctx: &egui::Context, chat: &str, title: String, side: Side, ratio: f32) {
    if crate::motion::screen_reader_attached(ctx) {
        return;
    }
    // The animation starts from the panel as it was: whole.
    ctx.animate_bool_with_time_and_easing(open_id(), true, 0.0, theme::ease_out);
    ctx.data_mut(|data| {
        data.insert_temp(
            ghost_id(),
            Ghost {
                chat: chat.to_owned(),
                title,
                side,
                ratio,
            },
        );
    });
    ctx.request_repaint();
}

/// How the chat panel is divided in this frame.
pub(super) struct Panes {
    /// The open chat's pane.
    open: Rect,
    /// The other pane when it is all there.
    other: Rect,
    /// The part of it that shows, and how much of the way in it is.
    visible: Rect,
    reveal: f32,
    other_side: Side,
    /// The chat on its way out, when `other` is only a card.
    ghost: Option<Ghost>,
    /// The grip between settled panes.
    divider: Option<Rect>,
}

/// Where the panes go in `area`, the chat panel's card, or `None` when it
/// holds one chat (and nothing is playing out of it).
pub(super) fn layout(app: &App, ctx: &egui::Context, area: Rect) -> Option<Panes> {
    publish_area(ctx, area, app);
    if app.page != Page::Chats || super::narrow(ctx) {
        return None;
    }
    let live = app.split_other().is_some() && state::fits(area.width(), GAP);
    let ghost = ctx.data(|data| data.get_temp::<Ghost>(ghost_id()));
    if !live && ghost.is_none() {
        // Nothing to play: the animation rests at its end, so a split made
        // later starts from nothing.
        ctx.animate_bool_with_time_and_easing(open_id(), false, 0.0, theme::ease_out);
        return None;
    }
    let reveal = theme::animate(ctx, open_id(), live, theme::MOTION_PANEL);
    if live {
        ctx.data_mut(|data| data.remove::<Ghost>(ghost_id()));
    } else if reveal <= 0.0 {
        ctx.data_mut(|data| data.remove::<Ghost>(ghost_id()));
        return None;
    }
    if reveal > 0.0 && reveal < 1.0 {
        ctx.request_repaint();
    }
    let (open_side, ratio) = match (&app.split, &ghost) {
        (Some(split), _) if live => (split.open_side, split.ratio),
        (_, Some(ghost)) => (ghost.side.other(), ghost.ratio),
        _ => return None,
    };
    let ratio = state::clamp_ratio(area.width(), GAP, ratio);
    let (left, right) = state::panes(area, GAP, ratio);
    let (open_final, other, other_side) = match open_side {
        Side::Left => (left, right, Side::Right),
        Side::Right => (right, left, Side::Left),
    };
    let visible = match other_side {
        Side::Right => Rect::from_min_max(
            pos2(other.right() - other.width() * reveal, other.top()),
            other.max,
        ),
        Side::Left => Rect::from_min_max(
            other.min,
            pos2(other.left() + other.width() * reveal, other.bottom()),
        ),
    };
    let divider = (live && reveal >= 1.0).then(|| {
        Rect::from_x_y_ranges(
            (left.right() - GRIP_REACH)..=(right.left() + GRIP_REACH),
            area.y_range(),
        )
    });
    Some(Panes {
        open: lerp_rect(area, open_final, reveal),
        other,
        visible,
        reveal,
        other_side,
        ghost: if live { None } else { ghost },
        divider,
    })
}

fn lerp_rect(from: Rect, to: Rect, t: f32) -> Rect {
    Rect::from_min_max(from.min.lerp(to.min, t), from.max.lerp(to.max, t))
}

/// How far the grip between panes reaches into each pane beside the gap.
const GRIP_REACH: f32 = 3.0;

/// Draws the panes of a split, the other one first: the open chat's pane draws
/// last, so what it leaves for the pointer and the selection wins.
pub(super) fn show(app: &mut App, ui: &mut egui::Ui, panes: Panes, open: Chat) {
    let palette = app.palette;
    let gutter = palette.rail();
    let ctx = ui.ctx().clone();
    let area = ui.max_rect();
    // The gap is the gutter's colour; the strips around the card are painted
    // by the window.
    let (left, right) = match panes.other_side {
        Side::Left => (panes.visible, panes.open),
        Side::Right => (panes.open, panes.visible),
    };
    if right.left() > left.right() {
        ui.painter().rect_filled(
            Rect::from_x_y_ranges(left.right()..=right.left(), area.y_range()),
            0.0,
            gutter,
        );
    }
    let other = match (&panes.ghost, app.split_other().cloned()) {
        (None, Some(chat)) => Some(chat),
        _ => None,
    };
    // The other pane.
    let slide = (1.0 - panes.reveal)
        * SLIDE
        * match panes.other_side {
            Side::Right => 1.0,
            Side::Left => -1.0,
        };
    card_back(app, ui, panes.visible, gutter);
    if let Some(chat) = other {
        let id = chat.id.clone();
        activate_on_press(app, ui, panes.visible, &id);
        let rect = panes.other.translate(vec2(slide, 0.0));
        let visible = panes.visible;
        ui.scope_builder(egui::UiBuilder::new().max_rect(visible), |ui| {
            ui.set_clip_rect(visible.intersect(ui.clip_rect()));
            ui.set_opacity(panes.reveal);
            conversation::show_scoped(app, ui, rect, chat, Pane::Passive);
        });
        card_front(
            app,
            ui,
            panes.visible,
            gutter,
            outline(&ctx, &palette, &id, false),
        );
    } else if let Some(ghost) = &panes.ghost {
        silhouette(app, ui, panes.visible, ghost, panes.reveal);
        card_front(
            app,
            ui,
            panes.visible,
            gutter,
            outline(&ctx, &palette, &ghost.chat, false),
        );
    }
    // The open chat's pane.
    let id = open.id.clone();
    card_back(app, ui, panes.open, gutter);
    conversation::show_scoped(app, ui, panes.open, open, Pane::Active);
    card_front(
        app,
        ui,
        panes.open,
        gutter,
        outline(&ctx, &palette, &id, true),
    );
    if let Some(grip) = panes.divider {
        divider(app, ui, grip, area);
    }
}

/// The accent on the open chat's pane eases in and out as focus moves.
fn outline(ctx: &egui::Context, palette: &Palette, chat: &str, active: bool) -> Stroke {
    let focus = theme::animate(
        ctx,
        egui::Id::new(("split-focus", chat)),
        active,
        theme::MOTION_QUICK,
    );
    let colour = palette
        .outline
        .lerp_to_gamma(palette.accent.gamma_multiply(0.9), focus);
    Stroke::new(1.25 + 0.5 * focus, colour)
}

/// A press anywhere in the other pane makes its chat the open one, queued
/// ahead of whatever the press itself starts, so every action a view takes
/// after it means this chat.
fn activate_on_press(app: &mut App, ui: &egui::Ui, pane: Rect, chat: &str) {
    let pressed = ui.input(|input| {
        input
            .pointer
            .primary_pressed()
            .then(|| input.pointer.press_origin())
            .flatten()
    });
    let Some(origin) = pressed.filter(|origin| pane.contains(*origin)) else {
        return;
    };
    // Not through a menu, a picker or a dialog above the panel.
    let covered = ui
        .ctx()
        .layer_id_at(origin)
        .is_some_and(|layer| layer.order > egui::Order::Middle);
    if !covered {
        app.actions.insert(0, Action::FocusSplit(chat.to_owned()));
    }
}

/// What a pane leaving the panel shows while it fades: the chat's picture and
/// name, centred on its card.
fn silhouette(app: &mut App, ui: &mut egui::Ui, rect: Rect, ghost: &Ghost, fade: f32) {
    let palette = app.palette;
    let picture = app.avatar(&ghost.chat);
    let centre = rect.center();
    let avatar = Rect::from_center_size(centre - vec2(0.0, 18.0), Vec2::splat(56.0));
    let painter = ui.painter().clone();
    let tint = |colour: Color32| colour.gamma_multiply(fade);
    let galley = painter.layout(
        ghost.title.clone(),
        theme::semibold(15.0),
        tint(palette.text),
        (rect.width() - 48.0).max(40.0),
    );
    let child = ui.new_child(egui::UiBuilder::new().max_rect(avatar));
    let mut child = child;
    child.set_opacity(fade);
    super::widgets::paint_avatar(
        &child,
        &palette,
        avatar,
        &ghost.title,
        &ghost.chat,
        picture.as_deref(),
    );
    painter.galley(
        pos2(centre.x - galley.size().x / 2.0, avatar.bottom() + 12.0),
        galley,
        tint(palette.text),
    );
}

/// The card behind a pane, over the gutter.
fn card_back(app: &App, ui: &egui::Ui, rect: Rect, gutter: Color32) {
    super::card_back(app, ui, rect, gutter);
}

/// The corners and the outline over a pane.
fn card_front(app: &App, ui: &egui::Ui, rect: Rect, gutter: Color32, outline: Stroke) {
    super::card_front(app, ui, rect, gutter, outline);
}

/// The grip between settled panes: dragging it moves the split, double-
/// clicking evens it out. A quiet line at rest, accented under the pointer.
fn divider(app: &mut App, ui: &mut egui::Ui, grip: Rect, area: Rect) {
    let palette = app.palette;
    let id = egui::Id::new("split-grip");
    let response = ui
        .interact(grip, id, Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let held = response.hovered() || response.dragged();
    let near = theme::animate(ui.ctx(), id.with("near"), held, theme::MOTION_QUICK);
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if let Some(split) = app.split.as_mut() {
        let room = (area.width() - GAP).max(1.0);
        if response.dragged() {
            let moved = response.drag_delta().x / room;
            split.ratio = state::clamp_ratio(area.width(), GAP, split.ratio + moved);
        }
        if response.double_clicked() {
            split.ratio = state::DEFAULT_RATIO;
        }
    }
    let centre = grip.center();
    let height = 44.0 + 14.0 * near;
    let line = Rect::from_center_size(centre, vec2(4.0 + 1.0 * near, height));
    let colour = palette
        .text
        .gamma_multiply(0.18)
        .lerp_to_gamma(palette.accent, near);
    ui.painter().rect_filled(line, 2.5, colour);
    if near > 0.0 {
        ui.ctx().request_repaint();
    }
}

// -- dragging a chat out of the list ---------------------------------------

/// How far the pointer moves before a pressed row is a drag of the chat.
const LIFT_DISTANCE: f32 = 10.0;
/// The card's size.
const CHIP: Vec2 = vec2(236.0, 52.0);
const LIFT_SECONDS: f64 = theme::MOTION_PANEL as f64;
const LAND_SECONDS: f64 = theme::MOTION_SCREEN as f64;
const RETURN_SECONDS: f64 = theme::MOTION_SCREEN as f64;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// The button went down on the row; not yet a drag.
    Pressed,
    /// Following the pointer, from `at`.
    Lifted { at: f64 },
    /// Let go over a zone: falling into it.
    Landing { at: f64, to: Pos2, zone: usize },
    /// Let go elsewhere: gliding back to its row.
    Returning { at: f64 },
}

#[derive(Clone)]
struct Lift {
    chat: ChatId,
    title: String,
    picture: Option<PathBuf>,
    /// The row it came from.
    origin: Rect,
    down: Pos2,
    phase: Phase,
    /// The card's centre, trailing the pointer a little.
    at: Pos2,
    moved: f64,
}

fn lift_id() -> egui::Id {
    egui::Id::new("split-lift")
}

fn zones_id() -> egui::Id {
    egui::Id::new("split-zones")
}

fn area_id() -> egui::Id {
    egui::Id::new("split-area")
}

fn load(ctx: &egui::Context) -> Option<Lift> {
    ctx.data(|data| data.get_temp::<Lift>(lift_id()))
}

fn store(ctx: &egui::Context, lift: Option<Lift>) {
    ctx.data_mut(|data| match lift {
        Some(lift) => {
            data.insert_temp(lift_id(), lift);
        }
        None => {
            data.remove::<Lift>(lift_id());
        }
    });
}

/// The chat panel's card as the window drew it this frame: where a chat may be
/// dropped, and whether the panel could take two.
#[derive(Clone, Copy)]
struct Area {
    card: Rect,
    /// The pane the open chat has, and the other's, as laid out settled.
    panes: Option<(Rect, Rect)>,
}

pub(super) fn publish_area(ctx: &egui::Context, area: Rect, app: &App) {
    let panes = (app.split_other().is_some() && state::fits(area.width(), GAP)).then(|| {
        let ratio = app
            .split
            .as_ref()
            .map_or(state::DEFAULT_RATIO, |split| split.ratio);
        state::panes(area, GAP, state::clamp_ratio(area.width(), GAP, ratio))
    });
    ctx.data_mut(|data| data.insert_temp(area_id(), Area { card: area, panes }));
}

/// Watches a chat row (or a rail avatar) for a press that may pick it up, and
/// dims it while its card is away. Called after the row is drawn.
pub fn watch(ui: &egui::Ui, rect: Rect, chat: &str, title: impl FnOnce() -> String) {
    let ctx = ui.ctx();
    let current = load(ctx);
    if let Some(lift) = &current {
        if lift.chat == chat && matches!(lift.phase, Phase::Lifted { .. } | Phase::Landing { .. }) {
            let dark = ui.visuals().dark_mode;
            let veil = if dark {
                Color32::from_black_alpha(70)
            } else {
                Color32::from_white_alpha(130)
            };
            ui.painter()
                .rect_filled(rect.intersect(ui.clip_rect()), 0.0, veil);
        }
        return;
    }
    let pressed = ui.input(|input| {
        input
            .pointer
            .primary_pressed()
            .then(|| input.pointer.press_origin())
            .flatten()
    });
    let Some(origin) = pressed else {
        return;
    };
    if !rect.contains(origin)
        || !ui.clip_rect().contains(origin)
        || ctx
            .layer_id_at(origin)
            .is_some_and(|layer| layer != ui.layer_id())
        || super::carry::carrying(ctx)
    {
        return;
    }
    store(
        ctx,
        Some(Lift {
            chat: chat.to_owned(),
            title: title(),
            picture: None,
            origin: rect,
            down: origin,
            phase: Phase::Pressed,
            at: origin,
            moved: 0.0,
        }),
    );
}

/// Takes Escape from the rest of the window while a card is carried: it
/// sends the card back. Called before the keyboard shortcuts.
pub fn begin(ctx: &egui::Context) {
    let Some(mut lift) = load(ctx) else {
        return;
    };
    if matches!(lift.phase, Phase::Lifted { .. })
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        lift.phase = Phase::Returning {
            at: ctx.input(|input| input.time),
        };
        ctx.stop_dragging();
        store(ctx, Some(lift));
    }
}

/// Where a dropped chat may go.
#[derive(Clone)]
struct Zone {
    rect: Rect,
    /// What dropping there does.
    drop: Drop,
    label: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drop {
    /// Open in a pane on this side, beside the open chat or in its place.
    Pane(Side),
    /// Open in the panel, as a click on the row does.
    Open,
}

/// The zones for dragging `chat` over the panel, in this frame.
fn zones(app: &App, ctx: &egui::Context, chat: &str) -> Vec<Zone> {
    let Some(area) = ctx.data(|data| data.get_temp::<Area>(area_id())) else {
        return Vec::new();
    };
    let locale = app.locale;
    let open = app.current_chat().map(|open| open.id.as_str());
    let tr = |text: &'static str| crate::i18n::gettext(locale, text).into_owned();
    let Some(open) = open else {
        // Nothing is open: the whole panel opens it.
        return vec![Zone {
            rect: area.card,
            drop: Drop::Open,
            label: tr("Open chat"),
        }];
    };
    if let Some((left, right)) = area.panes {
        let name = |side: Side| {
            app.pane_chat(side)
                .map(|id| app.chat_title_of(id))
                .unwrap_or_default()
        };
        return [(Side::Left, left), (Side::Right, right)]
            .into_iter()
            .map(|(side, rect)| Zone {
                rect,
                drop: Drop::Pane(side),
                label: tr("Replace {name}").replace("{name}", &name(side)),
            })
            .collect();
    }
    if open == chat {
        // Already the only chat in the panel: nowhere to take it.
        return Vec::new();
    }
    if !state::fits(area.card.width(), GAP) {
        return vec![Zone {
            rect: area.card,
            drop: Drop::Open,
            label: tr("Open here"),
        }];
    }
    let (left, right) = state::panes(area.card, GAP, state::DEFAULT_RATIO);
    vec![
        Zone {
            rect: left,
            drop: Drop::Pane(Side::Left),
            label: tr("Open on the left"),
        },
        Zone {
            rect: right,
            drop: Drop::Pane(Side::Right),
            label: tr("Open on the right"),
        },
    ]
}

/// Moves the card along a frame and draws it over everything: lifts a press
/// that went toward the panel, lights the zone under it, and opens the chat
/// on a drop. Called last, once the window has drawn.
pub fn drag(app: &mut App, ctx: &egui::Context) {
    let Some(mut lift) = load(ctx) else {
        fade_zones(app, ctx, &[], None, None);
        return;
    };
    let (now, pointer, down) = ctx.input(|input| {
        (
            input.time,
            input.pointer.latest_pos(),
            input.pointer.primary_down(),
        )
    });
    let reduce_motion = crate::motion::screen_reader_attached(ctx);
    match lift.phase {
        Phase::Pressed => {
            let Some(pointer) = pointer.filter(|_| down) else {
                store(ctx, None);
                return;
            };
            let moved = pointer - lift.down;
            if moved.length() < LIFT_DISTANCE {
                store(ctx, Some(lift));
                return;
            }
            // Toward the panel lifts it; any other way is the list scrolling
            // or a click that wandered.
            if moved.x > 0.0 && moved.x > moved.y.abs() * 1.5 {
                lift.picture = app.avatar(&lift.chat);
                lift.phase = Phase::Lifted { at: now };
                lift.at = pointer;
                lift.moved = now;
                // The drag is the card's from here: the list stops following
                // the pointer.
                ctx.set_dragged_id(lift_id());
            } else {
                store(ctx, None);
                return;
            }
        }
        Phase::Landing { at, .. } | Phase::Returning { at }
            if now - at
                >= if matches!(lift.phase, Phase::Landing { .. }) {
                    LAND_SECONDS
                } else {
                    RETURN_SECONDS
                }
                || reduce_motion =>
        {
            let _ = at;
            store(ctx, None);
            fade_zones(app, ctx, &[], None, None);
            return;
        }
        Phase::Lifted { .. } | Phase::Landing { .. } | Phase::Returning { .. } => {}
    }
    let pointer = pointer.unwrap_or(lift.at);
    let zones = zones(app, ctx, &lift.chat);
    let over = zones.iter().position(|zone| zone.rect.contains(pointer));
    if let Phase::Lifted { .. } = lift.phase {
        if !down {
            match over {
                Some(index) => {
                    let zone = &zones[index];
                    app.actions.push(match zone.drop {
                        Drop::Pane(side) => Action::OpenSplit {
                            chat: lift.chat.clone(),
                            side,
                        },
                        Drop::Open => Action::OpenChat(lift.chat.clone()),
                    });
                    lift.phase = Phase::Landing {
                        at: now,
                        to: zone.rect.center(),
                        zone: index,
                    };
                }
                None => lift.phase = Phase::Returning { at: now },
            }
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }
    let lit = match lift.phase {
        Phase::Lifted { .. } => over,
        Phase::Landing { zone, .. } => Some(zone),
        _ => None,
    };
    let shown = !matches!(lift.phase, Phase::Returning { .. } | Phase::Landing { .. });
    fade_zones(app, ctx, &zones, lit, Some(shown));
    paint_chip(app, ctx, &mut lift, pointer, now, reduce_motion);
    store(ctx, Some(lift));
    ctx.request_repaint();
}

/// The zones over the panel, fading in while a card is carried and out after,
/// and the light on the one under it.
fn fade_zones(
    app: &App,
    ctx: &egui::Context,
    zones: &[Zone],
    lit: Option<usize>,
    carried: Option<bool>,
) {
    let appear = theme::animate(ctx, zones_id(), carried == Some(true), theme::MOTION_PANEL);
    // What was drawn last is kept to fade it out once the card is let go.
    let kept = ctx.data(|data| data.get_temp::<Vec<(Rect, String, f32)>>(zones_id().with("kept")));
    let drawn: Vec<(Rect, String, f32)> = if zones.is_empty() {
        kept.unwrap_or_default()
    } else {
        zones
            .iter()
            .enumerate()
            .map(|(index, zone)| {
                let glow = theme::animate(
                    ctx,
                    zones_id().with(("lit", index)),
                    lit == Some(index),
                    theme::MOTION_QUICK,
                );
                (zone.rect, zone.label.clone(), glow)
            })
            .collect()
    };
    if appear <= 0.0 {
        ctx.data_mut(|data| data.remove::<Vec<(Rect, String, f32)>>(zones_id().with("kept")));
        return;
    }
    ctx.data_mut(|data| data.insert_temp(zones_id().with("kept"), drawn.clone()));
    let palette = app.palette;
    let ui = egui::Ui::new(
        ctx.clone(),
        zones_id().with("ui"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("split-zones-layer"),
            ))
            .max_rect(ctx.content_rect()),
    );
    let painter = ui.painter().clone();
    for (rect, label, glow) in drawn {
        let scale = 0.97 + 0.03 * appear;
        let card = Rect::from_center_size(rect.center(), rect.size() * scale).shrink(4.0);
        let radius = f32::from(chats::CARD_RADIUS);
        painter.rect(
            card,
            radius,
            palette.accent.gamma_multiply((0.05 + 0.10 * glow) * appear),
            Stroke::new(
                1.5 + 0.75 * glow,
                palette.accent.gamma_multiply((0.30 + 0.60 * glow) * appear),
            ),
            egui::StrokeKind::Inside,
        );
        paint_label(&ui, &palette, card, &label, appear, glow);
    }
}

/// The pill in a zone naming where the chat would go.
fn paint_label(ui: &egui::Ui, palette: &Palette, card: Rect, label: &str, appear: f32, glow: f32) {
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(
        label.to_owned(),
        theme::semibold(14.0),
        palette.text.gamma_multiply(appear),
    );
    let height = 38.0;
    let width = 16.0 + 18.0 + 8.0 + galley.size().x + 18.0;
    let pill = Rect::from_center_size(
        card.center(),
        vec2(width.min((card.width() - 24.0).max(60.0)), height) * (0.94 + 0.06 * glow),
    );
    painter.add(palette.float_shadow().as_shape(pill, height / 2.0));
    painter.rect(
        pill,
        height / 2.0,
        palette.overlay.gamma_multiply(appear),
        Stroke::new(1.0, palette.outline.gamma_multiply(appear)),
        egui::StrokeKind::Inside,
    );
    let icon = Rect::from_center_size(
        pos2(pill.left() + 16.0 + 9.0, pill.center().y),
        Vec2::splat(18.0),
    );
    theme::paint_icon(
        ui,
        Icon::Columns,
        icon,
        18.0,
        palette.accent.gamma_multiply(appear),
    );
    painter.galley(
        pos2(icon.right() + 8.0, pill.center().y - galley.size().y / 2.0),
        galley,
        palette.text.gamma_multiply(appear),
    );
}

/// Draws the card at the pointer, or on its way into a zone or back home.
fn paint_chip(
    app: &App,
    ctx: &egui::Context,
    lift: &mut Lift,
    pointer: Pos2,
    now: f64,
    reduce_motion: bool,
) {
    let palette = app.palette;
    let mut opacity = 1.0;
    let (centre, scale) = match lift.phase {
        Phase::Lifted { at } => {
            let lifting = if reduce_motion {
                1.0
            } else {
                theme::ease_out(((now - at) / LIFT_SECONDS) as f32)
            };
            // The card trails the pointer by a few frames, so it feels
            // carried rather than glued on.
            let dt = (now - lift.moved).clamp(0.0, 0.1) as f32;
            let step = if reduce_motion {
                1.0
            } else {
                1.0 - (-dt * 28.0).exp()
            };
            lift.at = lift.at.lerp(pointer, step);
            lift.moved = now;
            (lift.at, egui::lerp(0.9..=1.0, lifting))
        }
        Phase::Landing { at, to, .. } => {
            let t = ease_in_out(((now - at) / LAND_SECONDS) as f32);
            opacity = 1.0 - t * t;
            (lift.at.lerp(to, t), egui::lerp(1.0..=0.35, t))
        }
        Phase::Returning { at } => {
            let t = ease_in_out(((now - at) / RETURN_SECONDS) as f32);
            opacity = if t < 0.75 {
                1.0
            } else {
                1.0 - (t - 0.75) / 0.25
            };
            let home = lift.origin.left_center() + vec2(CHIP.x / 2.0, 0.0);
            (lift.at.lerp(home, t), egui::lerp(1.0..=0.9, t))
        }
        Phase::Pressed => return,
    };
    let card = Rect::from_center_size(centre, CHIP * scale);
    let layer = egui::LayerId::new(egui::Order::Tooltip, lift_id());
    let mut ui = egui::Ui::new(
        ctx.clone(),
        lift_id().with("ui"),
        egui::UiBuilder::new()
            .layer_id(layer)
            .max_rect(ctx.content_rect()),
    );
    ui.set_opacity(opacity);
    let painter = ui.painter().clone();
    painter.add(palette.float_shadow().as_shape(card, card.height() / 2.0));
    painter.rect(
        card,
        card.height() / 2.0,
        palette.overlay,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
    let avatar = Rect::from_center_size(
        pos2(card.left() + 8.0 * scale + 18.0 * scale, card.center().y),
        Vec2::splat(36.0 * scale),
    );
    super::widgets::paint_avatar(
        &ui,
        &palette,
        avatar,
        &lift.title,
        &lift.chat,
        lift.picture.as_deref(),
    );
    let name = painter.layout(
        lift.title.clone(),
        theme::semibold(14.0 * scale),
        palette.text,
        (card.right() - avatar.right() - 20.0 * scale).max(20.0),
    );
    painter.galley(
        pos2(
            avatar.right() + 10.0 * scale,
            card.center().y - name.size().y / 2.0,
        ),
        name,
        palette.text,
    );
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

// -- the pop-out window ----------------------------------------------------

/// The window's first size.
const POP_SIZE: Vec2 = vec2(380.0, 560.0);
/// The least it shrinks to.
const POP_MIN: Vec2 = vec2(300.0, 360.0);

/// Draws the pop-out, if there is one, in a window of its own. Called with the
/// window's own frame, so the pop-out redraws with it.
pub fn pop_out(app: &mut App, ctx: &egui::Context) {
    let Some(pop) = app.pop_out.as_ref() else {
        return;
    };
    let Some(chat) = app
        .chat(&pop.chat)
        .filter(|chat| !chat.locked || app.locked_folder_open())
        .cloned()
    else {
        app.actions.push(Action::ClosePopOut);
        return;
    };
    let title = app.chat_title(&chat);
    let builder = egui::ViewportBuilder::default()
        .with_title(title.clone())
        .with_inner_size(POP_SIZE)
        .with_min_inner_size(POP_MIN)
        .with_always_on_top();
    let id = egui::ViewportId::from_hash_of("zeus-pop-out");
    ctx.show_viewport_immediate(id, builder, |ui, class| {
        if class == egui::ViewportClass::Immediate
            && ui.ctx().input(|input| input.viewport().close_requested())
        {
            app.actions.push(Action::ClosePopOut);
            return;
        }
        pop_out_contents(app, ui, &chat, &title);
    });
}

fn pop_out_contents(app: &mut App, ui: &mut egui::Ui, chat: &Chat, title: &str) {
    let palette = app.palette;
    let rect = ui.max_rect();
    // The window has no material behind it: everything is solid.
    let ground = palette.window.to_opaque();
    ui.painter().rect_filled(rect, 0.0, ground);
    let before = app.actions.len();
    egui::Panel::top(ui.id().with("pop-out-bar"))
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(palette.panel.to_opaque())
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(36.0);
                let picture = app.avatar(&chat.id);
                let avatar = Rect::from_min_size(ui.cursor().min, Vec2::splat(32.0));
                ui.allocate_rect(avatar, Sense::hover());
                super::widgets::paint_avatar(
                    ui,
                    &palette,
                    avatar,
                    title,
                    &chat.id,
                    picture.as_deref(),
                );
                ui.add_space(4.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let dock = theme::icon_button(
                        ui,
                        Icon::Maximize,
                        18.0,
                        palette.secondary,
                        palette.text,
                        &crate::i18n::gettext(app.locale, "Open in Zeus"),
                    );
                    if dock.clicked() {
                        app.actions.push(Action::DockPopOut);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        super::widgets::line(
                            ui,
                            title,
                            theme::semibold(14.5),
                            palette.text,
                            (ui.available_width() - 4.0).max(40.0),
                            1,
                        )
                        .paint(
                            ui,
                            ui.cursor().min + vec2(0.0, 8.0),
                            palette.text,
                        );
                    });
                });
            });
        });
    conversation::pop_out_composer(app, ui, chat);
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(ground))
        .show(ui, |ui| {
            conversation::pop_out_transcript(app, ui, chat);
        });
    // Only what names its own message or chat leaves the window; the rest
    // belongs to a composer, a menu or a selection the pop-out does not have.
    let mut index = 0;
    app.actions.retain(|action| {
        let keep = index < before || App::pop_out_allows(action);
        index += 1;
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pane_slides_in_from_the_side_it_is_on() {
        let area = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 600.0));
        let (left, right) = state::panes(area, GAP, 0.5);
        let half = lerp_rect(area, left, 0.5);
        assert!(half.right() > left.right() && half.right() < area.right());
        assert_eq!(lerp_rect(area, right, 1.0), right);
        assert_eq!(lerp_rect(area, right, 0.0), area);
    }

    #[test]
    fn easing_runs_from_nothing_to_whole() {
        assert_eq!(ease_in_out(0.0), 0.0);
        assert!((ease_in_out(1.0) - 1.0).abs() < 1e-6);
        assert!(ease_in_out(0.25) < 0.25);
    }
}

/// Whether the chat panel at `area` draws as a split, or a pane is leaving it.
pub(super) fn active(app: &App, ctx: &egui::Context, area: Rect) -> bool {
    app.page == Page::Chats
        && app.current_chat().is_some()
        && (app.split_other().is_some() && state::fits(area.width(), GAP)
            || ctx.data(|data| data.get_temp::<Ghost>(ghost_id()).is_some()))
}
