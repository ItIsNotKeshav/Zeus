//! The form controls every settings row and dialog shares: the dropdown and
//! the text field. One height, one radius and one set of paddings, so a
//! control looks the same wherever it is drawn.
//!
//! Following Apple's Liquid Glass guidance, the controls themselves are flat
//! tinted capsules on the opaque content; only the transient layer they open,
//! the dropdown's list, is glass (a [`super::widgets::FrostedPopup`]).

use std::hash::Hash;

use egui::{Align, Layout, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

use super::widgets::{self, FrostedPopup};
use crate::theme::{self, Icon, Palette};

/// How tall a dropdown or text field is.
pub const HEIGHT: f32 = 36.0;
/// How wide a control is in a settings row, which keeps them in one column.
pub const WIDTH: f32 = 220.0;
/// The room between a control's edge and its text.
const PAD: f32 = 16.0;
/// The room kept for the chevron.
const CHEVRON: f32 = 34.0;
/// How tall a row of an open list is.
const OPTION_HEIGHT: f32 = 34.0;
/// The tallest an open list grows before it scrolls.
const LIST_HEIGHT: f32 = 320.0;
/// How long a control takes to ease between its states.
const EASE: f32 = 0.14;

/// The capsule behind a control, one step stronger when `lift` is raised.
fn paint_field(ui: &Ui, palette: &Palette, rect: Rect, lift: f32) {
    let fill = palette.tint(theme::TINT_FIELD + theme::TINT_HOVER * lift);
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
}

/// A dropdown: shows `selected`, and opens a frosted list of whatever
/// `options` draws with [`option`] when pressed. Returns the control's
/// response, for [`theme::reveal_focus`] and accessibility.
pub fn dropdown(
    ui: &mut Ui,
    palette: &Palette,
    salt: impl Hash + std::fmt::Debug,
    label: &str,
    selected: &str,
    width: f32,
    options: impl FnOnce(&mut Ui),
) -> Response {
    let width = width.min(ui.available_width()).max(CHEVRON + PAD);
    let id = ui.make_persistent_id(&salt);
    let (_, rect) = ui.allocate_space(vec2(width, HEIGHT));
    let response = ui.interact(rect, id, Sense::click());
    theme::reveal_focus(&response);
    theme::focus_outline(ui, id, rect, HEIGHT / 2.0);
    let enabled = ui.is_enabled();
    #[cfg(any(test, feature = "demo"))]
    if demo_open() == format!("{salt:?}") {
        egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&response));
    }
    let popup = egui::Popup::menu(&response)
        .width(width)
        .gap(6.0)
        .frame(widgets::menu_frame(palette));
    let open = popup.is_open();
    let lift = ui.ctx().animate_bool_with_time_and_easing(
        id.with("lift"),
        open || (enabled && response.hovered()),
        EASE,
        egui::emath::easing::quadratic_out,
    );
    if ui.is_rect_visible(rect) {
        paint_field(ui, palette, rect, lift);
        let line = widgets::line(
            ui,
            selected,
            theme::regular(14.0),
            palette.text,
            rect.width() - PAD - CHEVRON,
            1,
        );
        line.paint(
            ui,
            pos2(rect.left() + PAD, rect.center().y - line.size().y / 2.0),
            palette.text,
        );
        let chevron = Rect::from_center_size(
            pos2(rect.right() - CHEVRON / 2.0 - 2.0, rect.center().y),
            Vec2::splat(16.0),
        );
        let icon = if open {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        };
        icon.image(palette.secondary, 16.0).paint_at(ui, chevron);
    }
    response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, label.to_owned());
        info.current_text_value = Some(selected.to_owned());
        info
    });
    popup.show_frosted(ui.ctx(), |ui| {
        ui.set_min_width(width - 12.0);
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                options(ui);
            });
    });
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// One row of an open [`dropdown`]. Returns whether it was picked; picking
/// closes the list. Text goes through the shared line renderer, since names
/// can hold emoji.
pub fn option(ui: &mut Ui, palette: &Palette, text: &str, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), OPTION_HEIGHT), Sense::click());
    theme::reveal_focus(&response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, text.to_owned())
    });
    if ui.is_rect_visible(rect) {
        let lift = ui.ctx().animate_bool_with_time_and_easing(
            response.id.with("lift"),
            response.hovered(),
            0.1,
            egui::emath::easing::quadratic_out,
        );
        let strength = if selected {
            theme::TINT_SELECTED
        } else {
            theme::TINT_HOVER * lift
        };
        ui.painter().rect_filled(
            rect,
            f32::from(theme::RADIUS),
            palette.tint(strength.max(theme::TINT_HOVER * lift)),
        );
        let right = if selected { 36.0 } else { 14.0 };
        let line = widgets::line(
            ui,
            text,
            theme::regular(14.0),
            palette.text,
            rect.width() - 14.0 - right,
            1,
        );
        line.paint(
            ui,
            pos2(rect.left() + 14.0, rect.center().y - line.size().y / 2.0),
            palette.text,
        );
        if selected {
            let check = Rect::from_center_size(
                pos2(rect.right() - 18.0, rect.center().y),
                Vec2::splat(16.0),
            );
            Icon::Check.image(palette.accent, 16.0).paint_at(ui, check);
        }
    }
    let clicked = response.clicked();
    if clicked {
        ui.close();
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// A single-line text field in a capsule the size of a [`dropdown`].
pub struct Field<'a> {
    text: &'a mut String,
    id: Option<egui::Id>,
    hint: Option<&'a str>,
    password: bool,
    char_limit: Option<usize>,
    width: f32,
}

impl<'a> Field<'a> {
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            id: None,
            hint: None,
            password: false,
            char_limit: None,
            width: WIDTH,
        }
    }

    pub fn id(mut self, id: egui::Id) -> Self {
        self.id = Some(id);
        self
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn password(mut self) -> Self {
        self.password = true;
        self
    }

    pub fn char_limit(mut self, limit: usize) -> Self {
        self.char_limit = Some(limit);
        self
    }

    /// The width, where it is not [`WIDTH`]; `f32::INFINITY` fills the row.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn show(self, ui: &mut Ui, palette: &Palette) -> Response {
        let width = self.width.min(ui.available_width()).max(PAD * 2.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, HEIGHT), Sense::hover());
        let id = self.id.unwrap_or_else(|| ui.next_auto_id());
        let focused = ui.memory(|memory| memory.has_focus(id));
        let enabled = ui.is_enabled();
        let lift = ui.ctx().animate_bool_with_time_and_easing(
            id.with("lift"),
            focused || (enabled && ui.rect_contains_pointer(rect)),
            EASE,
            egui::emath::easing::quadratic_out,
        );
        paint_field(ui, palette, rect, lift);
        let inner = Rect::from_min_max(
            pos2(rect.left() + PAD, rect.top()),
            pos2(rect.right() - PAD, rect.bottom()),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let format = egui::TextFormat::simple(theme::regular(14.0), palette.text);
        let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
            crate::bidi::layout_field(ui, buffer.as_str(), &format, wrap)
        };
        let align = if crate::bidi::base_rtl(self.text) {
            Align::RIGHT
        } else {
            Align::LEFT
        };
        let mut edit = egui::TextEdit::singleline(self.text)
            .id(id)
            .font(theme::regular(14.0))
            .text_color(palette.text)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .desired_width(inner.width())
            .vertical_align(Align::Center)
            .horizontal_align(align)
            .password(self.password);
        if !self.password {
            edit = edit.layouter(&mut layouter);
        }
        if let Some(hint) = self.hint {
            edit = edit.hint_text(
                egui::RichText::new(hint)
                    .font(theme::regular(14.0))
                    .color(palette.dim),
            );
        }
        if let Some(limit) = self.char_limit {
            edit = edit.char_limit(limit);
        }
        let response = child.add_enabled(enabled, edit);
        // The padding is part of the field: pressing it focuses the text. It
        // is no stop of its own for Tab, which goes on to the next control.
        if enabled && ui.interact(rect, id.with("pad"), Sense::CLICK).clicked() {
            ui.memory_mut(|memory| memory.request_focus(response.id));
        }
        theme::focus_outline(ui, response.id, rect, HEIGHT / 2.0);
        if let Some(hint) = self.hint {
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label(hint));
        }
        response
    }
}

#[cfg(any(test, feature = "demo"))]
static DEMO_OPEN: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Demo: the dropdown whose salt prints as `salt` opens as it is drawn.
#[cfg(any(test, feature = "demo"))]
pub fn set_demo_open(salt: &str) {
    if let Ok(mut open) = DEMO_OPEN.lock() {
        *open = salt.to_owned();
    }
}

#[cfg(any(test, feature = "demo"))]
fn demo_open() -> String {
    DEMO_OPEN
        .lock()
        .map(|open| open.clone())
        .unwrap_or_default()
}
