//! The window making room for what opens beside the chat: the info card's
//! column, or the second pane of a split. Each asks under its own id for the
//! width it needs; the window grows by it while it is open and gives back
//! only what it gained when it closes. A maximized or full-screen window
//! cannot grow, and a window manager may refuse (tiling, snapping, the edge
//! of the screen): the layout then gives way instead, and nothing is shrunk
//! that did not grow.

use egui::vec2;

/// How long a window has to grow before the layout stops waiting for it: a
/// window manager that tiles or snaps it may never let it.
pub(super) const GROW_WAIT: f64 = 0.6;

/// Whether the window cannot grow: maximized or full screen.
pub(super) fn fixed_window(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        let viewport = input.viewport();
        viewport.maximized == Some(true) || viewport.fullscreen == Some(true)
    })
}

/// The window growing for one claim: its inner width before, the width
/// asked for, and when.
#[derive(Clone, Copy)]
struct Widened {
    from: f32,
    to: f32,
    at: f64,
}

fn widened(ctx: &egui::Context, id: egui::Id) -> Option<Widened> {
    ctx.data(|data| data.get_temp::<Widened>(id))
}

/// The window's inner width, in points.
pub(super) fn inner_width(ctx: &egui::Context) -> f32 {
    ctx.input(|input| {
        input
            .viewport()
            .inner_rect
            .map_or_else(|| input.content_rect().width(), |rect| rect.width())
    })
}

fn inner_height(ctx: &egui::Context) -> f32 {
    ctx.input(|input| {
        input
            .viewport()
            .inner_rect
            .map_or_else(|| input.content_rect().height(), |rect| rect.height())
    })
}

/// How much wider the window may get on its screen, when the platform says.
fn screen_room(ctx: &egui::Context) -> Option<f32> {
    ctx.input(|input| {
        let viewport = input.viewport();
        let monitor = viewport.monitor_size?;
        let outer = viewport.outer_rect.or(viewport.inner_rect)?;
        Some((monitor.x - outer.width()).max(0.0))
    })
}

/// How much wider the window could still get: none while it is maximized or
/// full screen, and as far as its screen reaches otherwise (without limit
/// where the platform does not say).
pub(super) fn growable(ctx: &egui::Context) -> f32 {
    if fixed_window(ctx) {
        0.0
    } else {
        screen_room(ctx).unwrap_or(f32::INFINITY)
    }
}

/// Whether the window grew by what `id` asked for.
pub(super) fn grown(ctx: &egui::Context, id: egui::Id) -> bool {
    widened(ctx, id).is_some_and(|widened| inner_width(ctx) >= widened.to - 1.0)
}

/// The width the window is still about to gain for `id`, so the layout does
/// not give way for the frames before it has grown.
pub(super) fn room_ahead(ctx: &egui::Context, id: egui::Id) -> f32 {
    let Some(widened) = widened(ctx, id) else {
        return 0.0;
    };
    let now = ctx.input(|input| input.time);
    if now - widened.at > GROW_WAIT {
        return 0.0;
    }
    (widened.to - inner_width(ctx)).max(0.0)
}

/// Grows the window by `by` for `id` while `open`, as far as the screen
/// allows, and shrinks it back by what it gained once `open` is false.
pub(super) fn widen(ctx: &egui::Context, id: egui::Id, by: f32, open: bool) {
    let fixed = fixed_window(ctx);
    let width = inner_width(ctx);
    let height = inner_height(ctx);
    match (open, widened(ctx, id)) {
        (true, None) if !fixed => {
            let by = screen_room(ctx).map_or(by, |room| by.min(room)).round();
            if by < 1.0 {
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(width + by, height)));
            let at = ctx.input(|input| input.time);
            ctx.data_mut(|data| {
                data.insert_temp(
                    id,
                    Widened {
                        from: width,
                        to: width + by,
                        at,
                    },
                )
            });
        }
        (false, Some(widened)) => {
            // Only what it grew by, and only while it is not maximized.
            let gained = (width - widened.from).clamp(0.0, widened.to - widened.from);
            if !fixed && gained > 1.0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(
                    width - gained,
                    height,
                )));
            }
            ctx.data_mut(|data| data.remove::<Widened>(id));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &egui::Context, width: f32, time: f64, run: impl FnMut(&egui::Context)) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(width, 700.0),
            )),
            time: Some(time),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .inner_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(width, 700.0),
        ));
        let mut run = run;
        let mut output = ctx.run_ui(input, |ui| run(ui.ctx()));
        // Headless tests must apply font-atlas updates themselves.
        output.textures_delta.clear();
    }

    #[test]
    fn a_window_gives_back_only_what_it_gained() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("room-test");
        frame(&ctx, 900.0, 0.0, |ctx| widen(ctx, id, 300.0, true));
        frame(&ctx, 900.0, 0.1, |ctx| {
            assert_eq!(room_ahead(ctx, id), 300.0, "waiting to grow");
            assert!(!grown(ctx, id));
        });
        // The window manager let it grow by only 100.
        frame(&ctx, 1000.0, 1.0, |ctx| {
            assert_eq!(room_ahead(ctx, id), 0.0, "it stops waiting");
            widen(ctx, id, 300.0, false);
        });
        frame(&ctx, 1000.0, 1.1, |ctx| assert!(widened(ctx, id).is_none()));
    }
}
