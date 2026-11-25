//! Settings > Appearance, a stack of unoutlined cards as under Chats: a theme
//! card (the live preview of a chat, a row of theme tiles to scroll, the
//! accent swatches and the colour wheel's dialog), a card for the mode (a
//! dropdown), Glass and the wallpaper's tint, the Doodles accordion alone,
//! a card for the font and zoom, and one for two folded groups (Interface,
//! Motion). The theme and the accent are two choices, one over the other.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};

use super::{Control, Group, Row, draw_entries, language_picker, segmented, translated};
use crate::app::App;
use crate::chat_themes::{Accent, ChatTheme, CustomAccent, Mode};
use crate::model::Action;
use crate::motion::MotionPreference;
use crate::settings::{DOODLE_DENSITY, DOODLE_INTENSITY, DoodleColor, DoodleStyle, FontChoice};
use crate::theme::{self, Icon, Palette};
use crate::ui::controls;
use crate::ui::widgets;
use crate::wallpaper;

/// How tall the preview is at the top of the theme card.
pub(super) const PREVIEW_HEIGHT: f32 = 260.0;

/// A group chat drawn by the transcript's own rows in the colours and bubble
/// style on screen, on the chat's wallpaper, filling `rect` at the top of the
/// theme card, so a change of theme, mode, accent, bubbles, glass, wallpaper
/// or motion shows as it would in the chats. Its top corners follow the
/// card's; the bottom ones are square, where the card goes on.
pub(super) fn preview(ui: &mut egui::Ui, app: &App, rect: Rect) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let locale = app.locale;
    let text = |source: &'static str| crate::i18n::gettext(locale, source).into_owned();
    let radius = theme::RADIUS_CARD;
    let look = app.wallpaper();
    ui.painter_at(rect).rect_filled(
        rect,
        CornerRadius {
            nw: radius,
            ne: radius,
            sw: 0,
            se: 0,
        },
        look.color,
    );
    if look.doodles && look.image.is_none() {
        // Rounded as the colour is, so none show past the card's corners.
        wallpaper::paint_doodles(
            ui,
            rect,
            CornerRadius {
                nw: radius,
                ne: radius,
                sw: 0,
                se: 0,
            },
            &look,
            look.style,
        );
    }

    // A pause, then Ava, our reply to her, Leo and our last word. Ids no
    // chat has, so nothing kept for a real chat's rows is shared.
    const CHAT: &str = "appearance-preview@g.us";
    const ME: &str = "appearance-preview-me@s.whatsapp.net";
    const AVA: &str = "appearance-preview-ava@s.whatsapp.net";
    const LEO: &str = "appearance-preview-leo@s.whatsapp.net";
    let start = crate::util::now() - 20 * 60;
    let message = |id: &str, sender: &str, name: Option<&str>, at: i64, body: String| {
        let own = sender == ME;
        crate::model::Message {
            id: id.to_owned(),
            chat: CHAT.to_owned(),
            sender: sender.to_owned(),
            sender_name: name.map(str::to_owned),
            from_me: own,
            timestamp: start + at,
            history_order: None,
            content: crate::model::Content::Text {
                text: body,
                preview: None,
            },
            status: if own {
                crate::model::Delivery::Read
            } else {
                crate::model::Delivery::None
            },
            delivered_at: own.then_some(start + at),
            read_at: own.then_some(start + at),
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    };
    let asked = text("Did you see the new themes?");
    let mut reply = message("reply", ME, None, 40, text("Trying them all right now"));
    reply.quoted = Some(crate::model::Quoted {
        chat: None,
        id: "asked".to_owned(),
        sender: AVA.to_owned(),
        sender_name: Some("Ava".to_owned()),
        summary: asked.clone(),
        labelled: Some(false),
        mentions: Vec::new(),
    });
    let messages = [
        message("asked", AVA, Some("Ava"), 0, asked),
        reply,
        message(
            "light",
            LEO,
            Some("Leo"),
            90,
            text("Pick one with a light mode too"),
        ),
        message("going", ME, None, 120, text("Going with it")),
    ];
    let chat = crate::model::Chat::new(CHAT.to_owned(), String::new());
    // The newest arrives again whenever Motion changes, so its setting can
    // be seen.
    let arrive_id = egui::Id::new(("appearance-preview-arrive", app.settings.motion_preference));
    let arrival = theme::appear(ui.ctx(), arrive_id, theme::MOTION_PANEL);
    crate::ui::conversation::still_transcript(ui, app, rect, &chat, &messages, ME, arrival);
}

/// A settings card's frame: the shared fill, with `margin` of room inside
/// it. Cards carry no outline, as on the other settings pages.
fn card_frame(palette: &Palette, margin: egui::Margin) -> egui::Frame {
    egui::Frame::new()
        .fill(theme::blend(palette.panel, palette.surface, 0.5))
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(margin)
}

/// The room between a card's edge and its controls.
const CARD_PAD: f32 = 16.0;

/// A faint hairline between two rows of one card, in the style of the
/// settings page's separators.
fn hairline(ui: &mut egui::Ui, palette: &Palette) {
    ui.add_space(theme::SPACE_XS);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(theme::OUTLINE_STROKE, palette.text.gamma_multiply(0.07)),
    );
    ui.add_space(theme::SPACE_M);
}

/// The theme card, Telegram's way: the preview flush at the top, a row of
/// theme tiles to scroll, then the accent swatches. A theme and an accent
/// are two choices, one over the other. It is a card of its own on the
/// page (drawn bare, so the preview meets the card's rounded top corners).
pub(super) fn theme_card(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    card_frame(&palette, egui::Margin::ZERO).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), PREVIEW_HEIGHT), Sense::hover());
        preview(ui, app, rect);
        ui.add_space(theme::SPACE_L);
        theme_tiles(ui, app);
        ui.add_space(theme::SPACE_M);
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().hline(
            line.x_range().shrink(CARD_PAD),
            line.center().y,
            Stroke::new(theme::OUTLINE_STROKE, palette.text.gamma_multiply(0.07)),
        );
        ui.add_space(theme::SPACE_S);
        accent_swatches(ui, app);
        ui.add_space(theme::SPACE_S);
    });
    accent_modal(ui.ctx(), app);
}

/// The card under the theme card: the mode, Glass and the wallpaper's tint,
/// a hairline between each.
pub(super) fn look_card(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let label = crate::i18n::gettext(app.locale, "Mode");
    widgets::setting_row(ui, &palette, &label, "", |ui| mode_menu(ui, app));
    // macOS draws no glass, so its slider would move nothing.
    if !cfg!(target_os = "macos") {
        hairline(ui, &palette);
        glass_wave(ui, app);
        // The room a setting row leaves under itself.
        ui.add_space(theme::SPACE_S);
    }
    hairline(ui, &palette);
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
}

/// Auto, light and dark as a dropdown, the closed button naming the current
/// mode (none is ticked while a theme file is in use).
fn mode_menu(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let custom = app.settings.custom_theme.is_some();
    let current = app.settings.mode;
    let selected = (!custom).then_some(current);
    let label = crate::i18n::gettext(locale, "Mode");
    let closed = crate::i18n::gettext(locale, current.label());
    let response = controls::dropdown(
        ui,
        &palette,
        "appearance_mode",
        &label,
        closed.as_ref(),
        controls::WIDTH,
        |ui| {
            for mode in Mode::ALL {
                // Fixed ids, so the demo tour can point at the options.
                let id = match mode {
                    Mode::System => "settings-mode-auto",
                    Mode::Light => "settings-mode-light",
                    Mode::Dark => "settings-mode-dark",
                };
                let option = ui.scope(|ui| {
                    controls::option_in(
                        ui,
                        &palette,
                        &crate::i18n::gettext(locale, mode.label()),
                        selected == Some(mode),
                        theme::regular(theme::TEXT_BODY),
                    )
                });
                // Hover only: the option underneath keeps the click (and
                // closes the list), the tour only reads where it is.
                ui.interact(option.response.rect, egui::Id::new(id), Sense::hover());
                if option.inner {
                    app.actions.push(Action::SetMode(mode));
                    if custom {
                        // A theme file has one mode of its own: leave it for
                        // the ready-made theme last used, in the mode asked
                        // for.
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
        },
    );
    // A fixed id on the closed button, so the demo tour can open it.
    ui.interact(
        response.rect,
        egui::Id::new("settings-mode"),
        Sense::hover(),
    );
}

/// The space between the colour wheel's dots.
const DOT_GAP: f32 = 15.0;
/// The hue at the wheel's right-hand edge, in OKLCH degrees: pink on the
/// right, blues and purples above, oranges and yellows below, greens left.
const HUE_AT_RIGHT: f32 = 350.0;
/// How close to the middle, or to a named accent, a pick lands on it.
const SNAP: f32 = 0.1;
/// How near the knob's middle a tap takes the chosen accent off: the knob
/// and its white ring.
const KNOB_REACH: f32 = 16.0;

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

/// Whether the theme's own accent is in use, so its swatch shows as chosen
/// and the wheel shows no knob. The accent is independent of the theme: it
/// lies over any of them, theme files included.
fn own_accent(app: &App) -> bool {
    app.settings.accent == Accent::Theme
        && app
            .settings
            .custom_accent
            .is_none_or(|custom| custom.strength <= CustomAccent::CENTRE)
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

/// Dots and knob for a colour wheel already allocated in `rect`.
#[allow(clippy::too_many_arguments)]
fn draw_colour_wheel(
    ui: &mut egui::Ui,
    rect: Rect,
    response: &egui::Response,
    palette: &Palette,
    enabled: bool,
    place: CustomAccent,
    show_knob: bool,
    knob_fill: Color32,
    on_knob: impl Fn(Pos2) -> bool,
) {
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
    // The lens follows the knob: with no colour picked from the wheel there
    // is no knob, and the dots lie flat, easing out as it goes.
    let lens = theme::animate(
        ui.ctx(),
        response.id.with("lens"),
        enabled && show_knob,
        theme::MOTION_QUICK,
    );
    let painter = ui.painter();
    for row in 0..rows {
        for column in 0..columns {
            let at = origin + vec2(column as f32 * DOT_GAP, row as f32 * DOT_GAP);
            // Dots swell near the knob, as under a lens.
            let near = lens * (1.0 - at.distance(knob) / (DOT_GAP * 3.0)).max(0.0);
            let color = colours[row * columns + column];
            let color = if enabled {
                color
            } else {
                color.gamma_multiply(0.35)
            };
            painter.circle_filled(at, 3.0 + 2.5 * near, color);
        }
    }
    if enabled && show_knob {
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
        painter.circle_filled(knob, size, knob_fill);
    }
    if enabled && (response.hovered() || response.dragged()) {
        let over_knob = !response.dragged() && response.hover_pos().is_some_and(on_knob);
        ui.ctx().set_cursor_icon(if over_knob {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Crosshair
        });
    }
}

/// The accent as a field of dots: drag the knob round for the hue and out
/// for the strength. The middle is the theme's own accent, and the named
/// accents catch the knob when it is let go near one.
fn colour_wheel(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let width = ui.available_width();
    let height = (width * 0.45).clamp(130.0, 190.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    theme::reveal_focus(&response);

    // Where the knob stood before this frame's pick moved it, and whether
    // a press began on it: a tap there, not dragged away, takes the chosen
    // accent off again for the theme's own.
    let shown = !own_accent(app);
    let before = wheel_point(rect, current_place(app));
    let on_knob = |pointer: Pos2| shown && pointer.distance(before) <= KNOB_REACH;
    let grabbed_id = response.id.with("knob");
    if response.is_pointer_button_down_on() && ui.input(|input| input.pointer.primary_pressed()) {
        let grabbed = response.interact_pointer_pos().is_some_and(on_knob);
        ui.ctx()
            .data_mut(|data| data.insert_temp(grabbed_id, grabbed));
    }
    let grabbed = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(grabbed_id))
        .unwrap_or(false);
    let cleared = response.clicked() && grabbed;
    if cleared {
        ui.ctx().data_mut(|data| data.remove::<bool>(grabbed_id));
        app.actions.push(Action::SetAccent(Accent::Theme));
    }

    let pressed = response.dragged() || response.is_pointer_button_down_on() || response.clicked();
    let picked = response
        .interact_pointer_pos()
        .filter(|_| pressed && !cleared)
        .map(|pointer| wheel_at(rect, pointer));
    if let Some(place) = picked
        && app.settings.custom_accent != Some(place)
    {
        app.actions.push(Action::SetCustomAccent(Some(place)));
    }
    if !cleared && (response.drag_stopped() || response.clicked()) {
        let place = picked.unwrap_or_else(|| current_place(app));
        app.actions.push(settle(place, rect));
    }
    let place = picked.unwrap_or_else(|| current_place(app));
    response.widget_info(|| {
        egui::WidgetInfo::slider(
            true,
            f64::from(place.hue),
            crate::i18n::gettext(app.locale, "Accent").as_ref(),
        )
    });

    draw_colour_wheel(
        ui,
        rect,
        &response,
        &palette,
        true,
        place,
        picked.is_some() || !own_accent(app),
        palette.accent,
        on_knob,
    );
}

/// The doodle ink colour on its own map: every pick is stored as
/// [`Action::SetDoodleCustom`], never the app accent.
fn doodle_colour_wheel(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let width = ui.available_width();
    let height = (width * 0.34).clamp(96.0, 130.0);
    let (rect, response) = ui
        .push_id("doodle-colour-wheel", |ui| {
            ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag())
        })
        .inner;
    theme::reveal_focus(&response);
    let stored = app
        .settings
        .doodle_custom
        .unwrap_or_else(|| current_place(app));
    let pressed = response.dragged() || response.is_pointer_button_down_on() || response.clicked();
    let picked = response
        .interact_pointer_pos()
        .filter(|_| pressed)
        .map(|pointer| wheel_at(rect, pointer));
    if let Some(place) = picked
        && app.settings.doodle_custom != Some(place)
    {
        app.actions.push(Action::SetDoodleCustom(place));
    }
    if response.drag_stopped() || response.clicked() {
        let place = picked.unwrap_or(stored);
        if app.settings.doodle_custom != Some(place) {
            app.actions.push(Action::SetDoodleCustom(place));
        }
    }
    let place = picked.unwrap_or(stored);
    let knob = {
        let [r, g, b] = place.rgb(palette.dark);
        Color32::from_rgb(r, g, b)
    };
    response.widget_info(|| {
        egui::WidgetInfo::slider(
            true,
            f64::from(place.hue),
            crate::i18n::gettext(app.locale, "Custom").as_ref(),
        )
    });
    let knob_at = wheel_point(rect, stored);
    let on_knob = |pointer: Pos2| pointer.distance(knob_at) <= KNOB_REACH;
    draw_colour_wheel(
        ui, rect, &response, &palette, true, place, true, knob, on_knob,
    );
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

/// How wide a theme tile is.
const TILE_WIDTH: f32 = 76.0;
/// How tall a theme tile's picture is.
const TILE_PICTURE_HEIGHT: f32 = 54.0;
/// How tall the name under a tile is.
const TILE_NAME_HEIGHT: f32 = 22.0;
/// The space between tiles.
const TILE_GAP: f32 = 12.0;

/// The themes as a row of small chat pictures to scroll: two bubbles on the
/// theme's background, in its colours and the accent in use, its name below.
/// Only Zeus's own themes are offered: each comes in Dark and Light and takes
/// any accent, which is the reader's own choice below, so a tile is chosen
/// whatever it is. Palette files and the shared presets, which have one mode
/// and colours of their own, are not listed.
fn theme_tiles(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let custom = app.settings.custom_theme.is_some();
    let accent = app.settings.accent;
    let wheel = app.settings.custom_accent;
    let items: Vec<(ChatTheme, Option<Palette>, bool, String)> = ChatTheme::ALL
        .into_iter()
        .map(|choice| {
            (
                choice,
                Some(choice.palette_with(palette.dark, accent, wheel)),
                !custom && app.settings.chat_theme == choice,
                choice.label().to_owned(),
            )
        })
        .collect();

    // The row opens with the chosen tile in view, and again whenever the
    // choice changes from elsewhere; a click on a tile leaves it where it is.
    let seen_id = egui::Id::new("appearance-theme-tile-seen");
    let chosen = items
        .iter()
        .position(|(_, _, selected, _)| *selected)
        .map(|index| (index, items[index].3.clone()));
    let seen: Option<String> = ui.data(|data| data.get_temp(seen_id));
    let mut area = egui::ScrollArea::horizontal()
        .id_salt("appearance-theme-tiles")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some((index, name)) = &chosen
        && seen.as_deref() != Some(name.as_str())
    {
        let middle = CARD_PAD + *index as f32 * (TILE_WIDTH + TILE_GAP) + TILE_WIDTH / 2.0;
        area = area.horizontal_scroll_offset((middle - ui.available_width() / 2.0).max(0.0));
        let name = name.clone();
        ui.data_mut(|data| data.insert_temp(seen_id, name));
    }

    let mut clicked = None;
    area.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = TILE_GAP;
            ui.add_space(CARD_PAD - TILE_GAP);
            for (choice, colours, selected, name) in &items {
                // Built-in themes by name, so the demo tour can point at them.
                let id = egui::Id::new(format!("settings-theme-{}", choice.label()));
                let (rect, response) = theme_tile(ui, &palette, id, *selected, name, colours);
                if response.clicked() {
                    let picture =
                        Rect::from_min_size(rect.min, vec2(rect.width(), TILE_PICTURE_HEIGHT));
                    clicked = Some((*choice, picture.center()));
                }
            }
            ui.add_space(CARD_PAD - TILE_GAP);
        });
    });
    // A theme leaves the accent as it is.
    if let Some((choice, center)) = clicked {
        app.actions.push(Action::RevealThemeFrom(center));
        app.actions.push(Action::SetChatTheme(choice));
    }
}

/// One theme tile: a rounded chat picture (an incoming bubble top left, an
/// outgoing one bottom right) with its name centred below. The chosen tile
/// has a ring in the accent and its name in it. `colours` is `None` for the
/// tile that adds a theme file.
fn theme_tile(
    ui: &mut egui::Ui,
    palette: &Palette,
    id: egui::Id,
    chosen: bool,
    name: &str,
    colours: &Option<Palette>,
) -> (Rect, egui::Response) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(TILE_WIDTH, TILE_PICTURE_HEIGHT + TILE_NAME_HEIGHT),
        Sense::hover(),
    );
    let response = ui.interact(rect, id, Sense::click());
    theme::reveal_focus(&response);
    let picture = Rect::from_min_size(rect.min, vec2(rect.width(), TILE_PICTURE_HEIGHT));
    let radius = 12.0;
    theme::focus_outline(ui, response.id, picture.expand(3.0), radius + 3.0);
    if ui.is_rect_visible(rect) {
        let t = theme::animate(ui.ctx(), response.id, chosen, theme::MOTION_QUICK);
        let painter = ui.painter();
        match colours {
            Some(own) => {
                painter.rect_filled(picture, radius, own.chat);
                let bubble = vec2(36.0, 14.0);
                let incoming = Rect::from_min_size(picture.min + vec2(8.0, 9.0), bubble);
                let outgoing = Rect::from_min_size(
                    pos2(
                        picture.right() - 8.0 - bubble.x,
                        picture.bottom() - 9.0 - bubble.y,
                    ),
                    bubble,
                );
                painter.rect_filled(incoming, 6.0, own.bubble_in);
                painter.rect_filled(outgoing, 6.0, own.bubble_out);
                painter.rect_stroke(
                    picture,
                    radius,
                    Stroke::new(1.0, palette.outline),
                    egui::StrokeKind::Inside,
                );
            }
            None => {
                painter.rect_filled(picture, radius, palette.surface);
                theme::paint_icon(
                    ui,
                    Icon::Plus,
                    Rect::from_center_size(picture.center(), Vec2::splat(20.0)),
                    20.0,
                    palette.secondary,
                );
            }
        }
        let painter = ui.painter();
        if t > 0.0 {
            painter.rect_stroke(
                picture.expand(3.0),
                radius + 3.0,
                Stroke::new(2.0, palette.accent.gamma_multiply(t)),
                egui::StrokeKind::Inside,
            );
        } else if response.hovered() {
            painter.rect_stroke(
                picture.expand(3.0),
                radius + 3.0,
                Stroke::new(1.0, palette.secondary),
                egui::StrokeKind::Inside,
            );
        }
        let color = if chosen {
            palette.accent
        } else {
            palette.secondary
        };
        let label = widgets::line(
            ui,
            name,
            theme::medium(theme::TEXT_CAPTION),
            color,
            rect.width(),
            1,
        );
        label.paint(
            ui,
            pos2(
                rect.center().x - label.size().x / 2.0,
                picture.bottom() + TILE_NAME_HEIGHT / 2.0 - label.size().y / 2.0 + 1.0,
            ),
            color,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, name)
    });
    let response = response
        .on_hover_text(name)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    (rect, response)
}

/// Whether a colour from the wheel is laid over the theme.
fn wheel_in_use(app: &App) -> bool {
    app.settings
        .custom_accent
        .is_some_and(|custom| custom.strength > CustomAccent::CENTRE)
}

/// The accents as a row of round swatches: first a cluster of dots that opens
/// the colour wheel, then the theme's own and each named accent, a small
/// light dot in each as in an outgoing bubble. The one in use wears a ring.
fn accent_swatches(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    // The theme the swatches are drawn over, in its own accent.
    let base = app
        .settings
        .custom_theme
        .as_ref()
        .and(app.settings.custom_theme_cache.as_ref())
        .map_or_else(
            || app.settings.chat_theme.palette(palette.dark, Accent::Theme),
            |theme| theme.palette,
        );
    let wheel_on = wheel_in_use(app);
    let count = Accent::ALL.len() + 1;
    let width = ui.available_width() - 2.0 * CARD_PAD;
    let step = (width / count as f32).min(46.0);
    let diameter = (step - 10.0).clamp(18.0, 30.0);
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let first = row.center().x - step * count as f32 / 2.0 + step / 2.0;
    let ring =
        |ui: &egui::Ui, response: &egui::Response, center: Pos2, chosen: bool, ink: Color32| {
            theme::focus_outline(ui, response.id, response.rect, response.rect.height() / 2.0);
            let t = theme::animate(ui.ctx(), response.id, chosen, theme::MOTION_QUICK);
            if t > 0.0 {
                ui.painter().circle_stroke(
                    center,
                    diameter / 2.0 + 4.0,
                    Stroke::new(2.0, ink.gamma_multiply(t)),
                );
            } else if response.hovered() {
                ui.painter().circle_stroke(
                    center,
                    diameter / 2.0 + 4.0,
                    Stroke::new(1.0, palette.secondary),
                );
            }
        };

    // The cluster of dots that opens the wheel.
    let center = pos2(first, row.center().y);
    let rect = Rect::from_center_size(center, Vec2::splat(diameter + 10.0));
    let response = ui.interact(rect, egui::Id::new("settings-accent-wheel"), Sense::click());
    theme::reveal_focus(&response);
    ring(ui, &response, center, wheel_on, palette.text);
    if ui.is_rect_visible(rect) {
        let dot = diameter * 0.12;
        let reach = diameter * 0.33;
        let colour = |hue: f32| {
            let [r, g, b] = CustomAccent::wheel_rgb(hue, 1.0, palette.dark);
            Color32::from_rgb(r, g, b)
        };
        let painter = ui.painter();
        painter.circle_filled(center, dot * 1.15, colour(300.0));
        for (index, hue) in [350.0, 40.0, 95.0, 160.0, 215.0, 265.0]
            .into_iter()
            .enumerate()
        {
            let angle = (index as f32 * 60.0 - 90.0).to_radians();
            painter.circle_filled(
                center + vec2(angle.cos(), angle.sin()) * reach,
                dot,
                colour(hue),
            );
        }
    }
    let title = crate::i18n::gettext(locale, "Accent colour");
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, wheel_on, title.as_ref())
    });
    if response
        .on_hover_text(title.as_ref())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        ui.ctx()
            .data_mut(|data| data.insert_temp(accent_modal_id(), true));
    }

    for (index, accent) in Accent::ALL.into_iter().enumerate() {
        let center = pos2(first + step * (index + 1) as f32, row.center().y);
        let rect = Rect::from_center_size(center, Vec2::splat(diameter + 10.0));
        let response = ui.interact(
            rect,
            egui::Id::new(("settings-accent", accent.label())),
            Sense::click(),
        );
        theme::reveal_focus(&response);
        let shown = accent.rgb().map_or(base, |rgb| base.with_accent(rgb));
        let chosen = !wheel_on && app.settings.accent == accent;
        ring(ui, &response, center, chosen, shown.accent_fill);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.circle_filled(center, diameter / 2.0, shown.accent_fill);
            painter.circle_filled(center, diameter * 0.2, shown.bubble_out);
        }
        let label = crate::i18n::gettext(locale, accent.label());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, label.as_ref())
        });
        if response
            .on_hover_text(label.as_ref())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.actions.push(Action::SetAccent(accent));
        }
    }
}

/// Where the open flag of the colour wheel's dialog is kept.
fn accent_modal_id() -> egui::Id {
    egui::Id::new("appearance-accent-modal-open")
}

/// The colour wheel in a dialog of its own, open while its flag is set.
fn accent_modal(ctx: &egui::Context, app: &mut App) {
    if !ctx
        .data(|data| data.get_temp::<bool>(accent_modal_id()))
        .unwrap_or(false)
    {
        return;
    }
    let palette = app.palette;
    let locale = app.locale;
    let frame = egui::Frame::new()
        .fill(palette.menu_glass())
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(theme::RADIUS_DIALOG))
        .inner_margin(egui::Margin::same(22))
        .shadow(palette.modal_shadow());
    let mut done = false;
    let response = egui::Modal::new(egui::Id::new("appearance-accent-modal"))
        .frame(frame)
        .backdrop_color(palette.shadow)
        .show(ctx, |ui| {
            ui.set_width(320.0);
            theme::text(
                ui,
                crate::i18n::gettext(locale, "Accent colour").into_owned(),
                theme::semibold(theme::TEXT_TITLE),
                palette.text,
            );
            ui.add_space(theme::SPACE_M);
            colour_wheel(ui, app);
            ui.add_space(theme::SPACE_M);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = crate::i18n::gettext(locale, "Done");
                    done = theme::pill_button(ui, &palette, &label, true).clicked();
                });
            });
        });
    if done || response.should_close() {
        ctx.data_mut(|data| data.remove::<bool>(accent_modal_id()));
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
    // Named above it, as the App icon block is, so the wave reads as a setting.
    widgets::rich_text(ui, &label, theme::medium(theme::TEXT_BODY), palette.text);
    ui.add_space(theme::SPACE_XS);
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

/// The App icon block: a wrapped grid of rounded-square previews, the one in
/// use ringed in the accent, a click switching at once. The choice dresses
/// the Dock tile, window, tray and taskbar of the running app only; the
/// installed bundle's icon (Finder, launchers) is not modified. Tiles carry
/// no caption: the name is the hover text and the accessible label.
pub(super) fn app_icon(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let title = crate::i18n::gettext(locale, "App icon");
    widgets::rich_text(ui, &title, theme::medium(theme::TEXT_BODY), palette.text);
    ui.add_space(theme::SPACE_S);
    let textures = app_icon_textures(ui.ctx());
    let current = app.settings.app_icon;
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
        for (icon, texture) in crate::app_icons::AppIcon::ALL.into_iter().zip(&textures) {
            let label = crate::i18n::gettext(locale, icon.label()).into_owned();
            let (rect, response) =
                ui.allocate_exact_size(Vec2::splat(APP_ICON_TILE), Sense::click());
            theme::reveal_focus(&response);
            theme::focus_outline(ui, response.id, rect, APP_ICON_RING);
            if ui.is_rect_visible(rect) {
                let chosen = icon == current;
                let picture = rect.shrink(4.0);
                egui::Image::new((texture.id(), picture.size())).paint_at(ui, picture);
                let painter = ui.painter();
                if chosen {
                    painter.rect_stroke(
                        rect,
                        APP_ICON_RING,
                        Stroke::new(2.0, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                } else if response.hovered() {
                    painter.rect_stroke(
                        rect,
                        APP_ICON_RING,
                        Stroke::new(1.0, palette.secondary),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::RadioButton,
                    true,
                    icon == current,
                    &label,
                )
            });
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(&label)
                .clicked()
            {
                clicked = Some(icon);
            }
        }
    });
    if let Some(icon) = clicked
        && icon != current
    {
        app.actions.push(Action::SetAppIcon(icon));
    }
}

/// An app icon tile's side, ring included.
const APP_ICON_TILE: f32 = 64.0;
/// The ring's corner radius, a little outside the squircle's.
const APP_ICON_RING: f32 = 18.0;
/// Pixels each preview is rasterized at, for tiles drawn at 56 points.
const APP_ICON_PREVIEW_PX: usize = 112;

/// Where the previews are kept, with the frame that last drew them.
fn app_icon_previews_id() -> egui::Id {
    egui::Id::new("app-icon-previews")
}

/// The frame number [`release_app_icon_previews`] compares against: the
/// root window's, which is where Settings draws.
fn root_frame(ctx: &egui::Context) -> u64 {
    ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT)
}

/// Frees the previews (about 700 KB of textures) once a whole frame has
/// passed without the block drawing them: Settings closed, another category
/// open, or a search that leaves the block out. Called every frame.
pub(super) fn release_app_icon_previews(ctx: &egui::Context) {
    let id = app_icon_previews_id();
    let stale = ctx
        .data(|data| data.get_temp::<(u64, Vec<egui::TextureHandle>)>(id))
        .is_some_and(|(drawn, _)| drawn + 1 < root_frame(ctx));
    if stale {
        ctx.data_mut(|data| data.remove::<(u64, Vec<egui::TextureHandle>)>(id));
    }
}

/// The previews of every icon, rasterized the first time the block draws and
/// kept in the context while it keeps drawing, so no frame decodes an SVG.
fn app_icon_textures(ctx: &egui::Context) -> Vec<egui::TextureHandle> {
    let id = app_icon_previews_id();
    let frame = root_frame(ctx);
    let held = ctx.data(|data| data.get_temp::<(u64, Vec<egui::TextureHandle>)>(id));
    if let Some((_, textures)) = held {
        ctx.data_mut(|data| data.insert_temp(id, (frame, textures.clone())));
        return textures;
    }
    let textures: Vec<_> = crate::app_icons::AppIcon::ALL
        .into_iter()
        .map(|icon| {
            let side = APP_ICON_PREVIEW_PX;
            let rgba =
                crate::app_icons::rgba(icon, side).unwrap_or_else(|| vec![0; side * side * 4]);
            let image = egui::ColorImage::from_rgba_unmultiplied([side, side], &rgba);
            ctx.load_texture(
                format!("app-icon-{icon:?}"),
                image,
                egui::TextureOptions::LINEAR,
            )
        })
        .collect();
    ctx.data_mut(|data| data.insert_temp(id, (frame, textures.clone())));
    textures
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

/// The Doodles accordion, closed until clicked: its body is
/// [`doodles_body`]. Anything else about the doodles (such as their colour
/// following the theme or standing alone) is another child of this group.
pub(super) fn doodles(ui: &mut egui::Ui, app: &mut App) {
    let group = Group {
        summary: Default::default(),
        children: vec![(Row::default(), Control::Block(Box::new(doodles_body)))],
        searching: !app.settings_search.trim().is_empty(),
    };
    let entries = vec![(
        Row {
            title: translated(app.locale, "Doodles"),
            ..Row::default()
        },
        Control::Group(group),
    )];
    accordion_entries(ui, app, entries);
}

/// A row of tiles (none, each pattern, a photo of your own), then how many
/// doodles there are and how strongly they show, each on a strip of the real
/// thing, and their colour.
fn doodles_body(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
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
                        |ui, picture, corners| match tile {
                            Doodle::Style(style) => {
                                wallpaper::paint_doodles(ui, picture, corners, &tile_look, style);
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
            picked = segmented(ui, &palette, "doodle-color", &options, Some(current), 76.0);
        },
    );
    let mut colour = current;
    if let Some(color) = picked {
        colour = color;
        if color == DoodleColor::Custom && app.settings.doodle_custom.is_none() {
            app.actions.push(Action::SetDoodleColor(color));
            app.actions
                .push(Action::SetDoodleCustom(current_place(app)));
        } else {
            app.actions.push(Action::SetDoodleColor(color));
        }
    }
    if colour == DoodleColor::Custom {
        ui.add_space(theme::SPACE_M);
        doodle_colour_wheel(ui, app);
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
                wallpaper::paint_doodles(ui, rect, 10.0, &look, style);
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

/// Draws folded groups with the shared accordion of the settings page.
fn accordion_entries(ui: &mut egui::Ui, app: &mut App, entries: Vec<(Row, Control)>) {
    let palette = app.palette;
    draw_entries(ui, app, &palette, entries, theme::SPACE_XS);
}

/// A tile with a picture above its name, ringed in the accent when chosen.
/// Returns whether it was clicked.
fn tile_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    chosen: bool,
    label: &str,
    fill: Color32,
    paint: impl FnOnce(&egui::Ui, Rect, CornerRadius),
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
        let corners = CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        };
        painter.rect_filled(picture, corners, fill);
        paint(&*ui, picture, corners);
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

/// The interface font as a dropdown: the closed button names the current
/// typeface and each choice in the list is set in its own.
pub(super) fn fonts(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    widgets::setting_row(
        ui,
        &palette,
        &crate::i18n::gettext(locale, "Font"),
        "",
        |ui| {
            let selected = app.settings.font;
            // Font family names stay in their own branding; only System is words.
            let name = |choice: FontChoice| match choice {
                FontChoice::System => crate::i18n::gettext(locale, choice.label()).into_owned(),
                _ => choice.label().to_owned(),
            };
            controls::dropdown(
                ui,
                &palette,
                "interface_font",
                &crate::i18n::gettext(locale, "Font"),
                &name(selected),
                controls::WIDTH,
                |ui| {
                    for choice in FontChoice::ALL {
                        if controls::option_in(
                            ui,
                            &palette,
                            &name(choice),
                            choice == selected,
                            theme::preview_font(choice, theme::TEXT_BODY),
                        ) {
                            app.actions.push(Action::SetFont(choice));
                        }
                    }
                },
            );
        },
    );
}

/// Font and Zoom, one card with a hairline between them.
pub(super) fn font_and_zoom(ui: &mut egui::Ui, app: &mut App) {
    fonts(ui, app);
    let palette = app.palette;
    hairline(ui, &palette);
    zoom(ui, app);
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

/// Settings opened less often, in two folded groups: Interface (language,
/// whether every message shows its time, the wallpaper's colour) and Motion
/// (animations and their speed). A search opens them.
pub(super) fn more(ui: &mut egui::Ui, app: &mut App) {
    let locale = app.locale;
    let searching = !app.settings_search.trim().is_empty();
    let row = |title: &'static str, description: &'static str| Row {
        title: translated(locale, title),
        description: translated(locale, description),
        keywords: Vec::new(),
    };
    let interface = vec![
        (row("Language", ""), Control::Row(Box::new(language_picker))),
        (
            row("Always show time", ""),
            Control::Toggle(|settings| &mut settings.always_show_time),
        ),
        (
            row("Wallpaper colour", ""),
            Control::Row(Box::new(|ui, app| {
                let palette = app.palette;
                let label = crate::i18n::gettext(app.locale, "Wallpaper colour");
                if theme::soft_button(ui, &palette, Some(Icon::ChevronRight), &label, false)
                    .clicked()
                {
                    app.actions
                        .push(Action::Open(crate::model::Page::Wallpaper));
                }
            })),
        ),
    ];
    let motion = vec![
        (
            row("Animations", ""),
            Control::Row(Box::new(|ui, app| {
                let palette = app.palette;
                let locale = app.locale;
                let options: Vec<_> = MotionPreference::ALL
                    .into_iter()
                    .map(|motion| (motion, crate::i18n::gettext(locale, motion.label())))
                    .collect();
                let current = app.settings.motion_preference;
                if let Some(motion) =
                    segmented(ui, &palette, "motion", &options, Some(current), 76.0)
                {
                    app.actions.push(Action::SetMotion(motion));
                }
            })),
        ),
        (
            row(
                "Motion speed",
                "Fast runs the chat's springs and animations in 70% of their time.",
            ),
            Control::Row(Box::new(super::motion_picker)),
        ),
    ];
    let group = |title: &'static str, children| {
        (
            Row {
                title: translated(locale, title),
                ..Row::default()
            },
            Control::Group(Group {
                summary: Default::default(),
                children,
                searching,
            }),
        )
    };
    let entries = vec![
        group("Interface", interface),
        (Row::default(), Control::Separator),
        group("Motion", motion),
    ];
    accordion_entries(ui, app, entries);
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
