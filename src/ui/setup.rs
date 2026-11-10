//! The first run, and linking a phone.
//!
//! A new installation opens on a short setup: a welcome, the look (light or
//! dark and a theme), an accent, font and doodles, notifications, privacy,
//! and last the phone link. Beside the choices a small window draws the app
//! as they leave it, so each one shows the moment it is made, and the link
//! step turns that window into the code to scan. Each step slides in from
//! the side it comes from, on the theme's own curve and pace.
//!
//! Once an account has linked (`Settings::setup_done`), an unlinked account
//! (a second one, or after unlinking) opens straight on the link step.

use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Id, Layout, Margin, Pos2, Rect, Sense, Stroke,
    UiBuilder, Vec2, pos2, vec2,
};

use crate::app::App;
use crate::backend::LinkStatus;
use crate::chat_themes::{Accent, ChatTheme, Mode};
use crate::i18n::gettext;
use crate::model::{Action, Delivery, Dialog};
use crate::qr::Qr;
use crate::settings::{DoodleStyle, FontChoice, NotificationSound};
use crate::theme::{self, BubbleStyle, Icon, Palette};
use crate::wallpaper;

use super::arrive::{self, Cascade};
use super::widgets;

/// The steps of the first-run setup, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    Welcome,
    Look,
    Personal,
    Notifications,
    Privacy,
    Link,
}

impl Step {
    pub const ALL: [Step; 6] = [
        Self::Welcome,
        Self::Look,
        Self::Personal,
        Self::Notifications,
        Self::Privacy,
        Self::Link,
    ];

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|step| *step == self)
            .unwrap_or_default()
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1).min(Self::ALL.len() - 1)]
    }

    fn previous(self) -> Self {
        Self::ALL[self.index().saturating_sub(1)]
    }

    /// The step a demo page names, such as `setup-look`; `setup` alone is
    /// the welcome.
    pub fn named(page: &str) -> Option<Self> {
        Some(match page {
            "setup" => Self::Welcome,
            "setup-look" => Self::Look,
            "setup-personal" => Self::Personal,
            "setup-notifications" => Self::Notifications,
            "setup-privacy" => Self::Privacy,
            "setup-link" => Self::Link,
            _ => return None,
        })
    }
}

/// Narrower than this, the choices stand alone, without the window beside
/// them.
const WIDE: f32 = 860.0;
/// The card's largest size.
const CARD_MAX: Vec2 = vec2(1060.0, 700.0);
/// The room between the card's edge and the stage inside it. The stage's
/// radius is the card's less this, so the two curves run together.
const STAGE_INSET: f32 = 8.0;
/// How far a step slides as it comes in.
const SLIDE: f32 = 28.0;
/// How tall the footer's buttons are.
const FOOTER: f32 = 42.0;
/// The ready-made themes offered beside the three looks; Settings has the
/// rest.
const MORE_THEMES: [ChatTheme; 6] = [
    ChatTheme::Catppuccin,
    ChatTheme::TokyoNight,
    ChatTheme::Dracula,
    ChatTheme::Nord,
    ChatTheme::RosePine,
    ChatTheme::Gruvbox,
];

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.window))
        .show(ui, |ui| {
            let first_run = !app.settings.setup_done;
            let step = if first_run {
                app.setup_step
            } else {
                Step::Link
            };
            let area = ui.max_rect();
            // Seconds since the setup opened: the light comes up, the card
            // rises into place, and what is on it follows top to bottom.
            let opened = arrive::shown_for(ui.ctx(), Id::new("setup-entrance"));
            let light = arrive::painted(ui);
            backdrop(ui, &palette, area, true);
            arrive::fade_since(
                ui,
                light,
                theme::ease_out((opened / (theme::MOTION_SCREEN * 2.0)).clamp(0.0, 1.0)),
            );
            keys(app, ui, step, first_run);
            let wide = area.width() >= WIDE;
            let card = card_rect(area, wide);
            let start = arrive::painted(ui);
            paint_card(ui, &palette, card);
            if wide {
                let left = (card.width() * 0.46).clamp(400.0, 480.0);
                let stage = Rect::from_min_max(
                    pos2(card.left() + left, card.top() + STAGE_INSET),
                    card.max - Vec2::splat(STAGE_INSET),
                );
                self::stage(app, ui, stage, step, opened);
                let column = Rect::from_min_max(card.min, pos2(card.left() + left, card.bottom()))
                    .shrink2(vec2(40.0, 36.0));
                self::column(app, ui, column, step, first_run, false, opened);
            } else {
                self::column(app, ui, card.shrink(28.0), step, first_run, true, opened);
            }
            let rise = theme::ease_out((opened / theme::MOTION_SCREEN).clamp(0.0, 1.0));
            if rise < 1.0 {
                arrive::transform(
                    ui,
                    start,
                    arrive::about(
                        card.center(),
                        0.965 + 0.035 * rise,
                        vec2(0.0, 22.0 * (1.0 - rise)),
                    ),
                );
                arrive::fade_since(ui, start, rise);
                ui.ctx().request_repaint();
            }
        });
}

/// The card, centred: wide enough for the choices and the stage beside them,
/// or for the choices alone.
fn card_rect(area: Rect, wide: bool) -> Rect {
    let size = if wide {
        vec2(
            (area.width() - 64.0).min(CARD_MAX.x),
            (area.height() - 48.0).min(CARD_MAX.y),
        )
    } else {
        vec2(
            (area.width() - 24.0).min(560.0),
            (area.height() - 24.0).min(760.0),
        )
    };
    Rect::from_center_size(area.center(), size.max(Vec2::ZERO))
}

fn paint_card(ui: &egui::Ui, palette: &Palette, card: Rect) {
    let radius = f32::from(theme::RADIUS_DIALOG);
    ui.painter()
        .add(palette.modal_shadow().as_shape(card, radius));
    ui.painter().rect(
        card,
        radius,
        palette.panel,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
}

/// The window behind the card: its own colour, lit softly from above by the
/// accent, and from the far corner by the link colour.
pub(super) fn backdrop(ui: &egui::Ui, palette: &Palette, area: Rect, drift: bool) {
    let still = !drift || crate::motion::screen_reader_attached(ui.ctx());
    let time = if still {
        0.0
    } else {
        // Slow enough that a frame every 50 ms is smooth.
        crate::motion::request_frame_after(ui.ctx(), std::time::Duration::from_millis(50));
        ui.input(|input| input.time) as f32
    };
    // Each light sways on its own slow period, so they never line up.
    let sway = |period: f32, phase: f32| (time * std::f32::consts::TAU / period + phase).sin();
    let (w, h) = (area.width(), area.height());
    // Light from above: the accent's own in the dark, and white in the
    // light, where a dark accent would read as a smudge rather than light.
    let top = if palette.dark {
        palette.accent.gamma_multiply(0.13)
    } else {
        Color32::WHITE.gamma_multiply(0.9)
    };
    glow(
        ui.painter(),
        pos2(area.center().x + w * 0.06 * sway(31.0, 0.0), area.top()),
        vec2(w * 0.75, h * 0.7),
        top,
    );
    // Two colours drifting round the corners behind the card: the accent's
    // and its neighbour when it has a hue, and a blue and a violet when it
    // is a grey.
    let (first, second) = if colourful(palette.accent) {
        (
            palette.accent,
            theme::blend(palette.accent, palette.link, 0.6),
        )
    } else {
        (
            Color32::from_rgb(86, 132, 255),
            Color32::from_rgb(168, 112, 255),
        )
    };
    let strength = if palette.dark { 0.11 } else { 0.13 };
    glow(
        ui.painter(),
        pos2(
            area.left() + w * (0.16 + 0.06 * sway(23.0, 1.0)),
            area.bottom() - h * (0.14 + 0.08 * sway(29.0, 2.0)),
        ),
        vec2(w * 0.42, h * 0.55),
        first.gamma_multiply(strength),
    );
    glow(
        ui.painter(),
        pos2(
            area.right() - w * (0.14 + 0.06 * sway(27.0, 3.0)),
            area.top() + h * (0.36 + 0.1 * sway(19.0, 0.5)),
        ),
        vec2(w * 0.38, h * 0.55),
        second.gamma_multiply(strength * 0.85),
    );
}

/// Whether a colour has a hue to speak of, rather than being a grey.
fn colourful(color: Color32) -> bool {
    let [r, g, b, _] = color.to_array();
    r.max(g).max(b) - r.min(g).min(b) > 48
}

/// A soft elliptical glow: `color` at `center`, fading out by `radius`.
pub(super) fn glow(painter: &egui::Painter, center: Pos2, radius: Vec2, color: Color32) {
    const SEGMENTS: u32 = 64;
    const RINGS: u32 = 12;
    let mut mesh = egui::Mesh::default();
    for ring in 0..=RINGS {
        let reach = ring as f32 / RINGS as f32;
        // Flat at the centre and gone at the edge with no step at either, so
        // it reads as light rather than a cone or a disc.
        let keep = 1.0 - reach * reach * (3.0 - 2.0 * reach);
        for segment in 0..SEGMENTS {
            let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(
                center + vec2(angle.cos() * radius.x, angle.sin() * radius.y) * reach,
                color.gamma_multiply(keep),
            );
        }
    }
    for ring in 0..RINGS {
        for segment in 0..SEGMENTS {
            let next = (segment + 1) % SEGMENTS;
            let [a, b] = [ring * SEGMENTS + segment, ring * SEGMENTS + next];
            let [c, d] = [a + SEGMENTS, b + SEGMENTS];
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Enter goes on from any step before the link, unless a control has the
/// keyboard, which Enter presses instead.
fn keys(app: &mut App, ui: &egui::Ui, step: Step, first_run: bool) {
    if !first_run || step == Step::Link || app.dialog.is_some() {
        return;
    }
    if ui.ctx().memory(|memory| memory.focused().is_some()) {
        return;
    }
    if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        app.setup_step = step.next();
    }
}

/// Seconds since the step on screen came in, and the side it came from: 1
/// from the right (forwards), -1 from the left (back). The first step is
/// already in; the setup's own entrance brings it.
fn transition(ctx: &egui::Context, step: Step) -> (f32, f32) {
    let id = Id::new("setup-step-transition");
    let now = ctx.input(|input| input.time);
    let (shown, since, direction) = ctx
        .data(|data| data.get_temp::<(Step, f64, f32)>(id))
        .unwrap_or((step, f64::NEG_INFINITY, 1.0));
    let (since, direction) = if shown == step {
        (since, direction)
    } else if step.index() > shown.index() {
        (now, 1.0)
    } else {
        (now, -1.0)
    };
    ctx.data_mut(|data| data.insert_temp(id, (step, since, direction)));
    if crate::motion::screen_reader_attached(ctx) {
        return (f32::INFINITY, direction);
    }
    ((now - since) as f32, direction)
}

/// The choices: the steps' dots along the top, the step itself, and Back
/// and Continue along the bottom.
fn column(
    app: &mut App,
    ui: &mut egui::Ui,
    column: Rect,
    step: Step,
    first_run: bool,
    narrow: bool,
    opened: f32,
) {
    let everything = arrive::painted(ui);
    let mut top = column.top();
    if first_run {
        let header = Rect::from_min_size(column.min, vec2(column.width(), 24.0));
        progress(app, ui, header, step);
        top = header.bottom() + 24.0;
    }
    let bottom = if first_run {
        column.bottom() - FOOTER - 16.0
    } else {
        column.bottom()
    };
    let body = Rect::from_min_max(pos2(column.left(), top), pos2(column.right(), bottom));
    let (since, direction) = transition(ui.ctx(), step);
    let start = arrive::painted(ui);
    // A little wider than the column, with the room given back inside, so
    // focus rings at its edges are not cut off.
    let room = 4.0;
    let mut content = ui.new_child(
        UiBuilder::new()
            .id_salt("setup-body")
            .max_rect(body.expand2(vec2(room, 0.0)))
            .layout(Layout::top_down(Align::Min)),
    );
    // Nothing past the card's own column while a step slides in.
    content.set_clip_rect(body.expand2(vec2(room + 2.0, 4.0)));
    // Linking on its own has no dots or footer to hang from, so it sits in
    // the middle of the column, by the height it had last frame.
    let height_id = Id::new(("setup-body-height", step.index()));
    let known = (!first_run)
        .then(|| ui.ctx().data(|data| data.get_temp::<f32>(height_id)))
        .flatten();
    egui::ScrollArea::vertical()
        .id_salt(("setup-scroll", step.index()))
        .auto_shrink([false, false])
        .show(&mut content, |ui| {
            if let Some(known) = known {
                ui.add_space(((body.height() - known) / 2.0).max(0.0));
            }
            let height = Frame::new()
                .inner_margin(Margin::symmetric(room as i8, 2))
                .show(ui, |ui| {
                    ui.set_width(body.width());
                    match step {
                        Step::Welcome => welcome(app, ui),
                        Step::Look => look(app, ui),
                        Step::Personal => personal(app, ui),
                        Step::Notifications => notifications(app, ui),
                        Step::Privacy => privacy(app, ui),
                        Step::Link => link(app, ui, narrow, first_run),
                    }
                })
                .response
                .rect
                .height();
            if !first_run && known.is_none_or(|known| (known - height).abs() > 1.0) {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(height_id, height.round()));
                ui.ctx().request_discard("setup body height changed");
            }
        });
    // A new step slides in from the side it comes from, its lines a beat
    // apart from the top down.
    arrive::cascade(
        ui,
        start,
        body,
        since,
        Cascade {
            delay: 0.0,
            spread: 0.12,
            duration: theme::MOTION_SCREEN,
            offset: vec2(direction * SLIDE, 0.0),
            from_opacity: 0.0,
        },
    );
    if first_run {
        let row = Rect::from_min_max(pos2(column.left(), column.bottom() - FOOTER), column.max);
        footer(app, ui, row, step);
    }
    // As the setup opens, the column follows the card up, top to bottom.
    arrive::cascade(
        ui,
        everything,
        column,
        opened,
        Cascade {
            delay: 0.08,
            spread: 0.3,
            duration: theme::MOTION_SCREEN,
            offset: vec2(0.0, 16.0),
            from_opacity: 0.0,
        },
    );
}

/// One dot for each step, the one on screen stretched into a bar, and Skip
/// on the right.
fn progress(app: &mut App, ui: &mut egui::Ui, row: Rect, step: Step) {
    let palette = app.palette;
    let locale = app.locale;
    let ctx = ui.ctx().clone();
    let mut x = row.left();
    for (index, each) in Step::ALL.into_iter().enumerate() {
        let on = theme::animate(
            &ctx,
            Id::new(("setup-dot", index)),
            each == step,
            theme::MOTION_PANEL,
        );
        let width = egui::lerp(6.0..=24.0, on);
        let dot = Rect::from_min_size(pos2(x, row.center().y - 3.0), vec2(width, 6.0));
        let past = palette.accent.gamma_multiply(0.45);
        let rest = if index < step.index() {
            past
        } else {
            palette.surface_active
        };
        ui.painter()
            .rect_filled(dot, 3.0, rest.lerp_to_gamma(palette.accent, on));
        x = dot.right() + 6.0;
    }
    let dots = Rect::from_min_max(row.min, pos2(x, row.bottom()));
    let label = gettext(locale, "Step {current} of {total}")
        .replace("{current}", &(step.index() + 1).to_string())
        .replace("{total}", &Step::ALL.len().to_string());
    ui.interact(dots, Id::new("setup-progress"), Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &label));
    if step != Step::Link {
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                let skip = gettext(locale, "Skip to linking");
                if theme::link(ui, skip.as_ref(), theme::medium(13.0), palette.secondary).clicked()
                {
                    app.setup_step = Step::Link;
                }
            },
        );
    }
}

fn footer(app: &mut App, ui: &mut egui::Ui, row: Rect, step: Step) {
    let palette = app.palette;
    let locale = app.locale;
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(row)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            if step == Step::Welcome {
                // The welcome has no Back: the fine print sits there instead.
                let room = Rect::from_min_max(row.min, pos2(row.right() - 170.0, row.bottom()));
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(room)
                        .layout(Layout::top_down(Align::Min)),
                    |ui| {
                        ui.add_space(5.0);
                        disclaimer(ui, &palette, locale);
                    },
                );
            } else if back_button(ui, &palette, &gettext(locale, "Back")).clicked() {
                app.setup_step = step.previous();
            }
            if step != Step::Link {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if step == Step::Welcome {
                        gettext(locale, "Get started")
                    } else {
                        gettext(locale, "Continue")
                    };
                    if primary_button(ui, &palette, &label, Some(Icon::ChevronRight)).clicked() {
                        app.setup_step = step.next();
                    }
                });
            }
        },
    );
}

/// The way on: a filled pill in the accent, its arrow nudging forward while
/// the pointer is over it.
pub(super) fn primary_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    icon: Option<Icon>,
) -> egui::Response {
    sized_primary_button(ui, palette, label, icon, 0.0)
}

/// [`primary_button`] at least `width` wide, its label centred.
pub(super) fn sized_primary_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    icon: Option<Icon>,
    width: f32,
) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme::semibold(14.0), palette.on_accent);
    let icon_width = if icon.is_some() { 16.0 + 6.0 } else { 0.0 };
    let pad = 22.0;
    let content = galley.size().x + icon_width;
    let size = vec2((content + pad * 2.0).max(width), FOOTER);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if ui.is_rect_visible(rect) {
        let hover = theme::animate(
            ui.ctx(),
            response.id.with("hover"),
            response.hovered(),
            theme::MOTION_QUICK,
        );
        let shape = if response.is_pointer_button_down_on() {
            rect.shrink(1.0)
        } else {
            rect
        };
        ui.painter().rect_filled(
            shape,
            shape.height() / 2.0,
            palette.accent.lerp_to_gamma(palette.accent_hover, hover),
        );
        let text = pos2(
            rect.center().x - content / 2.0,
            rect.center().y - galley.size().y / 2.0,
        );
        let end = text.x + galley.size().x;
        ui.painter().galley(text, galley, palette.on_accent);
        if let Some(icon) = icon {
            let at = pos2(end + 6.0 + 8.0 + 3.0 * hover, rect.center().y);
            theme::paint_icon(
                ui,
                icon,
                Rect::from_center_size(at, Vec2::splat(16.0)),
                16.0,
                palette.on_accent,
            );
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Back: a quiet arrow and word that fill in softly under the pointer.
fn back_button(ui: &mut egui::Ui, palette: &Palette, label: &str) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme::medium(14.0), palette.secondary);
    let size = vec2(14.0 + 16.0 + 4.0 + galley.size().x + 18.0, FOOTER);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if ui.is_rect_visible(rect) {
        let hover = theme::animate(
            ui.ctx(),
            response.id.with("hover"),
            response.hovered(),
            theme::MOTION_QUICK,
        );
        if hover > 0.0 {
            ui.painter().rect_filled(
                rect,
                rect.height() / 2.0,
                palette.surface_hover.gamma_multiply(hover),
            );
        }
        let color = palette.secondary.lerp_to_gamma(palette.text, hover);
        let arrow = pos2(rect.left() + 14.0 + 8.0 - 2.0 * hover, rect.center().y);
        theme::paint_icon(
            ui,
            Icon::ChevronLeft,
            Rect::from_center_size(arrow, Vec2::splat(16.0)),
            16.0,
            color,
        );
        ui.painter().galley(
            pos2(
                rect.left() + 14.0 + 16.0 + 4.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A step's title and the line under it.
fn title(ui: &mut egui::Ui, palette: &Palette, title: &str, subtitle: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(title)
                .font(theme::display(32.0))
                .color(palette.text),
        )
        .wrap()
        .selectable(false),
    );
    ui.add_space(8.0);
    if !subtitle.is_empty() {
        theme::paragraph(ui, subtitle, theme::regular(14.5), palette.secondary);
    }
    ui.add_space(22.0);
}

/// A group of choices' name, and a note under it when there is one.
fn heading(ui: &mut egui::Ui, palette: &Palette, label: &str, note: Option<&str>) {
    theme::text(ui, label, theme::semibold(13.5), palette.text);
    if let Some(note) = note {
        ui.add_space(2.0);
        theme::paragraph(ui, note, theme::regular(12.5), palette.secondary);
    }
    ui.add_space(10.0);
}

/// The accent, washed thin: behind icons and numbers drawn in it.
fn tint(palette: &Palette) -> Color32 {
    palette
        .accent
        .gamma_multiply(if palette.dark { 0.18 } else { 0.12 })
}

/// An icon in the accent on a rounded square of its tint.
fn icon_tile(ui: &mut egui::Ui, palette: &Palette, icon: Icon, side: f32) -> Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    paint_icon_tile(ui, palette, icon, rect);
    rect
}

/// [`icon_tile`] painted into `rect`, which the caller has made room for.
fn paint_icon_tile(ui: &egui::Ui, palette: &Palette, icon: Icon, rect: Rect) {
    let side = rect.width();
    ui.painter()
        .rect_filled(rect, (side * 0.3).round(), tint(palette));
    theme::paint_icon(
        ui,
        icon,
        Rect::from_center_size(rect.center(), Vec2::splat(side * 0.5)),
        side * 0.5,
        palette.accent,
    );
}

fn disclaimer(ui: &mut egui::Ui, palette: &Palette, locale: crate::i18n::Locale) {
    theme::paragraph(
        ui,
        gettext(
            locale,
            "Unofficial client. Using it may be against WhatsApp's terms of service.",
        ),
        theme::regular(11.5),
        palette.dim,
    );
}

fn welcome(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    let (mark, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
    theme::mark(ui, mark.center(), 64.0);
    ui.add_space(22.0);
    title(
        ui,
        &palette,
        &gettext(locale, "Welcome to Zeus"),
        &gettext(
            locale,
            "WhatsApp on your desktop: quick, private, and made to look the way you like.",
        ),
    );
    let features = [
        (
            Icon::Zap,
            "Native and quick",
            "Built in Rust with no browser inside, so it opens at once and stays light.",
        ),
        (
            Icon::ShieldCheck,
            "Private on this computer",
            "Your chats are kept here, encrypted, and go nowhere but WhatsApp.",
        ),
        (
            Icon::Palette,
            "Yours to shape",
            "The next steps pick a look, an accent, a font and the doodles behind your chats.",
        ),
    ];
    for (icon, heading, body) in features {
        ui.horizontal_top(|ui| {
            icon_tile(ui, &palette, icon, 40.0);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                theme::text(
                    ui,
                    gettext(locale, heading),
                    theme::semibold(14.5),
                    palette.text,
                );
                ui.add_space(2.0);
                theme::paragraph(
                    ui,
                    gettext(locale, body),
                    theme::regular(13.0),
                    palette.secondary,
                );
            });
        });
        ui.add_space(18.0);
    }
}

fn look(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    title(
        ui,
        &palette,
        &gettext(locale, "Choose your look"),
        &gettext(locale, "Light or dark, and a theme to wear it in."),
    );
    heading(ui, &palette, &gettext(locale, "Appearance"), None);
    mode_tiles(ui, app);
    ui.add_space(20.0);
    heading(ui, &palette, &gettext(locale, "Theme"), None);
    look_cards(ui, app);
    ui.add_space(12.0);
    theme_chips(ui, app);
    ui.add_space(10.0);
    theme::paragraph(
        ui,
        gettext(
            locale,
            "Change either any time. More themes are in Settings.",
        ),
        theme::regular(12.0),
        palette.dim,
    );
}

/// Light, dark and the system's, each as a small window in the theme on
/// screen; the system's is half of each.
fn mode_tiles(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let theme_now = app.settings.chat_theme;
    let accent = app.settings.accent;
    let custom = app.settings.custom_theme.is_some();
    let width = ui.available_width();
    let gap = 12.0;
    let tile = vec2((width - gap * 2.0) / 3.0, 96.0);
    let (area, _) = ui.allocate_exact_size(vec2(width, tile.y), Sense::hover());
    let radius = f32::from(theme::RADIUS_CARD);
    let r = radius as u8;
    let light = theme_now.palette(false, accent);
    let dark = theme_now.palette(true, accent);
    for (index, (mode, icon)) in [
        (Mode::Light, Icon::Sun),
        (Mode::Dark, Icon::Moon),
        (Mode::System, Icon::Monitor),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::from_min_size(area.min + vec2(index as f32 * (tile.x + gap), 0.0), tile);
        let response = ui.interact(rect, Id::new(("setup-mode", index)), Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, radius);
        let chosen = !custom && app.settings.mode == mode;
        let label = gettext(app.locale, mode.label());
        let start = arrive::painted(ui);
        if ui.is_rect_visible(rect) {
            let t = theme::animate(ui.ctx(), response.id, chosen, theme::MOTION_QUICK);
            let painter = ui.painter_at(rect.expand(4.0));
            let preview = Rect::from_min_size(rect.min, vec2(rect.width(), rect.height() - 32.0));
            let footer = Rect::from_min_max(pos2(rect.left(), preview.bottom()), rect.max);
            let top = CornerRadius {
                nw: r,
                ne: r,
                sw: 0,
                se: 0,
            };
            painter.rect_filled(
                footer,
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: r,
                    se: r,
                },
                palette.surface,
            );
            match mode {
                Mode::Light => mini_window(&painter, preview, &light, top),
                Mode::Dark => mini_window(&painter, preview, &dark, top),
                Mode::System => {
                    let half = preview.center().x;
                    let left = Rect::from_min_max(preview.min, pos2(half, preview.bottom()));
                    let right = Rect::from_min_max(pos2(half, preview.top()), preview.max);
                    mini_window(&painter.with_clip_rect(left), preview, &light, top);
                    mini_window(&painter.with_clip_rect(right), preview, &dark, top);
                }
            }
            let color = if chosen { palette.accent } else { palette.text };
            let galley = painter.layout_no_wrap(label.to_string(), theme::medium(12.5), color);
            let total = 14.0 + 6.0 + galley.size().x;
            let left = footer.center().x - total / 2.0;
            theme::paint_icon(
                ui,
                icon,
                Rect::from_center_size(pos2(left + 7.0, footer.center().y), Vec2::splat(14.0)),
                14.0,
                color,
            );
            painter.galley(
                pos2(left + 20.0, footer.center().y - galley.size().y / 2.0),
                galley,
                color,
            );
            ring(&painter, &palette, rect, radius, response.hovered(), t);
        }
        arrive::spring(ui, start, &response, chosen);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                chosen,
                label.as_ref(),
            )
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.actions.push(Action::RevealThemeFrom(rect.center()));
            app.actions.push(Action::SetMode(mode));
            if custom {
                // A theme file has one mode of its own: leave it for the
                // theme on screen, in the mode asked for, as Settings does.
                let choice = if theme_now.is_look() {
                    app.settings.last_theme
                } else {
                    theme_now
                };
                app.actions.push(Action::SetChatTheme(choice));
            }
        }
    }
}

/// A tile's edge: the outline, lighter under the pointer, and the accent
/// easing in round the chosen one.
fn ring(
    painter: &egui::Painter,
    palette: &Palette,
    rect: Rect,
    radius: f32,
    hovered: bool,
    t: f32,
) {
    let edge = if hovered {
        Stroke::new(1.0, palette.secondary)
    } else {
        Stroke::new(1.0, palette.outline)
    };
    painter.rect_stroke(rect, radius, edge, egui::StrokeKind::Inside);
    if t > 0.0 {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(2.0 + t, palette.accent.gamma_multiply(t)),
            egui::StrokeKind::Inside,
        );
    }
}

/// A window in `colours` in a few strokes: the chat list beside a
/// conversation with a message each way.
fn mini_window(painter: &egui::Painter, rect: Rect, colours: &Palette, corners: CornerRadius) {
    painter.rect_filled(rect, corners, colours.chat);
    let list = Rect::from_min_max(
        rect.min,
        pos2(rect.left() + rect.width() * 0.34, rect.bottom()),
    );
    painter.rect_filled(
        list,
        CornerRadius {
            nw: corners.nw,
            sw: corners.sw,
            ne: 0,
            se: 0,
        },
        colours.panel,
    );
    for row in 0..3 {
        let y = list.top() + 14.0 + row as f32 * 15.0;
        painter.circle_filled(pos2(list.left() + 11.0, y), 4.0, colours.surface_active);
        let bar = Rect::from_min_max(
            pos2(list.left() + 19.0, y - 2.0),
            pos2(list.right() - 6.0, y + 2.0),
        );
        painter.rect_filled(bar, 2.0, colours.secondary.gamma_multiply(0.45));
    }
    let inner =
        Rect::from_min_max(pos2(list.right(), rect.top()), rect.max).shrink2(vec2(9.0, 12.0));
    let line = 10.0;
    let theirs = Rect::from_min_size(inner.min, vec2(inner.width() * 0.62, line));
    painter.rect_filled(theirs, line / 2.0, colours.bubble_in);
    let ours = Rect::from_min_size(
        pos2(inner.right() - inner.width() * 0.5, theirs.bottom() + 6.0),
        vec2(inner.width() * 0.5, line),
    );
    painter.rect_filled(ours, line / 2.0, colours.bubble_out);
    let short = Rect::from_min_size(
        pos2(inner.left(), ours.bottom() + 6.0),
        vec2(inner.width() * 0.36, line),
    );
    painter.rect_filled(short, line / 2.0, colours.bubble_in);
}

/// The three looks as Settings draws them, side by side.
fn look_cards(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let custom = app.settings.custom_theme.is_some();
    let width = ui.available_width();
    let gap = 10.0;
    let card = vec2((width - gap * 2.0) / 3.0, 88.0);
    let (area, _) = ui.allocate_exact_size(vec2(width, card.y), Sense::hover());
    for (index, choice) in ChatTheme::LOOKS.into_iter().enumerate() {
        let rect = Rect::from_min_size(area.min + vec2(index as f32 * (card.x + gap), 0.0), card);
        let response = ui.interact(rect, Id::new(("setup-look", index)), Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, f32::from(theme::RADIUS_CARD));
        let selected = !custom && app.settings.chat_theme == choice;
        let start = arrive::painted(ui);
        if ui.is_rect_visible(rect) {
            super::settings::theme_card(
                ui,
                &palette,
                rect,
                choice.label(),
                &choice.palette(palette.dark, app.settings.accent),
                selected,
                response.hovered(),
            );
        }
        arrive::spring(ui, start, &response, selected);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                selected,
                choice.label(),
            )
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.actions.push(Action::RevealThemeFrom(rect.center()));
            app.actions.push(Action::SetChatTheme(choice));
        }
    }
}

/// A handful of the ready-made themes, as chips with their colours.
fn theme_chips(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let custom = app.settings.custom_theme.is_some();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for choice in MORE_THEMES {
            let colours = choice.palette(palette.dark, Accent::Theme);
            let selected = !custom && app.settings.chat_theme == choice;
            let response = theme_chip(ui, &palette, &colours, choice.label(), selected);
            if response.clicked() {
                app.actions
                    .push(Action::RevealThemeFrom(response.rect.center()));
                app.actions.push(Action::SetChatTheme(choice));
            }
        }
    });
}

fn theme_chip(
    ui: &mut egui::Ui,
    palette: &Palette,
    colours: &Palette,
    label: &str,
    selected: bool,
) -> egui::Response {
    let color = if selected {
        palette.accent
    } else {
        palette.text
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), theme::medium(12.5), color);
    let size = vec2(10.0 + 18.0 + 8.0 + galley.size().x + 14.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let start = arrive::painted(ui);
    if ui.is_rect_visible(rect) {
        let hover = theme::animate(
            ui.ctx(),
            response.id.with("hover"),
            response.hovered(),
            theme::MOTION_QUICK,
        );
        let radius = rect.height() / 2.0;
        let fill = if selected {
            tint(palette)
        } else {
            palette.surface.lerp_to_gamma(palette.surface_hover, hover)
        };
        ui.painter().rect_filled(rect, radius, fill);
        if selected {
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(1.5, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        // The theme's canvas, with its accent set into it.
        let swatch = pos2(rect.left() + 10.0 + 9.0, rect.center().y);
        ui.painter().circle_filled(swatch, 9.0, colours.chat);
        ui.painter()
            .circle_stroke(swatch, 9.0, Stroke::new(1.0, palette.outline));
        ui.painter()
            .circle_filled(swatch + vec2(3.0, 3.0), 4.0, colours.accent);
        ui.painter().galley(
            pos2(
                rect.left() + 10.0 + 18.0 + 8.0,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
    }
    arrive::spring(ui, start, &response, selected);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn personal(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    title(
        ui,
        &palette,
        &gettext(locale, "Make it yours"),
        &gettext(locale, "An accent, a typeface, and doodles for your chats."),
    );
    let chat_theme = app.settings.chat_theme;
    let takes = app.settings.custom_theme.is_none() && chat_theme.takes_accent();
    let note = if takes {
        None
    } else if chat_theme == ChatTheme::Messages {
        Some(gettext(locale, "Messages keeps the system blue."))
    } else {
        Some(gettext(
            locale,
            "This theme keeps its own accent. Zeus and WhatsApp take one.",
        ))
    };
    heading(ui, &palette, &gettext(locale, "Accent"), note.as_deref());
    super::settings::accent_swatches(ui, app, takes);
    ui.add_space(18.0);
    heading(ui, &palette, &gettext(locale, "Font"), None);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for choice in FontChoice::ALL {
            // Font family names stay in their own branding.
            let label = match choice {
                FontChoice::System => gettext(locale, choice.label()),
                _ => choice.label().into(),
            };
            let chosen = app.settings.font == choice;
            let start = arrive::painted(ui);
            let response = theme::soft_button(ui, &palette, None, &label, chosen);
            arrive::spring(ui, start, &response, chosen);
            if response.clicked() && !chosen {
                app.actions.push(Action::SetFont(choice));
            }
        }
    });
    ui.add_space(18.0);
    let plain = palette.bubbles == BubbleStyle::Messages;
    let note = plain.then(|| gettext(locale, "Messages draws its chats without them."));
    heading(ui, &palette, &gettext(locale, "Doodles"), note.as_deref());
    doodle_tiles(ui, app);
}

/// No doodles, then each style over the wallpaper's colour, four to a row.
fn doodle_tiles(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let base = app.wallpaper();
    // Drawn stronger than behind a chat, so a style reads at this size; the
    // dark ink needs more of it to show at all.
    let look = wallpaper::Look {
        density: 1.6,
        intensity: base.intensity * if palette.dark { 2.4 } else { 1.4 },
        ..base
    };
    let shown = app.settings.show_wallpaper;
    let current = app.settings.doodle_style;
    let width = ui.available_width();
    let columns = 4;
    let gap = 10.0;
    let tile = vec2((width - gap * (columns - 1) as f32) / columns as f32, 68.0);
    let count = DoodleStyle::ALL.len() + 1;
    let rows = count.div_ceil(columns);
    let (area, _) = ui.allocate_exact_size(
        vec2(width, rows as f32 * (tile.y + gap) - gap),
        Sense::hover(),
    );
    let radius = 16.0;
    let r = radius as u8;
    for index in 0..count {
        let style = index.checked_sub(1).map(|index| DoodleStyle::ALL[index]);
        let chosen = match style {
            None => !shown,
            Some(style) => shown && style == current,
        };
        let label = match style {
            None => crate::i18n::pgettext(app.locale, "doodles", "None"),
            Some(style) => gettext(app.locale, style.label()),
        };
        let rect = Rect::from_min_size(
            area.min
                + vec2(
                    (index % columns) as f32 * (tile.x + gap),
                    (index / columns) as f32 * (tile.y + gap),
                ),
            tile,
        );
        let response = ui.interact(rect, Id::new(("setup-doodle", index)), Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, radius);
        let start = arrive::painted(ui);
        if ui.is_rect_visible(rect) {
            let t = theme::animate(ui.ctx(), response.id, chosen, theme::MOTION_QUICK);
            let painter = ui.painter_at(rect.expand(4.0));
            let preview = Rect::from_min_size(rect.min, vec2(rect.width(), rect.height() - 26.0));
            let footer = Rect::from_min_max(pos2(rect.left(), preview.bottom()), rect.max);
            painter.rect_filled(
                footer,
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: r,
                    se: r,
                },
                palette.surface,
            );
            painter.rect_filled(
                preview,
                CornerRadius {
                    nw: r,
                    ne: r,
                    sw: 0,
                    se: 0,
                },
                look.color,
            );
            match style {
                Some(style) => wallpaper::paint_doodles(ui, preview, &look, style),
                None => theme::paint_icon(
                    ui,
                    Icon::Ban,
                    Rect::from_center_size(preview.center(), Vec2::splat(20.0)),
                    20.0,
                    palette.secondary,
                ),
            }
            let color = if chosen { palette.accent } else { palette.text };
            let name = widgets::line(
                ui,
                &label,
                theme::medium(12.0),
                color,
                footer.width() - 12.0,
                1,
            );
            name.paint(
                ui,
                pos2(
                    footer.center().x - name.size().x / 2.0,
                    footer.center().y - name.size().y / 2.0,
                ),
                color,
            );
            ring(&painter, &palette, rect, radius, response.hovered(), t);
        }
        arrive::spring(ui, start, &response, chosen);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                chosen,
                label.as_ref(),
            )
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.actions.push(match style {
                None => Action::SetWallpaperDoodles(false),
                Some(style) => Action::SetDoodleStyle(style),
            });
        }
    }
}

/// Rows of switches on one rounded panel, as iOS groups its settings.
fn group<R>(ui: &mut egui::Ui, palette: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .fill(theme::blend(
            palette.panel,
            palette.surface,
            if palette.dark { 0.55 } else { 0.45 },
        ))
        .stroke(Stroke::new(1.0, palette.outline.gamma_multiply(0.7)))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(Margin::symmetric(0, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

/// The hairline between two rows of a group, from under their text.
fn separator(ui: &mut egui::Ui, palette: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.left() + 58.0..=rect.right() - 14.0,
        rect.center().y,
        Stroke::new(1.0, palette.outline.gamma_multiply(0.7)),
    );
}

/// One row of a group: its icon, its name and what it does, and a switch.
/// Returns whether the switch was flipped.
fn switch_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: Icon,
    label: &str,
    description: &str,
    value: &mut bool,
    enabled: bool,
) -> bool {
    let mut changed = false;
    ui.add_enabled_ui(enabled, |ui| {
        let row = ui
            .horizontal(|ui| {
                // The icon's place: it is drawn once the row's height is
                // known, centred on it as the switch is.
                ui.add_space(14.0 + 32.0 + 12.0);
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 40.0 - 28.0).max(100.0));
                    ui.add_space(11.0);
                    widgets::rich_text(ui, label, theme::medium(14.0), palette.text);
                    if !description.is_empty() {
                        ui.add_space(2.0);
                        theme::paragraph(ui, description, theme::regular(12.5), palette.secondary);
                    }
                    ui.add_space(11.0);
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(14.0);
                    let response = widgets::switch(ui, palette, value);
                    theme::reveal_focus(&response);
                    let on = *value;
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Checkbox,
                            ui.is_enabled(),
                            on,
                            label,
                        )
                    });
                    changed = response.changed();
                });
            })
            .response
            .rect;
        let tile = Rect::from_min_size(
            pos2(row.left() + 14.0, row.center().y - 16.0),
            Vec2::splat(32.0),
        );
        paint_icon_tile(ui, palette, icon, tile);
    });
    changed
}

fn notifications(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    title(
        ui,
        &palette,
        &gettext(locale, "Stay in the loop"),
        &gettext(
            locale,
            "How Zeus tells you about new messages, and whether it keeps them coming while the window is closed.",
        ),
    );
    group(ui, &palette, |ui| {
        let mut on = app.account().settings.notifications;
        if switch_row(
            ui,
            &palette,
            Icon::Bell,
            &gettext(locale, "Desktop notifications"),
            &gettext(
                locale,
                "For chats you are not looking at. Muted chats stay quiet.",
            ),
            &mut on,
            true,
        ) {
            app.account_mut().settings.notifications = on;
            app.account_mut().mark_settings_dirty();
        }
        separator(ui, &palette);
        let mut content = app.settings.notification_content;
        if switch_row(
            ui,
            &palette,
            Icon::Eye,
            &gettext(locale, "Show message previews"),
            &gettext(locale, "When off, they say only “New message”."),
            &mut content,
            on,
        ) {
            app.settings.notification_content = content;
            app.actions.push(Action::SettingsChanged);
        }
        separator(ui, &palette);
        let mut sound = app.settings.message_sound != NotificationSound::None;
        if switch_row(
            ui,
            &palette,
            Icon::Volume2,
            &gettext(locale, "Play a sound"),
            &gettext(
                locale,
                "A soft chime for new messages, and another when someone mentions you.",
            ),
            &mut sound,
            on,
        ) {
            (app.settings.message_sound, app.settings.mention_sound) = if sound {
                (NotificationSound::Receive, NotificationSound::Alert)
            } else {
                (NotificationSound::None, NotificationSound::None)
            };
            app.actions.push(Action::SettingsChanged);
        }
        separator(ui, &palette);
        let mut keep = app.settings.keep_running_in_background;
        if switch_row(
            ui,
            &palette,
            Icon::Monitor,
            &gettext(locale, "Keep running when the window closes"),
            &gettext(locale, "Zeus stays in the tray, so messages keep arriving."),
            &mut keep,
            true,
        ) {
            app.settings.keep_running_in_background = keep;
            app.actions.push(Action::SettingsChanged);
        }
        if let Some(mut start) = app.start_with_system {
            separator(ui, &palette);
            if switch_row(
                ui,
                &palette,
                Icon::Clock,
                &gettext(locale, "Start at login"),
                &gettext(locale, "Starts in the tray, without a window."),
                &mut start,
                true,
            ) {
                app.actions.push(Action::SetStartWithSystem(start));
            }
        }
    });
}

fn privacy(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    title(
        ui,
        &palette,
        &gettext(locale, "Private by default"),
        &gettext(
            locale,
            "Decide what other people see. Everything else stays on this computer.",
        ),
    );
    group(ui, &palette, |ui| {
        let mut receipts = app.account().settings.send_read_receipts;
        if switch_row(
            ui,
            &palette,
            Icon::CheckCheck,
            &gettext(locale, "Send read receipts"),
            &gettext(locale, "Let people see when you read their messages."),
            &mut receipts,
            true,
        ) {
            app.account_mut().settings.send_read_receipts = receipts;
            app.account_mut().mark_settings_dirty();
        }
        separator(ui, &palette);
        let mut typing = app.account().settings.send_typing;
        if switch_row(
            ui,
            &palette,
            Icon::Keyboard,
            &gettext(locale, "Show when you are typing"),
            &gettext(locale, "Let people see when you are writing to them."),
            &mut typing,
            true,
        ) {
            app.account_mut().settings.send_typing = typing;
            app.account_mut().mark_settings_dirty();
        }
        separator(ui, &palette);
        let mut download = app.account().settings.auto_download;
        if switch_row(
            ui,
            &palette,
            Icon::Download,
            &gettext(locale, "Download files automatically"),
            &gettext(
                locale,
                "Files up to 64 MiB download as they come into view.",
            ),
            &mut download,
            true,
        ) {
            app.account_mut().settings.auto_download = download;
            app.account_mut().mark_settings_dirty();
        }
    });
    ui.add_space(16.0);
    Frame::new()
        .fill(tint(&palette).gamma_multiply(0.6))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                icon_tile(ui, &palette, Icon::ShieldCheck, 32.0);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(1.0);
                    theme::text(
                        ui,
                        gettext(locale, "Encrypted on this computer"),
                        theme::medium(14.0),
                        palette.text,
                    );
                    ui.add_space(2.0);
                    theme::paragraph(
                        ui,
                        gettext(
                            locale,
                            "Your message archive is encrypted, with its key in your system's keyring. No telemetry, and no Zeus servers.",
                        ),
                        theme::regular(12.5),
                        palette.secondary,
                    );
                });
            });
        });
}

/// What the link step's stage shows, from the link's state.
enum Linking {
    /// The code to scan.
    Qr(String),
    /// The code to type on the phone, and the number it is for.
    PairCode(String, Option<String>),
    /// Waiting, with what for.
    Busy(String),
    /// Linking stopped with this message.
    Failed(String),
}

fn linking(app: &App) -> Linking {
    let locale = app.locale;
    match app.link.clone() {
        LinkStatus::Unlinked {
            pair_code: Some(code),
            pairing_phone,
            ..
        } => Linking::PairCode(code, pairing_phone),
        LinkStatus::Unlinked {
            pairing_phone: Some(phone),
            ..
        } => Linking::Busy(
            gettext(locale, "Requesting a code for +{phone}…").replace("{phone}", &phone),
        ),
        LinkStatus::Unlinked { qr: Some(qr), .. } => Linking::Qr(qr),
        LinkStatus::Unlinked { .. } => {
            Linking::Busy(gettext(locale, "Waiting for a code from WhatsApp…").into_owned())
        }
        LinkStatus::LoggedOut => {
            Linking::Busy(gettext(locale, "Requesting a new code…").into_owned())
        }
        LinkStatus::Failed(message) => Linking::Failed(message),
        LinkStatus::Starting | LinkStatus::Connecting => {
            Linking::Busy(gettext(locale, "Connecting to WhatsApp…").into_owned())
        }
        LinkStatus::Connected | LinkStatus::Disconnected { .. } => {
            Linking::Busy(gettext(locale, "Linked. Waiting for your chats…").into_owned())
        }
    }
}

fn link(app: &mut App, ui: &mut egui::Ui, narrow: bool, first_run: bool) {
    let palette = app.palette;
    let locale = app.locale;
    let state = linking(app);
    if !first_run {
        // With no welcome before it, the mark says whose window this is.
        let (mark, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
        theme::mark(ui, mark.center(), 52.0);
        ui.add_space(20.0);
    }
    match &state {
        Linking::Failed(message) => {
            title(ui, &palette, &gettext(locale, "Linking stopped"), "");
            ui.add(
                egui::Label::new(
                    egui::RichText::new(message)
                        .font(theme::regular(13.5))
                        .color(palette.danger),
                )
                .wrap(),
            );
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if primary_button(ui, &palette, &gettext(locale, "Try again"), None).clicked() {
                    app.actions.push(Action::Reconnect);
                }
                if super::login::archive_key_lost(message)
                    && back_button(ui, &palette, &gettext(locale, "Start over…")).clicked()
                {
                    app.actions
                        .push(Action::ShowDialog(Dialog::ConfirmStartOver));
                }
            });
            ui.add_space(24.0);
        }
        Linking::PairCode(code, phone) => {
            let subtitle = match phone {
                Some(phone) => gettext(
                    locale,
                    "WhatsApp on +{phone} asks for it when you link with a phone number.",
                )
                .replace("{phone}", phone),
                None => gettext(
                    locale,
                    "WhatsApp on your phone asks for it when you link with a phone number.",
                )
                .into_owned(),
            };
            title(
                ui,
                &palette,
                &gettext(locale, "Enter this code on your phone"),
                &subtitle,
            );
            if narrow {
                pair_code_tile(app, ui, code);
                ui.add_space(24.0);
            }
            steps(
                ui,
                &palette,
                locale,
                [
                    "Open WhatsApp on your phone",
                    "Tap Menu or Settings, then Linked devices",
                    "Tap Link a device, then Link with phone number instead",
                ],
            );
        }
        Linking::Qr(_) | Linking::Busy(_) => {
            let again = matches!(app.link, LinkStatus::LoggedOut);
            let (heading, subtitle) = if again {
                (
                    gettext(locale, "Link your phone again"),
                    gettext(
                        locale,
                        "This computer was unlinked from your phone. Scan the new code to bring your chats back.",
                    ),
                )
            } else {
                (
                    gettext(locale, "Link your phone"),
                    gettext(
                        locale,
                        "Scan the code with WhatsApp and your chats start arriving here.",
                    ),
                )
            };
            title(ui, &palette, &heading, &subtitle);
            if narrow {
                match &state {
                    Linking::Qr(code) => {
                        ui.vertical_centered(|ui| {
                            let side = ui.available_width().min(260.0);
                            let (tile, _) =
                                ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
                            qr_tile(ui, &palette, tile, code);
                        });
                    }
                    Linking::Busy(label) => super::login::busy(ui, palette.accent, label),
                    _ => {}
                }
                ui.add_space(24.0);
            }
            steps(
                ui,
                &palette,
                locale,
                [
                    "Open WhatsApp on your phone",
                    "Tap Menu or Settings, then Linked devices",
                    "Tap Link a device and point your phone at the code",
                ],
            );
            if matches!(
                app.link,
                LinkStatus::Unlinked {
                    pairing_phone: None,
                    ..
                }
            ) {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    theme::icon(ui, Icon::Smartphone, 16.0, palette.link);
                    if theme::link(
                        ui,
                        gettext(locale, "Link with phone number instead"),
                        theme::medium(13.5),
                        palette.link,
                    )
                    .clicked()
                    {
                        app.actions.push(Action::ShowDialog(Dialog::PairWithPhone));
                    }
                });
            }
        }
    }
    super::accounts::login_choices(app, ui);
    ui.add_space(22.0);
    disclaimer(ui, &palette, locale);
}

/// The numbered steps on the phone.
fn steps(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: crate::i18n::Locale,
    steps: [&'static str; 3],
) {
    for (index, step) in steps.into_iter().enumerate() {
        ui.horizontal_top(|ui| {
            let (badge, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            ui.painter()
                .circle_filled(badge.center(), 13.0, tint(palette));
            ui.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                (index + 1).to_string(),
                theme::semibold(12.5),
                palette.accent,
            );
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                theme::paragraph(
                    ui,
                    gettext(locale, step),
                    theme::regular(14.0),
                    palette.text,
                );
            });
        });
        ui.add_space(12.0);
    }
}

/// The code on a white tile, as a phone's camera wants it whatever the
/// theme, with the accent's corner marks round it like a viewfinder.
fn qr_tile(ui: &egui::Ui, palette: &Palette, tile: Rect, code: &str) {
    let painter = ui.painter();
    painter.add(palette.float_shadow().as_shape(tile, 24.0));
    painter.rect_filled(tile, 24.0, Color32::WHITE);
    let inner = tile.shrink(tile.width() * 0.07);
    match Qr::encode(code) {
        Some(qr) => qr.paint(ui, inner, Color32::BLACK, Color32::WHITE),
        None => theme::paint_icon(ui, Icon::CircleAlert, inner, 32.0, palette.danger),
    }
    // The viewfinder breathes, slowly, as if looking for the phone.
    let breath = if crate::motion::screen_reader_attached(ui.ctx()) {
        0.0
    } else {
        crate::motion::request_frame_after(ui.ctx(), std::time::Duration::from_millis(33));
        let time = ui.input(|input| input.time) as f32;
        (1.0 - (time * std::f32::consts::TAU / 2.8).cos()) / 2.0
    };
    let frame = tile.expand(10.0 + 4.0 * theme::ease_out(breath));
    let stroke = Stroke::new(3.0, palette.accent);
    for (corner, out) in [
        (frame.left_top(), vec2(1.0, 1.0)),
        (frame.right_top(), vec2(-1.0, 1.0)),
        (frame.right_bottom(), vec2(-1.0, -1.0)),
        (frame.left_bottom(), vec2(1.0, -1.0)),
    ] {
        bracket(painter, corner, out, 26.0, 30.0, stroke);
    }
    ui.interact(tile, Id::new("setup-qr"), Sense::hover())
        .widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Image,
                true,
                "QR code to link this computer",
            )
        });
}

/// One viewfinder corner at `corner`, its legs running `inward` for
/// `length`, turning on an arc of `radius`.
fn bracket(
    painter: &egui::Painter,
    corner: Pos2,
    inward: Vec2,
    radius: f32,
    length: f32,
    stroke: Stroke,
) {
    const STEPS: usize = 10;
    let centre = corner + inward * radius;
    let mut points = vec![corner + vec2(0.0, inward.y * length)];
    for step in 0..=STEPS {
        // From pointing back along x to pointing back along y.
        let angle = step as f32 / STEPS as f32 * std::f32::consts::FRAC_PI_2;
        let offset = vec2(-inward.x * angle.cos(), -inward.y * angle.sin()) * radius;
        points.push(centre + offset);
    }
    points.push(corner + vec2(inward.x * length, 0.0));
    painter.add(egui::Shape::line(points, stroke));
}

/// The pairing code, large, and Copy under it.
fn pair_code_tile(app: &mut App, ui: &mut egui::Ui, code: &str) {
    let palette = app.palette;
    let shown = if code.len() == 8 && !code.contains('-') {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_owned()
    };
    ui.vertical_centered(|ui| {
        Frame::new()
            .fill(palette.surface)
            .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
            .inner_margin(Margin::symmetric(28, 16))
            .show(ui, |ui| {
                let mut job = egui::text::LayoutJob::single_section(
                    shown.clone(),
                    egui::TextFormat {
                        font_id: theme::bold(36.0),
                        color: palette.text,
                        extra_letter_spacing: 3.0,
                        ..Default::default()
                    },
                );
                job.wrap.max_rows = 1;
                ui.label(job);
            });
        ui.add_space(12.0);
        let label = gettext(app.locale, "Copy code");
        if theme::soft_button(ui, &palette, Some(Icon::Copy), &label, false).clicked() {
            app.actions.push(Action::CopyText(code.to_owned()));
        }
    });
}

/// The right of the card: the small window, or on the link step the code.
fn stage(app: &mut App, ui: &mut egui::Ui, stage: Rect, step: Step, opened: f32) {
    let palette = app.palette;
    let radius = f32::from(theme::RADIUS_DIALOG) - STAGE_INSET;
    // A shade off the card, lit from above by the accent.
    ui.painter().rect_filled(
        stage,
        radius,
        theme::blend(
            palette.panel,
            palette.text,
            if palette.dark { 0.035 } else { 0.03 },
        ),
    );
    glow(
        &ui.painter().with_clip_rect(stage.shrink(radius / 2.0)),
        pos2(stage.center().x, stage.top() + stage.height() * 0.3),
        stage.size() * vec2(0.55, 0.5),
        palette
            .accent
            .gamma_multiply(if palette.dark { 0.08 } else { 0.06 }),
    );
    let link = theme::animate(
        ui.ctx(),
        Id::new("setup-stage-link"),
        step == Step::Link,
        theme::MOTION_SCREEN,
    );
    if link < 1.0 {
        let mut layer = ui.new_child(UiBuilder::new().id_salt("setup-preview").max_rect(stage));
        layer.set_clip_rect(stage);
        layer.multiply_opacity(1.0 - link);
        let margin = 36.0;
        let width = stage.width() - margin * 2.0;
        let height = (stage.height() - margin * 2.0).min(width * 0.8);
        let window = Rect::from_center_size(stage.center(), vec2(width, height))
            .translate(vec2(-24.0 * link, 0.0));
        let start = arrive::painted(ui);
        preview(app, &mut layer, window, step);
        // Each choice redraws the window from the top down, so the change
        // reads as it lands; and as the setup opens, the window rises
        // after the card.
        arrive::cascade(
            ui,
            start,
            window,
            changed_for(ui.ctx(), app),
            Cascade {
                delay: 0.0,
                spread: 0.16,
                duration: theme::MOTION_PANEL * 1.5,
                offset: vec2(0.0, 6.0),
                from_opacity: 0.35,
            },
        );
        arrive::cascade(
            ui,
            start,
            window,
            opened,
            Cascade {
                delay: 0.18,
                spread: 0.2,
                duration: theme::MOTION_SCREEN,
                offset: vec2(0.0, 28.0),
                from_opacity: 0.0,
            },
        );
    }
    if link > 0.0 {
        let mut layer = ui.new_child(UiBuilder::new().id_salt("setup-code").max_rect(stage));
        layer.set_clip_rect(stage);
        layer.multiply_opacity(link);
        link_stage(
            app,
            &mut layer,
            stage.translate(vec2(24.0 * (1.0 - link), 0.0)),
        );
    }
}

/// Seconds since a choice the preview draws last changed: the mode, the
/// theme, the accent, the font, the doodles or read receipts.
fn changed_for(ctx: &egui::Context, app: &App) -> f32 {
    use std::hash::{Hash, Hasher};
    let settings = &app.settings;
    let mut hasher = std::hash::DefaultHasher::new();
    format!(
        "{} {:?} {:?} {:?} {:?} {} {} {}",
        app.palette.dark,
        settings.chat_theme,
        settings.accent,
        settings.font,
        settings.doodle_style,
        settings.show_wallpaper,
        settings.custom_theme.is_some(),
        app.account().settings.send_read_receipts,
    )
    .hash(&mut hasher);
    let look = hasher.finish();
    let id = Id::new("setup-preview-look");
    let now = ctx.input(|input| input.time);
    let (was, since) = ctx
        .data(|data| data.get_temp::<(u64, f64)>(id))
        .unwrap_or((look, f64::NEG_INFINITY));
    let since = if was == look { since } else { now };
    ctx.data_mut(|data| data.insert_temp(id, (look, since)));
    if crate::motion::screen_reader_attached(ctx) {
        return f32::INFINITY;
    }
    (now - since) as f32
}

/// The stage of the link step: the code to scan or type, or what the link
/// is waiting for.
fn link_stage(app: &mut App, ui: &mut egui::Ui, stage: Rect) {
    let palette = app.palette;
    let locale = app.locale;
    match linking(app) {
        Linking::Qr(code) => {
            let side = (stage.width().min(stage.height()) * 0.56).clamp(200.0, 300.0);
            let tile = Rect::from_center_size(stage.center() - vec2(0.0, 30.0), Vec2::splat(side));
            qr_tile(ui, &palette, tile, &code);
            ui.painter().text(
                pos2(stage.center().x, tile.bottom() + 40.0),
                Align2::CENTER_CENTER,
                gettext(locale, "Scan with WhatsApp"),
                theme::semibold(16.0),
                palette.text,
            );
            ui.painter().text(
                pos2(stage.center().x, tile.bottom() + 62.0),
                Align2::CENTER_CENTER,
                gettext(locale, "Linked devices, then Link a device"),
                theme::regular(12.5),
                palette.secondary,
            );
        }
        Linking::PairCode(code, _) => {
            let area = Rect::from_center_size(stage.center(), vec2(stage.width() - 48.0, 160.0));
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(area)
                    .layout(Layout::top_down(Align::Center)),
                |ui| pair_code_tile(app, ui, &code),
            );
        }
        Linking::Busy(label) => {
            let spinner =
                Rect::from_center_size(stage.center() - vec2(0.0, 14.0), Vec2::splat(28.0));
            theme::paint_spinner(ui, spinner, 28.0, palette.accent);
            ui.painter().text(
                pos2(stage.center().x, spinner.bottom() + 24.0),
                Align2::CENTER_CENTER,
                label,
                theme::medium(14.0),
                palette.secondary,
            );
        }
        Linking::Failed(_) => {
            let disc = stage.center() - vec2(0.0, 18.0);
            ui.painter()
                .circle_filled(disc, 44.0, palette.danger.gamma_multiply(0.14));
            theme::paint_icon(
                ui,
                Icon::CircleAlert,
                Rect::from_center_size(disc, Vec2::splat(40.0)),
                40.0,
                palette.danger,
            );
            ui.painter().text(
                pos2(stage.center().x, disc.y + 70.0),
                Align2::CENTER_CENTER,
                gettext(locale, "Zeus could not reach WhatsApp"),
                theme::medium(14.0),
                palette.secondary,
            );
        }
    }
}

/// The chats as they look with the choices made so far: a small window
/// with the rail, the chat list and a conversation, and on the notification
/// and privacy steps what those choices change.
fn preview(app: &App, ui: &mut egui::Ui, window: Rect, step: Step) {
    let palette = app.palette;
    let locale = app.locale;
    let text = |source: &'static str| gettext(locale, source).into_owned();
    let radius = 16.0;
    let r = radius as u8;
    let painter = ui.painter().clone();
    painter.add(palette.float_shadow().as_shape(window, radius));
    painter.rect_filled(window, radius, palette.window);

    // The rail.
    let rail = Rect::from_min_max(window.min, pos2(window.left() + 46.0, window.bottom()));
    painter.rect_filled(
        rail,
        CornerRadius {
            nw: r,
            sw: r,
            ne: 0,
            se: 0,
        },
        palette.rail(),
    );
    for (index, icon) in [Icon::MessageCircle, Icon::Phone, Icon::Star]
        .into_iter()
        .enumerate()
    {
        let at = pos2(rail.center().x, rail.top() + 28.0 + index as f32 * 38.0);
        if index == 0 {
            painter.rect_filled(
                Rect::from_center_size(at, Vec2::splat(32.0)),
                11.0,
                palette.surface_active,
            );
        }
        let color = if index == 0 {
            palette.text
        } else {
            palette.secondary
        };
        theme::paint_icon(
            ui,
            icon,
            Rect::from_center_size(at, Vec2::splat(17.0)),
            17.0,
            color,
        );
    }
    theme::paint_icon(
        ui,
        Icon::Settings,
        Rect::from_center_size(
            pos2(rail.center().x, rail.bottom() - 60.0),
            Vec2::splat(17.0),
        ),
        17.0,
        palette.secondary,
    );
    let me = pos2(rail.center().x, rail.bottom() - 26.0);
    painter.circle_filled(me, 12.0, palette.avatar(188.0));
    painter.text(
        me,
        Align2::CENTER_CENTER,
        "Y",
        theme::semibold(11.0),
        Color32::WHITE,
    );

    // The chat list, whose colour runs on behind the conversation card.
    let side = palette.sidebar();
    let list_width = (window.width() * 0.34).clamp(130.0, 190.0);
    let list = Rect::from_min_max(
        pos2(rail.right(), window.top()),
        pos2(rail.right() + list_width, window.bottom()),
    );
    painter.rect_filled(
        Rect::from_min_max(list.min, window.max),
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: r,
            se: r,
        },
        side,
    );
    painter.text(
        pos2(list.left() + 14.0, list.top() + 24.0),
        Align2::LEFT_CENTER,
        text("Chats"),
        theme::bold(17.0),
        palette.text,
    );
    let search = Rect::from_min_size(
        pos2(list.left() + 12.0, list.top() + 44.0),
        vec2(list.width() - 24.0, 28.0),
    );
    painter.rect_filled(search, 14.0, palette.surface);
    theme::paint_icon(
        ui,
        Icon::Search,
        Rect::from_center_size(
            pos2(search.left() + 16.0, search.center().y),
            Vec2::splat(13.0),
        ),
        13.0,
        palette.dim,
    );
    painter.text(
        pos2(search.left() + 28.0, search.center().y),
        Align2::LEFT_CENTER,
        text("Search"),
        theme::regular(12.0),
        palette.dim,
    );
    let rows: [(&str, &str, f32, u32, &str); 5] = [
        ("Ava", "Did you see the new themes?", 212.0, 0, "9:41"),
        ("Design crew", "Leo: New icons are up", 28.0, 3, "9:30"),
        ("Priya", "Photo", 152.0, 1, "8:52"),
        ("Leo", "See you at six", 276.0, 0, "8:15"),
        ("Mum", "Call me when you are free", 336.0, 0, "Mon"),
    ];
    let mut y = search.bottom() + 10.0;
    for (index, (name, last, hue, unread, time)) in rows.into_iter().enumerate() {
        let row = Rect::from_min_size(pos2(list.left(), y), vec2(list.width(), 48.0));
        if row.bottom() > list.bottom() - 6.0 {
            break;
        }
        if index == 0 {
            painter.rect_filled(row.shrink2(vec2(6.0, 2.0)), 12.0, palette.surface_hover);
        }
        let avatar = pos2(row.left() + 14.0 + 15.0, row.center().y);
        painter.circle_filled(avatar, 15.0, palette.avatar(hue));
        painter.text(
            avatar,
            Align2::CENTER_CENTER,
            name.chars().next().unwrap_or_default(),
            theme::semibold(12.0),
            Color32::WHITE,
        );
        let left = avatar.x + 15.0 + 10.0;
        let time = painter.layout_no_wrap(time.to_owned(), theme::regular(10.5), palette.dim);
        let room = row.right() - 12.0 - left;
        painter.galley(
            pos2(row.right() - 12.0 - time.size().x, row.center().y - 15.0),
            time.clone(),
            palette.dim,
        );
        let name = widgets::line(
            ui,
            name,
            theme::medium(12.5),
            palette.text,
            room - time.size().x - 6.0,
            1,
        );
        painter.galley(
            pos2(left, row.center().y - 16.0),
            name.galley.clone(),
            palette.text,
        );
        let badge = if unread > 0 { 22.0 } else { 0.0 };
        let preview = widgets::line(
            ui,
            last,
            theme::regular(11.5),
            palette.secondary,
            room - badge,
            1,
        );
        painter.galley(
            pos2(left, row.center().y + 1.0),
            preview.galley.clone(),
            palette.secondary,
        );
        if unread > 0 {
            let at = pos2(row.right() - 12.0 - 8.0, row.center().y + 8.0);
            painter.circle_filled(at, 8.5, palette.accent);
            painter.text(
                at,
                Align2::CENTER_CENTER,
                unread.to_string(),
                theme::semibold(10.0),
                palette.on_accent,
            );
        }
        y = row.bottom();
    }

    // The conversation card, on its wallpaper.
    let chat = Rect::from_min_max(
        pos2(list.right(), window.top() + 6.0),
        window.max - vec2(6.0, 6.0),
    );
    let chat_radius = 12.0;
    let look = wallpaper::Look {
        density: (app.settings.doodle_density * 1.6).min(*crate::settings::DOODLE_DENSITY.end()),
        ..app.wallpaper()
    };
    wallpaper::paint_rect(ui, chat, &look);
    painter.add(widgets::card_corners(chat, chat_radius, side));
    let header = Rect::from_min_size(chat.min, vec2(chat.width(), 52.0));
    let header_fill = if palette.vibrant() {
        palette.vibrant_header(palette.chat)
    } else {
        palette.panel
    };
    let c = chat_radius as u8;
    painter.rect_filled(
        header,
        CornerRadius {
            nw: c,
            ne: c,
            sw: 0,
            se: 0,
        },
        header_fill,
    );
    let avatar = pos2(header.left() + 16.0 + 15.0, header.center().y);
    painter.circle_filled(avatar, 15.0, palette.avatar(212.0));
    painter.text(
        avatar,
        Align2::CENTER_CENTER,
        "A",
        theme::semibold(12.0),
        Color32::WHITE,
    );
    painter.text(
        pos2(avatar.x + 24.0, header.center().y - 8.0),
        Align2::LEFT_CENTER,
        "Ava",
        theme::semibold(13.5),
        palette.text,
    );
    painter.text(
        pos2(avatar.x + 24.0, header.center().y + 9.0),
        Align2::LEFT_CENTER,
        text("online"),
        theme::regular(11.0),
        palette.secondary,
    );
    for (index, icon) in [Icon::Search, Icon::Phone].into_iter().enumerate() {
        let at = pos2(
            header.right() - 22.0 - index as f32 * 32.0,
            header.center().y,
        );
        theme::paint_icon(
            ui,
            icon,
            Rect::from_center_size(at, Vec2::splat(16.0)),
            16.0,
            palette.secondary,
        );
    }

    // On the privacy step a notice slides in above the messages.
    let notice = theme::animate(
        ui.ctx(),
        Id::new("setup-preview-notice"),
        step == Step::Privacy,
        theme::MOTION_PANEL,
    );
    let mut top = header.bottom() + 14.0;
    if notice > 0.0 {
        let label = text("Encrypted on this computer");
        let galley = painter.layout_no_wrap(label, theme::medium(11.0), palette.secondary);
        let width = galley.size().x + 12.0 + 6.0 + 24.0;
        let chip = Rect::from_center_size(
            pos2(chat.center().x, top + 13.0 - 8.0 * (1.0 - notice)),
            vec2(width.min(chat.width() - 24.0), 26.0),
        );
        let mut layer = ui.new_child(UiBuilder::new().id_salt("setup-notice").max_rect(chip));
        layer.multiply_opacity(notice);
        let fill = if palette.vibrant() {
            palette.glass()
        } else {
            palette.panel
        };
        layer.painter().rect(
            chip,
            13.0,
            fill,
            Stroke::new(1.0, palette.outline),
            egui::StrokeKind::Inside,
        );
        theme::paint_icon(
            &layer,
            Icon::Lock,
            Rect::from_center_size(pos2(chip.left() + 18.0, chip.center().y), Vec2::splat(12.0)),
            12.0,
            palette.secondary,
        );
        layer.painter().galley(
            pos2(chip.left() + 30.0, chip.center().y - galley.size().y / 2.0),
            galley,
            palette.secondary,
        );
        top += 36.0 * notice;
    }

    // The messages, ours ticked as read receipts would show them.
    let composer_top = chat.bottom() - 12.0 - 34.0;
    let bubble_radius = match palette.bubbles {
        BubbleStyle::WhatsApp => 8.0,
        BubbleStyle::Messages | BubbleStyle::Nothing => 16.0,
    };
    let receipts = app.account().settings.send_read_receipts;
    let messages = [
        (text("Did you see the new themes?"), false),
        (text("Trying them all right now"), true),
        (text("This one looks so clean"), false),
        (text("And it opens in a blink"), true),
    ];
    for (body, own) in messages {
        let colours = palette.on_bubble(own);
        let galley = painter.layout(
            body,
            theme::regular(12.5),
            colours.text,
            chat.width() * 0.62,
        );
        let meta = if own { 22.0 } else { 0.0 };
        let size = galley.size() + vec2(24.0 + meta, 16.0);
        if top + size.y > composer_top - 8.0 {
            break;
        }
        let left = if own {
            chat.right() - 14.0 - size.x
        } else {
            chat.left() + 14.0
        };
        let shape = Rect::from_min_size(pos2(left, top), size);
        if palette.bubbles == BubbleStyle::WhatsApp {
            painter.add(palette.bubble_shadow().as_shape(shape, bubble_radius));
        }
        painter.rect_filled(
            shape,
            bubble_radius,
            if own {
                palette.bubble_out
            } else {
                palette.bubble_in
            },
        );
        painter.galley(shape.min + vec2(12.0, 8.0), galley, colours.text);
        if own {
            let ticks = Rect::from_center_size(
                pos2(shape.right() - 17.0, shape.bottom() - 11.0),
                Vec2::splat(13.0),
            );
            let status = if receipts {
                Delivery::Read
            } else {
                Delivery::Delivered
            };
            widgets::ticks_in(ui, &colours, ticks, status, colours.secondary);
        }
        top = shape.bottom() + 6.0;
    }

    // The message field, on glass or a solid pill.
    let field = Rect::from_min_max(
        pos2(chat.left() + 12.0, composer_top),
        pos2(chat.right() - 12.0 - 42.0, chat.bottom() - 12.0),
    );
    let field_fill = if palette.vibrant() {
        palette.glass()
    } else {
        palette.surface
    };
    painter.rect_filled(field, field.height() / 2.0, field_fill);
    painter.text(
        pos2(field.left() + 16.0, field.center().y),
        Align2::LEFT_CENTER,
        text("Message"),
        theme::regular(12.5),
        palette.dim,
    );
    let send = pos2(chat.right() - 12.0 - 17.0, field.center().y);
    painter.circle_filled(send, 17.0, palette.accent);
    theme::paint_icon(
        ui,
        Icon::Send,
        Rect::from_center_size(send, Vec2::splat(15.0)),
        15.0,
        palette.on_accent,
    );
    painter.rect_stroke(
        window,
        radius,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );

    // On the notifications step, a notification drops in over the corner,
    // saying what the choices let it say.
    let shown = theme::animate(
        ui.ctx(),
        Id::new("setup-preview-notification"),
        step == Step::Notifications && app.account().settings.notifications,
        theme::MOTION_PANEL,
    );
    if shown > 0.0 {
        notification(app, ui, window, shown);
    }
}

/// A desktop notification as the choices make it: the sender and message,
/// or only "New message", and a speaker while it plays a sound.
fn notification(app: &App, ui: &mut egui::Ui, window: Rect, shown: f32) {
    let palette = app.palette;
    let locale = app.locale;
    let size = vec2(272.0, 68.0);
    let card = Rect::from_min_size(
        pos2(
            window.right() - size.x + 18.0,
            window.top() - 18.0 - 12.0 * (1.0 - shown),
        ),
        size,
    );
    let mut layer = ui.new_child(
        UiBuilder::new()
            .id_salt("setup-notification")
            .max_rect(card),
    );
    layer.multiply_opacity(shown);
    let painter = layer.painter().clone();
    painter.add(palette.float_shadow().as_shape(card, 18.0));
    painter.rect(
        card,
        18.0,
        palette.overlay,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
    let mark = pos2(card.left() + 14.0 + 16.0, card.center().y);
    theme::mark(&layer, mark, 32.0);
    let left = mark.x + 16.0 + 12.0;
    let right = card.right() - 14.0;
    let preview = app.settings.notification_content;
    let sound = app.settings.message_sound != NotificationSound::None;
    let (sender, body) = if preview {
        (
            "Ava".to_owned(),
            gettext(locale, "Are we still on for tonight?"),
        )
    } else {
        ("Zeus".to_owned(), gettext(locale, "New message"))
    };
    painter.text(
        pos2(left, card.center().y - 9.0),
        Align2::LEFT_CENTER,
        sender,
        theme::semibold(13.0),
        palette.text,
    );
    let now = painter.text(
        pos2(right, card.center().y - 9.0),
        Align2::RIGHT_CENTER,
        gettext(locale, "now"),
        theme::regular(11.0),
        palette.dim,
    );
    if sound {
        theme::paint_icon(
            &layer,
            Icon::Volume2,
            Rect::from_center_size(pos2(now.left() - 12.0, now.center().y), Vec2::splat(13.0)),
            13.0,
            palette.dim,
        );
    }
    let line = widgets::line(
        &layer,
        &body,
        theme::regular(12.5),
        palette.secondary,
        right - left,
        1,
    );
    painter.galley(
        pos2(left, card.center().y + 2.0),
        line.galley.clone(),
        palette.secondary,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_run_in_order_and_stop_at_either_end() {
        assert_eq!(Step::Welcome.previous(), Step::Welcome);
        assert_eq!(Step::Link.next(), Step::Link);
        let mut step = Step::Welcome;
        for expected in &Step::ALL[1..] {
            step = step.next();
            assert_eq!(step, *expected);
        }
        for page in [
            "setup",
            "setup-look",
            "setup-personal",
            "setup-notifications",
            "setup-privacy",
            "setup-link",
        ] {
            assert!(Step::named(page).is_some(), "{page}");
        }
        assert_eq!(Step::named("login"), None);
    }

    /// The background lights stay quiet: a grey accent leaves the corner
    /// lights to their own blue and violet, and a coloured one leads them.
    #[test]
    fn only_a_coloured_accent_tints_the_corner_lights() {
        assert!(!colourful(Color32::WHITE));
        assert!(!colourful(Color32::from_rgb(20, 20, 22)));
        assert!(colourful(Color32::from_rgb(10, 132, 255)));
    }
}
