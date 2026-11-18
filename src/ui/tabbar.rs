//! The tab bar that floats at the foot of the chat list's column, as
//! Telegram's desktop app has one: a rounded pill of four tabs, left to
//! right Chats, Calls, our own avatar (the account switcher, which rises from
//! the bar as a drawer, `super::accounts::tab_button`) and Settings. The tab
//! on screen stands on a plate of its own, which slides to the next one
//! picked. The list scrolls on under the bar: a list drawn beside it leaves
//! [`RESERVE`] at its foot, so its last row comes to rest clear of it.
//!
//! Beside a hidden or collapsed list there is no bar; the rail
//! (`super::rail`) leads to the same places, and both lead there through
//! [`Destination::actions`].

use egui::{CornerRadius, Rect, Sense, Shape, Stroke, pos2, vec2};

use crate::app::App;
use crate::i18n::gettext;
use crate::theme::{self, Icon, Palette};

use super::focus::{Stop, TabStop};
use super::rail::Destination;

/// Height of the pill.
pub const HEIGHT: f32 = 52.0;
/// The gap between the pill and its column's bottom and sides.
pub const MARGIN: f32 = 12.0;
/// The room a list scrolling under the pill leaves at its foot, so its last
/// row rests a margin above the pill rather than under it.
pub const RESERVE: f32 = HEIGHT + 2.0 * MARGIN;

/// How many tabs the bar holds.
const TABS: usize = 4;
/// The widest a tab's cell grows: past four of these the pill stops
/// growing with its column and stands in its middle.
const CELL_WIDTH: f32 = 72.0;
/// Room between the pill's edge and its tabs' cells. Each cell, the whole of
/// its share of the pill less this, is its tab's target: 44 points tall.
const PAD: f32 = 4.0;
/// How far the lit plate sits inside the pill, at its ends as above and
/// below, so its round ends are concentric with the pill's.
const PLATE_INSET: f32 = 6.0;
/// How round a plate is: wholly, at either end.
const PLATE_RADIUS: f32 = HEIGHT / 2.0 - PLATE_INSET;
/// A tab's icon.
const ICON: f32 = 22.0;

/// One of the bar's tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Chats,
    Calls,
    Profile,
    Settings,
}

impl Tab {
    /// Every tab, left to right.
    pub const ALL: [Tab; 4] = [Tab::Chats, Tab::Calls, Tab::Profile, Tab::Settings];

    /// Where Tab reaches it: the rail's own stops, which are never on
    /// screen at the same time as the bar.
    fn stop(self) -> Stop {
        match self {
            Self::Chats => Stop::Chats,
            Self::Calls => Stop::Calls,
            Self::Profile => Stop::Profile,
            Self::Settings => Stop::Settings,
        }
    }
}

/// Stable id of a tab's response, for tests and the demo.
pub fn tab_id(tab: Tab) -> egui::Id {
    egui::Id::new(("tab-bar-tab", tab as u8))
}

/// Where the pill was laid out this frame, while it is drawn: toasts rise
/// above it, and layout tests read it.
pub(crate) fn rect_id() -> egui::Id {
    egui::Id::new("tab-bar")
}

/// The pill's layer, above the list that scrolls under it.
fn layer_id() -> egui::Id {
    rect_id().with("layer")
}

/// The pill's own target under its tabs, so a click between two tabs, or at
/// the pill's rounded ends, does not reach a row scrolled under it.
fn reach_id() -> egui::Id {
    rect_id().with("reach")
}

/// The tab on screen: Profile while the account drawer is open, else the
/// page's. The chat list's Starred, Archived and Locked views are Chats, and
/// the wallpaper page is part of Settings.
pub fn lit(app: &App, ctx: &egui::Context) -> Tab {
    if super::accounts::drawer_open(ctx) {
        Tab::Profile
    } else if Destination::Calls.lit(app) {
        Tab::Calls
    } else if Destination::Settings.lit(app) {
        Tab::Settings
    } else {
        Tab::Chats
    }
}

/// Where the pill sits in `column`: centred, a margin above its foot, and as
/// wide as four roomy tabs or the column less its margins, whichever is less.
pub(crate) fn pill_rect(column: Rect) -> Rect {
    let width = (column.width() - 2.0 * MARGIN)
        .min(TABS as f32 * CELL_WIDTH)
        .max(0.0);
    Rect::from_min_size(
        pos2(
            column.center().x - width / 2.0,
            column.bottom() - MARGIN - HEIGHT,
        ),
        vec2(width, HEIGHT),
    )
}

/// The cell of the tab at `position` (0 for the first, fractions between
/// two) in `pill`.
fn cell_at(pill: Rect, position: f32) -> Rect {
    let inner = pill.shrink(PAD);
    let width = inner.width() / TABS as f32;
    Rect::from_min_size(
        pos2(inner.left() + position * width, inner.top()),
        vec2(width, inner.height()),
    )
}

/// Each tab's cell in `pill`, left to right.
fn cells(pill: Rect) -> [Rect; TABS] {
    std::array::from_fn(|index| cell_at(pill, index as f32))
}

/// The plate a lit or hovered tab stands on in its `cell`: a pill of its
/// own, its ends concentric with the bar's.
fn plate(cell: Rect) -> Rect {
    cell.shrink(PLATE_INSET - PAD)
}

/// A plate's corners, from [`PLATE_RADIUS`].
fn plate_corners() -> CornerRadius {
    CornerRadius::same(PLATE_RADIUS as u8)
}

/// The tight shadow the pill casts on the list under it. Three short layers:
/// a contact line, then two small falls. Alphas are for a light list; a dark
/// list needs a little more before they read.
pub(crate) fn shadow_layers(palette: &Palette) -> [egui::epaint::Shadow; 3] {
    const LAYERS: [(i8, u8, f32); 3] = [(1, 1, 18.0), (1, 2, 12.0), (2, 4, 8.0)];
    let [r, g, b, _] = palette.shadow.to_srgba_unmultiplied();
    let strength = if palette.dark { 1.4 } else { 1.0 };
    LAYERS.map(|(y, blur, alpha)| egui::epaint::Shadow {
        offset: [0, y],
        blur,
        spread: 0,
        color: egui::Color32::from_rgba_unmultiplied(
            r,
            g,
            b,
            (alpha * strength).round().min(255.0) as u8,
        ),
    })
}

pub(crate) fn paint_shadow(
    painter: &egui::Painter,
    pill: Rect,
    corners: CornerRadius,
    palette: &Palette,
) {
    for shadow in shadow_layers(palette) {
        painter.add(shadow.as_shape(pill, corners));
    }
}

/// The bar at the foot of `column`, floating over what the column holds.
/// The list under it scrolls on beneath: give it [`RESERVE`] at its foot.
pub fn show(app: &mut App, ui: &mut egui::Ui, column: Rect) {
    let pill = pill_rect(column);
    if pill.width() <= 0.0 {
        return;
    }
    ui.ctx().data_mut(|data| data.insert_temp(rect_id(), pill));
    // A narrow window's slide moves only what is drawn on its view's own
    // layer, so the copies it draws of the list carry a copy of the bar
    // there, solid, as their header is.
    let copy = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(super::chats::slide_copy_id()))
        .is_some();
    let builder = egui::UiBuilder::new().id_salt("tab-bar").max_rect(pill);
    if copy {
        let mut bar_ui = ui.new_child(builder);
        bar(app, &mut bar_ui, pill, false);
        return;
    }
    // On a layer of its own above the list, as the list's header is, so its
    // glass joins the window's sheet (see `super::glass`).
    let layer = egui::LayerId::new(egui::Order::Middle, layer_id());
    let mut bar_ui = ui.new_child(builder.layer_id(layer));
    bar_ui.interact(pill, reach_id(), Sense::CLICK | Sense::DRAG);
    let frosted = app.palette.vibrant() && crate::backdrop::blur();
    bar(app, &mut bar_ui, pill, frosted);
}

/// The pill and its tabs in `pill`, over frosted glass when `frosted`.
fn bar(app: &mut App, ui: &mut egui::Ui, pill: Rect, frosted: bool) {
    let palette = app.palette;
    let painter = ui.painter().clone();
    let corners = CornerRadius::same((HEIGHT / 2.0) as u8);
    // Raised off the list: a tight shadow under it and a hairline round it,
    // the line that parts the bar from the rows scrolling beneath.
    paint_shadow(&painter, pill, corners, &palette);
    let (fill, edge) = if frosted {
        painter.add(super::glass::sheet_rounded(pill, HEIGHT / 2.0));
        (palette.frosted(palette.overlay), palette.glass_edge())
    } else {
        (
            palette.overlay,
            Stroke::new(theme::OUTLINE_STROKE, palette.outline),
        )
    };
    painter.rect_filled(pill, corners, fill);
    painter.rect_stroke(pill, corners, edge, egui::StrokeKind::Inside);

    let ctx = ui.ctx().clone();
    let on_screen = lit(app, &ctx);
    // One plate, sliding from tab to tab (`Tab::ALL` is in their order);
    // held within the pill at its ends, where the spring's bounce would carry
    // it past them.
    let target = f32::from(on_screen as u8);
    let spring = crate::motion::Spring::SNAPPY;
    let position = crate::motion::spring_to(&ctx, rect_id().with("plate"), target, spring);
    let position = position.clamp(0.0, (TABS - 1) as f32);
    painter.rect_filled(
        plate(cell_at(pill, position)),
        plate_corners(),
        palette.tint(theme::TINT_SELECTED),
    );

    for (tab, cell) in Tab::ALL.into_iter().zip(cells(pill)) {
        // The hover's plate goes under what the tab draws, once its
        // response says whether it is hovered.
        let under = painter.add(Shape::Noop);
        let response = match tab {
            Tab::Profile => super::accounts::tab_button(app, ui, tab_id(tab), cell, pill),
            _ => tab_response(app, ui, tab, cell, on_screen == tab),
        };
        theme::focus_outline(ui, response.id, plate(cell), PLATE_RADIUS);
        let hover = theme::animate(
            &ctx,
            response.id.with("tab-hover"),
            (response.hovered() || response.has_focus()) && on_screen != tab,
            theme::MOTION_QUICK,
        );
        if hover > 0.0 {
            painter.set(
                under,
                Shape::rect_filled(
                    plate(cell),
                    plate_corners(),
                    palette.tint(theme::TINT_HOVER).gamma_multiply(hover),
                ),
            );
        }
        if tab != Tab::Profile && ui.is_rect_visible(cell) {
            paint_tab(app, ui, tab, cell, &response);
        }
        let response = response.tab_stop(tab.stop());
        if tab != Tab::Profile && response.clicked() {
            let actions = destination(tab).actions(app);
            app.actions.extend(actions);
        }
    }
}

/// Where a tab other than Profile leads: the rail's destination of the same
/// name, whose actions bring the window there (Chats back to all chats from
/// Starred, Archived or Locked; Settings closed again from Settings).
fn destination(tab: Tab) -> Destination {
    match tab {
        Tab::Calls => Destination::Calls,
        Tab::Settings => Destination::Settings,
        Tab::Chats | Tab::Profile => Destination::Chats,
    }
}

/// The icon a tab other than Profile shows.
fn icon(tab: Tab) -> Icon {
    match tab {
        Tab::Calls => Icon::TabCalls,
        Tab::Settings => Icon::TabSettings,
        Tab::Chats | Tab::Profile => Icon::TabChats,
    }
}

/// Seconds the chat bubble's tilt takes.
const WOBBLE_SECONDS: f32 = 0.35;
/// Seconds the phone's ringing shake takes.
const RING_SECONDS: f32 = 0.42;
/// Seconds the gear takes to turn, in either direction.
const GEAR_SECONDS: f32 = 0.35;
/// How far the gear turns while hovered.
const GEAR_TURN: f32 = 60.0 * std::f32::consts::PI / 180.0;
/// Seconds our avatar takes to grow, and to shrink back.
const AVATAR_SECONDS: f32 = 0.15;
/// How much larger our avatar is while hovered.
const AVATAR_GROW: f32 = 0.06;
/// The scale of a pressed tab.
const PRESSED: f32 = 0.92;
/// Where the bubble's tail is in its icon, as a share of its size: the
/// tilt turns about it.
const TAIL: egui::Vec2 = egui::vec2(0.08, 0.92);

fn radians(degrees: f32) -> f32 {
    degrees * std::f32::consts::PI / 180.0
}

/// The chat bubble's tilt in radians, `t` (0 to 1) through its play: a
/// damped swing, first back about 10 degrees, then forward about 5, and
/// at rest again (0) at the end. Never past 13 degrees.
fn wobble(t: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        return 0.0;
    }
    -radians(13.0) * (std::f32::consts::TAU * t).sin() * (-1.1 * t).exp()
}

/// The phone's shake in radians, `t` (0 to 1) through its play: three
/// swings dying away, about 12 degrees at first, at rest at the end. Never
/// past 13 degrees.
fn ring(t: f32) -> f32 {
    if t <= 0.0 || t >= 1.0 {
        return 0.0;
    }
    radians(13.0) * (3.0 * std::f32::consts::TAU * t).sin() * (1.0 - t)
}

/// The spring a tab follows to its pressed scale: quick, no bounce.
const PRESS_DOWN: crate::motion::Spring = crate::motion::Spring::settling_in(0.1);
/// The spring a released tab follows back to 1: it passes it by about 3%.
const PRESS_UP: crate::motion::Spring = crate::motion::Spring::new(0.12, 0.7);

/// A tab's scale while the pointer presses it: 0.92 on the way down, then
/// back to 1 with a little overshoot. Only a pointer presses; a key's
/// activation never reaches here.
pub(super) fn press_scale(ctx: &egui::Context, id: egui::Id, down: bool) -> f32 {
    let spring = if down { PRESS_DOWN } else { PRESS_UP };
    let target = if down { PRESSED } else { 1.0 };
    crate::motion::spring_to(ctx, id.with("tab-press"), target, spring)
}

/// What a hover's one-shot play remembers between frames.
#[derive(Clone, Copy)]
struct Play {
    start: f64,
    hovered: bool,
}

/// How far a one-shot play, begun when the pointer enters, has run: 0 to 1,
/// and 1 at rest (also while motion is reduced). A play under way finishes
/// before another starts. Asks for frames only while it runs.
fn hover_play(ctx: &egui::Context, id: egui::Id, hovered: bool, seconds: f32) -> f32 {
    let seconds = f64::from(theme::motion_time(ctx, seconds));
    if seconds <= 0.0 {
        return 1.0;
    }
    let now = ctx.input(|input| input.time);
    let id = id.with("tab-play");
    let mut play = ctx.data(|data| data.get_temp::<Play>(id)).unwrap_or(Play {
        start: f64::NEG_INFINITY,
        hovered: false,
    });
    let running = now - play.start < seconds;
    if hovered && !play.hovered && !running {
        play.start = now;
    }
    play.hovered = hovered;
    ctx.data_mut(|data| data.insert_temp(id, play));
    let progress = ((now - play.start) / seconds).clamp(0.0, 1.0) as f32;
    if progress < 1.0 {
        crate::motion::request_frame(ctx);
    }
    progress
}

/// The scale of our avatar's tab: grown while the pointer is over it, shrunk
/// and sprung back as it is pressed.
pub(super) fn avatar_scale(ctx: &egui::Context, response: &egui::Response) -> f32 {
    let grow = ctx.animate_bool_with_time_and_easing(
        response.id.with("tab-grow"),
        response.hovered(),
        theme::motion_time(ctx, AVATAR_SECONDS),
        egui::emath::easing::cubic_out,
    );
    (1.0 + AVATAR_GROW * grow) * press_scale(ctx, response.id, response.is_pointer_button_down_on())
}

/// What a waiting release says on Settings, which shows it, while there is
/// one.
fn update_label(app: &App) -> Option<String> {
    let release = app.update.as_ref()?;
    let label = gettext(app.locale, "Update available, version {current} to {next}")
        .replace("{current}", env!("CARGO_PKG_VERSION"))
        .replace("{next}", &release.version);
    Some(label)
}

/// The target of a tab other than Profile, the whole of its `cell`, with its
/// name and shortcut in the tooltip, as the rail's destinations have them.
fn tab_response(app: &App, ui: &egui::Ui, tab: Tab, cell: Rect, lit: bool) -> egui::Response {
    // What the click does now: from Settings it closes them, whichever tab
    // is lit.
    let settings = if Destination::Settings.lit(app) {
        "Close settings (Ctrl+,)"
    } else {
        "Settings (Ctrl+,)"
    };
    let label = match tab {
        Tab::Calls => format!(
            "{} ({})",
            gettext(app.locale, "Calls"),
            super::keys::label("Ctrl+Shift+2")
        ),
        Tab::Settings => super::keys::label(settings),
        Tab::Chats | Tab::Profile => format!(
            "{} ({})",
            gettext(app.locale, "Chats"),
            super::keys::label("Ctrl+Shift+1")
        ),
    };
    let response = ui
        .interact(cell, tab_id(tab), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    theme::reveal_focus(&response);
    let spoken = match (tab, update_label(app)) {
        (Tab::Settings, Some(update)) => format!("{label}, {update}"),
        _ => label.clone(),
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            lit,
            spoken.as_str(),
        )
    });
    response.on_hover_text(label)
}

/// A tab's icon in its `cell`, in the text colour, thin. Under the pointer
/// it plays its own motion (the bubble tilts, the phone rings, the gear
/// turns), and pressed it shrinks and springs back. A dot on Settings says a
/// release is waiting there.
fn paint_tab(app: &App, ui: &egui::Ui, tab: Tab, cell: Rect, response: &egui::Response) {
    let palette = &app.palette;
    let ctx = ui.ctx();
    // The pointer only: a focused tab, or one a key activates, stays still.
    let hovered = response.hovered();
    let (angle, origin) = match tab {
        Tab::Chats | Tab::Profile => (
            wobble(hover_play(ctx, response.id, hovered, WOBBLE_SECONDS)),
            TAIL,
        ),
        Tab::Calls => (
            ring(hover_play(ctx, response.id, hovered, RING_SECONDS)),
            egui::vec2(0.5, 0.5),
        ),
        Tab::Settings => {
            let turn = ctx.animate_bool_with_time_and_easing(
                response.id.with("tab-turn"),
                hovered,
                theme::motion_time(ctx, GEAR_SECONDS),
                egui::emath::easing::cubic_out,
            );
            (GEAR_TURN * turn, egui::vec2(0.5, 0.5))
        }
    };
    let size = ICON * press_scale(ctx, response.id, response.is_pointer_button_down_on());
    let rect = Rect::from_center_size(cell.center(), vec2(size, size));
    icon(tab)
        .image(palette.text, size)
        .rotate(angle, origin)
        .paint_at(ui, rect);
    if tab == Tab::Settings && app.update.is_some() {
        // Ringed in the pill's colour, so it stands off the gear.
        let at = cell.center() + vec2(ICON / 2.0 - 1.0, -ICON / 2.0 + 1.0);
        ui.painter().circle_filled(at, 5.0, palette.overlay);
        ui.painter().circle_filled(at, 3.5, palette.accent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, Page};
    use crate::ui::accounts::{drawer_id, drawer_open};

    fn app() -> (tempfile::TempDir, App) {
        let root = tempfile::tempdir().unwrap();
        let app = App::headless(
            crate::paths::AppDirs::under(root.path()),
            crate::settings::Settings::default(),
        )
        .0;
        (root, app)
    }

    fn samples() -> impl Iterator<Item = f32> {
        (0..=200).map(|step| step as f32 / 200.0)
    }

    /// The bubble's tilt starts and ends at rest, goes back about 10 degrees
    /// first and forward about 5 after, and never past 13.
    #[test]
    fn the_wobble_rests_at_both_ends_and_stays_bounded() {
        assert_eq!(wobble(0.0), 0.0);
        assert_eq!(wobble(1.0), 0.0);
        assert_eq!(wobble(2.0), 0.0);
        let low = samples().map(wobble).fold(0.0, f32::min);
        let high = samples().map(wobble).fold(0.0, f32::max);
        assert!((radians(9.0)..=radians(13.0)).contains(&-low), "{low}");
        assert!((radians(4.0)..=radians(7.0)).contains(&high), "{high}");
        assert!(wobble(0.1) < 0.0, "back first");
    }

    /// The phone's shake rests at both ends, swings both ways three times,
    /// and stays within 13 degrees.
    #[test]
    fn the_ring_rests_at_both_ends_and_stays_bounded() {
        assert_eq!(ring(0.0), 0.0);
        assert_eq!(ring(1.0), 0.0);
        let peak = samples().map(|t| ring(t).abs()).fold(0.0, f32::max);
        assert!(
            peak <= radians(13.0) + 1e-6 && peak >= radians(10.0),
            "{peak}"
        );
        let crossings = samples()
            .map(ring)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| pair[0] * pair[1] < 0.0)
            .count();
        assert!((4..=6).contains(&crossings), "{crossings}");
    }

    /// A press settles on 0.92, a release springs back past 1 by about
    /// 3% and comes to rest on it.
    #[test]
    fn a_press_shrinks_and_a_release_springs_back_with_overshoot() {
        let down = |t: f64| PRESSED + (1.0 - PRESSED) * (1.0 - PRESS_DOWN.value(t));
        let up = |t: f64| PRESSED + (1.0 - PRESSED) * PRESS_UP.value(t);
        assert!((down(0.0) - 1.0).abs() < 1e-6);
        assert!((down(2.0) - PRESSED).abs() < 1e-3);
        assert!(samples().all(|t| down(f64::from(t) * 0.3) >= PRESSED - 1e-3));
        assert!((up(0.0) - PRESSED).abs() < 1e-6);
        assert!((up(3.0) - 1.0).abs() < 1e-6, "at rest on 1");
        let peak = samples()
            .map(|t| up(f64::from(t) * 0.5))
            .fold(0.0, f32::max);
        assert!((1.01..=1.05).contains(&peak), "{peak}");
    }

    /// A list column `width` by `height` points at the window's top left.
    fn column(width: f32, height: f32) -> Rect {
        Rect::from_min_size(egui::Pos2::ZERO, vec2(width, height))
    }

    const WINDOW: egui::Vec2 = egui::Vec2::new(900.0, 600.0);

    fn frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, WINDOW)),
                events,
                ..Default::default()
            },
            |ui| show(app, ui, column(320.0, WINDOW.y)),
        );
        output.textures_delta.clear();
    }

    fn click(app: &mut App, ctx: &egui::Context, tab: Tab) {
        let pos = ctx
            .read_response(tab_id(tab))
            .expect("the tab is drawn")
            .rect
            .center();
        frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        frame(app, ctx, Vec::new());
    }

    /// One tab is lit at a time: the page's, with the chat list's own views
    /// under Chats, or Profile while the account drawer is open.
    #[test]
    fn the_lit_tab_follows_the_page_and_the_drawer() {
        let (_root, mut app) = app();
        let ctx = egui::Context::default();
        assert_eq!(lit(&app, &ctx), Tab::Chats);
        app.show_starred = true;
        assert_eq!(lit(&app, &ctx), Tab::Chats, "Starred too");
        app.show_starred = false;
        app.show_archived = true;
        assert_eq!(lit(&app, &ctx), Tab::Chats, "Archived too");
        app.show_archived = false;
        app.page = Page::Calls;
        assert_eq!(lit(&app, &ctx), Tab::Calls);
        app.page = Page::Settings;
        assert_eq!(lit(&app, &ctx), Tab::Settings);
        app.page = Page::Wallpaper;
        assert_eq!(lit(&app, &ctx), Tab::Settings, "the wallpaper too");
        egui::Popup::open_id(&ctx, drawer_id());
        assert_eq!(lit(&app, &ctx), Tab::Profile);
        egui::Popup::close_id(&ctx, drawer_id());
        assert_eq!(lit(&app, &ctx), Tab::Settings);
    }

    /// A list that leaves `RESERVE` at its foot rests its last row a margin
    /// above the pill, which stands a margin above the column's foot.
    #[test]
    fn the_reserve_clears_the_pill_by_a_margin() {
        let column = column(320.0, 700.0);
        let pill = pill_rect(column);
        assert!((pill.height() - HEIGHT).abs() < 1e-3);
        assert!((column.bottom() - MARGIN - pill.bottom()).abs() < 1e-3);
        let last_row = column.bottom() - RESERVE;
        assert!((pill.top() - MARGIN - last_row).abs() < 1e-3);
    }

    /// The pill keeps its margins in the narrowest column, with targets 44
    /// points tall, and stops growing in a wide one, standing in its middle.
    #[test]
    fn the_pill_fits_a_narrow_column_and_stands_centred_in_a_wide_one() {
        for width in [200.0, 260.0, 320.0, 520.0] {
            let column = column(width, 600.0);
            let pill = pill_rect(column);
            assert!(
                pill.left() >= column.left() + MARGIN - 1e-3
                    && pill.right() <= column.right() - MARGIN + 1e-3,
                "{width}: {pill:?}"
            );
            assert!((pill.center().x - column.center().x).abs() < 1e-3);
            assert!(pill.width() <= TABS as f32 * CELL_WIDTH + 1e-3);
            let cells = cells(pill);
            for cell in cells {
                assert!(pill.contains_rect(cell), "{width}: {cell:?}");
                assert!(cell.height() >= 44.0, "{width}: {cell:?}");
                assert!(cell.contains_rect(plate(cell)));
            }
            for pair in cells.windows(2) {
                assert!(pair[0].right() <= pair[1].left() + 1e-3);
            }
            if width >= 260.0 {
                assert!(cells[0].width() >= 44.0, "{width}: {:?}", cells[0]);
            }
        }
        let wide = pill_rect(column(520.0, 600.0));
        assert!((wide.width() - TABS as f32 * CELL_WIDTH).abs() < 1e-3);
        // A column too narrow for its margins has no bar at all.
        assert!(pill_rect(column(10.0, 600.0)).width() <= 0.0);
    }

    /// The bar records where it is, draws every tab inside it, and its tabs
    /// lead where the rail's destinations do; our avatar opens the drawer,
    /// which lights its tab.
    #[test]
    fn the_tabs_lead_where_the_rail_does() {
        let (_root, mut app) = app();
        let ctx = egui::Context::default();
        app.attach(&ctx);
        frame(&mut app, &ctx, Vec::new());
        let pill = ctx
            .data(|data| data.get_temp::<Rect>(rect_id()))
            .expect("the bar records where it is");
        assert_eq!(pill, pill_rect(column(320.0, WINDOW.y)));
        for tab in Tab::ALL {
            let rect = ctx.read_response(tab_id(tab)).expect("every tab").rect;
            assert!(pill.contains_rect(rect), "{tab:?}: {rect:?}");
        }

        // Calls opens the call history.
        app.actions.clear();
        click(&mut app, &ctx, Tab::Calls);
        let actions = app.actions.as_slice();
        assert!(matches!(actions, [Action::Open(Page::Calls)]));

        // Settings opens the settings.
        app.actions.clear();
        click(&mut app, &ctx, Tab::Settings);
        let actions = app.actions.as_slice();
        assert!(matches!(actions, [Action::ToggleSettings]));

        // Our avatar opens the drawer, and nothing else yet.
        app.actions.clear();
        click(&mut app, &ctx, Tab::Profile);
        assert!(drawer_open(&ctx));
        assert_eq!(lit(&app, &ctx), Tab::Profile);
        assert!(app.actions.is_empty());
    }
}
