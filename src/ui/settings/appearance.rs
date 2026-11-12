//! Settings > Appearance: one card for the theme (the mode, a colour wheel
//! for the accent, the themes, Glass and the wallpaper's tint), the doodles,
//! the font, zoom and a More group, beside a live preview of a chat.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

use super::{block_heading, language_picker, segmented, slide_position};
use crate::app::App;
use crate::chat_themes::{Accent, ChatTheme, CustomAccent, Mode};
use crate::model::Action;
use crate::motion::MotionPreference;
use crate::settings::{DOODLE_DENSITY, DOODLE_INTENSITY, DoodleColor, DoodleStyle, FontChoice};
use crate::theme::{self, Icon, Palette};
use crate::ui::widgets;
use crate::wallpaper;

/// The room between the preview's edge and what is drawn in it.
const PREVIEW_PAD: f32 = 20.0;
/// How tall the preview is when it sits above the controls.
pub(super) const PREVIEW_HEIGHT: f32 = 248.0;

/// A conversation drawn in the colours on screen, filling `rect`, so a change
/// of theme, mode, accent, glass, wallpaper or motion shows as it would in
/// the chats. Beside the controls it runs the window's height; above them it
/// is [`PREVIEW_HEIGHT`] tall.
pub(super) fn preview(ui: &mut egui::Ui, app: &App, rect: Rect) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let palette = app.palette;
    let locale = app.locale;
    let text = |source: &'static str| crate::i18n::gettext(locale, source).into_owned();
    let painter = ui.painter_at(rect);
    let radius = f32::from(theme::RADIUS_CARD);
    let look = app.wallpaper();
    painter.rect_filled(rect, radius, look.color);

    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 58.0));
    let field_top = rect.bottom() - PREVIEW_PAD - 30.0;
    // The doodles stop short of the rounded corners at the bottom.
    let body = Rect::from_min_max(
        pos2(rect.left(), header.bottom()),
        pos2(rect.right(), rect.bottom() - radius),
    );
    if look.doodles && look.image.is_none() {
        wallpaper::paint_doodles(ui, body, &look, look.style);
    }

    let header_fill = if palette.vibrant() {
        palette.vibrant_header(palette.chat)
    } else {
        palette.panel
    };
    let r = radius as u8;
    painter.rect_filled(
        header,
        CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        header_fill,
    );
    let avatar = pos2(header.left() + PREVIEW_PAD + 14.0, header.center().y);
    painter.circle_filled(avatar, 14.0, palette.avatar(0.58));
    painter.text(
        avatar,
        Align2::CENTER_CENTER,
        "A",
        theme::semibold(theme::TEXT_CONTROL),
        Color32::WHITE,
    );
    painter.text(
        pos2(avatar.x + 22.0, header.center().y - 8.0),
        Align2::LEFT_CENTER,
        "Ava",
        theme::semibold(theme::TEXT_BODY),
        palette.text,
    );
    painter.text(
        pos2(avatar.x + 22.0, header.center().y + 8.0),
        Align2::LEFT_CENTER,
        text("online"),
        theme::regular(theme::TEXT_CAPTION),
        palette.secondary,
    );

    // Messages from the bottom up, newest last, as many as fit. The newest
    // arrives again whenever Motion changes, so its setting can be seen.
    let messages = [
        (text("Did you see the new themes?"), false),
        (text("Trying them all right now"), true),
        (text("Pick one with a light mode too"), false),
        (text("This one, with the doodles"), true),
        (text("Looks great"), false),
        (text("Going with it"), true),
    ];
    let arrive_id = egui::Id::new(("appearance-preview-arrive", app.settings.motion_preference));
    let arrival = theme::appear(ui.ctx(), arrive_id, theme::MOTION_PANEL);
    let mut bottom = field_top - 14.0;
    for (index, (label, own)) in messages.iter().enumerate().rev() {
        let colours = palette.on_bubble(*own);
        let galley = painter.layout_no_wrap(
            label.clone(),
            theme::regular(theme::TEXT_CONTROL),
            colours.text,
        );
        let meta = if *own { 30.0 } else { 0.0 };
        let size = galley.size() + vec2(28.0 + meta, 18.0);
        let top = bottom - size.y;
        if top < header.bottom() + 12.0 {
            break;
        }
        let newest = index == messages.len() - 1;
        let lift = if newest { (1.0 - arrival) * 18.0 } else { 0.0 };
        let fade = if newest { arrival } else { 1.0 };
        let left = if *own {
            rect.right() - PREVIEW_PAD - size.x
        } else {
            rect.left() + PREVIEW_PAD
        };
        let shape = Rect::from_min_size(pos2(left, top + lift), size);
        let fill = if *own {
            palette.bubble_out
        } else {
            palette.bubble_in
        };
        painter.rect_filled(shape, theme::RADIUS, fill.gamma_multiply(fade));
        painter.galley(
            shape.min + vec2(14.0, 9.0),
            galley,
            colours.text.gamma_multiply(fade),
        );
        if *own {
            let ticks = Rect::from_center_size(
                pos2(shape.right() - 22.0, shape.center().y),
                Vec2::splat(14.0),
            );
            widgets::ticks_in(
                ui,
                &colours,
                ticks,
                crate::model::Delivery::Read,
                colours.secondary,
            );
        }
        bottom = top - 6.0;
    }

    // The message field, on glass or a solid pill.
    let field = Rect::from_min_max(
        pos2(rect.left() + PREVIEW_PAD - 4.0, field_top),
        pos2(
            rect.right() - PREVIEW_PAD - 40.0,
            rect.bottom() - PREVIEW_PAD + 2.0,
        ),
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
        theme::regular(theme::TEXT_CONTROL),
        palette.dim,
    );
    let send = pos2(rect.right() - PREVIEW_PAD - 14.0, field.center().y);
    painter.circle_filled(send, 16.0, palette.accent);
    theme::paint_icon(
        ui,
        Icon::Send,
        Rect::from_center_size(send, Vec2::splat(16.0)),
        16.0,
        palette.on_accent,
    );
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, palette.outline),
        egui::StrokeKind::Inside,
    );
}

/// The theme card: the mode, the colour wheel, the themes, Glass and the
/// wallpaper's tint.
pub(super) fn theme_card(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    egui::Frame::new()
        .fill(theme::blend(palette.panel, palette.surface, 0.5))
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| mode_icons(ui, app));
            ui.add_space(theme::SPACE_M);
            colour_wheel(ui, app);
            ui.add_space(theme::SPACE_M);
            theme_swatches(ui, app);
            ui.add_space(theme::SPACE_S);
            glass_wave(ui, app);
            ui.add_space(theme::SPACE_S);
            let mut tint = app.settings.tint_wallpaper;
            let label = crate::i18n::gettext(app.locale, "Tint the wallpaper");
            let mut changed = false;
            widgets::setting_row(ui, &palette, &label, "", |ui| {
                let response = widgets::switch(ui, &palette, &mut tint);
                theme::reveal_focus(&response);
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Checkbox,
                        ui.is_enabled(),
                        tint,
                        label.as_ref(),
                    )
                });
                changed = response.changed();
            });
            if changed {
                app.actions.push(Action::SetTintWallpaper(tint));
            }
        });
    ui.add_space(theme::SPACE_S);
}

/// Auto, light and dark as three icons in one pill, the chosen one filled
/// with the accent.
fn mode_icons(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let custom = app.settings.custom_theme.is_some();
    let selected = (!custom).then_some(app.settings.mode);
    let segment = vec2(52.0, 32.0);
    let (outer, _) = ui.allocate_exact_size(
        vec2(segment.x * Mode::ALL.len() as f32 + 6.0, segment.y + 6.0),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(outer, outer.height() / 2.0, palette.surface);
    let slide = selected
        .and_then(|mode| Mode::ALL.iter().position(|option| *option == mode))
        .map(|index| slide_position(ui, ui.id().with("mode-icons-slide"), index as f32));
    if let Some(position) = slide {
        let fill = Rect::from_min_size(outer.min + vec2(3.0 + segment.x * position, 3.0), segment);
        ui.painter()
            .rect_filled(fill, fill.height() / 2.0, palette.accent);
    }
    for (index, mode) in Mode::ALL.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            outer.min + vec2(3.0 + segment.x * index as f32, 3.0),
            segment,
        );
        // Fixed ids, so the demo tour can point at them.
        let id = match mode {
            Mode::System => "settings-mode-auto",
            Mode::Light => "settings-mode-light",
            Mode::Dark => "settings-mode-dark",
        };
        let response = ui.interact(rect, egui::Id::new(id), Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
        let lit = slide.is_some_and(|position| (position - index as f32).abs() < 0.5);
        if !lit && response.hovered() {
            ui.painter()
                .rect_filled(rect, rect.height() / 2.0, palette.surface_hover);
        }
        let icon = match mode {
            Mode::System => Icon::Sparkles,
            Mode::Light => Icon::Sun,
            Mode::Dark => Icon::Moon,
        };
        let color = if lit {
            palette.on_accent
        } else if response.hovered() {
            palette.text
        } else {
            palette.secondary
        };
        theme::paint_icon(
            ui,
            icon,
            Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
            18.0,
            color,
        );
        let label = crate::i18n::gettext(app.locale, mode.label());
        let on = selected == Some(mode);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                on,
                label.as_ref(),
            )
        });
        if response
            .on_hover_text(label.as_ref())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.actions.push(Action::SetMode(mode));
            if custom {
                // A theme file has one mode of its own: leave it for the
                // ready-made theme last used, in the mode asked for.
                let theme = app.settings.chat_theme;
                let theme = if theme.is_look() {
                    app.settings.last_theme
                } else {
                    theme
                };
                app.actions.push(Action::SetChatTheme(theme));
            }
        }
    }
}

/// The space between the colour wheel's dots.
const DOT_GAP: f32 = 15.0;
/// The hue at the wheel's right-hand edge, in OKLCH degrees: pink on the
/// right, blues and purples above, oranges and yellows below, greens left.
const HUE_AT_RIGHT: f32 = 350.0;
/// How close to the middle, or to a named accent, a pick lands on it.
const SNAP: f32 = 0.1;

/// Where `pointer` falls on a wheel drawn in `rect`: the middle is the
/// theme's own accent, the angle the hue, the distance how strong.
fn wheel_at(rect: Rect, pointer: Pos2) -> CustomAccent {
    let half = wheel_half(rect);
    let u = (pointer.x - rect.center().x) / half.x;
    let v = (pointer.y - rect.center().y) / half.y;
    let strength = u.hypot(v).min(1.0);
    let angle = (-v).atan2(u).to_degrees();
    CustomAccent {
        hue: (HUE_AT_RIGHT - angle).rem_euclid(360.0),
        strength,
    }
}

/// Where `place` is drawn on a wheel in `rect`; the inverse of [`wheel_at`].
fn wheel_point(rect: Rect, place: CustomAccent) -> Pos2 {
    let half = wheel_half(rect);
    let angle = (HUE_AT_RIGHT - place.hue).to_radians();
    pos2(
        rect.center().x + place.strength * angle.cos() * half.x,
        rect.center().y - place.strength * angle.sin() * half.y,
    )
}

/// How far the wheel reaches from its middle: to the outermost dots.
fn wheel_half(rect: Rect) -> Vec2 {
    vec2(
        (rect.width() / 2.0 - DOT_GAP / 2.0).max(1.0),
        (rect.height() / 2.0 - DOT_GAP / 2.0).max(1.0),
    )
}

/// The place on the wheel of the accent in use: a colour from the wheel, a
/// named accent, or the middle for the theme's own.
fn current_place(app: &App) -> CustomAccent {
    if let Some(custom) = app.settings.custom_accent {
        return custom;
    }
    app.settings.accent.rgb().map_or(
        CustomAccent {
            hue: 0.0,
            strength: 0.0,
        },
        CustomAccent::nearest,
    )
}

/// What a pick at `place` becomes once let go: the theme's own in the
/// middle, a named accent close to one, or the colour itself.
fn settle(place: CustomAccent, rect: Rect) -> Action {
    if place.strength < SNAP {
        return Action::SetAccent(Accent::Theme);
    }
    let at = wheel_point(rect, place);
    let reach = wheel_half(rect).min_elem() * SNAP;
    let named = Accent::ALL.into_iter().find(|accent| {
        accent
            .rgb()
            .is_some_and(|rgb| wheel_point(rect, CustomAccent::nearest(rgb)).distance(at) < reach)
    });
    match named {
        Some(accent) => Action::SetAccent(accent),
        None => Action::SetCustomAccent(Some(place)),
    }
}

/// The accent as a field of dots: drag the knob round for the hue and out
/// for the strength. The middle is the theme's own accent, and the named
/// accents catch the knob when it is let go near one.
fn colour_wheel(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let enabled = app.settings.custom_theme.is_none();
    let width = ui.available_width();
    let height = (width * 0.45).clamp(130.0, 190.0);
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
    theme::reveal_focus(&response);

    let pressed = response.dragged() || response.is_pointer_button_down_on() || response.clicked();
    let picked = response
        .interact_pointer_pos()
        .filter(|_| enabled && pressed)
        .map(|pointer| wheel_at(rect, pointer));
    if let Some(place) = picked
        && app.settings.custom_accent != Some(place)
    {
        app.actions.push(Action::SetCustomAccent(Some(place)));
    }
    if enabled && (response.drag_stopped() || response.clicked()) {
        let place = picked.unwrap_or_else(|| current_place(app));
        app.actions.push(settle(place, rect));
    }
    let place = picked.unwrap_or_else(|| current_place(app));
    response.widget_info(|| {
        egui::WidgetInfo::slider(
            enabled,
            f64::from(place.hue),
            crate::i18n::gettext(app.locale, "Accent").as_ref(),
        )
    });

    if !ui.is_rect_visible(rect) {
        return;
    }
    let columns = ((rect.width() / DOT_GAP).floor() as usize).max(3) | 1;
    let rows = ((rect.height() / DOT_GAP).floor() as usize).max(3) | 1;
    let colours = dot_colours(ui.ctx(), rect, columns, rows, palette.dark);
    let knob = wheel_point(rect, place);
    let origin = rect.center()
        - vec2(
            (columns - 1) as f32 * DOT_GAP / 2.0,
            (rows - 1) as f32 * DOT_GAP / 2.0,
        );
    let painter = ui.painter();
    for row in 0..rows {
        for column in 0..columns {
            let at = origin + vec2(column as f32 * DOT_GAP, row as f32 * DOT_GAP);
            // Dots swell near the knob, as under a lens.
            let near = (1.0 - at.distance(knob) / (DOT_GAP * 3.0)).max(0.0);
            let color = colours[row * columns + column];
            let color = if enabled {
                color
            } else {
                color.gamma_multiply(0.35)
            };
            painter.circle_filled(at, 3.0 + 2.5 * near, color);
        }
    }
    if enabled {
        let active = theme::animate(
            ui.ctx(),
            response.id,
            response.hovered() || response.dragged(),
            theme::MOTION_QUICK,
        );
        let size = 11.0 + 2.0 * active;
        for (grow, alpha) in [(5.0, 12u8), (3.0, 18), (1.5, 26)] {
            painter.circle_filled(
                knob + vec2(0.0, 1.5),
                size + 3.0 + grow,
                Color32::from_black_alpha(alpha),
            );
        }
        painter.circle_filled(knob, size + 3.0, Color32::WHITE);
        painter.circle_filled(knob, size, palette.accent);
    }
    if enabled && (response.hovered() || response.dragged()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
}

/// The wheel's dot colours for its size and mode, worked out once and kept:
/// each is a gamut search, too many to repeat every frame.
fn dot_colours(
    ctx: &egui::Context,
    rect: Rect,
    columns: usize,
    rows: usize,
    dark: bool,
) -> std::sync::Arc<Vec<Color32>> {
    let key = egui::Id::new((
        "appearance-wheel-dots",
        columns,
        rows,
        dark,
        rect.width() as u32,
        rect.height() as u32,
    ));
    if let Some(colours) = ctx.data(|data| data.get_temp::<std::sync::Arc<Vec<Color32>>>(key)) {
        return colours;
    }
    let origin = rect.center()
        - vec2(
            (columns - 1) as f32 * DOT_GAP / 2.0,
            (rows - 1) as f32 * DOT_GAP / 2.0,
        );
    let mut colours = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for column in 0..columns {
            let at = origin + vec2(column as f32 * DOT_GAP, row as f32 * DOT_GAP);
            let [r, g, b] = wheel_at(rect, at).rgb(dark);
            colours.push(Color32::from_rgb(r, g, b));
        }
    }
    let colours = std::sync::Arc::new(colours);
    ctx.data_mut(|data| data.insert_temp(key, std::sync::Arc::clone(&colours)));
    colours
}

/// One swatch in the theme row.
enum Swatch {
    Theme(ChatTheme),
    File(String),
    /// Opens the folder theme files are read from.
    Add,
}

/// How wide a theme swatch is.
const SWATCH: f32 = 30.0;
/// The space between swatches.
const SWATCH_GAP: f32 = 10.0;
/// How wide the paging arrows are.
const CHEVRON: f32 = 24.0;

/// The themes as round swatches, each its own background with its accent at
/// the centre, then the theme files of the reader's, then one to add more.
/// Arrows page through them when they do not fit.
fn theme_swatches(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let custom = app.settings.custom_theme.clone();
    let mut items: Vec<(Swatch, Option<Palette>, bool, String)> = ChatTheme::ALL
        .into_iter()
        .map(|choice| {
            (
                Swatch::Theme(choice),
                Some(choice.palette(palette.dark, Accent::Theme)),
                custom.is_none() && app.settings.chat_theme == choice,
                choice.label().to_owned(),
            )
        })
        .collect();
    for file in app.custom_themes.picker_themes() {
        let name = file
            .filename
            .strip_suffix(".json")
            .unwrap_or(&file.filename)
            .to_owned();
        items.push((
            Swatch::File(file.filename.clone()),
            Some(file.palette),
            custom.as_deref() == Some(file.filename.as_str()),
            name,
        ));
    }
    items.push((
        Swatch::Add,
        None,
        false,
        crate::i18n::gettext(app.locale, "Open themes folder").into_owned(),
    ));

    let (row, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), SWATCH + 12.0), Sense::hover());
    let middle = row.width() - 2.0 * (CHEVRON + SWATCH_GAP);
    let fit = (((middle + SWATCH_GAP) / (SWATCH + SWATCH_GAP)).floor() as usize).max(1);
    let pages = items.len().div_ceil(fit);
    let page_id = ui.id().with("theme-swatch-page");
    let chosen_page = items
        .iter()
        .position(|(_, _, selected, _)| *selected)
        .map_or(0, |index| index / fit);
    let mut page = ui
        .data(|data| data.get_temp::<usize>(page_id))
        .unwrap_or(chosen_page)
        .min(pages.saturating_sub(1));

    let arrows = [
        (
            Icon::ChevronLeft,
            row.left() + CHEVRON / 2.0,
            page > 0,
            -1isize,
        ),
        (
            Icon::ChevronRight,
            row.right() - CHEVRON / 2.0,
            page + 1 < pages,
            1,
        ),
    ];
    for (icon, x, live, step) in arrows {
        if pages < 2 {
            break;
        }
        let rect = Rect::from_center_size(pos2(x, row.center().y), Vec2::splat(CHEVRON));
        let sense = if live { Sense::click() } else { Sense::hover() };
        let response = ui.interact(rect, ui.id().with(("theme-swatch-arrow", step)), sense);
        let color = if !live {
            palette.dim
        } else if response.hovered() {
            palette.text
        } else {
            palette.secondary
        };
        theme::paint_icon(ui, icon, rect, 18.0, color);
        if live
            && response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
        {
            page = page.saturating_add_signed(step);
        }
    }
    ui.data_mut(|data| data.insert_temp(page_id, page));

    let shown: Vec<_> = items.into_iter().skip(page * fit).take(fit).collect();
    let span = shown.len() as f32 * (SWATCH + SWATCH_GAP) - SWATCH_GAP;
    let mut x = row.center().x - span / 2.0 + SWATCH / 2.0;
    for (index, (swatch, colours, selected, name)) in shown.into_iter().enumerate() {
        let center = pos2(x, row.center().y);
        x += SWATCH + SWATCH_GAP;
        let rect = Rect::from_center_size(center, Vec2::splat(SWATCH + 6.0));
        // Built-in themes by name, so the demo tour can point at them.
        let id = match &swatch {
            Swatch::Theme(choice) => egui::Id::new(format!("settings-theme-{}", choice.label())),
            _ => ui.id().with(("theme-swatch", page, index)),
        };
        let response = ui.interact(rect, id, Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
        let painter = ui.painter();
        let t = theme::animate(ui.ctx(), response.id, selected, theme::MOTION_QUICK);
        if t > 0.0 {
            painter.circle_stroke(
                center,
                SWATCH / 2.0 + 3.5,
                Stroke::new(2.0, palette.text.gamma_multiply(t)),
            );
        } else if response.hovered() {
            painter.circle_stroke(
                center,
                SWATCH / 2.0 + 3.5,
                Stroke::new(1.0, palette.secondary),
            );
        }
        match colours {
            Some(own) => {
                painter.circle_filled(center, SWATCH / 2.0, own.chat);
                painter.circle_filled(center, SWATCH * 0.24, own.accent);
                painter.circle_stroke(center, SWATCH / 2.0, Stroke::new(1.0, palette.outline));
            }
            None => {
                painter.circle_stroke(
                    center,
                    SWATCH / 2.0 - 0.5,
                    Stroke::new(1.0, palette.secondary),
                );
                theme::paint_icon(
                    ui,
                    Icon::Plus,
                    Rect::from_center_size(center, Vec2::splat(16.0)),
                    16.0,
                    palette.secondary,
                );
            }
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, &name)
        });
        if response
            .on_hover_text(name.as_str())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            match swatch {
                Swatch::Theme(choice) => {
                    app.actions.push(Action::RevealThemeFrom(center));
                    app.actions.push(Action::SetChatTheme(choice));
                }
                Swatch::File(filename) => {
                    app.actions.push(Action::RevealThemeFrom(center));
                    app.actions.push(Action::SetCustomTheme(filename));
                }
                Swatch::Add => app.actions.push(Action::OpenThemesFolder),
            }
        }
    }
}

/// A slider whose track `paint_track` draws, given the track's rect, where
/// the thumb is and the value, under the shared glass thumb. Returns the
/// response, changed while it moves `value`.
fn track_slider(
    ui: &mut egui::Ui,
    palette: &Palette,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    size: Vec2,
    label: &str,
    paint_track: impl FnOnce(&egui::Ui, Rect, f32, f32),
) -> egui::Response {
    let (start, end) = (*range.start(), *range.end());
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let thumb = widgets::SLIDER_THUMB;
    let travel = rect.left() + thumb.x / 2.0..=rect.right() - thumb.x / 2.0;
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.is_pointer_button_down_on() || response.clicked())
    {
        let t = ((pointer.x - travel.start()) / (travel.end() - travel.start())).clamp(0.0, 1.0);
        let next = egui::lerp(start..=end, t);
        if (next - *value).abs() > f32::EPSILON {
            *value = next;
            response.mark_changed();
        }
    }
    theme::reveal_focus(&response);
    if ui.is_rect_visible(rect) {
        let t = ((*value - start) / (end - start)).clamp(0.0, 1.0);
        let x = egui::lerp(travel, t);
        paint_track(&*ui, rect, x, *value);
        let active = theme::animate(
            ui.ctx(),
            response.id,
            response.hovered() || response.dragged(),
            theme::MOTION_QUICK,
        );
        widgets::slider_thumb(ui, palette, pos2(x, rect.center().y), active);
    }
    let shown = f64::from(*value);
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), shown, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Glass, from clear on the left to solid on the right, on a wave that
/// flattens as it goes: the more solid, the stiller. While the system
/// reduces transparency it shows Solid and cannot be moved.
fn glass_wave(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let reduced = crate::backdrop::system_reduces_transparency();
    let mut tint = if reduced { 1.0 } else { app.settings.glass };
    let label = crate::i18n::gettext(app.locale, "Glass").into_owned();
    let response = ui
        .add_enabled_ui(!reduced, |ui| {
            track_slider(
                ui,
                &palette,
                &mut tint,
                0.0..=1.0,
                vec2(ui.available_width(), 30.0),
                &label,
                |ui, rect, thumb, _value| {
                    let wave = |from: f32, to: f32| -> Vec<Pos2> {
                        let mut points = Vec::new();
                        let mut x = from;
                        while x <= to {
                            let along = (x - rect.left()) / rect.width();
                            let height = 6.0 * (1.0 - along).powf(0.8);
                            let phase = (x - rect.left()) / 34.0 * std::f32::consts::TAU;
                            points.push(pos2(x, rect.center().y - height * phase.sin()));
                            x += 2.0;
                        }
                        points
                    };
                    let painter = ui.painter();
                    painter.add(egui::Shape::line(
                        wave(rect.left(), thumb),
                        Stroke::new(2.5, palette.accent),
                    ));
                    painter.add(egui::Shape::line(
                        wave(thumb, rect.right()),
                        Stroke::new(2.0, palette.secondary.gamma_multiply(0.45)),
                    ));
                },
            )
        })
        .inner;
    if response.changed() {
        app.settings.set_glass(tint);
        app.actions.push(Action::SettingsChanged);
    }
    response.on_hover_text(label);
}

/// A doodle tile's size.
const TILE: Vec2 = vec2(96.0, 92.0);
/// How tall the pictured part of a tile is.
const TILE_PICTURE: f32 = 66.0;

/// One tile in the doodle row.
#[derive(Clone, Copy, PartialEq)]
enum Doodle {
    None,
    Style(DoodleStyle),
    Photo,
}

/// The doodles: a row of tiles (none, each pattern, a photo of your own),
/// then how many there are and how strongly they show, each on a strip of
/// the real thing, and their colour.
pub(super) fn doodles(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    block_heading(ui, &palette, &crate::i18n::gettext(locale, "Doodles"), "");
    let look = app.wallpaper();
    // Each tile shows its whole pattern, whatever the density.
    let tile_look = wallpaper::Look {
        density: 1.0,
        ..look.clone()
    };
    let has_image = app.account().settings.wallpaper_image.is_some();
    let shown = app.settings.show_wallpaper;
    let current = app.settings.doodle_style;
    let chosen = if has_image {
        Doodle::Photo
    } else if !shown {
        Doodle::None
    } else {
        Doodle::Style(current)
    };
    let mut tiles = vec![Doodle::None];
    tiles.extend(DoodleStyle::ALL.into_iter().map(Doodle::Style));
    tiles.push(Doodle::Photo);
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt("appearance-doodles")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for tile in tiles {
                    let label = match tile {
                        Doodle::None => {
                            crate::i18n::pgettext(locale, "doodles", "None").into_owned()
                        }
                        Doodle::Style(style) => {
                            crate::i18n::gettext(locale, style.label()).into_owned()
                        }
                        Doodle::Photo => crate::i18n::gettext(locale, "Photo").into_owned(),
                    };
                    let picked = tile_button(
                        ui,
                        &palette,
                        tile == chosen,
                        &label,
                        tile_look.color,
                        |ui, picture| match tile {
                            Doodle::Style(style) => {
                                wallpaper::paint_doodles(ui, picture, &tile_look, style);
                            }
                            Doodle::None | Doodle::Photo => {
                                let icon = if tile == Doodle::Photo {
                                    Icon::Image
                                } else {
                                    Icon::Ban
                                };
                                theme::paint_icon(
                                    ui,
                                    icon,
                                    Rect::from_center_size(picture.center(), Vec2::splat(24.0)),
                                    24.0,
                                    palette.secondary,
                                );
                            }
                        },
                    );
                    if picked {
                        clicked = Some(tile);
                    }
                }
            });
        });
    match clicked {
        Some(Doodle::None) => {
            if has_image {
                app.actions.push(Action::RemoveWallpaperImage);
            }
            app.actions.push(Action::SetWallpaperDoodles(false));
        }
        Some(Doodle::Style(style)) => {
            if has_image {
                app.actions.push(Action::RemoveWallpaperImage);
            }
            app.actions.push(Action::SetDoodleStyle(style));
        }
        Some(Doodle::Photo) => app.actions.push(Action::PickWallpaperImage),
        None => {}
    }

    if chosen == Doodle::None || chosen == Doodle::Photo {
        ui.add_space(theme::SPACE_S);
        return;
    }
    ui.add_space(theme::SPACE_M);
    let style = current;
    let density = app
        .settings
        .doodle_density
        .clamp(*DOODLE_DENSITY.start(), *DOODLE_DENSITY.end());
    if let Some(density) = doodle_strip(
        ui,
        app,
        "Density",
        density,
        DOODLE_DENSITY,
        |look, value| look.density = value,
        style,
    ) {
        app.actions.push(Action::SetDoodleDensity(density));
    }
    let intensity = app
        .settings
        .doodle_intensity
        .clamp(*DOODLE_INTENSITY.start(), *DOODLE_INTENSITY.end());
    if let Some(intensity) = doodle_strip(
        ui,
        app,
        "Strength",
        intensity,
        DOODLE_INTENSITY,
        |look, value| look.intensity = value,
        style,
    ) {
        app.actions.push(Action::SetDoodleIntensity(intensity));
    }
    let current = app.settings.doodle_color;
    let options: Vec<_> = DoodleColor::ALL
        .into_iter()
        .map(|color| (color, crate::i18n::gettext(locale, color.label())))
        .collect();
    let mut picked = None;
    widgets::setting_row(
        ui,
        &palette,
        &crate::i18n::gettext(locale, "Doodle colour"),
        "",
        |ui| {
            picked = segmented(ui, &palette, "doodle-color", &options, Some(current), 88.0);
        },
    );
    if let Some(color) = picked {
        app.actions.push(Action::SetDoodleColor(color));
    }
}

/// How wide the names beside the doodle strips are.
const STRIP_LABEL: f32 = 84.0;

/// A slider for one doodle setting whose track is a strip of the wallpaper
/// itself, drawn with the value under the thumb, so dragging it shows the
/// doodles thinning out or filling in, fading or darkening, as they will.
fn doodle_strip(
    ui: &mut egui::Ui,
    app: &App,
    label: &'static str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    apply: impl Fn(&mut wallpaper::Look, f32),
    style: DoodleStyle,
) -> Option<f32> {
    let palette = app.palette;
    let name = crate::i18n::gettext(app.locale, label).into_owned();
    let base = app.wallpaper();
    let mut value = value;
    let mut moved = None;
    ui.horizontal(|ui| {
        let (name_rect, _) = ui.allocate_exact_size(vec2(STRIP_LABEL, 36.0), Sense::hover());
        ui.painter().text(
            pos2(name_rect.left(), name_rect.center().y),
            Align2::LEFT_CENTER,
            &name,
            theme::medium(theme::TEXT_BODY),
            palette.text,
        );
        let width = ui.available_width();
        let response = track_slider(
            ui,
            &palette,
            &mut value,
            range,
            vec2(width, 36.0),
            &name,
            |ui, rect, _thumb, value| {
                let mut look = base.clone();
                apply(&mut look, value);
                let painter = ui.painter();
                painter.rect_filled(rect, 10.0, look.color);
                wallpaper::paint_doodles(ui, rect.shrink2(vec2(8.0, 0.0)), &look, style);
                painter.rect_stroke(
                    rect,
                    10.0,
                    Stroke::new(1.0, palette.outline),
                    egui::StrokeKind::Inside,
                );
            },
        );
        if response.changed() {
            moved = Some(value);
        }
    });
    ui.add_space(theme::SPACE_S);
    moved
}

/// A tile with a picture above its name, ringed in the accent when chosen.
/// Returns whether it was clicked.
fn tile_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    chosen: bool,
    label: &str,
    fill: Color32,
    paint: impl FnOnce(&egui::Ui, Rect),
) -> bool {
    let (rect, response) = ui.allocate_exact_size(TILE, Sense::click());
    theme::reveal_focus(&response);
    let radius = 14.0;
    theme::focus_outline(ui, response.id, rect, radius);
    if ui.is_rect_visible(rect) {
        let t = theme::animate(ui.ctx(), response.id, chosen, theme::MOTION_QUICK);
        let picture = Rect::from_min_size(rect.min, vec2(rect.width(), TILE_PICTURE));
        let footer = Rect::from_min_max(pos2(rect.left(), picture.bottom()), rect.max);
        let r = radius as u8;
        let painter = ui.painter();
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
            picture,
            CornerRadius {
                nw: r,
                ne: r,
                sw: 0,
                se: 0,
            },
            fill,
        );
        paint(&*ui, picture);
        let color = if chosen { palette.accent } else { palette.text };
        let name = widgets::line(
            ui,
            label,
            theme::medium(theme::TEXT_CAPTION),
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
        let edge = if response.hovered() {
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
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, label)
    });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// The interface font as tiles, each showing "Aa" in its own typeface.
pub(super) fn fonts(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    block_heading(ui, &palette, &crate::i18n::gettext(locale, "Font"), "");
    let selected = app.settings.font;
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt("appearance-fonts")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for choice in FontChoice::ALL {
                    // Font family names stay in their own branding.
                    let label = match choice {
                        FontChoice::System => {
                            crate::i18n::gettext(locale, choice.label()).into_owned()
                        }
                        _ => choice.label().to_owned(),
                    };
                    let picked = tile_button(
                        ui,
                        &palette,
                        choice == selected,
                        &label,
                        palette.surface_hover,
                        |ui, picture| {
                            ui.painter().text(
                                picture.center(),
                                Align2::CENTER_CENTER,
                                "Aa",
                                theme::preview_font(choice, 28.0),
                                palette.text,
                            );
                        },
                    );
                    if picked {
                        clicked = Some(choice);
                    }
                }
            });
        });
    if let Some(choice) = clicked {
        app.actions.push(Action::SetFont(choice));
    }
    ui.add_space(theme::SPACE_S);
}

/// The zoom steps the slider stops at.
const ZOOM_STOPS: [f32; 9] = [0.8, 0.9, 1.0, 1.1, 1.2, 1.35, 1.5, 1.75, 2.0];

/// Where `zoom` falls along [`ZOOM_STOPS`], 0 at the first stop and 1 at
/// the last; zooms past the ends sit at them.
fn zoom_position(zoom: f32) -> f32 {
    let last = ZOOM_STOPS.len() - 1;
    if zoom <= ZOOM_STOPS[0] {
        return 0.0;
    }
    for index in 0..last {
        let (low, high) = (ZOOM_STOPS[index], ZOOM_STOPS[index + 1]);
        if zoom <= high {
            return (index as f32 + (zoom - low) / (high - low)) / last as f32;
        }
    }
    1.0
}

/// The stop nearest `position` along the slider.
fn zoom_stop(position: f32) -> f32 {
    let last = (ZOOM_STOPS.len() - 1) as f32;
    ZOOM_STOPS[(position.clamp(0.0, 1.0) * last).round() as usize]
}

/// Zoom as a stepped slider between a small and a large "A". The window
/// zooms when the thumb is let go, so the slider does not move under the
/// pointer while it is dragged.
pub(super) fn zoom(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let drag_id = ui.id().with("appearance-zoom-drag");
    let dragging: Option<f32> = ui.data(|data| data.get_temp(drag_id));
    let zoom = dragging.unwrap_or(app.settings.zoom);
    let title = crate::i18n::gettext(locale, "Zoom");
    ui.horizontal(|ui| {
        widgets::rich_text(ui, &title, theme::medium(theme::TEXT_BODY), palette.text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            theme::text(
                ui,
                format!("{:.0}%", zoom * 100.0),
                theme::medium(theme::TEXT_BODY),
                palette.secondary,
            );
        });
    });
    ui.add_space(theme::SPACE_XS);
    let (pill, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    ui.painter()
        .rect_filled(pill, pill.height() / 2.0, palette.surface);
    let small = pos2(pill.left() + 22.0, pill.center().y);
    let large = pos2(pill.right() - 24.0, pill.center().y);
    ui.painter().text(
        small,
        Align2::CENTER_CENTER,
        "A",
        theme::regular(theme::TEXT_CAPTION),
        palette.text,
    );
    ui.painter().text(
        large,
        Align2::CENTER_CENTER,
        "A",
        theme::regular(theme::TEXT_TITLE),
        palette.text,
    );
    let track = Rect::from_min_max(
        pos2(small.x + 20.0, pill.top()),
        pos2(large.x - 22.0, pill.bottom()),
    );
    let response = ui.interact(
        track,
        ui.id().with("appearance-zoom"),
        Sense::click_and_drag(),
    );
    theme::reveal_focus(&response);
    let thumb_radius = 11.0;
    let travel = track.left() + thumb_radius..=track.right() - thumb_radius;
    let pointer = response.interact_pointer_pos().filter(|_| {
        response.dragged() || response.is_pointer_button_down_on() || response.clicked()
    });
    if let Some(pointer) = pointer {
        let position = (pointer.x - travel.start()) / (travel.end() - travel.start());
        ui.data_mut(|data| data.insert_temp(drag_id, zoom_stop(position)));
    }
    if response.drag_stopped() || response.clicked() {
        let target = ui
            .data(|data| data.get_temp::<f32>(drag_id))
            .unwrap_or(app.settings.zoom);
        ui.data_mut(|data| data.remove::<f32>(drag_id));
        if (target - app.settings.zoom).abs() > 0.001 {
            app.actions.push(Action::ZoomBy(target - app.settings.zoom));
        }
    }
    if ui.is_rect_visible(track) {
        let painter = ui.painter();
        let y = track.center().y;
        let x = egui::lerp(travel.clone(), zoom_position(zoom));
        painter.line_segment(
            [pos2(*travel.start(), y), pos2(x, y)],
            Stroke::new(3.0, palette.accent),
        );
        painter.line_segment(
            [pos2(x, y), pos2(*travel.end(), y)],
            Stroke::new(3.0, palette.surface_active),
        );
        for stop in ZOOM_STOPS {
            let at = egui::lerp(travel.clone(), zoom_position(stop));
            let color = if at <= x {
                palette.accent
            } else {
                palette.secondary.gamma_multiply(0.6)
            };
            painter.circle_filled(pos2(at, y), 3.0, color);
        }
        let active = theme::animate(
            ui.ctx(),
            response.id,
            response.hovered() || response.dragged(),
            theme::MOTION_QUICK,
        );
        let radius = thumb_radius + active;
        for (grow, alpha) in [(4.0, 12u8), (2.0, 20)] {
            painter.circle_filled(
                pos2(x, y + 1.5),
                radius + grow,
                Color32::from_black_alpha(alpha),
            );
        }
        painter.circle_filled(pos2(x, y), radius, Color32::WHITE);
        painter.circle_stroke(
            pos2(x, y),
            radius,
            Stroke::new(1.0, Color32::from_black_alpha(30)),
        );
    }
    let percent = f64::from(zoom * 100.0);
    response.widget_info(|| egui::WidgetInfo::slider(true, percent, title.as_ref()));
    let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.add_space(theme::SPACE_M);
}

/// How tall a folding group's own row is.
const FOLD_ROW: f32 = 40.0;

/// Settings opened less often, folded under More: the language, motion and
/// whether every message shows its time. A search opens it.
pub(super) fn more(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        egui::Id::new("settings-appearance-more"),
        false,
    );
    if !app.settings_search.trim().is_empty() && !state.is_open() {
        state.set_open(true);
    }
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), FOLD_ROW), Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, 12.0);
    let title = crate::i18n::gettext(locale, "More");
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            title.as_ref(),
            theme::medium(theme::TEXT_BODY),
            palette.text,
        );
        let chevron = if state.is_open() {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        theme::paint_icon(
            ui,
            chevron,
            Rect::from_center_size(
                pos2(rect.right() - 10.0, rect.center().y),
                Vec2::splat(18.0),
            ),
            18.0,
            if response.hovered() {
                palette.text
            } else {
                palette.secondary
            },
        );
    }
    let open = state.is_open();
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::CollapsingHeader,
            ui.is_enabled(),
            open,
            title.as_ref(),
        )
    });
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        state.toggle(ui);
    }
    state.show_body_unindented(ui, |ui| {
        ui.add_space(theme::SPACE_S);
        widgets::setting_row(
            ui,
            &palette,
            &crate::i18n::gettext(locale, "Language"),
            "",
            |ui| language_picker(ui, app),
        );
        let options: Vec<_> = MotionPreference::ALL
            .into_iter()
            .map(|motion| (motion, crate::i18n::gettext(locale, motion.label())))
            .collect();
        let current = app.settings.motion_preference;
        let mut picked = None;
        widgets::setting_row(
            ui,
            &palette,
            &crate::i18n::gettext(locale, "Animations"),
            "",
            |ui| {
                picked = segmented(ui, &palette, "motion", &options, Some(current), 76.0);
            },
        );
        if let Some(motion) = picked {
            app.actions.push(Action::SetMotion(motion));
        }
        let mut always = app.settings.always_show_time;
        let label = crate::i18n::gettext(locale, "Always show time");
        let mut changed = false;
        widgets::setting_row(ui, &palette, &label, "", |ui| {
            let response = widgets::switch(ui, &palette, &mut always);
            theme::reveal_focus(&response);
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Checkbox,
                    ui.is_enabled(),
                    always,
                    label.as_ref(),
                )
            });
            changed = response.changed();
        });
        if changed {
            app.settings.always_show_time = always;
            app.actions.push(Action::SettingsChanged);
        }
        let wallpaper = crate::i18n::gettext(locale, "Wallpaper colour");
        widgets::setting_row(ui, &palette, &wallpaper, "", |ui| {
            if theme::soft_button(ui, &palette, Some(Icon::ChevronRight), &wallpaper, false)
                .clicked()
            {
                app.actions
                    .push(Action::Open(crate::model::Page::Wallpaper));
            }
        });
    });
    ui.add_space(theme::SPACE_XS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_on_the_wheel_is_drawn_where_it_is_picked() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 160.0));
        for hue in [0.0, 45.0, 120.0, 200.0, 290.0, 359.0] {
            for strength in [0.2, 0.5, 1.0] {
                let place = CustomAccent { hue, strength };
                let back = wheel_at(rect, wheel_point(rect, place));
                let turn = (back.hue - hue).rem_euclid(360.0);
                assert!(
                    turn.min(360.0 - turn) < 0.5,
                    "{hue} came back as {}",
                    back.hue
                );
                assert!((back.strength - strength).abs() < 0.01);
            }
        }
    }

    #[test]
    fn the_middle_of_the_wheel_settles_on_the_themes_own_accent() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(300.0, 160.0));
        let middle = CustomAccent {
            hue: 120.0,
            strength: 0.03,
        };
        assert_eq!(settle(middle, rect), Action::SetAccent(Accent::Theme));
    }

    #[test]
    fn zoom_stops_map_both_ways() {
        for stop in ZOOM_STOPS {
            assert!((zoom_stop(zoom_position(stop)) - stop).abs() < f32::EPSILON);
        }
        assert_eq!(zoom_position(0.6), 0.0);
        assert_eq!(zoom_position(1.0), 2.0 / 8.0);
        assert_eq!(zoom_position(2.0), 1.0);
        // A zoom between stops, as Ctrl+Plus leaves it, sits between them.
        let between = zoom_position(1.3);
        assert!(between > zoom_position(1.2) && between < zoom_position(1.35));
    }
}
