//! Two-finger touchpad swipes, sideways over a chat in the list or across
//! the open chat. Settings say what each of the four does
//! ([`crate::settings::Swipes`]). The row follows the fingers while they are
//! down and the swipe acts once they let go past [`THRESHOLD`]; short of it
//! the row eases back. A gesture that starts upward or downward stays a
//! scroll.
//!
//! The platforms report a touchpad differently, all as wheel events:
//!
//! - macOS: deltas in points, with the fingers' touch and lift as phases,
//!   then a momentum glide that winit also reports as a start and an end.
//! - Wayland: deltas in points, a phase on the first event and an end when
//!   the fingers lift.
//! - Windows precision touchpads: fractions of a wheel notch (in lines), and
//!   no phases at all; Windows keeps sending a fading glide after the lift.
//!   Older touchpad drivers send whole notches.
//! - X11: fractions of a notch from smooth scrolling, no phases.
//!
//! So lines count as well as points, at the step a wheel notch scrolls, and
//! where nothing says when the fingers lift a pause in the events stands for
//! it. Whatever follows a swipe (the glide) is swallowed until the events go
//! quiet or the fingers touch again.

use egui::layers::ShapeIdx;
use egui::{self, Context, Id, Pos2, Rect, Vec2, pos2, vec2};

use crate::app::App;
use crate::backend::LinkStatus;
use crate::model::{Action, Chat, Dialog};
use crate::settings::{SwipeAction, SwipeSlot};
use crate::theme::{self, Icon, Palette};

/// How far the fingers travel, in points, before letting go acts.
pub const THRESHOLD: f32 = 90.0;
/// Travel before the gesture picks an axis, so the first jitter of the
/// fingers does not decide it.
const SLOP: f32 = 6.0;
/// A gesture this much wider than tall goes sideways, as fastframe-scroll
/// decides its axis lock.
const SIDEWAYS: f32 = 1.2;
/// A pause this long between events is the fingers lifting, where the
/// platform does not say (Windows, X11). Windows' glide after a lift keeps
/// events coming more often than this.
const QUIET: f64 = 0.25;
/// Where the platform reports lifts, a gesture that went this long without
/// one (a lost end event) ends anyway.
const STRANDED: f64 = 0.8;
/// After a swipe acts, events closer together than this are its glide.
const TAIL_GAP: f64 = 0.12;
/// A touch reported this long after the lift is the fingers coming down
/// again; macOS starts the glide itself with a touch right after the lift.
const FRESH_TOUCH: f64 = 0.08;
/// The most a row follows the fingers.
const MAX_PULL: f32 = 56.0;
/// How long a row takes to ease back.
const EASE_BACK: f32 = 0.18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    Sideways,
    Upright,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    /// Fingers are moving.
    Tracking,
    /// The fingers let go this frame: the one frame a swipe acts in.
    Released,
    /// The glide after a swipe, which belongs to it.
    Tail,
}

/// One frame of wheel input.
#[derive(Clone, Copy, Debug, Default)]
struct Frame {
    /// The movement in points, the way the content would scroll.
    delta: Vec2,
    began: bool,
    lifted: bool,
    /// Whether any of it came in lines rather than points, for the log.
    lines: bool,
    events: u32,
    time: f64,
    pointer: Option<Pos2>,
    /// A modifier is held (a zoom, a sideways scroll) or something covers
    /// the rows.
    blocked: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Gesture {
    stage: Stage,
    travel: Vec2,
    start: Option<Pos2>,
    axis: Option<Axis>,
    began_at: f64,
    /// Input time of the latest event.
    last: f64,
    released_at: f64,
    /// The platform says when fingers touch and lift. Kept across gestures.
    announced: bool,
    events: u32,
    lines: bool,
    /// It started over something that scrolls sideways, which has it.
    claimed: bool,
}

impl Gesture {
    fn step(&mut self, frame: Frame) {
        let moved = frame.delta != Vec2::ZERO;
        self.announced |= frame.began || frame.lifted;
        if self.stage == Stage::Released {
            self.stage = Stage::Tail;
        }
        if self.stage == Stage::Tail {
            let touched = frame.began && frame.time - self.released_at > FRESH_TOUCH;
            if !touched && frame.time - self.last <= TAIL_GAP {
                if moved || frame.lifted {
                    self.last = frame.time;
                }
                return;
            }
            self.reset();
        }
        if moved && !frame.blocked {
            if self.stage == Stage::Idle {
                self.stage = Stage::Tracking;
                self.start = frame.pointer;
                self.began_at = frame.time;
            }
            self.travel += frame.delta;
            self.last = frame.time;
            self.events += frame.events;
            self.lines |= frame.lines;
            if self.axis.is_none() && self.travel.length() >= SLOP {
                self.axis = Some(if self.travel.x.abs() > self.travel.y.abs() * SIDEWAYS {
                    Axis::Sideways
                } else {
                    Axis::Upright
                });
            }
        }
        if self.stage != Stage::Tracking {
            return;
        }
        let idle = frame.time - self.last;
        let lifted = frame.lifted || idle > if self.announced { STRANDED } else { QUIET };
        if !lifted {
            return;
        }
        if self.axis == Some(Axis::Sideways) {
            log::debug!(
                "swipe: {:.0} points sideways ({:.0} upright) in {:.2} s, {} events in {}, {}",
                self.travel.x,
                self.travel.y,
                self.last - self.began_at,
                self.events,
                if self.lines { "lines" } else { "points" },
                if frame.lifted {
                    "lift reported"
                } else {
                    "lift inferred from a pause"
                },
            );
            self.stage = Stage::Released;
            self.released_at = frame.time;
            self.last = frame.time;
        } else {
            self.reset();
        }
    }

    fn reset(&mut self) {
        *self = Self {
            announced: self.announced,
            ..Self::default()
        };
    }

    fn pull(&self, area: Rect) -> Option<Pull> {
        let sideways = !self.claimed
            && self.axis == Some(Axis::Sideways)
            && matches!(self.stage, Stage::Tracking | Stage::Released);
        if !sideways || self.travel.x == 0.0 || !self.start.is_some_and(|at| area.contains(at)) {
            return None;
        }
        let travel = self.travel.x.abs();
        Some(Pull {
            right: self.travel.x > 0.0,
            progress: (travel / THRESHOLD).min(1.0),
            fired: self.stage == Stage::Released && travel >= THRESHOLD,
        })
    }
}

/// A sideways swipe in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pull {
    /// The content moves right: the fingers do, with natural scrolling.
    pub right: bool,
    /// How far toward the threshold, 0 to 1.
    pub progress: f32,
    /// The fingers let go past the threshold this frame.
    pub fired: bool,
}

fn id() -> Id {
    Id::new("trackpad-swipe")
}

/// A wheel event's travel in points. Lines (Windows, X11 and mouse wheels)
/// count at the step a notch scrolls, so a swipe takes the stroke a scroll
/// of the same length would.
fn points(unit: egui::MouseWheelUnit, delta: Vec2, line: f32, page: f32) -> Vec2 {
    match unit {
        egui::MouseWheelUnit::Point => delta,
        egui::MouseWheelUnit::Line => delta * line,
        egui::MouseWheelUnit::Page => delta * page,
    }
}

fn read(ctx: &Context, covered: bool) -> Frame {
    let line = ctx.options(|options| options.input_options.line_scroll_speed);
    let first_pass = ctx.current_pass_index() == 0;
    ctx.input(|input| {
        let mut frame = Frame {
            time: input.time,
            pointer: input.pointer.hover_pos(),
            blocked: covered || input.modifiers.any(),
            ..Frame::default()
        };
        // A pass egui redoes sees the same events again.
        if !first_pass {
            return frame;
        }
        for event in &input.events {
            if let egui::Event::MouseWheel {
                unit, delta, phase, ..
            } = event
            {
                frame.delta += points(*unit, *delta, line, input.content_rect().height());
                frame.began |= *phase == egui::TouchPhase::Start;
                frame.lifted |= matches!(phase, egui::TouchPhase::End | egui::TouchPhase::Cancel);
                frame.lines |= *unit == egui::MouseWheelUnit::Line;
                frame.events += 1;
            }
        }
        frame
    })
}

/// Reads this frame's wheel input. Call once a frame, before anything is
/// drawn; `covered` when a dialog, menu or preview lies over the rows.
pub fn update(ctx: &Context, covered: bool) {
    let frame = read(ctx, covered);
    let mut gesture = ctx
        .data(|data| data.get_temp::<Gesture>(id()))
        .unwrap_or_default();
    gesture.step(frame);
    match gesture.stage {
        Stage::Tracking => {
            // Come back to notice the pause that ends it.
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(QUIET / 2.0));
        }
        Stage::Released | Stage::Tail => ctx.request_repaint(),
        Stage::Idle => {}
    }
    ctx.data_mut(|data| data.insert_temp(id(), gesture));
}

/// Leaves a gesture that started over `rect` to it: for something inside a
/// row that scrolls sideways, such as a carousel. Call each frame it shows.
pub fn yield_to(ctx: &Context, rect: Rect) {
    ctx.data_mut(|data| {
        let gesture = data.get_temp_mut_or_default::<Gesture>(id());
        if gesture.stage == Stage::Tracking && gesture.start.is_some_and(|at| rect.contains(at)) {
            gesture.claimed = true;
        }
    });
}

/// The sideways swipe that started inside `area`, if one is under way.
pub fn over(ctx: &Context, area: Rect) -> Option<Pull> {
    ctx.data(|data| data.get_temp::<Gesture>(id()))
        .unwrap_or_default()
        .pull(area)
}

/// Whether the action does anything to `chat` now; `message` when the
/// fingers started on one, for a reply.
pub fn available(app: &App, action: SwipeAction, chat: &Chat, message: bool) -> bool {
    match action {
        SwipeAction::None => false,
        SwipeAction::Reply => message && chat.can_send(),
        // Channels cannot be favorites, as on the phone.
        SwipeAction::ToggleFavorite => !chat.is_channel(),
        // Deleting reaches the phone, so it waits for a connection.
        SwipeAction::Delete => matches!(app.link, LinkStatus::Connected),
        _ => true,
    }
}

/// What the action does to `chat`, if it is [`available`]. `message` is the
/// message under the fingers, for a reply.
pub fn actions(
    app: &mut App,
    action: SwipeAction,
    chat: &Chat,
    message: Option<&str>,
) -> Vec<Action> {
    if !available(app, action, chat, message.is_some()) {
        return Vec::new();
    }
    let id = chat.id.clone();
    match action {
        SwipeAction::None => Vec::new(),
        SwipeAction::Reply => message
            .map(|message| vec![Action::Reply(message.to_owned())])
            .unwrap_or_default(),
        SwipeAction::OpenChat => vec![Action::OpenChat(id)],
        SwipeAction::CloseChat => vec![Action::CloseChat],
        SwipeAction::NextChat | SwipeAction::PreviousChat => {
            let step = if action == SwipeAction::NextChat {
                1
            } else {
                -1
            };
            let mut actions = Vec::new();
            super::keys::step_chat(app, step, None, &mut actions);
            actions
        }
        SwipeAction::Delete => vec![Action::ShowDialog(Dialog::ConfirmDeleteChat(id))],
        SwipeAction::ToggleRead
        | SwipeAction::TogglePin
        | SwipeAction::ToggleArchive
        | SwipeAction::ToggleMute
        | SwipeAction::ToggleFavorite => {
            let (done, undo, said) = change(app, action, chat);
            let title = app.chat_title(chat);
            let message = format!("{title} {}", crate::i18n::gettext(app.locale, said));
            vec![done, Action::ToastUndo { message, undo }]
        }
    }
}

/// A change to the chat's state: the action, what undoes it, and what to
/// say it did, after the chat's name.
fn change(app: &App, action: SwipeAction, chat: &Chat) -> (Action, Vec<Action>, &'static str) {
    let id = chat.id.clone();
    match action {
        SwipeAction::ToggleRead if chat.looks_unread() => (
            Action::MarkRead(id.clone()),
            vec![Action::MarkUnread(id)],
            "marked as read",
        ),
        SwipeAction::ToggleRead => (
            Action::MarkUnread(id.clone()),
            vec![Action::MarkRead(id)],
            "marked as unread",
        ),
        SwipeAction::TogglePin => (
            Action::SetPinned(id.clone(), !chat.pinned),
            vec![Action::SetPinned(id, chat.pinned)],
            if chat.pinned { "unpinned" } else { "pinned" },
        ),
        SwipeAction::ToggleArchive => {
            let mut undo = vec![Action::SetArchived(id.clone(), chat.archived)];
            // Archiving the open chat closes it; undoing opens it again.
            if app.open_chat.as_deref() == Some(id.as_str()) {
                undo.push(Action::OpenChat(id.clone()));
            }
            (
                Action::SetArchived(id, !chat.archived),
                undo,
                if chat.archived {
                    "unarchived"
                } else {
                    "archived"
                },
            )
        }
        SwipeAction::ToggleMute if chat.muted(crate::util::now()) => (
            Action::SetMuted(id.clone(), None),
            vec![Action::SetMuted(id, chat.muted_until)],
            "unmuted",
        ),
        SwipeAction::ToggleMute => (
            Action::SetMuted(id.clone(), Some(0)),
            vec![Action::SetMuted(id, None)],
            "muted",
        ),
        // The favorite, the one change left.
        _ => (
            Action::SetFavorite(id.clone(), !chat.favorite),
            vec![Action::SetFavorite(id, chat.favorite)],
            if chat.favorite {
                "removed from favorites"
            } else {
                "added to favorites"
            },
        ),
    }
}

/// The picture of what the action will do to `chat`.
pub fn icon(action: SwipeAction, chat: &Chat) -> Icon {
    match action {
        SwipeAction::None | SwipeAction::OpenChat => Icon::MessageCircle,
        SwipeAction::Reply => Icon::Reply,
        SwipeAction::CloseChat => Icon::ArrowLeft,
        SwipeAction::NextChat => Icon::ArrowDown,
        SwipeAction::PreviousChat => Icon::ArrowUp,
        SwipeAction::ToggleRead if chat.looks_unread() => Icon::CheckCheck,
        SwipeAction::ToggleRead => Icon::MessageCircle,
        SwipeAction::TogglePin if chat.pinned => Icon::PinOff,
        SwipeAction::TogglePin => Icon::Pin,
        SwipeAction::ToggleArchive => Icon::Archive,
        SwipeAction::ToggleMute if chat.muted(crate::util::now()) => Icon::Bell,
        SwipeAction::ToggleMute => Icon::BellOff,
        SwipeAction::ToggleFavorite => Icon::Heart,
        SwipeAction::Delete => Icon::Trash,
    }
}

/// The action the settings give a swipe this way, here.
pub fn action(app: &App, in_chat: bool, right: bool) -> SwipeAction {
    app.settings.swipes.get(SwipeSlot::of(in_chat, right))
}

/// Follows the fingers exactly, and eases back to rest once they let go.
/// Remembers which way the row went, to ease back the same way.
fn ease(ctx: &Context, id: Id, pull: Option<Pull>) -> (f32, bool) {
    let right_id = id.with("right");
    match pull {
        Some(pull) => {
            ctx.data_mut(|data| data.insert_temp(right_id, pull.right));
            (
                ctx.animate_value_with_time(id, pull.progress, 0.0),
                pull.right,
            )
        }
        None => {
            let time = if crate::motion::screen_reader_attached(ctx) {
                0.0
            } else {
                EASE_BACK
            };
            let right = ctx
                .data(|data| data.get_temp::<bool>(right_id))
                .unwrap_or(false);
            (ctx.animate_value_with_time(id, 0.0, time), right)
        }
    }
}

/// Where a row's shapes begin, for [`slide_row`].
pub fn mark(ui: &egui::Ui) -> ShapeIdx {
    ui.ctx().graphics(|graphics| {
        graphics
            .get(ui.layer_id())
            .map_or(ShapeIdx(0), |list| list.next_idx())
    })
}

/// Slides everything drawn in this layer since `start` with the fingers,
/// and grows the action's icon in the space it leaves, inside `lane`'s sides
/// and level with its middle. `icon` gives the icon of the action a swipe
/// that way does, or `None` where it does nothing, and then the row stays.
/// Returns how far the row is pulled, 0 to 1, so the caller can hide what
/// should not ride along.
pub fn slide_row(
    ui: &egui::Ui,
    id: Id,
    start: ShapeIdx,
    lane: Rect,
    pull: Option<Pull>,
    icon: impl Fn(bool) -> Option<Icon>,
    palette: &Palette,
) -> f32 {
    let pull = pull.filter(|pull| icon(pull.right).is_some());
    let (eased, right) = ease(ui.ctx(), id, pull);
    if eased <= 0.0 {
        return 0.0;
    }
    let shift = rubber_band(eased);
    let by = if right { shift } else { -shift };
    let layer = ui.layer_id();
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in start.0..list.next_idx().0 {
            list.mutate_shape(ShapeIdx(index), |clipped| {
                clipped.shape.translate(vec2(by, 0.0));
            });
        }
    });
    let x = if right {
        lane.left() + shift / 2.0 + 6.0
    } else {
        lane.right() - shift / 2.0 - 6.0
    };
    if let Some(icon) = icon(right) {
        badge(ui, pos2(x, lane.center().y), 30.0, eased, icon, palette);
    }
    ui.ctx().request_repaint();
    eased
}

/// A badge at the open chat's edge for an action on the whole chat: it
/// slides in from the side the fingers come from, as a browser's back swipe
/// does.
pub fn edge_badge(
    ui: &egui::Ui,
    id: Id,
    area: Rect,
    pull: Option<Pull>,
    icon: impl Fn(bool) -> Option<Icon>,
    palette: &Palette,
) {
    let pull = pull.filter(|pull| icon(pull.right).is_some());
    let (eased, right) = ease(ui.ctx(), id, pull);
    if eased <= 0.0 {
        return;
    }
    let inset = rubber_band(eased) - 8.0;
    let x = if right {
        area.left() + inset
    } else {
        area.right() - inset
    };
    if let Some(icon) = icon(right) {
        badge(ui, pos2(x, area.center().y), 40.0, eased, icon, palette);
    }
    ui.ctx().request_repaint();
}

/// A round badge holding `icon`, growing and filling in with `eased`; full
/// at the threshold, so the reader sees when letting go will act.
fn badge(ui: &egui::Ui, center: Pos2, size: f32, eased: f32, icon: Icon, palette: &Palette) {
    let ready = eased >= 1.0;
    let size = size * eased.sqrt();
    let painter = ui.painter();
    let (fill, ink) = if ready {
        (palette.accent, palette.on_accent)
    } else {
        (palette.surface_active, palette.accent)
    };
    painter.circle_filled(center, size / 2.0, fill.gamma_multiply(eased));
    theme::paint_icon(
        ui,
        icon,
        Rect::from_center_size(center, Vec2::splat(size)),
        16.0 * eased.sqrt(),
        ink.gamma_multiply(eased),
    );
}

/// A rubber band: follows the fingers at first, then resists.
fn rubber_band(progress: f32) -> f32 {
    MAX_PULL * (1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moving(time: f64, dx: f32, dy: f32) -> Frame {
        Frame {
            delta: vec2(dx, dy),
            time,
            pointer: Some(pos2(100.0, 100.0)),
            events: 1,
            ..Frame::default()
        }
    }

    fn quiet(time: f64) -> Frame {
        Frame {
            time,
            ..Frame::default()
        }
    }

    fn area() -> Rect {
        Rect::from_min_size(pos2(0.0, 50.0), vec2(300.0, 100.0))
    }

    /// Windows: fractions of a notch, no phases, a glide after the lift.
    #[test]
    fn windows_swipe_acts_after_the_pause() {
        let mut gesture = Gesture::default();
        for step in 0..10 {
            gesture.step(moving(f64::from(step) * 0.01, -12.0, 0.5));
        }
        let pull = gesture.pull(area()).unwrap();
        assert!(!pull.right && pull.progress >= 1.0 && !pull.fired);
        gesture.step(quiet(0.2));
        assert!(!gesture.pull(area()).unwrap().fired, "too soon for a lift");
        gesture.step(quiet(0.4));
        assert!(gesture.pull(area()).unwrap().fired);
        gesture.step(quiet(0.41));
        assert_eq!(gesture.pull(area()), None, "acts once");
    }

    /// macOS and Wayland: a reported lift acts at once, and the momentum
    /// glide after it, which macOS opens with a touch, is swallowed.
    #[test]
    fn reported_lift_acts_and_swallows_the_glide() {
        let mut gesture = Gesture::default();
        gesture.step(Frame {
            began: true,
            ..moving(0.0, 4.0, 0.0)
        });
        for step in 1..12 {
            gesture.step(moving(f64::from(step) * 0.016, 10.0, -1.0));
        }
        gesture.step(Frame {
            lifted: true,
            ..quiet(0.2)
        });
        let pull = gesture.pull(area()).unwrap();
        assert!(pull.right && pull.fired);
        gesture.step(Frame {
            began: true,
            ..moving(0.21, 30.0, 0.0)
        });
        for step in 0..20 {
            gesture.step(moving(0.22 + f64::from(step) * 0.016, 8.0, 0.0));
        }
        assert_eq!(gesture.pull(area()), None);
        // The fingers come down again: a new swipe.
        gesture.step(Frame {
            began: true,
            ..moving(1.0, 20.0, 0.0)
        });
        assert!(gesture.pull(area()).is_some());
    }

    #[test]
    fn upright_gesture_is_a_scroll() {
        let mut gesture = Gesture::default();
        for step in 0..10 {
            gesture.step(moving(f64::from(step) * 0.01, 3.0, -20.0));
        }
        gesture.step(quiet(0.5));
        assert_eq!(gesture.pull(area()), None);
    }

    #[test]
    fn short_swipe_does_not_act() {
        let mut gesture = Gesture::default();
        for step in 0..4 {
            gesture.step(moving(f64::from(step) * 0.01, 10.0, 0.0));
        }
        gesture.step(quiet(0.5));
        assert!(!gesture.pull(area()).unwrap().fired);
    }

    #[test]
    fn swipe_belongs_to_where_it_started() {
        let mut gesture = Gesture::default();
        for step in 0..10 {
            gesture.step(moving(f64::from(step) * 0.01, 12.0, 0.0));
        }
        let elsewhere = Rect::from_min_size(pos2(400.0, 50.0), vec2(300.0, 100.0));
        assert_eq!(gesture.pull(elsewhere), None);
        assert!(gesture.pull(area()).is_some());
    }

    #[test]
    fn scroller_keeps_its_gesture() {
        let mut gesture = Gesture::default();
        gesture.step(moving(0.0, 12.0, 0.0));
        gesture.claimed = true;
        gesture.step(moving(0.01, 120.0, 0.0));
        assert_eq!(gesture.pull(area()), None);
    }

    #[test]
    fn held_modifier_does_not_swipe() {
        let mut gesture = Gesture::default();
        for step in 0..10 {
            gesture.step(Frame {
                blocked: true,
                ..moving(f64::from(step) * 0.01, 12.0, 0.0)
            });
        }
        assert_eq!(gesture.pull(area()), None);
    }

    #[test]
    fn lines_count_at_the_wheel_step() {
        let windows = points(egui::MouseWheelUnit::Line, vec2(-0.1, 0.0), 120.0, 800.0);
        assert_eq!(windows, vec2(-12.0, 0.0));
        let mac = points(egui::MouseWheelUnit::Point, vec2(5.0, 1.0), 120.0, 800.0);
        assert_eq!(mac, vec2(5.0, 1.0));
    }
}
