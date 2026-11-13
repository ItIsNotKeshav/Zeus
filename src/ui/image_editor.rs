//! Crops, turns and draws on a staged picture over the whole window, before
//! it is sent: a pen and a highlighter in a few colours, emoji and stickers
//! laid on top, and the one-time view switch of a single photo.

use egui::{Align, Color32, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};

use super::widgets;
use crate::app::{App, Pending};
use crate::model::{
    Action, CropEdge, PictureCrop, PictureMark, PictureSource, PictureSticker, PictureStroke,
    StickerArt,
};
use crate::theme::{self, Icon};

/// Room left around the cropper, in points.
const MARGIN: f32 = 28.0;
/// Height of the row of controls above and below the picture.
const BAR: f32 = 44.0;
/// Side of a corner or edge handle, in points.
const HANDLE: f32 = 13.0;
/// How far around a handle a drag still counts as gripping it.
const GRIP: f32 = 22.0;
/// The crop's border, in points.
const BORDER: f32 = 2.0;
/// The round button that keeps the edit, like the composer's send button.
const KEEP: f32 = 36.0;
/// A tool's round button in the row under the picture.
const TOOL: f32 = 34.0;
/// A colour swatch.
const SWATCH: f32 = 22.0;
/// How wide the pen draws on screen, in points; the line keeps that width
/// against the picture whatever its size.
const PEN_WIDTH: f32 = 5.0;
/// How wide the highlighter draws on screen, in points.
const HIGHLIGHTER_WIDTH: f32 = 20.0;
/// A new sticker's side, as a share of the crop's shorter side.
const STICKER_SHARE: f32 = 0.28;
/// The smallest a sticker can be made, in points on screen.
const STICKER_LEAST: f32 = 24.0;
/// The sticker tray's height when open.
const TRAY: f32 = 132.0;
/// A cell of the sticker tray.
const TRAY_CELL: f32 = 40.0;

/// The colours the pen and highlighter offer, straight sRGB.
const COLOURS: [[u8; 3]; 8] = [
    [255, 255, 255],
    [17, 17, 17],
    [239, 68, 68],
    [249, 115, 22],
    [250, 204, 21],
    [34, 197, 94],
    [59, 130, 246],
    [168, 85, 247],
];
/// Their names, for the tooltips and screen readers.
const COLOUR_NAMES: [&str; 8] = [
    "White", "Black", "Red", "Orange", "Yellow", "Green", "Blue", "Purple",
];

/// The emoji the sticker tray offers before the user's own stickers.
const EMOJI: [&str; 24] = [
    "😀", "😂", "😍", "🥰", "😎", "🤩", "🥳", "😮", "🤔", "😭", "😡", "😴", "👍", "👏", "🙏", "💪",
    "👀", "❤️", "🔥", "✨", "🎉", "💯", "⭐", "💀",
];

/// What a drag on the picture does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Tool {
    /// Moves the crop and its edges, and the stickers on the picture.
    #[default]
    Crop,
    Pen,
    Highlighter,
    /// The tray of emoji and stickers is open.
    Sticker,
}

impl Tool {
    fn draws(self) -> bool {
        matches!(self, Self::Pen | Self::Highlighter)
    }
}

/// The editor's own state between frames: the tool, the colour, the line
/// being drawn, and the sticker last picked up.
#[derive(Clone, Debug, Default)]
struct State {
    tool: Tool,
    /// An index into [`COLOURS`].
    colour: usize,
    /// The line under the pointer, in the picture's pixels, until it lifts.
    drawing: Option<PictureStroke>,
    /// The sticker that shows its handles, an index into the edit's marks.
    selected: Option<usize>,
}

fn state_id() -> egui::Id {
    egui::Id::new("picture-editor-state")
}

/// A change asked for by the controls under the picture. It is recorded while
/// the window is drawn and applied afterwards, so the edit is not consumed
/// from inside the closure that borrows it.
#[derive(Clone, Copy)]
enum Change {
    Reset,
    Turn { clockwise: bool },
    Undo,
}

/// The cropper's layer.
pub(crate) fn area_id() -> egui::Id {
    egui::Id::new("picture-editor")
}

/// Where the cropper notes its "Done" button, which focus enters on when the
/// keys or a screen reader opened it.
pub(crate) fn keep_note_id() -> egui::Id {
    egui::Id::new("picture-editor-keep")
}

/// Draws the editor over the whole window and records whatever its controls
/// asked for. The staged picture is left alone until the edit is kept.
pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut edit) = app.picture_edit.clone() else {
        // A closed editor starts the next picture with the crop tool.
        ctx.data_mut(|data| data.remove::<State>(state_id()));
        return;
    };
    let palette = app.palette;
    let locale = app.locale;
    let gettext = |text: &'static str| crate::i18n::gettext(locale, text).into_owned();
    let mut state = ctx
        .data(|data| data.get_temp::<State>(state_id()))
        .unwrap_or_else(|| State {
            colour: 2,
            ..State::default()
        });
    if state
        .selected
        .is_some_and(|at| !matches!(edit.marks.get(at), Some(PictureMark::Sticker(_))))
    {
        state.selected = None;
    }
    let view_once_offered = app.pending_supports_view_once();
    let view_once = app.pending_view_once;
    let own_stickers: Vec<std::path::PathBuf> = app
        .sticker_packs
        .iter()
        .flat_map(|pack| pack.stickers.iter().cloned())
        .take(48)
        .collect();
    // Captured before the edit is borrowed mutably, so the closure does not
    // need the whole app.
    let source = edit.source.clone();
    let pasted = pasted_texture(app, ctx, &edit);
    let screen = ctx.content_rect();
    let mut actions = Vec::new();
    let mut change = None;
    if !edit.marks.is_empty()
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z))
    {
        change = Some(Change::Undo);
    }
    // It covers the window, so it keeps the keyboard as the viewer does:
    // Tab walks its own controls, never the chat hidden under it. The
    // pointer already met only the cropper.
    ctx.memory_mut(|memory| {
        memory.set_modal_layer(egui::LayerId::new(egui::Order::Foreground, area_id()));
    });
    egui::Area::new(area_id())
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::hover());
            // The window takes the theme's own colour, so the controls on it
            // read in a light theme as in a dark one.
            ui.painter().rect_filled(rect, 0.0, palette.window);
            let room = rect.shrink(MARGIN);
            let top = Rect::from_min_size(room.min, vec2(room.width(), BAR));
            let bottom =
                Rect::from_min_size(pos2(room.min.x, room.max.y - BAR), vec2(room.width(), BAR));
            // The tray of stickers opens above the tools, pushing the picture
            // up as it comes.
            let tray_open = state.tool == Tool::Sticker;
            let tray_shown = theme::animate(
                ui.ctx(),
                ui.id().with("tray"),
                tray_open,
                if tray_open {
                    theme::MOTION_PANEL
                } else {
                    theme::MOTION_PANEL * theme::CLOSE
                },
            );
            let tray_room = (TRAY + 8.0) * theme::ease_out(tray_shown);
            let (wide, tall) = edit.displayed_size();
            let inner = Rect::from_min_max(
                pos2(room.min.x, top.max.y),
                pos2(room.max.x, bottom.min.y - tray_room),
            );
            let picture = fitted(vec2(wide as f32, tall as f32), inner);
            let scale = picture.width() / wide.max(1) as f32;
            if let Some(id) = texture(ui, &source, pasted.as_ref(), picture.size()) {
                paint_turned(ui, id, picture, edit.turns);
            }
            let to_picture = |at: Pos2| {
                let local = (at - picture.min) / scale;
                [local.x, local.y]
            };
            let to_screen_point = |[x, y]: [f32; 2]| picture.min + vec2(x * scale, y * scale);
            // Clipped to the picture, so a line drawn off its edge does not
            // spill over the window.
            let painter = ui.painter_at(picture);
            for (at, mark) in edit.marks.iter().enumerate() {
                match mark {
                    PictureMark::Stroke(stroke) => {
                        paint_stroke(&painter, stroke, &to_screen_point, scale);
                    }
                    PictureMark::Sticker(sticker) => {
                        // A sticker just laid grows in from a little under its
                        // size.
                        let landed =
                            theme::appear(ui.ctx(), sticker_motion_id(at), theme::MOTION_QUICK);
                        let side = sticker.size * scale * (0.6 + 0.4 * theme::ease_out(landed));
                        let place = Rect::from_center_size(
                            to_screen_point(sticker.centre),
                            Vec2::splat(side),
                        );
                        paint_sticker(ui, &painter, &sticker.art, place, landed);
                    }
                }
            }
            if let Some(stroke) = &state.drawing {
                paint_stroke(&painter, stroke, &to_screen_point, scale);
            }
            if state.tool.draws() {
                // The whole picture takes the pen; the crop keeps its place.
                let canvas = ui
                    .interact(picture, ui.id().with("draw"), Sense::DRAG)
                    .on_hover_cursor(egui::CursorIcon::Crosshair);
                if let Some(at) = canvas.interact_pointer_pos() {
                    let point = to_picture(at);
                    if canvas.drag_started() || state.drawing.is_none() {
                        let highlighter = state.tool == Tool::Highlighter;
                        let width = if highlighter {
                            HIGHLIGHTER_WIDTH
                        } else {
                            PEN_WIDTH
                        };
                        state.drawing = Some(PictureStroke {
                            points: vec![point],
                            colour: COLOURS[state.colour],
                            width: width / scale,
                            highlighter,
                        });
                    } else if let Some(stroke) = &mut state.drawing
                        && stroke.points.last().is_none_or(|last| {
                            let (dx, dy) = (last[0] - point[0], last[1] - point[1]);
                            // A new point only once the pointer has moved
                            // about a point on screen.
                            (dx * dx + dy * dy).sqrt() * scale >= 1.0
                        })
                    {
                        stroke.points.push(point);
                    }
                }
                if !canvas.is_pointer_button_down_on()
                    && let Some(stroke) = state.drawing.take()
                {
                    edit.marks.push(PictureMark::Stroke(stroke));
                }
                shade_outside(
                    ui,
                    palette,
                    picture,
                    to_screen(edit.crop, picture, scale),
                    false,
                );
            } else {
                state.drawing = None;
                // The region is kept in the picture's pixels, so a drag is
                // turned into pixels before it is applied.
                let moving = ui.interact(
                    to_screen(edit.crop, picture, scale),
                    ui.id().with("crop-inside"),
                    // `Sense::DRAG` is not focusable: a drag needs the pointer,
                    // and a focusable area is a nameless Tab stop.
                    Sense::DRAG,
                );
                if moving.dragged() {
                    let delta = moving.drag_delta() / scale;
                    edit.crop = edit.crop.drag(
                        CropEdge::Inside,
                        delta.x.round() as i64,
                        delta.y.round() as i64,
                        wide,
                        tall,
                    );
                }
                for (edge, at) in handles(to_screen(edit.crop, picture, scale)) {
                    let grip = Rect::from_center_size(at, Vec2::splat(GRIP));
                    let response = ui
                        .interact(grip, ui.id().with(("crop-edge", edge as u8)), Sense::DRAG)
                        .on_hover_cursor(cursor(edge));
                    if response.dragged() {
                        let delta = response.drag_delta() / scale;
                        edit.crop = edit.crop.drag(
                            edge,
                            delta.x.round() as i64,
                            delta.y.round() as i64,
                            wide,
                            tall,
                        );
                    }
                }
                shade_outside(
                    ui,
                    palette,
                    picture,
                    to_screen(edit.crop, picture, scale),
                    true,
                );
                // Stickers sit over the crop, so they take the pointer first.
                move_stickers(ui, palette, &mut edit, &mut state, picture, scale);
            }
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(top)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    let close = gettext("Cancel (Esc)");
                    if theme::icon_button(
                        ui,
                        Icon::X,
                        18.0,
                        palette.secondary,
                        palette.text,
                        &close,
                    )
                    .clicked()
                    {
                        actions.push(Action::CancelPictureEdit);
                    }
                    let keep = gettext("Done");
                    let kept = theme::circle_button(
                        ui,
                        Icon::Check,
                        KEEP,
                        palette.accent,
                        palette.accent_hover,
                        palette.on_accent,
                        &keep,
                    );
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(keep_note_id(), kept.id));
                    if kept.clicked() {
                        actions.push(Action::ApplyPictureEdit);
                    }
                    if view_once_offered {
                        ui.add_space(8.0);
                        let label = gettext("View once");
                        if tool_button(ui, palette, Icon::ViewOnce, view_once, &label).clicked() {
                            actions.push(Action::SetPendingViewOnce(!view_once));
                        }
                    }
                },
            );
            if tray_shown > 0.0 {
                let tray = Rect::from_min_size(
                    pos2(room.min.x, bottom.min.y - tray_room),
                    vec2(room.width(), TRAY),
                );
                if let Some(art) = sticker_tray(ui, palette, tray, tray_shown, &own_stickers) {
                    // Laid in the middle of the crop, a little over a
                    // quarter of its shorter side, and picked up.
                    let side = (edit.crop.width.min(edit.crop.height) as f32 * STICKER_SHARE)
                        .max(STICKER_LEAST / scale);
                    edit.marks.push(PictureMark::Sticker(PictureSticker {
                        art,
                        centre: [
                            edit.crop.x as f32 + edit.crop.width as f32 / 2.0,
                            edit.crop.y as f32 + edit.crop.height as f32 / 2.0,
                        ],
                        size: side,
                    }));
                    let at = edit.marks.len() - 1;
                    theme::forget_motion(ui.ctx(), sticker_motion_id(at));
                    state.selected = Some(at);
                    state.tool = Tool::Crop;
                }
            }
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(bottom)
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (tool, icon, label) in [
                        (Tool::Crop, Icon::Crop, "Crop"),
                        (Tool::Pen, Icon::Pencil, "Pen"),
                        (Tool::Highlighter, Icon::Highlighter, "Highlighter"),
                        (Tool::Sticker, Icon::Sticker, "Stickers and emoji"),
                    ] {
                        let label = gettext(label);
                        if tool_button(ui, palette, icon, state.tool == tool, &label).clicked() {
                            // A second press on the open tray closes it.
                            state.tool = if tool == state.tool && tool == Tool::Sticker {
                                Tool::Crop
                            } else {
                                tool
                            };
                            state.selected = None;
                        }
                    }
                    let undo = gettext("Undo");
                    let can_undo = !edit.marks.is_empty();
                    if ui
                        .add_enabled_ui(can_undo, |ui| {
                            theme::icon_button(
                                ui,
                                Icon::Undo,
                                18.0,
                                if can_undo {
                                    palette.secondary
                                } else {
                                    palette.dim
                                },
                                palette.text,
                                &undo,
                            )
                        })
                        .inner
                        .clicked()
                    {
                        change = Some(Change::Undo);
                    }
                    swatches(ui, palette, &mut state, &gettext);
                },
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(bottom)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    let reset = gettext("Reset");
                    if theme::pill_button(ui, &palette, &reset, false).clicked() {
                        change = Some(Change::Reset);
                    }
                    let right = gettext("Turn right");
                    if theme::icon_button(
                        ui,
                        Icon::RotateRight,
                        18.0,
                        palette.secondary,
                        palette.text,
                        &right,
                    )
                    .clicked()
                    {
                        change = Some(Change::Turn { clockwise: true });
                    }
                    let left = gettext("Turn left");
                    if theme::icon_button(
                        ui,
                        Icon::RotateLeft,
                        18.0,
                        palette.secondary,
                        palette.text,
                        &left,
                    )
                    .clicked()
                    {
                        change = Some(Change::Turn { clockwise: false });
                    }
                },
            );
        });
    match change {
        Some(Change::Reset) => {
            edit = edit.reset();
            state.selected = None;
            state.drawing = None;
        }
        Some(Change::Turn { clockwise }) => {
            edit = edit.turned(clockwise);
            state.drawing = None;
        }
        Some(Change::Undo) => {
            if edit.marks.pop().is_some() && state.selected == Some(edit.marks.len()) {
                state.selected = None;
            }
        }
        None => {}
    }
    ctx.data_mut(|data| data.insert_temp(state_id(), state));
    app.picture_edit = Some(edit);
    app.actions.extend(actions);
}

/// The motion of the sticker at `at` in the marks landing.
fn sticker_motion_id(at: usize) -> egui::Id {
    egui::Id::new(("picture-editor-sticker", at))
}

/// Draws a line from the picture's pixels onto the screen, as the edit will
/// burn it in.
fn paint_stroke(
    painter: &egui::Painter,
    stroke: &PictureStroke,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
    scale: f32,
) {
    let [r, g, b] = stroke.colour;
    let alpha = if stroke.highlighter {
        crate::backend::picture_edit::HIGHLIGHTER_ALPHA
    } else {
        1.0
    };
    let colour = Color32::from_rgba_unmultiplied(r, g, b, (alpha * 255.0).round() as u8);
    let width = (stroke.width * scale).max(1.0);
    let points: Vec<Pos2> = stroke.points.iter().copied().map(to_screen).collect();
    match points.as_slice() {
        [] => {}
        [dot] => {
            painter.circle_filled(*dot, width / 2.0, colour);
        }
        _ => {
            painter.add(egui::Shape::line(
                points.clone(),
                Stroke::new(width, colour),
            ));
            if !stroke.highlighter {
                // Round ends, as the line is burnt in. A see-through line
                // would darken where they overlap it, so it keeps flat ends.
                for end in [points[0], points[points.len() - 1]] {
                    painter.circle_filled(end, width / 2.0, colour);
                }
            }
        }
    }
}

/// Draws a sticker's emoji or picture into `place`, faded by `shown`.
fn paint_sticker(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    art: &StickerArt,
    place: Rect,
    shown: f32,
) {
    match art {
        StickerArt::Emoji(cluster) => {
            let mut faded = ui.new_child(egui::UiBuilder::new().max_rect(place));
            faded.set_clip_rect(painter.clip_rect());
            faded.multiply_opacity(shown);
            crate::emoji::paint_cluster(&faded, cluster, place);
        }
        StickerArt::File(path) => {
            let image = widgets::file_image(ui, path);
            if let Ok(egui::load::TexturePoll::Ready { texture }) =
                image.load_for_size(ui.ctx(), place.size())
            {
                // Fitted upright inside its square, as it is burnt in.
                let fit = fitted(texture.size, place);
                painter.image(
                    texture.id,
                    fit,
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE.gamma_multiply(shown),
                );
            }
        }
    }
}

/// Lets the stickers on the picture be picked up and moved, made larger or
/// smaller from their corner, and taken off with their ×.
fn move_stickers(
    ui: &mut egui::Ui,
    palette: theme::Palette,
    edit: &mut crate::model::PictureEdit,
    state: &mut State,
    picture: Rect,
    scale: f32,
) {
    let mut remove = None;
    for at in 0..edit.marks.len() {
        let PictureMark::Sticker(sticker) = &mut edit.marks[at] else {
            continue;
        };
        let place = Rect::from_center_size(
            picture.min + vec2(sticker.centre[0] * scale, sticker.centre[1] * scale),
            Vec2::splat(sticker.size * scale),
        );
        let body = ui
            .interact(
                place,
                ui.id().with(("sticker", at)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab);
        if body.clicked() || body.drag_started() {
            state.selected = Some(at);
        }
        if body.dragged() {
            let delta = body.drag_delta() / scale;
            sticker.centre[0] += delta.x;
            sticker.centre[1] += delta.y;
        }
        let picked = theme::animate(
            ui.ctx(),
            ui.id().with(("sticker-picked", at)),
            state.selected == Some(at),
            theme::MOTION_QUICK,
        );
        if picked <= 0.0 {
            continue;
        }
        let ring = palette.accent.gamma_multiply(picked);
        ui.painter().rect_stroke(
            place.expand(4.0),
            6.0,
            Stroke::new(BORDER, ring),
            StrokeKind::Outside,
        );
        if state.selected != Some(at) {
            continue;
        }
        // The corner handle grows or shrinks it about its middle.
        let corner =
            Rect::from_center_size(place.right_bottom() + vec2(4.0, 4.0), Vec2::splat(HANDLE));
        let sizing = ui
            .interact(
                corner.expand((GRIP - HANDLE) / 2.0),
                ui.id().with(("sticker-size", at)),
                Sense::DRAG,
            )
            .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
        if sizing.dragged() {
            let grow = (sizing.drag_delta().x + sizing.drag_delta().y) / scale;
            sticker.size = (sticker.size + grow).max(STICKER_LEAST / scale);
        }
        ui.painter()
            .circle_filled(corner.center(), HANDLE / 2.0, ring);
        let close = Rect::from_center_size(place.right_top() + vec2(4.0, -4.0), Vec2::splat(20.0));
        let gone = ui
            .interact(close, ui.id().with(("sticker-remove", at)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().circle_filled(close.center(), 10.0, ring);
        theme::paint_icon(
            ui,
            Icon::X,
            close,
            12.0,
            palette.on_accent.gamma_multiply(picked),
        );
        if gone.clicked() {
            remove = Some(at);
        }
    }
    if let Some(at) = remove {
        edit.marks.remove(at);
        state.selected = None;
        // Later stickers moved down a place; their landing is not replayed.
        for later in at..=edit.marks.len() {
            theme::forget_motion(ui.ctx(), sticker_motion_id(later));
        }
    }
}

/// A round tool button: lit in the accent while its tool is on, easing
/// between the two.
fn tool_button(
    ui: &mut egui::Ui,
    palette: theme::Palette,
    icon: Icon,
    on: bool,
    label: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(TOOL), Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, TOOL / 2.0);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), on, label)
    });
    if ui.is_rect_visible(rect) {
        let lit = theme::animate(ui.ctx(), response.id.with("on"), on, theme::MOTION_QUICK);
        let hover = theme::animate(
            ui.ctx(),
            response.id.with("hover"),
            response.hovered() || response.has_focus(),
            theme::MOTION_QUICK,
        );
        let idle = Color32::TRANSPARENT.lerp_to_gamma(palette.surface_hover, hover);
        let fill = idle.lerp_to_gamma(palette.accent, lit);
        ui.painter().circle_filled(rect.center(), TOOL / 2.0, fill);
        let tint = palette.secondary.lerp_to_gamma(palette.on_accent, lit);
        theme::paint_icon(ui, icon, rect, 18.0, tint);
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

/// The colours, shown beside the tools while the pen or highlighter is in
/// hand, growing in as it is picked up.
fn swatches(
    ui: &mut egui::Ui,
    palette: theme::Palette,
    state: &mut State,
    gettext: &impl Fn(&'static str) -> String,
) {
    let drawing = state.tool.draws();
    let shown = theme::animate(
        ui.ctx(),
        egui::Id::new("picture-editor-swatches"),
        drawing,
        if drawing {
            theme::MOTION_PANEL
        } else {
            theme::MOTION_PANEL * theme::CLOSE
        },
    );
    if shown <= 0.0 {
        return;
    }
    ui.add_space(10.0 * shown);
    let eased = theme::ease_out(shown);
    for (at, [r, g, b]) in COLOURS.into_iter().enumerate() {
        let side = SWATCH * eased;
        let (rect, response) = ui.allocate_exact_size(vec2(side, SWATCH), Sense::click());
        let name = gettext(COLOUR_NAMES[at]);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                state.colour == at,
                &name,
            )
        });
        let picked = theme::animate(
            ui.ctx(),
            response.id.with("picked"),
            state.colour == at,
            theme::MOTION_QUICK,
        );
        let centre = rect.center();
        let radius = side / 2.0 * (0.8 + 0.2 * picked);
        let colour = Color32::from_rgb(r, g, b).gamma_multiply(shown);
        ui.painter().circle_filled(centre, radius, colour);
        // A thin ring keeps white and black visible on either theme, and a
        // wider one marks the colour in hand.
        ui.painter().circle_stroke(
            centre,
            radius,
            Stroke::new(1.0, palette.secondary.gamma_multiply(0.5 * shown)),
        );
        if picked > 0.0 {
            ui.painter().circle_stroke(
                centre,
                radius + 3.0,
                Stroke::new(2.0, palette.accent.gamma_multiply(picked * shown)),
            );
        }
        if drawing && response.on_hover_text(&name).clicked() {
            state.colour = at;
        }
    }
}

/// The tray of emoji and the user's stickers, sliding up from the tools.
/// Returns what was picked to lay on the picture.
fn sticker_tray(
    ui: &mut egui::Ui,
    palette: theme::Palette,
    tray: Rect,
    shown: f32,
    stickers: &[std::path::PathBuf],
) -> Option<StickerArt> {
    let eased = theme::ease_out(shown);
    let place = tray.translate(vec2(0.0, 12.0 * (1.0 - eased)));
    let mut picked = None;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(place)
            .layout(Layout::top_down(Align::Min)),
    );
    child.multiply_opacity(shown);
    if shown < 1.0 {
        child.disable();
    }
    child.painter().rect_filled(place, 16.0, palette.surface);
    let inside = place.shrink(8.0);
    child.scope_builder(egui::UiBuilder::new().max_rect(inside), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("picture-editor-tray")
            .max_height(inside.height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for cluster in EMOJI {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(TRAY_CELL), Sense::click());
                        tray_hover(ui, palette, &response, rect);
                        crate::emoji::paint_cluster(ui, cluster, rect.shrink(6.0));
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            picked = Some(StickerArt::Emoji(cluster.to_owned()));
                        }
                    }
                    for path in stickers {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(TRAY_CELL), Sense::click());
                        tray_hover(ui, palette, &response, rect);
                        if ui.is_rect_visible(rect) {
                            widgets::file_image(ui, path)
                                .fit_to_exact_size(Vec2::splat(TRAY_CELL - 8.0))
                                .paint_at(ui, rect.shrink(4.0));
                        }
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            picked = Some(StickerArt::File(path.clone()));
                        }
                    }
                });
            });
    });
    picked
}

/// The tint under a tray cell the pointer is over.
fn tray_hover(ui: &egui::Ui, palette: theme::Palette, response: &egui::Response, rect: Rect) {
    let hover = theme::animate(
        ui.ctx(),
        response.id.with("hover"),
        response.hovered(),
        theme::MOTION_QUICK,
    );
    if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, 10.0, palette.surface_hover.gamma_multiply(hover));
    }
}
/// The largest rectangle of `shape`'s proportions centred in `room`.
fn fitted(shape: Vec2, room: Rect) -> Rect {
    let scale = (room.width() / shape.x.max(1.0)).min(room.height() / shape.y.max(1.0));
    Rect::from_center_size(room.center(), shape * scale)
}

/// The texture to draw a pasted picture with, built from its buffer when the
/// strip no longer has it. Cropping a pasted picture leaves a file on disk and
/// takes the strip's copy of the pixels with it, so on the second visit the
/// pixels have to come from the buffer the crop was taken from.
fn pasted_texture(
    app: &mut App,
    ctx: &egui::Context,
    edit: &crate::model::PictureEdit,
) -> Option<egui::TextureHandle> {
    if let Some(at) = app.staged_at(edit.target)
        && let Some(Pending::Picture {
            texture: Some(handle),
            ..
        }) = app.pending.get(at).map(|staged| &staged.item)
    {
        return Some(handle.clone());
    }
    let PictureSource::Pasted(rgba) = &edit.source else {
        return None;
    };
    let key = std::sync::Arc::as_ptr(rgba) as usize;
    if let Some((cached, handle)) = &app.picture_texture
        && *cached == key
    {
        return Some(handle.clone());
    }
    let image = pasted_image(rgba, edit.width, edit.height)?;
    let handle = ctx.load_texture(
        format!("edited-picture-{key}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    app.picture_texture = Some((key, handle.clone()));
    Some(handle)
}

/// A pasted buffer as an image to upload, capped the way the strip caps its own
/// copy so a large paste does not meet the GPU's texture limit here.
fn pasted_image(rgba: &[u8], width: u32, height: u32) -> Option<egui::ColorImage> {
    // Borrowed: a copy of a 4K bitmap is 33 MB.
    let full =
        image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(width.max(1), height.max(1), rgba)?;
    if width > 1024 || height > 1024 {
        let scale = 1024.0 / width.max(height) as f32;
        let (wide, tall) = (
            ((width as f32 * scale) as u32).max(1),
            ((height as f32 * scale) as u32).max(1),
        );
        let small =
            image::imageops::resize(&full, wide, tall, image::imageops::FilterType::Triangle);
        Some(egui::ColorImage::from_rgba_unmultiplied(
            [small.width() as usize, small.height() as usize],
            &small,
        ))
    } else {
        Some(egui::ColorImage::from_rgba_unmultiplied(
            [full.width() as usize, full.height() as usize],
            &full,
        ))
    }
}

/// The picture's texture, once it is ready to draw.
fn texture(
    ui: &mut egui::Ui,
    source: &PictureSource,
    pasted: Option<&egui::TextureHandle>,
    size: Vec2,
) -> Option<egui::TextureId> {
    match source {
        PictureSource::File(path) => {
            let image = widgets::file_image(ui, path);
            match image.load_for_size(ui.ctx(), size) {
                Ok(egui::load::TexturePoll::Ready { texture }) => Some(texture.id),
                _ => None,
            }
        }
        PictureSource::Pasted(_) => pasted.map(egui::TextureHandle::id),
    }
}

/// Which corner of the texture each corner of the picture takes, turned by
/// whole quarter turns clockwise. Read as the picture's own corners, going
/// clockwise from the top left.
fn turned_corners(turns: u8) -> [Pos2; 4] {
    let quarter = usize::from(turns % 4);
    let corners = [
        Pos2::new(0.0, 0.0),
        Pos2::new(1.0, 0.0),
        Pos2::new(1.0, 1.0),
        Pos2::new(0.0, 1.0),
    ];
    [
        corners[(4 - quarter) % 4],
        corners[(5 - quarter) % 4],
        corners[(6 - quarter) % 4],
        corners[(7 - quarter) % 4],
    ]
}

/// Draws the picture into `rect`, turned by whole quarter turns clockwise.
/// The turn is taken out of the texture's own corners, so the pixels are
/// never copied and turning a picture costs nothing.
fn paint_turned(ui: &egui::Ui, texture: egui::TextureId, rect: Rect, turns: u8) {
    let places = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let mut mesh = egui::epaint::Mesh {
        texture_id: texture,
        ..Default::default()
    };
    for (place, uv) in places.into_iter().zip(turned_corners(turns)) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: place,
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// Where a crop's edges and corners are on screen.
fn handles(crop: Rect) -> [(CropEdge, Pos2); 8] {
    [
        (CropEdge::TopLeft, crop.left_top()),
        (CropEdge::Top, crop.center_top()),
        (CropEdge::TopRight, crop.right_top()),
        (CropEdge::Right, crop.right_center()),
        (CropEdge::BottomRight, crop.right_bottom()),
        (CropEdge::Bottom, crop.center_bottom()),
        (CropEdge::BottomLeft, crop.left_bottom()),
        (CropEdge::Left, crop.left_center()),
    ]
}

/// The crop's place on screen, given where the picture is drawn.
fn to_screen(crop: PictureCrop, picture: Rect, scale: f32) -> Rect {
    Rect::from_min_size(
        picture.min + vec2(crop.x as f32 * scale, crop.y as f32 * scale),
        vec2(crop.width as f32 * scale, crop.height as f32 * scale),
    )
}

/// The cursor that fits the edge being dragged.
fn cursor(edge: CropEdge) -> egui::CursorIcon {
    match edge {
        CropEdge::Left | CropEdge::Right => egui::CursorIcon::ResizeHorizontal,
        CropEdge::Top | CropEdge::Bottom => egui::CursorIcon::ResizeVertical,
        CropEdge::TopLeft | CropEdge::BottomRight => egui::CursorIcon::ResizeNwSe,
        CropEdge::TopRight | CropEdge::BottomLeft => egui::CursorIcon::ResizeNeSw,
        CropEdge::Inside => egui::CursorIcon::Grab,
    }
}

/// Fades what the crop leaves out toward the window, and marks the crop's
/// border in the theme's accent, with its handles while it can be dragged.
fn shade_outside(
    ui: &egui::Ui,
    palette: theme::Palette,
    picture: Rect,
    crop: Rect,
    with_handles: bool,
) {
    let shade = palette.window.gamma_multiply(0.7);
    for outside in [
        Rect::from_min_max(picture.min, pos2(picture.max.x, crop.min.y)),
        Rect::from_min_max(pos2(picture.min.x, crop.max.y), picture.max),
        Rect::from_min_max(
            pos2(picture.min.x, crop.min.y),
            pos2(crop.min.x, crop.max.y),
        ),
        Rect::from_min_max(
            pos2(crop.max.x, crop.min.y),
            pos2(picture.max.x, crop.max.y),
        ),
    ] {
        if outside.is_positive() {
            ui.painter().rect_filled(outside, 0.0, shade);
        }
    }
    ui.painter().rect_stroke(
        crop,
        0.0,
        Stroke::new(BORDER, palette.accent),
        StrokeKind::Inside,
    );
    if !with_handles {
        return;
    }
    for (_, at) in handles(crop) {
        let handle = Rect::from_center_size(at, Vec2::splat(HANDLE));
        ui.painter().rect_filled(handle, 2.0, palette.accent);
        ui.painter().rect_stroke(
            handle,
            2.0,
            Stroke::new(1.0, palette.on_accent),
            StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_fills_the_room_in_one_direction() {
        let room = Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(1000.0, 500.0));
        let wide = fitted(vec2(1920.0, 1080.0), room);
        assert!((wide.height() - 500.0).abs() < 0.01);
        assert!((wide.width() - 500.0 * 16.0 / 9.0).abs() < 0.01);
        assert_eq!(wide.center(), room.center());
        let tall = fitted(vec2(100.0, 1000.0), room);
        assert!((tall.height() - 500.0).abs() < 0.01 && tall.width() < 60.0);
    }

    #[test]
    fn a_quarter_turn_clockwise_puts_the_top_left_at_the_top_right() {
        assert_eq!(
            turned_corners(1),
            [
                Pos2::new(0.0, 1.0),
                Pos2::new(0.0, 0.0),
                Pos2::new(1.0, 0.0),
                Pos2::new(1.0, 1.0),
            ]
        );
    }

    #[test]
    fn turning_takes_whole_quarters_and_comes_back_round() {
        assert_eq!(
            turned_corners(0),
            [
                Pos2::new(0.0, 0.0),
                Pos2::new(1.0, 0.0),
                Pos2::new(1.0, 1.0),
                Pos2::new(0.0, 1.0),
            ]
        );
        assert_eq!(turned_corners(4), turned_corners(0));
        assert_eq!(turned_corners(7), turned_corners(3));
        // Half a turn puts each corner where the one across from it was.
        assert_eq!(turned_corners(2)[0], Pos2::new(1.0, 1.0));
        assert_eq!(turned_corners(2)[1], Pos2::new(0.0, 1.0));
    }
}
