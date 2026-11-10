//! Placeholders in the shape of what is loading, under a slow shimmer: the
//! whole window while Zeus starts, before its chats have loaded, and the
//! chat list's rows while the phone sends the first history. Once the
//! account is ready the window fades from the placeholders to the real
//! interface. With a screen reader attached they hold still and leave at
//! once.

use egui::{Align2, Color32, CornerRadius, Painter, Pos2, Rect, Sense, Ui, pos2, vec2};

use crate::app::App;
use crate::theme::{self, Palette};

use super::{chats, rail};

/// Seconds one shimmer takes to cross the window.
const SHIMMER: f64 = 1.8;
/// How far the shimmer's highlight reaches either side of its centre, as a
/// share of the width it crosses.
const BAND: f32 = 0.18;
/// How much brighter a placeholder is at the shimmer's centre.
const LIFT: f32 = 0.7;
/// A row's picture, as the list draws it.
const AVATAR: f32 = 48.0;
/// The list's own inset from its column's left edge.
const LIST_INSET: f32 = 24.0;
/// Below this width the window shows the list alone, as a narrow window does.
const NARROW: f32 = 700.0;

fn id() -> egui::Id {
    egui::Id::new("startup-skeleton")
}

/// One frame of the placeholders' look: how opaque they are (1 while
/// showing, falling to 0 as they leave) and where the shimmer is.
struct Look<'a> {
    palette: &'a Palette,
    opacity: f32,
    /// The shimmer's centre, from 0 at the left edge to 1 at the right;
    /// `None` while it holds still.
    shimmer: Option<f32>,
    width: f32,
    left: f32,
}

impl Look<'_> {
    /// The colour of a placeholder at `at`: the text faintly over its
    /// background, brighter under the shimmer.
    fn bone(&self, at: Pos2) -> Color32 {
        let mut strength = 0.09;
        if let Some(centre) = self.shimmer {
            // Slanted, so the wave runs down and across.
            let x = (at.x - self.left + at.y * 0.25) / self.width.max(1.0);
            let distance = (x - centre) / BAND;
            strength *= 1.0 + LIFT * (-distance * distance).exp();
        }
        self.palette.text.gamma_multiply(strength * self.opacity)
    }

    fn fill(&self, color: Color32) -> Color32 {
        color.gamma_multiply(self.opacity)
    }

    fn bar(&self, painter: &Painter, rect: Rect) {
        painter.rect_filled(
            rect,
            CornerRadius::from(rect.height() / 2.0),
            self.bone(rect.center()),
        );
    }

    fn block(&self, painter: &Painter, rect: Rect, radius: u8) {
        painter.rect_filled(rect, CornerRadius::same(radius), self.bone(rect.center()));
    }

    fn dot(&self, painter: &Painter, centre: Pos2, radius: f32) {
        painter.circle_filled(centre, radius, self.bone(centre));
    }
}

/// The shimmer's position now, and a frame asked for to move it; `None`
/// with a screen reader attached, when it holds still.
fn shimmer(ctx: &egui::Context) -> Option<f32> {
    if crate::motion::screen_reader_attached(ctx) {
        return None;
    }
    crate::motion::request_frame_after(ctx, std::time::Duration::from_millis(33));
    let time = ctx.input(|input| input.time);
    // From just off the left edge to just off the right one.
    Some(((time % SHIMMER) / SHIMMER) as f32 * (1.0 + 2.0 * BAND) - BAND)
}

/// The whole window as placeholders while Zeus starts, `label` saying what
/// it waits for.
pub fn window(app: &App, ui: &mut Ui, label: &str) {
    let ctx = ui.ctx().clone();
    // Shown at once, with nothing to fade in from; the animation is kept
    // running so `fade_out` can take it down from here.
    theme::animate(&ctx, id(), true, theme::MOTION_SCREEN);
    let opacity = 1.0;
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(app.palette.window))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let response = ui.allocate_rect(rect, Sense::hover());
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
            paint_window(ui.painter(), rect, app, opacity, shimmer(&ctx), Some(label));
        });
}

/// Fades the placeholders out over whatever replaced them; nothing once
/// they have gone, or when the window never showed them.
pub fn fade_out(app: &App, ctx: &egui::Context) {
    let opacity = theme::animate(ctx, id(), false, theme::MOTION_SCREEN);
    if opacity <= 0.0 {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, id()));
    let rect = ctx.content_rect();
    painter.rect_filled(rect, 0, app.palette.window.gamma_multiply(opacity));
    paint_window(&painter, rect, app, opacity, shimmer(ctx), None);
}

fn paint_window(
    painter: &Painter,
    rect: Rect,
    app: &App,
    opacity: f32,
    shimmer: Option<f32>,
    label: Option<&str>,
) {
    let palette = &app.palette;
    let look = Look {
        palette,
        opacity,
        shimmer,
        width: rect.width(),
        left: rect.left(),
    };
    let narrow = rect.width() < NARROW;

    // The rail: its two tabs at the top, settings and our picture at the
    // bottom.
    let rail_rect = Rect::from_min_max(rect.min, pos2(rect.left() + rail::WIDTH, rect.bottom()));
    painter.rect_filled(rail_rect, 0, look.fill(palette.rail()));
    let rail_x = rail_rect.center().x;
    for index in 0..2 {
        look.dot(
            painter,
            pos2(rail_x, rect.top() + 40.0 + 48.0 * index as f32),
            14.0,
        );
    }
    for index in 0..2 {
        look.dot(
            painter,
            pos2(rail_x, rect.bottom() - 40.0 - 48.0 * index as f32),
            14.0,
        );
    }

    // The list: its title, search field, filter chips and rows.
    let list_right = if narrow {
        rect.right()
    } else {
        (rail::PANE_WIDTH + app.settings.sidebar_width + rect.left()).min(rect.right())
    };
    let list = Rect::from_min_max(
        pos2(rail_rect.right(), rect.top()),
        pos2(list_right, rect.bottom()),
    );
    painter.rect_filled(list, 0, look.fill(palette.sidebar()));
    let left = list.left() + LIST_INSET;
    let right = list.right() - LIST_INSET;
    let top = rect.top() + chats::HEADER_DROP;
    look.bar(
        painter,
        Rect::from_min_size(pos2(left, top + 16.0), vec2(96.0, 20.0)),
    );
    look.bar(
        painter,
        Rect::from_min_max(pos2(left, top + 64.0), pos2(right, top + 100.0)),
    );
    let mut x = left;
    for width in [40.0, 76.0, 72.0, 84.0] {
        if x + width > right {
            break;
        }
        look.bar(
            painter,
            Rect::from_min_size(pos2(x, top + 116.0), vec2(width, 28.0)),
        );
        x += width + 8.0;
    }
    rows(
        &look,
        painter,
        Rect::from_min_max(pos2(list.left(), top + 160.0), list.max),
    );

    if narrow {
        return;
    }

    // The conversation card: a header, a few bubbles either side, and the
    // composer, with what Zeus waits for in the middle.
    let card = Rect::from_min_max(
        pos2(list.right() + chats::CARD_GAP, rect.top() + chats::CARD_GAP),
        pos2(
            rect.right() - chats::CARD_GAP,
            rect.bottom() - chats::CARD_GAP,
        ),
    );
    if card.width() < 120.0 {
        return;
    }
    painter.rect_filled(
        card,
        CornerRadius::same(chats::CARD_RADIUS),
        look.fill(palette.panel),
    );
    let inner_left = card.left() + 20.0;
    let header = card.top() + chats::HEADER_DROP + 20.0;
    look.dot(painter, pos2(inner_left + 20.0, header + 4.0), 20.0);
    look.bar(
        painter,
        Rect::from_min_size(pos2(inner_left + 52.0, header - 8.0), vec2(140.0, 14.0)),
    );
    look.bar(
        painter,
        Rect::from_min_size(pos2(inner_left + 52.0, header + 12.0), vec2(72.0, 10.0)),
    );
    let composer = Rect::from_min_max(
        pos2(card.left() + 16.0, card.bottom() - 64.0),
        pos2(card.right() - 16.0, card.bottom() - 16.0),
    );
    look.bar(painter, composer);
    let bubbles = [(false, 0.42), (true, 0.30), (false, 0.24), (true, 0.48)];
    let mut y = composer.top() - 24.0;
    for (own, share) in bubbles.iter().rev() {
        let width = (card.width() * share).max(80.0);
        let height = 40.0;
        y -= height;
        if y < header + 60.0 {
            break;
        }
        let x = if *own {
            card.right() - 24.0 - width
        } else {
            card.left() + 24.0
        };
        look.block(
            painter,
            Rect::from_min_size(pos2(x, y), vec2(width, height)),
            super::widgets::BUBBLE_RADIUS,
        );
        y -= 12.0;
    }
    if let Some(label) = label {
        painter.text(
            card.center(),
            Align2::CENTER_CENTER,
            label,
            theme::regular(12.0),
            palette.dim.gamma_multiply(opacity),
        );
    }
}

/// Chat-list rows as placeholders filling `rect`: a picture, a name, a
/// line of preview and a time, at the list's own row height.
fn rows(look: &Look<'_>, painter: &Painter, rect: Rect) {
    let left = rect.left() + LIST_INSET;
    let right = rect.right() - LIST_INSET;
    // Each row is a little shorter than the last, so they read as varied.
    let shares = [0.62, 0.48, 0.70, 0.54, 0.40, 0.66, 0.50, 0.58];
    let mut top = rect.top();
    let mut index = 0;
    while top + theme::ROW_HEIGHT <= rect.bottom() {
        let middle = top + theme::ROW_HEIGHT / 2.0;
        look.dot(painter, pos2(left + AVATAR / 2.0, middle), AVATAR / 2.0);
        let text_left = left + AVATAR + 12.0;
        let room = (right - text_left).max(0.0);
        let share = shares[index % shares.len()];
        look.bar(
            painter,
            Rect::from_min_size(
                pos2(text_left, middle - 14.0),
                vec2(room * share * 0.7, 12.0),
            ),
        );
        look.bar(
            painter,
            Rect::from_min_size(pos2(text_left, middle + 6.0), vec2(room * share, 10.0)),
        );
        look.bar(
            painter,
            Rect::from_min_size(pos2(right - 36.0, middle - 13.0), vec2(36.0, 10.0)),
        );
        top += theme::ROW_HEIGHT;
        index += 1;
    }
}

/// The chat list while the phone sends the first history: placeholder rows
/// under a shimmer, with `label` and `detail` said for a screen reader and
/// shown small beneath them.
pub fn list(ui: &mut Ui, palette: &Palette, label: &str, detail: &str) {
    let rect = ui.available_rect_before_wrap();
    let rows_rect = Rect::from_min_max(
        rect.min,
        pos2(
            rect.right(),
            (rect.top() + theme::ROW_HEIGHT * 6.0).min(rect.bottom()),
        ),
    );
    let response = ui.allocate_rect(rows_rect, Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("{label}. {detail}"))
    });
    let look = Look {
        palette,
        opacity: 1.0,
        shimmer: shimmer(ui.ctx()),
        width: rect.width(),
        left: rect.left(),
    };
    rows(&look, ui.painter(), rows_rect);
    ui.add_space(8.0);
    ui.vertical_centered(|ui| {
        theme::text(ui, detail, theme::regular(12.0), palette.dim);
    });
}
