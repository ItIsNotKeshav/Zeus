//! Zeus's own selection motion and kept selection bar.

use super::*;

/// How the check boxes, the selected rows' bands and the selection bar slide
/// as a selection starts and ends.
pub(super) const SELECT_SLIDE: crate::motion::Spring = crate::motion::Spring::new(0.4, 0.15);
/// Seconds the selection's check boxes, bands and bar take to fade in or out.
pub(super) const SELECT_FADE: f32 = 0.18;
/// A check box ticked pops from this share of its size on [`SELECT_POP`].
pub(super) const SELECT_POP_FROM: f32 = 0.6;
/// The spring a ticked check box pops on.
pub(super) const SELECT_POP: crate::motion::Spring = crate::motion::Spring::new(0.3, 0.25);
/// Seconds a row's band and tick take to fade as it joins or leaves the
/// selection.
pub(super) const SELECT_TICK_FADE: f32 = 0.12;
/// How far below its place the selection bar rises from, and sinks to as
/// it leaves.
pub(super) const SELECT_BAR_RISE: f32 = 14.0;

/// Where a chat's selection furniture (the check column, the selected rows'
/// bands and the bar over the composer) stands as a selection starts or
/// ends. Kept per chat in the context's memory and brought up to date once
/// a pass by [`selection_motion`]; every change of course starts from where
/// it is.
#[derive(Clone, Default)]
pub(super) struct SelectionMotion {
    /// Whether the chat is selecting.
    pub(super) on: bool,
    /// How far in the furniture has slid: 0 out, 1 in, a little past either
    /// on its spring. With its speed, per second.
    pub(super) slide: f32,
    pub(super) velocity: f32,
    /// How opaque the furniture is, and how opaque it was when it last
    /// turned, and when that was.
    pub(super) opacity: f32,
    pub(super) fade_from: f32,
    pub(super) turned: f64,
    /// When this was last brought up to date, and the pass it was.
    pub(super) time: f64,
    pub(super) pass: Option<u64>,
    /// The messages ticked when last seen: while selecting, those ticked;
    /// once it ends, those to draw ticked on their way out.
    pub(super) ticked: Vec<String>,
    /// When each message last joined (`true`) or left the selection, and
    /// how far its tick was in then, for its pop and its band's fade.
    pub(super) marks: HashMap<String, (f64, bool, f32)>,
}

impl SelectionMotion {
    pub(super) fn id(chat: &str) -> egui::Id {
        egui::Id::new(("selection-motion", chat))
    }

    pub(super) fn target(&self) -> f32 {
        if self.on { 1.0 } else { 0.0 }
    }

    /// How opaque the furniture is at `now`, easing from where it turned.
    pub(super) fn opacity_at(&self, now: f64) -> f32 {
        let t = ((now - self.turned) as f32 / SELECT_FADE).clamp(0.0, 1.0);
        if t >= 1.0 {
            return self.target();
        }
        egui::lerp(self.fade_from..=self.target(), theme::ease_out(t))
    }

    /// Whether the check column, bands or bar are still to be drawn while
    /// no selection is open: the selection is leaving.
    pub(super) fn leaving(&self) -> bool {
        !self.on && (self.opacity > 0.0 || self.slide.abs() > 1e-3)
    }

    /// How far the row of a message the check column moves is drawn from
    /// where it is laid out: a row of the other side's slides with the
    /// column, an own row on the right stays where it is.
    pub(super) fn row_shift(&self, from_me: bool) -> f32 {
        if from_me {
            0.0
        } else {
            SELECT_GUTTER * (self.slide - self.target())
        }
    }

    /// Where the check box for a row whose column starts at `left` is
    /// centred, level with `y`: in from the left edge as it slides.
    pub(super) fn check_rect(&self, left: f32, y: f32) -> Rect {
        let x = left + SELECT_BOX / 2.0 - SELECT_GUTTER * (1.0 - self.slide);
        Rect::from_center_size(pos2(x, y), Vec2::splat(SELECT_BOX))
    }

    /// How far a message's tick and band are in at `now`, 0 to 1, when it
    /// is `ticked` or not.
    pub(super) fn tick(&self, id: &str, ticked: bool, now: f64) -> f32 {
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
    pub(super) fn pop(&self, id: &str, now: f64) -> f32 {
        match self.marks.get(id) {
            Some(&(at, true, _)) => {
                SELECT_POP_FROM + (1.0 - SELECT_POP_FROM) * SELECT_POP.value(now - at)
            }
            _ => 1.0,
        }
    }

    /// Whether anything is still moving at `now`.
    pub(super) fn moving(&self, now: f64) -> bool {
        (self.slide - self.target()).abs() > 1e-3
            || self.velocity.abs() > 1e-2
            || (self.opacity - self.target()).abs() > 1e-4
            || self.marks.values().any(|&(at, joined, _)| {
                (now - at) < f64::from(SELECT_TICK_FADE)
                    || (joined && !SELECT_POP.settled(now - at))
            })
    }
}

/// Whether the selection's furniture goes to its place at once: while
/// motion is reduced (as with a screen reader attached), and while the
/// window is in the background.
pub(super) fn selection_calm(ctx: &egui::Context, focused: bool) -> bool {
    !focused || crate::motion::reduced(ctx) || crate::motion::window_resting(ctx)
}

/// Brings `chat`'s selection motion up to `selected` (`None` when it is
/// not selecting) and returns it. The composer and the transcript both ask
/// in a pass; the second finds it done. A chat not drawn in the pass before
/// (another was open, or the window was away) takes what changed meanwhile
/// at once, as does a `calm` one.
pub(super) fn selection_motion(
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
pub(super) struct KeptBar {
    pub(super) chat: String,
    pub(super) shapes: std::sync::Arc<Vec<egui::epaint::ClippedShape>>,
}

pub(super) fn kept_bar_id(bar: bool) -> egui::Id {
    egui::Id::new(("selection-swap", bar))
}

/// Keeps what `layer` holds from `start` on, drawn by `chat`'s composer (or
/// its selection bar when `bar`), for [`paint_kept_bar`]. Frosted glass is a
/// paint callback that cannot fade, so the copy leaves it out.
pub(super) fn keep_bar(
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
pub(super) fn paint_kept_bar(
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
pub(super) fn settle_bar(
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
pub(super) fn fade_shape(shape: &mut egui::Shape, opacity: f32) {
    if opacity < 1.0 {
        let opacity = opacity.max(0.0);
        egui::epaint::shape_transform::adjust_colors(shape, move |colour| {
            if *colour != Color32::PLACEHOLDER {
                *colour = colour.gamma_multiply(opacity);
            }
        });
    }
}
