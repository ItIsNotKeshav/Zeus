//! Zeus's own parts of the transcript: glides, typing, sends and arrivals.

use super::*;

/// Protocol album parent carried by pictures and ordinary videos.
/// Draws the deleted messages leaving from just below the message `above`
/// (`None` for the top of the loaded history). They only paint: their view
/// has no copy rows, menus or selection, and their actions are dropped.
pub(super) fn leaving_rows(
    ui: &mut egui::Ui,
    view: &View<'_>,
    leaving: &[crate::ui::leaving::Leaving],
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
            rows: None,
            ..view.clone()
        };
        let message = &ghost.message;
        let id = bubble_id(&view.chat.id, &message.id).with("leaving");
        crate::ui::leaving::show(
            ui,
            id,
            crate::ui::leaving::progress(ghost, now),
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

/// The colours egui paints a scroll bar's handle in, idle, hovered, and held.
pub(super) fn handle_colors(visuals: &mut egui::Visuals) -> [&mut egui::Color32; 3] {
    let widgets = &mut visuals.widgets;
    [
        &mut widgets.inactive.fg_stroke.color,
        &mut widgets.hovered.fg_stroke.color,
        &mut widgets.active.fg_stroke.color,
    ]
}

/// Whether egui holds a text selection in any label.
pub(super) fn has_text_selection(ctx: &egui::Context) -> bool {
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
pub(super) struct TextEnds {
    /// The rows that may hold an end, by message id.
    pub(super) ids: HashSet<String>,
    /// The rows of messages this pass laid out where an end may have moved.
    pub(super) moved: Vec<String>,
    /// The span of message indices a copy this pass reads.
    pub(super) copy: Option<std::ops::RangeInclusive<usize>>,
    /// A primary press without Shift or another button.
    pub(super) fresh_press: bool,
    /// Any button pressed, held or let go.
    pub(super) busy: bool,
}

impl TextEnds {
    pub(super) fn begin(ui: &egui::Ui, mut ids: HashSet<String>, messages: &[Message]) -> Self {
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
    pub(super) fn holds(&self, index: usize, members: &[Message]) -> bool {
        self.copy.as_ref().is_some_and(|span| span.contains(&index))
            || (!self.ids.is_empty() && members.iter().any(|member| self.ids.contains(&member.id)))
    }

    /// Notes a row this pass laid out; `near` says whether it lies near the
    /// view, where a fresh press can have put both ends.
    pub(super) fn laid_out(&mut self, members: &[Message], near: bool) {
        if self.busy && (near || !self.fresh_press) {
            self.moved
                .extend(members.iter().map(|member| member.id.clone()));
        }
    }

    /// The rows to keep laid out from the next pass on, once every row that
    /// could move an end has been drawn.
    pub(super) fn finish(self, ctx: &egui::Context) -> HashSet<String> {
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

/// The tail still counts as the bottom this far from the content end.
/// `scroll_to_rect` stops a few points short, so a one-point test never sees
/// a reader who is following the newest message.
pub(super) const BOTTOM_SLACK: f32 = 24.0;

/// Each row of the list starts rising for a new message this long after
/// the row below it, as in iMessage, so the list ripples up from the bottom.
pub(super) const WAVE_STEP: f64 = 0.015;
/// Rows this far up and beyond all start together.
pub(super) const WAVE_ROWS: usize = 6;
/// The longest any row waits to start rising.
pub(super) const WAVE_CAP: f64 = WAVE_STEP * WAVE_ROWS as f64;

/// How long a row with `rows_below` rows under it waits before it rises.
pub(super) fn wave_delay(rows_below: usize) -> f64 {
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
pub(super) struct Glide {
    pub(super) newest: Option<String>,
    pub(super) height: f32,
    pub(super) at_bottom: bool,
    pub(super) lifts: Vec<(f64, f32)>,
}

impl Glide {
    pub(super) fn id(chat: &str) -> egui::Id {
        egui::Id::new(("message-glide", chat))
    }

    /// How far below its place a row with `rows_below` rows under it is
    /// drawn at `now`: every lift rises on the sent bubble's own vertical
    /// spring, the row starting [`wave_delay`] after the newest.
    pub(super) fn shift(&self, now: f64, rows_below: usize) -> f32 {
        let delay = wave_delay(rows_below);
        self.lifts
            .iter()
            .map(|&(start, distance)| distance * (1.0 - SEND_RISE.value(now - start - delay)))
            .sum()
    }

    /// Whether any row is still on its way at `now`.
    pub(super) fn moving(&self, now: f64) -> bool {
        self.lifts
            .iter()
            .any(|&(start, _)| !SEND_RISE.settled(now - start - WAVE_CAP))
    }
}

/// Moves `shape` down by what `shift_at` gives for the height of its
/// middle. A list of shapes moves each by its own.
pub(super) fn shift_shape(shape: &mut egui::Shape, shift_at: &impl Fn(f32) -> f32) {
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
pub(super) fn slides_with_transcript(clip: Rect, viewport: Rect) -> bool {
    let along = (clip.top() - viewport.top()).abs() <= 1.0
        && (clip.bottom() - viewport.bottom()).abs() <= 1.0;
    let inside = clip.top() >= viewport.top() - 0.5 && clip.bottom() <= viewport.bottom() + 0.5;
    along || inside
}

/// Message text is painted with the panel clip, which is taller than the
/// viewport, so [`slides_with_transcript`] skips it. The scrollbar shares
/// that clip and sits on the trailing edge.
pub(super) fn shape_follows_transcript(shape: &egui::Shape, viewport: Rect) -> bool {
    let bounds = shape.visual_bounding_rect();
    bounds.intersects(viewport) && bounds.left() < viewport.right() - 18.0
}

/// Lifts the list for a new last message and draws it rising into place:
/// each row a [`wave_delay`] after the row below it, on the sent bubble's
/// vertical spring. `rows` are the tops of the rows laid out this pass, and
/// `layers` the layers they were drawn on, each with its first shape of the
/// pass: the transcript's, and the one a message edited in its bubble is
/// lifted onto.
pub(super) fn glide_new_message<R>(
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
/// The messages of `chat`'s pane. The other pane of a split, and the
/// pop-out, keep their place away from the app's: it is swapped in around the
/// drawing.
pub(super) fn messages_in(
    app: &mut App,
    ui: &mut egui::Ui,
    chat: &Chat,
    under: f32,
    over: f32,
    pane: Pane,
) {
    match pane {
        Pane::Passive => app.with_other_view(|app| messages(app, ui, chat, under, over, pane)),
        Pane::PopOut => app.with_pop_out_view(|app| messages(app, ui, chat, under, over, pane)),
        Pane::Bubble => {
            let id = chat.id.clone();
            app.with_bubble_view(&id, |app| messages(app, ui, chat, under, over, pane))
        }
        Pane::Solo | Pane::Active => messages(app, ui, chat, under, over, pane),
    }
}

/// A padded avatar-and-dots row, without a message bubble or typing label.
pub(super) fn typing_indicator(ui: &mut egui::Ui, view: &View<'_>, typers: &[(String, String)]) {
    let palette = view.palette;
    // Keep the indicator comfortably separated from the last message and
    // composer. Its height stays fixed while the dots animate, so motion
    // never shifts the transcript layout.
    ui.add_space(crate::theme::SPACE_XS);
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
    ui.add_space(4.0);
}

pub(super) const TYPING_BUBBLE_PAD_X: f32 = 14.0;
pub(super) const TYPING_BUBBLE_PAD_Y: f32 = 11.0;

pub(super) fn typing_bubble_id(chat: &str) -> egui::Id {
    egui::Id::new(("typing-bubble", chat))
}

/// Where and when the typing bubble was last drawn, in which pass, and
/// since when it has been up without a break.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TypingSeen {
    pub(super) rect: Rect,
    pub(super) drawn: f64,
    pub(super) pass: u64,
    pub(super) since: f64,
}

impl TypingSeen {
    /// The record for a typing bubble drawn at `rect` at `time`, in `pass`,
    /// after `before`. Drawn in the pass before, or earlier in this one, it
    /// is the same appearance however long ago that was, since a resting
    /// window draws seldom; after a gap it has appeared again.
    pub(super) fn after(before: Option<Self>, rect: Rect, time: f64, pass: u64) -> Self {
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

pub(super) fn send_flight_id() -> egui::Id {
    egui::Id::new("send-flight")
}

/// A sent text on its way from the composer to its bubble: `from` is where
/// its text sat in the composer's field, `field` and `ink` the field's fill
/// and text colour, and `message` the row it became, with the time that row
/// was first drawn.
#[derive(Clone)]
pub(super) struct SendFlight {
    pub(super) chat: String,
    pub(super) from: Rect,
    pub(super) field: Color32,
    pub(super) ink: Color32,
    pub(super) sent: f64,
    /// Unix seconds at the send, to tell its message from older fresh ones.
    pub(super) wall: i64,
    pub(super) message: Option<(String, f64)>,
}

/// The message a sent text is flying to, once its row has been drawn.
#[cfg(test)]
pub(crate) fn flying_to(ctx: &egui::Context) -> Option<String> {
    ctx.data(|data| data.get_temp::<Vec<SendFlight>>(send_flight_id()))
        .and_then(|flights| flights.into_iter().find_map(|flight| flight.message))
        .map(|(id, _)| id)
}

/// How old a message may be when first drawn and still be animated in: one
/// just sent or received, not history or a chat opened later.
pub(super) const ARRIVAL_FRESH: i64 = 3;
/// How long ago the typing bubble may have been drawn for a received
/// message to grow out of it: the frame before, with room for a slow one.
pub(super) const TYPING_HANDOFF: f64 = 0.2;

// iMessage's send and receive, as observed: Apple publishes no values.

/// A sent bubble's way up from the composer, and the list's as it makes
/// room for any new last message: a spring that passes its place a little.
pub(super) const SEND_RISE: crate::motion::Spring = crate::motion::Spring::new(0.5, 0.2);
/// A sent bubble's way across to its column: critically damped, so it never
/// swings past it.
pub(super) const SEND_ACROSS: crate::motion::Spring = crate::motion::Spring::new(0.35, 0.0);
/// How long a sent bubble takes to turn from the composer field's colours
/// to its own.
pub(super) const SEND_TINT_SECONDS: f64 = 0.18;
/// A received bubble pops out of its tail corner on this spring, from
/// [`POP_FROM_SCALE`] of its size; so does the typing bubble.
pub(super) const POP: crate::motion::Spring = crate::motion::Spring::new(0.45, 0.25);
pub(super) const POP_FROM_SCALE: f32 = 0.6;
/// How long a popping bubble takes to become opaque: quickly, so it is seen
/// growing rather than fading.
pub(super) const POP_FADE_SECONDS: f64 = 0.1;
/// How much the typing bubble swells at the top of a breath, and how long
/// one breath takes.
pub(super) const BREATH_SCALE: f32 = 0.03;
pub(super) const BREATH_SECONDS: f64 = 1.8;

/// Whether a sent bubble has reached its place, both ways, `elapsed`
/// seconds after its row was first drawn.
pub(super) fn send_settled(elapsed: f64) -> bool {
    SEND_RISE.settled(elapsed) && SEND_ACROSS.settled(elapsed)
}

/// The size of a popping bubble `elapsed` seconds in: from
/// [`POP_FROM_SCALE`] to a hair past 1, settling on 1.
pub(super) fn pop_scale(elapsed: f64) -> f32 {
    POP_FROM_SCALE + (1.0 - POP_FROM_SCALE) * POP.value(elapsed)
}

/// The opacity of a popping bubble `elapsed` seconds in.
pub(super) fn pop_opacity(elapsed: f64) -> f32 {
    theme::ease_out((elapsed / POP_FADE_SECONDS) as f32)
}

/// The typing bubble's size `elapsed` seconds after it appeared: a slow
/// breath from 1 to `1 + BREATH_SCALE` and back.
pub(super) fn breath_scale(elapsed: f64) -> f32 {
    let phase = (elapsed.max(0.0) * std::f64::consts::TAU / BREATH_SECONDS).cos() as f32;
    1.0 + BREATH_SCALE * 0.5 * (1.0 - phase)
}

/// Scales about `pivot`.
pub(super) fn scale_about(pivot: egui::Pos2, scale: f32) -> egui::emath::TSTransform {
    egui::emath::TSTransform::new(pivot.to_vec2() * (1.0 - scale), scale)
}

/// How long the glide to a jumped-to message takes, in seconds.
pub(super) const JUMP_SCROLL_SECONDS: f32 = 0.35;

/// Plans a jump of `distance` points (positive scrolls down) in a view
/// `viewport` points tall: a target more than two views away is first reached
/// instantly up to one view short of it, and the rest glides. Returns the
/// instant part and the gliding part.
pub(super) fn jump_plan(distance: f32, viewport: f32) -> (f32, f32) {
    if distance.abs() > viewport * 2.0 {
        let instant = distance - distance.signum() * viewport;
        (instant, distance - instant)
    } else {
        (0.0, distance)
    }
}

/// Strong ease-out, `cubic-bezier(0.23, 1, 0.32, 1)`: a jump's glide to a
/// quoted message.
pub(super) fn ease_out_strong(t: f32) -> f32 {
    cubic_bezier(t, 0.23, 1.0, 0.32, 1.0)
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at time `t`: Newton's method finds the
/// curve parameter whose x is `t`, and its y is the eased value.
pub(super) fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
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

pub(super) fn bezier_sample(t: f32, a: f32, b: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
}

pub(super) fn bezier_slope(t: f32, a: f32, b: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * a + 6.0 * u * t * (b - a) + 3.0 * t * t * (1.0 - b)
}

/// Stiffness and damping for Motion's `{ duration, bounce }` spring. `duration`
/// is milliseconds. Velocity is 0 and mass is 1, as in that config.
pub(super) fn spring_from_duration(duration_ms: f64, bounce: f64) -> (f64, f64) {
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
pub(super) fn spring_unit(t_ms: f64, stiffness: f64, damping: f64) -> f64 {
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
