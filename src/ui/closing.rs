//! Surfaces that leave the way they came.
//!
//! egui stops drawing a menu or a dialog in the pass it closes, so on its
//! own a surface that eased open vanishes at once. Each open surface here
//! keeps a copy of what it drew in its last pass; once it is gone, [`paint`]
//! plays the copy on, fading and shrinking to [`theme::CLOSE_SCALE`] about
//! the point it grew from, in [`theme::CLOSE`] of its opening time, and then
//! drops it. The copy is only paint: it takes no clicks, hovers or keys.
//!
//! A backdrop over the whole window only fades: it stays where it is.
//! Frosted glass is a paint callback that cannot fade, so the copy leaves
//! it out and closes over the window as it is.

use std::collections::HashMap;

use egui::epaint::ClippedShape;
use egui::{Context, Id, LayerId, Pos2};

use crate::theme;

/// What an open surface drew last, to play as it closes.
#[derive(Clone)]
struct Kept {
    /// Each shape, and whether it is a backdrop, which does not shrink.
    shapes: Vec<(ClippedShape, bool)>,
    pivot: Pos2,
    /// How long it took to open.
    seconds: f32,
    /// How opaque it was in its last pass, when it closed half open.
    opacity: f32,
    /// The pass it was last drawn in.
    pass: u64,
    /// When it closed, once it has.
    closed: Option<f64>,
}

fn kept_id() -> Id {
    Id::new("zeus-closing")
}

/// Fades every shape `layer` holds so far to `opacity`, and scales all but
/// a backdrop by `scale` about `pivot`: one frame of a surface opening.
pub fn settle(ctx: &Context, layer: LayerId, pivot: Pos2, opacity: f32, scale: f32) {
    if opacity >= 1.0 && scale >= 1.0 {
        return;
    }
    let window = ctx.content_rect();
    let transform = about(pivot, scale);
    ctx.graphics_mut(|graphics| {
        let Some(list) = graphics.get_mut(layer) else {
            return;
        };
        for index in 0..list.next_idx().0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                fade(&mut clipped.shape, opacity);
                if scale < 1.0 && !backdrop(clipped, window) {
                    clipped.transform(transform);
                }
            });
        }
    });
}

/// Whether `clipped` covers the whole window: a modal's backdrop.
fn backdrop(clipped: &ClippedShape, window: egui::Rect) -> bool {
    clipped.shape.visual_bounding_rect().contains_rect(window)
}

fn about(pivot: Pos2, scale: f32) -> egui::emath::TSTransform {
    egui::emath::TSTransform::new(pivot.to_vec2() * (1.0 - scale), scale)
}

fn fade(shape: &mut egui::Shape, opacity: f32) {
    if opacity < 1.0 {
        egui::epaint::shape_transform::adjust_colors(shape, move |colour| {
            if *colour != egui::Color32::PLACEHOLDER {
                *colour = colour.gamma_multiply(opacity);
            }
        });
    }
}

/// Keeps what `layer` holds this pass, drawn by a surface that is open and
/// `opacity` of the way in, to play once it closes, shrinking about
/// `pivot`. `seconds` is how long it took to open. Call it after the
/// surface has drawn, and after [`settle`].
pub fn keep(ctx: &Context, layer: LayerId, pivot: Pos2, seconds: f32, opacity: f32) {
    if crate::motion::screen_reader_attached(ctx) {
        return;
    }
    let window = ctx.content_rect();
    let Some(shapes) = ctx.graphics(|graphics| {
        graphics.get(layer).map(|list| {
            list.all_entries()
                .filter(|clipped| {
                    !matches!(clipped.shape, egui::Shape::Callback(_) | egui::Shape::Noop)
                })
                .map(|clipped| (clipped.clone(), backdrop(clipped, window)))
                .collect::<Vec<_>>()
        })
    }) else {
        return;
    };
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|data| {
        let all = data.get_temp_mut_or_default::<HashMap<LayerId, Kept>>(kept_id());
        all.insert(
            layer,
            Kept {
                shapes,
                pivot,
                seconds,
                opacity,
                pass,
                closed: None,
            },
        );
    });
}

/// Eases the surface `layer` holds in as it opens, fading and growing from
/// [`theme::CLOSE_SCALE`] about `pivot` in `seconds`, and keeps it to leave
/// the way it came. Call it once the surface has drawn this pass; it returns
/// how far in the surface is.
pub fn surface(ctx: &Context, layer: LayerId, pivot: Pos2, seconds: f32) -> f32 {
    let pass = ctx.cumulative_pass_nr();
    let id = kept_id().with(layer.id).with("appear");
    let was_open = ctx.data(|data| {
        data.get_temp::<HashMap<LayerId, Kept>>(kept_id())
            .and_then(|all| {
                all.get(&layer)
                    .map(|kept| kept.closed.is_none() && kept.pass + 1 >= pass)
            })
            .unwrap_or(false)
    });
    if !was_open {
        // Opening anew: from the beginning.
        ctx.animate_bool_with_time(id, false, 0.0);
    }
    let appear = theme::animate(ctx, id, true, seconds);
    let scale = theme::CLOSE_SCALE + (1.0 - theme::CLOSE_SCALE) * appear;
    settle(ctx, layer, pivot, appear, scale);
    keep(ctx, layer, pivot, seconds, appear);
    appear
}

/// Forgets what `layer` kept, for a surface closed by something that
/// replaces it at once, so it does not linger.
pub fn forget(ctx: &Context, layer: LayerId) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<HashMap<LayerId, Kept>>(kept_id())
            .remove(&layer);
    });
}

/// Plays the surfaces that have closed since their last pass. Call it once
/// a pass, after everything else has drawn.
pub fn paint(ctx: &Context) {
    let pass = ctx.cumulative_pass_nr();
    let now = ctx.input(|input| input.time);
    let mut playing = Vec::new();
    ctx.data_mut(|data| {
        let all = data.get_temp_mut_or_default::<HashMap<LayerId, Kept>>(kept_id());
        all.retain(|_, kept| {
            if kept.pass >= pass {
                return true;
            }
            let closed = *kept.closed.get_or_insert(now);
            let seconds = kept.seconds * theme::CLOSE;
            let t = if seconds > 0.0 {
                ((now - closed) as f32 / seconds).clamp(0.0, 1.0)
            } else {
                1.0
            };
            if t >= 1.0 {
                return false;
            }
            playing.push((kept.clone(), theme::ease_out(t)));
            true
        });
    });
    for (layer, (kept, t)) in playing.into_iter().enumerate() {
        let opacity = kept.opacity * (1.0 - t);
        let scale = 1.0 - (1.0 - theme::CLOSE_SCALE) * t;
        let transform = about(kept.pivot, scale);
        let ghost = LayerId::new(egui::Order::Foreground, kept_id().with(layer));
        ctx.graphics_mut(|graphics| {
            let list = graphics.entry(ghost);
            for (mut clipped, backdrop) in kept.shapes {
                fade(&mut clipped.shape, opacity);
                if !backdrop {
                    clipped.transform(transform);
                }
                list.add(clipped.clip_rect, clipped.shape);
            }
        });
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(ctx: &Context, time: f64, draw: impl FnMut(&Context)) -> egui::FullOutput {
        let mut draw = draw;
        let mut output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                ..Default::default()
            },
            |ui| {
                draw(ui.ctx());
                paint(ui.ctx());
            },
        );
        output.textures_delta.clear();
        output
    }

    fn surface(ctx: &Context) {
        let layer = LayerId::new(egui::Order::Foreground, Id::new("menu"));
        ctx.layer_painter(layer).rect_filled(
            egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(100.0, 50.0)),
            0.0,
            egui::Color32::WHITE,
        );
        keep(ctx, layer, egui::pos2(10.0, 10.0), theme::MOTION_QUICK, 1.0);
    }

    fn rects(output: &egui::FullOutput) -> Vec<(egui::Rect, egui::Color32)> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some((rect.rect, rect.fill)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_closed_surface_fades_and_shrinks_then_goes() {
        let ctx = Context::default();
        let open = pass(&ctx, 0.0, surface);
        assert_eq!(rects(&open).len(), 1);
        // The pass after it closes still shows it, fainter and smaller.
        pass(&ctx, 0.01, |_| ());
        let closing = pass(
            &ctx,
            0.01 + f64::from(theme::MOTION_QUICK * theme::CLOSE) / 2.0,
            |_| (),
        );
        let [(rect, fill)] = rects(&closing)[..] else {
            panic!("the closing copy is drawn once");
        };
        assert!(fill.a() < 255 && fill.a() > 0, "{fill:?}");
        assert!(rect.width() < 100.0 && rect.width() > 96.0, "{rect:?}");
        assert_eq!(
            rect.min,
            egui::pos2(10.0, 10.0),
            "it shrinks towards its pivot"
        );
        let gone = pass(&ctx, 1.0, |_| ());
        assert!(rects(&gone).is_empty());
        let after = pass(&ctx, 1.1, |_| ());
        assert!(rects(&after).is_empty());
    }

    #[test]
    fn an_open_surface_draws_once() {
        let ctx = Context::default();
        pass(&ctx, 0.0, surface);
        let again = pass(&ctx, 0.016, surface);
        assert_eq!(rects(&again).len(), 1);
    }
}
