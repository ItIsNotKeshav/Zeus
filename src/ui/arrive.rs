//! Motion for surfaces that arrive whole: the setup, the lock card and the
//! splash. Each works on what a surface has already painted, so the surface
//! draws as if still and moves only on screen, never where it takes clicks.
//!
//! - [`shown_for`]: how long a surface has been on screen.
//! - [`cascade`]: shapes come in top to bottom, each by how far down it sits.
//! - [`spring`]: a choice grows under the pointer, gives under a press and
//!   pops when chosen.

use egui::emath::TSTransform;
use egui::epaint::{Shape, shape_transform};
use egui::layers::ShapeIdx;
use egui::{Color32, Id, Rect, Vec2};

use crate::theme;

/// A surface not drawn for longer than this has gone; drawn again, it
/// arrives afresh.
const GONE: f64 = 0.25;

/// Seconds since the surface `id` names appeared, counting from the first
/// frame it was drawn after an absence. Always long past with a screen
/// reader, which wants the surface still.
pub fn shown_for(ctx: &egui::Context, id: Id) -> f32 {
    let now = ctx.input(|input| input.time);
    let (since, last) = ctx
        .data(|data| data.get_temp::<(f64, f64)>(id))
        .unwrap_or((now, f64::NEG_INFINITY));
    let since = if now - last > GONE { now } else { since };
    ctx.data_mut(|data| data.insert_temp(id, (since, now)));
    if crate::motion::reduced(ctx) {
        return f32::INFINITY;
    }
    (now - since) as f32
}

/// The next shape the ui's layer will hold: pass it to the functions here
/// once the shapes to move are painted.
pub fn painted(ui: &egui::Ui) -> ShapeIdx {
    let layer = ui.layer_id();
    ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(ShapeIdx(0), |list| list.next_idx())
    })
}

/// How shapes come in: each moves in from `offset` and fades up from
/// `from_opacity` over `duration`, starting `delay` in plus its share of
/// `spread`, by how far down the area it sits.
#[derive(Clone, Copy)]
pub struct Cascade {
    pub delay: f32,
    pub spread: f32,
    pub duration: f32,
    pub offset: Vec2,
    pub from_opacity: f32,
}

impl Cascade {
    fn end(&self) -> f32 {
        self.delay + self.spread + self.duration
    }
}

/// Moves the shapes painted since `start` as `how` says, `elapsed` seconds
/// in, and asks for the next frame while any is still on its way.
pub fn cascade(ui: &egui::Ui, start: ShapeIdx, area: Rect, elapsed: f32, how: Cascade) {
    if elapsed >= how.end() {
        return;
    }
    ui.ctx().request_repaint();
    let layer = ui.layer_id();
    let height = area.height().max(1.0);
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in start.0..list.next_idx().0 {
            list.mutate_shape(ShapeIdx(index), |clipped| {
                let bounds = clipped.shape.visual_bounding_rect();
                if !bounds.is_finite() {
                    return;
                }
                let depth = ((bounds.top() - area.top()) / height).clamp(0.0, 1.0);
                let progress =
                    ((elapsed - how.delay - depth * how.spread) / how.duration).clamp(0.0, 1.0);
                let t = theme::ease_out(progress);
                if t >= 1.0 {
                    return;
                }
                clipped
                    .shape
                    .transform(TSTransform::from_translation(how.offset * (1.0 - t)));
                fade(
                    &mut clipped.shape,
                    how.from_opacity + (1.0 - how.from_opacity) * t,
                );
            });
        }
    });
}

/// Moves and scales the shapes painted since `start`.
pub fn transform(ui: &egui::Ui, start: ShapeIdx, transform: TSTransform) {
    let layer = ui.layer_id();
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        let end = list.next_idx();
        list.transform_range(start, end, transform);
    });
}

/// Fades the shapes painted since `start` to `opacity`.
pub fn fade_since(ui: &egui::Ui, start: ShapeIdx, opacity: f32) {
    if opacity >= 1.0 {
        return;
    }
    let layer = ui.layer_id();
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        for index in start.0..list.next_idx().0 {
            list.mutate_shape(ShapeIdx(index), |clipped| fade(&mut clipped.shape, opacity));
        }
    });
}

/// Scales about `center` by `scale`, then moves by `offset`.
pub fn about(center: egui::Pos2, scale: f32, offset: Vec2) -> TSTransform {
    TSTransform::new(center.to_vec2() * (1.0 - scale) + offset, scale)
}

fn fade(shape: &mut Shape, opacity: f32) {
    match shape {
        Shape::Text(text) => text.opacity_factor *= opacity,
        Shape::Vec(shapes) => {
            for shape in shapes {
                fade(shape, opacity);
            }
        }
        shape => shape_transform::adjust_colors(shape, move |color| {
            if *color != Color32::PLACEHOLDER {
                *color = color.gamma_multiply(opacity);
            }
        }),
    }
}

/// A choice's give: it grows a touch under the pointer, sinks under a
/// press, and pops once when it becomes the chosen one. Call it after the
/// choice is painted, with where its painting began.
pub fn spring(ui: &egui::Ui, start: ShapeIdx, response: &egui::Response, chosen: bool) {
    let ctx = ui.ctx();
    let hover = theme::animate(
        ctx,
        response.id.with("spring-hover"),
        response.hovered(),
        theme::MOTION_QUICK,
    );
    let press = theme::animate(
        ctx,
        response.id.with("spring-press"),
        response.is_pointer_button_down_on(),
        theme::MOTION_QUICK * 0.6,
    );
    let pop = pop(ctx, response.id.with("spring-pop"), chosen);
    let scale = 1.0 + 0.015 * hover - 0.035 * press + 0.05 * pop;
    if (scale - 1.0).abs() > 0.0005 {
        transform(ui, start, about(response.rect.center(), scale, Vec2::ZERO));
    }
}

/// How long a [`pop`] lasts.
const POP: f64 = 0.36;

/// A bump that rises and settles once each time `on` turns true: 0 at rest,
/// up to about 0.4 a moment after.
pub fn pop(ctx: &egui::Context, id: Id, on: bool) -> f32 {
    let now = ctx.input(|input| input.time);
    let (was, since) = ctx
        .data(|data| data.get_temp::<(bool, f64)>(id))
        .unwrap_or((on, f64::NEG_INFINITY));
    let since = if on && !was { now } else { since };
    ctx.data_mut(|data| data.insert_temp(id, (on, since)));
    if crate::motion::reduced(ctx) {
        return 0.0;
    }
    let progress = ((now - since) / POP) as f32;
    if !(0.0..1.0).contains(&progress) {
        return 0.0;
    }
    ctx.request_repaint();
    (progress * std::f32::consts::PI).sin() * (1.0 - progress)
}
