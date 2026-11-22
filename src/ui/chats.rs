//! The left panel: the chat list, with the tab bar floating at its foot.

use egui::{Align, Frame, Key, Layout, Margin, Rect, Sense, Vec2, pos2, vec2};

use super::widgets::FrostedPopup;
use crate::app::App;
use crate::backend::LinkStatus;
use crate::model::{Action, Chat, ChatFilter, Contact, Dialog, Message, Page};
use crate::theme::{self, Icon, Palette};

use super::conversation;
use super::focus::{Stop, TabStop};
use super::labels;
use super::rail::Destination;
use super::widgets;

/// The list panel's margins: as much room on either side, so the rows'
/// cards (and the header's controls over them) stand as far from the
/// window's left edge as from the conversation card, and end where the tab
/// bar floating over the panel does; and the header's drop at the top. The
/// tab bar keeps its own margin at the bottom.
const LIST_MARGIN: Margin = Margin {
    left: 4,
    right: 4,
    top: HEADER_DROP as i8,
    bottom: 0,
};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    search_keyboard(app, ui);
    let palette = app.palette;
    let panel = egui::Panel::left("chats")
        .resizable(true)
        .default_size(app.settings.sidebar_width)
        .size_range(LIST_MIN_WIDTH..=520.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.column())
                .inner_margin(LIST_MARGIN),
        );
    let response = panel.show(ui, |ui| column(app, ui));
    fold_by_drag(app, ui.ctx(), response.response.rect.left());
    // The tab bar floats at the foot of the whole column, as it does beside
    // Settings' categories, so it stays put going from one to the other.
    super::tabbar::show(app, ui, response.response.rect);
    let width = response.response.rect.width();
    ui.ctx()
        .data_mut(|data| data.insert_temp(panel_rect_id(), response.response.rect));
    if (width - app.settings.sidebar_width).abs() > 1.0 {
        app.settings.sidebar_width = width;
        app.actions.push(Action::SettingsChanged);
    }
}

/// The list's narrowest width while it stands beside a chat.
pub(crate) const LIST_MIN_WIDTH: f32 = 260.0;
/// How far past the list's narrowest width its border is dragged before the
/// list folds into the rail, and how far out of the rail it is pulled before
/// the list comes back. The list has no button for this: its border is it.
pub(crate) const FOLD_PULL: f32 = 60.0;

/// Whether a border dragged to `pointer_x` folds a list whose left edge is
/// `left`: well past its narrowest width.
pub(crate) fn folds_at(left: f32, pointer_x: f32) -> bool {
    pointer_x < left + LIST_MIN_WIDTH - FOLD_PULL
}

/// Whether the rail's edge at `edge`, pulled to `pointer_x`, brings the list
/// back.
pub(crate) fn unfolds_at(edge: f32, pointer_x: f32) -> bool {
    pointer_x > edge + FOLD_PULL
}

/// Folds the chat list into the rail when its border, being resized, is
/// dragged well past the list's narrowest width. The drag ends there, so
/// the same press cannot bring the list straight back.
fn fold_by_drag(app: &mut App, ctx: &egui::Context, left: f32) {
    // egui's resizable panel names its border's handle this way.
    let handle = egui::Id::new("chats").with("__resize");
    if !ctx.is_being_dragged(handle) {
        return;
    }
    if let Some(pointer) = ctx.input(|input| input.pointer.latest_pos())
        && folds_at(left, pointer.x)
    {
        ctx.stop_dragging();
        app.actions.push(Action::ToggleSidebar);
    }
}

/// The rail's right edge as a handle: pulled out far enough, it brings the
/// folded list back, as the list's border folded it.
pub(crate) fn unfold_by_drag(app: &mut App, ui: &mut egui::Ui) {
    let edge = ui.available_rect_before_wrap().left();
    let rect = Rect::from_x_y_ranges(edge - 3.0..=edge + 5.0, ui.max_rect().y_range());
    let response = ui
        .interact(rect, ui.id().with("rail-unfold"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
        && unfolds_at(edge, pointer.x)
    {
        ui.ctx().stop_dragging();
        app.actions.push(Action::ToggleSidebar);
    }
}

/// Where the resizable chat-list panel was laid out, for resize tests.
pub(crate) fn panel_rect_id() -> egui::Id {
    egui::Id::new("chat-list-panel")
}

/// The chat list across the whole window, when the window is too narrow to
/// hold it beside a conversation. Opening a chat replaces it. No rail stands
/// beside it: the tab bar at its foot leads to Calls, our account and
/// Settings, and the title's menu to the other lists.
pub fn full_show(app: &mut App, ui: &mut egui::Ui) {
    search_keyboard(app, ui);
    let palette = app.palette;
    let response = egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(palette.column())
                .inner_margin(LIST_MARGIN),
        )
        .show(ui, |ui| column(app, ui));
    super::tabbar::show(app, ui, response.response.rect);
}

/// The gutter between the window's edges and the floating conversation card.
pub(crate) const CARD_GAP: f32 = 8.0;
/// How round the conversation card is: the composer's field and the margin
/// around it, so the card's bottom corners follow the field's.
pub(crate) const CARD_RADIUS: u8 =
    super::conversation::COMPOSER_RADIUS + super::conversation::COMPOSER_MARGIN as u8;
/// How far the list sets its header below the top of its column. The chat
/// header in the card beside it moves down as far, so the chat's name,
/// picture and buttons sit on the same line as the list's title.
pub(crate) const HEADER_DROP: f32 = BASE_DROP + LIGHTS_DROP;
/// The header's drop where nothing sits above it.
pub(crate) const BASE_DROP: f32 = 12.0;
/// On macOS the traffic lights get a row of their own above the headers:
/// the bar they are centred in is `LIGHTS_BAR` tall, so they stand as far
/// from the window's top as from its left (16 points, AppKit's own inset).
#[cfg(target_os = "macos")]
pub const LIGHTS_BAR: f32 = 30.0;
#[cfg(not(target_os = "macos"))]
pub const LIGHTS_BAR: f32 = crate::theme::TOP_BAR_HEIGHT;
/// How far the headers sit lower for the lights' row: just enough over the
/// 12-point drop that the title clears the lights, so the top of the window
/// is not left empty.
#[cfg(target_os = "macos")]
const LIGHTS_DROP: f32 = 6.0;
#[cfg(not(target_os = "macos"))]
const LIGHTS_DROP: f32 = 0.0;
/// Where a list row's avatar starts, from the row's edge: a little right of
/// the search field's left edge (the card's inset), so they read as aligned.
pub(crate) const ROW_LEAD: f32 = 10.0;

/// How far below the list's header its frost and veil carry on, easing off.
const FROST_TAIL: f32 = 36.0;
/// How far up into the header the frost's ease reaches, so it is all but
/// full at the header's bottom edge and the lightest blur stays under it.
const FROST_OVERLAP: f32 = 8.0;

/// The list pane's header and the list under it. On a vibrant window the
/// header goes on a layer above the list, which scrolls on under it through a
/// frost that is strongest at the top and eases to sharp towards the bottom,
/// as the conversation's header does over its messages. Elsewhere a hairline
/// under the header marks where the list is cut off while it is scrolled.
/// The panel around it draws the tab bar over the list's foot.
fn column(app: &mut App, ui: &mut egui::Ui) {
    // A narrow window's slide draws copies of the list; they keep the plain
    // header, so one layer is never drawn twice.
    let copy = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(slide_copy_id()))
        .is_some();
    if copy || !app.palette.vibrant() || !crate::backdrop::blur() {
        // Last frame's list says whether it is scrolled: the body resets it.
        let scrolled = list_scrolled(ui.ctx());
        let head = header(app, ui);
        body(app, ui);
        if scrolled {
            let edge = head.shrink2(vec2(widgets::ROW_CARD_X, 0.0));
            widgets::paint_edge_below(ui, &app.palette, edge.left(), edge.right(), edge.bottom());
        }
        return;
    }
    let area = ui.available_rect_before_wrap();
    // The frost reaches up through the margin above the header, so the rows
    // scrolling under it are not cut off at a line below the panel's top.
    let top = area.top() - HEADER_DROP;
    let layer = egui::LayerId::new(egui::Order::Middle, ui.id().with("chat-list-header"));
    let reach_id = layer.id.with("reach");
    ui.scope_builder(egui::UiBuilder::new().layer_id(layer), |ui| {
        // A click in a gap of the header must not reach a row scrolled
        // under it.
        if let Some(last) = ui.ctx().data(|data| data.get_temp::<Rect>(reach_id)) {
            ui.interact(last, reach_id, Sense::click_and_drag());
        }
        let glass = ui.painter().add(egui::Shape::Noop);
        let veil = ui.painter().add(egui::Shape::Noop);
        let _ = header(app, ui);
        let bar = ui.min_rect();
        let reach = Rect::from_x_y_ranges(area.x_range(), top..=bar.bottom());
        // Fully frosted down to the filters, so no row under the header can
        // be read, then easing off over the tail below them.
        let frosted = reach.with_max_y(reach.bottom() + FROST_TAIL);
        let ease = FROST_TAIL + FROST_OVERLAP;
        ui.painter()
            .set(glass, super::glass::sheet_progressive(frosted, ease));
        ui.painter().set(
            veil,
            conversation::header_veil(
                app.palette.vibrant_header(app.palette.column()),
                frosted,
                ease,
                0.0,
            ),
        );
        ui.ctx().data_mut(|data| data.insert_temp(reach_id, reach));
    });
    // The list starts where the header's layer left the panel's cursor, and
    // its scroll areas reach back up behind the header from there.
    let inset = ui.available_rect_before_wrap().top() - top;
    ui.ctx()
        .data_mut(|data| data.insert_temp(header_inset_id(), inset));
    body(app, ui);
    ui.ctx()
        .data_mut(|data| data.remove::<f32>(header_inset_id()));
}

/// How far the list's scroll areas reach up under the frosted header while
/// the body is drawn.
fn header_inset_id() -> egui::Id {
    egui::Id::new("chat-list-header-inset")
}

/// Whether the list under the header was scrolled off its top when it was
/// last drawn, so the plain header draws a hairline over it.
fn list_scrolled_id() -> egui::Id {
    egui::Id::new("chat-list-scrolled")
}

/// Where the scroll area of the list being drawn starts, which
/// [`plain_rows`] measures its content's offset from.
fn list_top_id() -> egui::Id {
    egui::Id::new("chat-list-scroll-top")
}

fn list_scrolled(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(list_scrolled_id()))
        .unwrap_or(false)
}

/// Notes that the list under the header is `offset` points down.
fn note_scrolled(ctx: &egui::Context, offset: f32) {
    if offset > 0.5 {
        ctx.data_mut(|data| data.insert_temp(list_scrolled_id(), true));
    }
}

/// Runs `contents` in a rect that reaches up under the frosted header, given
/// how far (nothing, where the header is not frosted): its scroll area starts
/// with that much space, so the first row rests below the header and the rest
/// scroll up under it.
pub(super) fn under_header<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui, f32) -> R,
) -> R {
    let inset = ui
        .ctx()
        .data(|data| data.get_temp::<f32>(header_inset_id()))
        .unwrap_or(0.0);
    // The scroll bar's handle takes the accent; the rows put the plain
    // colours back (see [`scroll_area`]).
    let accent = ui.visuals().selection.stroke.color;
    let plain = widget_visuals(ui.visuals_mut()).map(|widget| {
        let color = widget.fg_stroke.color;
        widget.fg_stroke.color = accent;
        color
    });
    let inner = if inset <= 0.0 {
        contents(ui, 0.0)
    } else {
        let mut rect = ui.available_rect_before_wrap();
        rect.min.y -= inset;
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            contents(ui, inset)
        })
        .inner
    };
    for (widget, color) in widget_visuals(ui.visuals_mut()).into_iter().zip(plain) {
        widget.fg_stroke.color = color;
    }
    inner
}

/// The widget states egui paints a scroll bar's handle in.
fn widget_visuals(visuals: &mut egui::Visuals) -> [&mut egui::style::WidgetVisuals; 3] {
    let widgets = &mut visuals.widgets;
    [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ]
}

/// A vertical scroll area for a list drawn by [`under_header`]: its bar
/// runs from the first row down, clear of the header, rather than from the
/// panel's top, and ends above the tab bar floating over the list's foot.
pub(super) fn scroll_area(ui: &egui::Ui, inset: f32) -> egui::ScrollArea {
    let rect = ui.available_rect_before_wrap();
    let top = (rect.top() + inset).min(rect.bottom());
    let bottom = (rect.bottom() - super::tabbar::RESERVE).max(top);
    ui.ctx()
        .data_mut(|data| data.insert_temp(list_top_id(), rect.top()));
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_bar_rect(Rect::from_x_y_ranges(rect.x_range(), top..=bottom))
}

/// Puts back, at the top of a list's [`scroll_area`], the colours its accent
/// scroll bar took, so the rows' own widgets keep the theme's, and notes how
/// far the list is scrolled for the header's hairline: the content starts
/// that far above the scroll area's top.
pub(super) fn plain_rows(ui: &mut egui::Ui) {
    plain_colours(ui);
    if let Some(top) = ui.ctx().data(|data| data.get_temp::<f32>(list_top_id())) {
        note_scrolled(ui.ctx(), top - ui.max_rect().top());
    }
}

/// [`plain_rows`]' colours alone, for rows laid out inside the scroll area
/// rather than at its top.
fn plain_colours(ui: &mut egui::Ui) {
    let global = ui.ctx().global_style();
    let mut plain = global.visuals.clone();
    let plain = widget_visuals(&mut plain).map(|widget| widget.fg_stroke.color);
    for (widget, color) in widget_visuals(ui.visuals_mut()).into_iter().zip(plain) {
        widget.fg_stroke.color = color;
    }
}

/// [`egui::ScrollArea::show_rows`] with `inset` points of space above the
/// first of its `total` rows, and room below the last for it to scroll clear
/// of the tab bar.
fn show_rows_under(
    area: egui::ScrollArea,
    ui: &mut egui::Ui,
    row_height: f32,
    total: usize,
    inset: f32,
    stretch: f32,
    add_rows: impl FnOnce(&mut egui::Ui, std::ops::Range<usize>),
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let spacing = ui.spacing().item_spacing.y;
    let step = row_height + spacing;
    let output = area.show_viewport(ui, |ui, viewport| {
        let rows = (inset + step * total as f32 - spacing).max(0.0);
        ui.set_height(rows + super::tabbar::RESERVE);
        let mut first = ((viewport.min.y - inset) / step).floor().max(0.0) as usize;
        let mut last = ((viewport.max.y - inset) / step).ceil().max(0.0) as usize + 1;
        if last > total {
            let shown = last - first;
            last = total;
            first = total.saturating_sub(shown);
        }
        let y_min = ui.max_rect().top() + inset + first as f32 * step;
        let y_max = ui.max_rect().top() + inset + last as f32 * step;
        let rect = Rect::from_x_y_ranges(ui.max_rect().x_range(), y_min..=y_max)
            .translate(vec2(0.0, stretch));
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |rows_ui| {
            plain_colours(rows_ui);
            rows_ui.skip_ahead_auto_ids(first);
            add_rows(rows_ui, first..last);
        });
    });
    note_scrolled(ui.ctx(), output.state.offset.y);
    output
}

/// What the list pane holds under its header: the chats, or on the Calls
/// tab the call history or the new-call list. Whichever it is says again
/// whether it is scrolled.
fn body(app: &mut App, ui: &mut egui::Ui) {
    ui.ctx().data_mut(|data| {
        data.insert_temp(list_scrolled_id(), false);
        data.remove::<f32>(list_top_id());
    });
    if app.page == Page::Calls {
        super::calls::list(app, ui);
    } else {
        list(app, ui);
    }
}

/// Walks matching chats while the global search field keeps keyboard focus.
/// Enter leaves search and opens the reached chat ready for typing. A
/// disabled list (beneath a narrow window's sliding chat) leaves the keys
/// to the view coming in.
fn search_keyboard(app: &mut App, ui: &egui::Ui) {
    let field = egui::Id::new("chat-search");
    if !ui.is_enabled()
        || app.search.trim().is_empty()
        || app.locked_folder_open()
        || app.secret_code_matched()
        || !ui.memory(|memory| memory.has_focus(field))
    {
        return;
    }
    let (down, up, enter) = ui.input_mut(|input| {
        (
            super::keys::take_plain(input, Key::ArrowDown),
            super::keys::take_plain(input, Key::ArrowUp),
            super::keys::take_plain(input, Key::Enter),
        )
    });
    if !down && !up && !enter {
        return;
    }
    let chats: Vec<_> = app
        .visible_chats()
        .into_iter()
        .map(|chat| chat.id.clone())
        .collect();
    if chats.is_empty() {
        app.search_selected = None;
        return;
    }
    let before = app.search_selected.clone();
    let mut index = before
        .as_ref()
        .and_then(|selected| chats.iter().position(|chat| chat == selected));
    if down {
        index = Some(index.map_or(0, |index| (index + 1).min(chats.len() - 1)));
    }
    if up {
        index = index.map(|index| index.saturating_sub(1));
    }
    app.search_selected = index.map(|index| chats[index].clone());
    if app.search_selected != before {
        let selected = app.search_selected.clone();
        app.scroll_chat_into_view = selected;
    }
    if enter {
        let chat = app
            .search_selected
            .clone()
            .unwrap_or_else(|| chats[0].clone());
        ui.memory_mut(|memory| memory.surrender_focus(field));
        app.actions.push(Action::Search(String::new()));
        app.actions.push(Action::OpenChat(chat));
    }
}

/// The list's header: its first row, then the search field and the chips.
/// Returns where it was laid out.
fn header(app: &mut App, ui: &mut egui::Ui) -> Rect {
    if theme::macos_chrome(ui.ctx()) {
        return macos_header(app, ui);
    }
    // The same vertical margins as the conversation header beside it, and a
    // row as tall as its own, so the two titles share a centre line. Across,
    // the header spans the rows' hover and selection cards: the search field,
    // the chips and the buttons on the right end where the cards do, and the
    // title starts on the avatars' line (see [`title_lead`]).
    Frame::new()
        .inner_margin(Margin {
            left: widgets::ROW_CARD_X as i8,
            right: widgets::ROW_CARD_X as i8,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            let row = ui.allocate_ui_with_layout(
                vec2(ui.available_width(), super::conversation::HEADER_ROW),
                Layout::left_to_right(Align::Center),
                |ui| header_row(app, ui, false),
            );
            ui.ctx()
                .data_mut(|data| data.insert_temp(header_row_id(), row.response.rect));

            ui.add_space(4.0);
            search_and_filters(app, ui);
        })
        .response
        .rect
}

/// The header's first row: the title (on the Chats tab the menu of the
/// lists the column shows), or the new-call list's Back and "New call", and
/// the buttons on the right, with the macOS shortcuts in their tooltips when
/// `macos`.
fn header_row(app: &mut App, ui: &mut egui::Ui, macos: bool) {
    let palette = app.palette;
    let buttons = list_header_buttons(app);
    // The title measures its room from the buttons after it.
    let buttons_width = header_buttons_width(ui, &buttons);
    let lead = title_lead(ui);
    if app.page == Page::Calls && app.new_call {
        ui.add_space(lead);
        new_call_header(app, ui, theme::TEXT_LIST_TITLE, buttons_width);
    } else if app.page == Page::Calls {
        ui.add_space(lead);
        header_title(
            ui,
            &crate::i18n::gettext(app.locale, "Calls"),
            theme::bold(theme::TEXT_LIST_TITLE),
            palette.text,
            buttons_width,
        );
    } else {
        // The menu's hover plate reaches a little before its text.
        ui.add_space((lead - TITLE_INSET).max(0.0));
        let mark = sync_mark_width(app, ui.ctx());
        title_menu(app, ui, buttons_width + mark, macos);
        sync_mark(app, ui, mark);
    }
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        header_button_row(app, ui, &buttons, macos);
    });
}

/// The sync mark's icon, and the room it takes after the title.
const SYNC_MARK_ICON: f32 = 16.0;
const SYNC_MARK_ROOM: f32 = 24.0;

fn sync_mark_id() -> egui::Id {
    egui::Id::new("chat-list-sync-mark")
}

/// How much of the row the sync mark takes this frame: all of its room
/// while it shows or fades, none once it has gone.
fn sync_mark_width(app: &App, ctx: &egui::Context) -> f32 {
    let busy = super::sync_status(app).is_some();
    let shown = theme::animate(ctx, sync_mark_id(), busy, theme::MOTION_PANEL);
    if shown > 0.0 { SYNC_MARK_ROOM } else { 0.0 }
}

/// A refresh icon after the Chats title that turns while the link connects
/// or loads the phone's history, fading in and out as that starts and ends,
/// in place of a toast. Its tooltip says what it waits for. `width` is the
/// room [`sync_mark_width`] gave it.
fn sync_mark(app: &App, ui: &mut egui::Ui, width: f32) {
    if width <= 0.0 {
        return;
    }
    let ctx = ui.ctx().clone();
    let status = super::sync_status(app);
    let shown = theme::animate(&ctx, sync_mark_id(), status.is_some(), theme::MOTION_PANEL);
    let (rect, response) = ui.allocate_exact_size(vec2(width, TITLE_HEIGHT), Sense::hover());
    if let Some((text, _)) = &status {
        // Read the frame first: asking the context inside its own data lock
        // would wait on itself forever.
        let frame = ctx.cumulative_frame_nr();
        ctx.data_mut(|data| data.insert_temp(super::sync_mark_drawn_id(), frame));
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
        response.on_hover_text(text.as_str());
    }
    if !ui.is_rect_visible(rect) || shown <= 0.0 {
        return;
    }
    let palette = app.palette;
    let color = match status {
        Some((_, false)) => palette.secondary,
        _ => palette.accent,
    };
    let icon = Rect::from_center_size(
        pos2(rect.left() + SYNC_MARK_ICON / 2.0 + 4.0, rect.center().y),
        Vec2::splat(SYNC_MARK_ICON),
    );
    // A turn every second and a half, at the spinner's frame rate; still,
    // and so asking for no frames, under reduced motion.
    let angle = if crate::motion::reduced(&ctx) {
        0.0
    } else {
        crate::motion::request_frame_after(&ctx, std::time::Duration::from_millis(33));
        let time = ctx.input(|input| input.time);
        (time * std::f64::consts::TAU / 1.5).rem_euclid(std::f64::consts::TAU) as f32
    };
    Icon::Refresh
        .image(color.gamma_multiply(shown), SYNC_MARK_ICON)
        .rotate(angle, Vec2::splat(0.5))
        .paint_at(ui, icon);
}

/// How far into the header's first row its title starts: on the line of the
/// rows' avatars, inside the cards the header spans, or right of the macOS
/// traffic lights, which lie over the header now that the list runs to the
/// window's left edge.
fn title_lead(ui: &egui::Ui) -> f32 {
    // The lights have a row of their own above the header, so even on macOS
    // the title starts on the avatars' line.
    let _ = ui;
    ROW_LEAD - widgets::ROW_CARD_X
}

/// Stable id of the title's menu button on the Chats tab, for tests and the
/// demo.
pub fn title_menu_id() -> egui::Id {
    egui::Id::new("chat-list-title-menu")
}

/// How far the title's text sits inside its menu button, whose hover plate
/// reaches that much past it on each side.
const TITLE_INSET: f32 = 8.0;
/// The chevron after the title, and the room before it.
const TITLE_CHEVRON: f32 = 16.0;
const TITLE_GAP: f32 = 4.0;
/// How tall the title's menu button is: a comfortable target in the row.
const TITLE_HEIGHT: f32 = 32.0;
/// What a row of the title's menu takes after its label: a check, or the
/// archived chats' unread count.
const TITLE_MENU_TRAIL: f32 = 40.0;

/// The list the column shows now, which the title names. The starred
/// messages cover the rest, and the locked folder covers the archive.
fn shown_list(app: &App) -> Destination {
    if app.show_starred {
        Destination::Starred
    } else if app.locked_folder {
        Destination::Locked
    } else if app.show_archived {
        Destination::Archived
    } else {
        Destination::Chats
    }
}

/// The lists the title's menu offers, in its order: Chats, Starred,
/// Archived, and Locked chats while there are any (or while the folder is
/// what the column shows).
fn title_menu_entries(app: &App) -> Vec<Destination> {
    let mut entries = vec![
        Destination::Chats,
        Destination::Starred,
        Destination::Archived,
    ];
    if Destination::Locked.offered(app) || shown_list(app) == Destination::Locked {
        entries.push(Destination::Locked);
    }
    entries
}

/// A list's name, as the title and its menu write it.
fn list_title(list: Destination) -> &'static str {
    match list {
        Destination::Starred => "Starred",
        Destination::Archived => "Archived",
        Destination::Locked => "Locked chats",
        _ => "Chats",
    }
}

/// A list's icon in the title's menu, the one the rail gives it.
fn list_icon(list: Destination) -> Icon {
    match list {
        Destination::Starred => Icon::Star,
        Destination::Archived => Icon::Archive,
        Destination::Locked => Icon::Lock,
        _ => Icon::MessageCircle,
    }
}

/// What picking `list` in the title's menu does: what the rail's button does,
/// and leaving the locked folder for the archive or the starred messages,
/// which it would otherwise keep covering.
fn title_menu_actions(app: &App, list: Destination) -> Vec<Action> {
    let mut actions = list.actions(app);
    if app.locked_folder
        && matches!(list, Destination::Archived | Destination::Starred)
        && !actions.contains(&Action::CloseLockedFolder)
    {
        actions.push(Action::CloseLockedFolder);
    }
    actions
}

/// The Chats tab's title: the name of the list on screen and a chevron, a
/// button that opens the menu of the lists the column shows (Chats, Starred,
/// Archived and Locked chats), in place of the rail's buttons for them. The
/// name is cut short before the buttons after it, `buttons` points wide,
/// but the chevron always shows, as the menu is the only way to those lists.
/// Beside the macOS traffic lights the other lists' longer names are a size
/// smaller, as their titles were there.
fn title_menu(app: &mut App, ui: &mut egui::Ui, buttons: f32, _macos: bool) {
    let palette = app.palette;
    let shown = shown_list(app);
    let title = crate::i18n::gettext(app.locale, list_title(shown));
    let size = theme::TEXT_LIST_TITLE;
    let spacing = ui.spacing().item_spacing.x;
    let room = (ui.available_width() - buttons - spacing).max(0.0);
    let text_room = (room - 2.0 * TITLE_INSET - TITLE_GAP - TITLE_CHEVRON).max(0.0);
    let line = widgets::line(ui, &title, theme::bold(size), palette.text, text_room, 1);
    let width = 2.0 * TITLE_INSET + line.size().x + TITLE_GAP + TITLE_CHEVRON;
    let (rect, _) = ui.allocate_exact_size(vec2(width, TITLE_HEIGHT), Sense::hover());
    let response = ui
        .interact(rect, title_menu_id(), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    theme::reveal_focus(&response);
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let label = format!(
        "{title}, {}",
        crate::i18n::gettext(app.locale, "switch to Starred or Archived")
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), &label)
    });
    if ui.is_rect_visible(rect) {
        let lit = theme::animate(
            ui.ctx(),
            response.id.with("title-hover"),
            open || response.hovered() || response.has_focus(),
            theme::MOTION_QUICK,
        );
        if lit > 0.0 {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(theme::RADIUS),
                palette.tint(theme::TINT_HOVER * lit),
            );
        }
        line.paint(
            ui,
            pos2(
                rect.left() + TITLE_INSET,
                rect.center().y - line.size().y / 2.0,
            ),
            palette.text,
        );
        let chevron = Rect::from_center_size(
            pos2(
                rect.right() - TITLE_INSET - TITLE_CHEVRON / 2.0,
                rect.center().y,
            ),
            Vec2::splat(TITLE_CHEVRON),
        );
        let icon = if open {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        };
        let tint = palette.secondary.lerp_to_gamma(palette.text, lit);
        icon.image(tint, TITLE_CHEVRON).paint_at(ui, chevron);
    }
    // Back's place in the order: the title leads out of the starred
    // messages, the archive and the locked folder as Back did.
    let response = response.tab_stop(Stop::Back);
    let entries = title_menu_entries(app);
    let labels: Vec<_> = entries
        .iter()
        .map(|list| crate::i18n::gettext(app.locale, list_title(*list)))
        .collect();
    let names: Vec<&str> = labels.iter().map(|label| label.as_ref()).collect();
    let menu_width = (widgets::menu_width(ui, &names, true) + TITLE_MENU_TRAIL).max(180.0);
    let archived_unread = u32::try_from(app.archived_unread()).unwrap_or(u32::MAX);
    let mut picked = None;
    egui::Popup::menu(&response)
        .width(menu_width)
        .show_tight(ui.ctx(), &palette, |ui| {
            for (list, name) in entries.iter().zip(&names) {
                let badge = if *list == Destination::Archived {
                    archived_unread
                } else {
                    0
                };
                if title_menu_row(ui, &palette, *list, name, *list == shown, badge) {
                    picked = Some(*list);
                }
            }
        });
    if let Some(list) = picked {
        let actions = title_menu_actions(app, list);
        app.actions.extend(actions);
    }
}

/// One list in the title's menu: its icon and name, with a check on the one
/// on screen and, for the archive, the count of its unread chats, as the
/// rail's Archived button carries. Returns whether it was picked.
fn title_menu_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    list: Destination,
    name: &str,
    current: bool,
    badge: u32,
) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    theme::reveal_focus(&response);
    let spoken = if badge > 0 {
        format!("{name}, {badge}")
    } else {
        name.to_owned()
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            current,
            spoken.as_str(),
        )
    });
    if ui.is_rect_visible(rect) {
        if response.hovered() || response.has_focus() {
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(theme::RADIUS),
                palette.surface_hover,
            );
        }
        let tint = if current {
            palette.accent
        } else {
            palette.secondary
        };
        let middle = rect.center().y;
        let icon = Rect::from_center_size(pos2(rect.left() + 18.0, middle), Vec2::splat(16.0));
        list_icon(list).image(tint, 16.0).paint_at(ui, icon);
        let mut right = rect.right() - 10.0;
        if badge > 0 {
            let at = pos2(right - 10.0, middle);
            let width = widgets::badge(ui, palette, at, badge, false);
            right -= width + 6.0;
        }
        let x = rect.left() + 36.0;
        let line = widgets::line(
            ui,
            name,
            theme::regular(theme::TEXT_BODY),
            palette.text,
            (right - x).max(0.0),
            1,
        );
        line.paint(ui, pos2(x, middle - line.size().y / 2.0), palette.text);
    }
    let clicked = response.clicked();
    if clicked {
        ui.close();
    }
    let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// The new-call list's header, as WhatsApp's drawer has it: Back and "New
/// call" in place of the tab's title.
fn new_call_header(app: &mut App, ui: &mut egui::Ui, title: f32, buttons: f32) {
    let palette = app.palette;
    if theme::icon_button(
        ui,
        Icon::ArrowLeft,
        18.0,
        palette.secondary,
        palette.text,
        crate::i18n::gettext(app.locale, "Back to the calls").as_ref(),
    )
    .tab_stop(Stop::Back)
    .clicked()
    {
        app.actions.push(Action::ShowNewCall(false));
    }
    header_title(
        ui,
        crate::i18n::gettext(app.locale, "New call").as_ref(),
        theme::bold(title),
        palette.text,
        buttons,
    );
}

/// The search field under the header's first row, and the filter chips
/// under it. The Calls tab searches its own list by name or number, and its
/// chips pick every call or the missed ones.
fn search_and_filters(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let width = ui.available_width();
    if app.page == Page::Calls {
        let hint = crate::i18n::gettext(app.locale, "Search name or number");
        let (id, mut text) = if app.new_call {
            ("new-call-search", app.new_call_search.clone())
        } else {
            ("calls-search", app.calls_search.clone())
        };
        let response = widgets::search_field(
            ui,
            &palette,
            egui::Id::new(id),
            &mut text,
            hint.as_ref(),
            width,
        )
        .tab_stop(Stop::Search);
        if app.new_call {
            // The list opens ready for a name, as New chat does.
            if ui.memory(|memory| memory.focused().is_none()) && ui.is_enabled() {
                response.request_focus();
            }
            app.new_call_search = text;
        } else {
            app.calls_search = text;
            super::calls::filter_chips(app, ui);
        }
        return;
    }
    let mut text = app.search.clone();
    let response = widgets::search_field(
        ui,
        &palette,
        egui::Id::new("chat-search"),
        &mut text,
        crate::i18n::gettext(app.locale, "Search").as_ref(),
        width,
    )
    .tab_stop(Stop::Search);
    if text != app.search {
        app.actions.push(Action::Search(text));
    }
    // A disabled field cannot take the keyboard; the request waits.
    if app.focus_search && ui.is_enabled() {
        app.focus_search = false;
        response.request_focus();
    }
    filter_chips(app, ui);
}

/// A button on the right of the chat list header's first row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaderButton {
    NewChat,
    NewCall,
    /// A release is waiting: the accent arrow the rail carried.
    Update,
}

/// The size of the header buttons' icons.
const HEADER_ICON: f32 = 18.0;

/// The buttons on the right of the chat list header, rightmost first, as
/// `header_button_row` draws them: New chat. The list is hidden by dragging
/// its border past its narrowest width (or ⌘B / Ctrl+B), so it has no
/// button for it. Calls, our account and Settings are in the tab bar
/// at the column's foot, and Starred, Archived and Locked chats in the
/// title's menu. The title before them measures its room from the same list.
///
/// On the Calls tab, as in WhatsApp's Calls pane, New call takes New chat's
/// place.
fn header_buttons(calls: bool) -> Vec<HeaderButton> {
    if calls {
        vec![HeaderButton::NewCall]
    } else {
        vec![HeaderButton::NewChat]
    }
}

/// The buttons the list header draws for what it shows: none of its own over
/// the new-call list, whose Back leads out of it, or the starred messages,
/// whose title's menu does. On the Chats tab a waiting release adds the
/// update arrow, leftmost, which the rail carried before the list stood
/// alone in its column.
fn list_header_buttons(app: &App) -> Vec<HeaderButton> {
    let calls = app.page == Page::Calls;
    let mut buttons = if (calls && app.new_call) || (!calls && app.show_starred) {
        Vec::new()
    } else {
        header_buttons(calls)
    };
    if !calls && app.update.is_some() {
        buttons.push(HeaderButton::Update);
    }
    buttons
}

/// Draws `buttons` in a right-to-left layout, with the macOS shortcuts in
/// their tooltips when `macos`.
fn header_button_row(app: &mut App, ui: &mut egui::Ui, buttons: &[HeaderButton], macos: bool) {
    let palette = app.palette;
    for button in buttons {
        match button {
            HeaderButton::NewChat => {
                if theme::icon_button(
                    ui,
                    Icon::SquarePen,
                    HEADER_ICON,
                    palette.secondary,
                    palette.text,
                    if macos { "New chat (⌘N)" } else { "New chat" },
                )
                .tab_stop(Stop::NewChat)
                .clicked()
                {
                    app.actions.push(Action::ShowDialog(Dialog::NewChat));
                }
            }
            HeaderButton::NewCall => {
                if theme::icon_button(
                    ui,
                    Icon::PhonePlus,
                    HEADER_ICON,
                    palette.secondary,
                    palette.text,
                    crate::i18n::gettext(app.locale, "New call").as_ref(),
                )
                .tab_stop(Stop::NewChat)
                .clicked()
                {
                    app.actions.push(Action::ShowNewCall(true));
                }
            }
            HeaderButton::Update => update_button(app, ui),
        }
    }
}

/// The accent arrow among the header's buttons while a release is waiting,
/// as the rail's update ellipse was: it opens that release's page, where the
/// update is downloaded. It keeps the rail's id, so tests find it either way.
fn update_button(app: &mut App, ui: &mut egui::Ui) {
    let Some(release) = app.update.clone() else {
        return;
    };
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::splat(HEADER_ICON + theme::ICON_BUTTON_PAD),
        Sense::hover(),
    );
    let response = ui
        .interact(rect, super::rail::update_id(), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    theme::reveal_focus(&response);
    let locale = app.locale;
    let label = crate::i18n::gettext(locale, "Update available, version {current} to {next}")
        .replace("{current}", env!("CARGO_PKG_VERSION"))
        .replace("{next}", &release.version);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
    });
    if ui.is_rect_visible(rect) {
        let lit = theme::animate(
            ui.ctx(),
            response.id.with("update-hover"),
            response.hovered() || response.has_focus(),
            theme::MOTION_QUICK,
        );
        let fill = palette.accent_fill.lerp_to_gamma(palette.accent_hover, lit);
        let scale = if response.is_pointer_button_down_on() {
            0.92
        } else {
            1.0
        };
        let ellipse = Rect::from_center_size(rect.center(), vec2(28.0, 22.0) * scale);
        ui.painter().add(egui::Shape::ellipse_filled(
            ellipse.center(),
            ellipse.size() * 0.5,
            fill,
        ));
        theme::paint_icon(ui, Icon::ArrowUp, ellipse, 14.0 * scale, palette.on_accent);
    }
    if response.on_hover_text(label).clicked() {
        app.actions.push(Action::OpenUrl(release.url));
    }
}

/// How wide `buttons` are drawn, with the spacing between them.
fn header_buttons_width(ui: &egui::Ui, buttons: &[HeaderButton]) -> f32 {
    row_width(
        buttons.len(),
        HEADER_ICON + theme::ICON_BUTTON_PAD,
        ui.spacing().item_spacing.x,
    )
}

/// How wide `count` items `edge` points wide are with `spacing` between them.
fn row_width(count: usize, edge: f32, spacing: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    count as f32 * edge + (count - 1) as f32 * spacing
}

/// Draws the chat list header's title when it fits before the buttons that
/// follow it on the right, `buttons` points wide, and leaves it out
/// otherwise: in a narrow list it ran under them. The tab bar's lit tab, or
/// Back before it, says where you are.
fn header_title(
    ui: &mut egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    buttons: f32,
) {
    let width = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), color)
        .size()
        .x;
    let spacing = ui.spacing().item_spacing.x;
    if title_fits(width, ui.available_width(), buttons, spacing) {
        theme::text(ui, text, font, color);
    }
}

/// Whether a title `width` points wide fits in `available` beside a row of
/// buttons `buttons` points wide, with `spacing` between the two.
fn title_fits(width: f32, available: f32, buttons: f32, spacing: f32) -> bool {
    width + spacing + buttons <= available
}

/// Where the chat list header's first row was laid out, for layout tests.
pub(crate) fn header_row_id() -> egui::Id {
    egui::Id::new("chat-list-header-row")
}

/// [`header`] in the macOS chrome. The list runs to the window's left edge,
/// so the traffic lights sit over the start of this header, and the title
/// starts right of them.
fn macos_header(app: &mut App, ui: &mut egui::Ui) -> Rect {
    let mut drag = ui.max_rect();
    drag.min.y -= HEADER_DROP;
    drag.max.y = drag.min.y + HEADER_DROP + 60.0;
    super::titlebar_drag(ui, drag);
    Frame::new()
        .inner_margin(Margin::symmetric(widgets::ROW_CARD_X as i8, 8))
        .show(ui, |ui| {
            let row = ui.horizontal(|ui| {
                ui.set_min_height(super::conversation::HEADER_ROW);
                header_row(app, ui, true);
            });
            ui.ctx()
                .data_mut(|data| data.insert_temp(header_row_id(), row.response.rect));
            ui.add_space(8.0);
            search_and_filters(app, ui);
        })
        .response
        .rect
}

/// Stable main-list row id used by interaction tests.
pub fn chat_row_id(chat: &str) -> egui::Id {
    egui::Id::new(("chat-row", chat))
}

/// Stable filter-chip id used by interaction tests.
pub fn filter_chip_id(filter: ChatFilter) -> egui::Id {
    egui::Id::new(("chat-filter", filter as u8))
}

/// Width of the fade over the filter chips' right edge.
pub(super) const CHIP_FADE: f32 = 32.0;

/// Filter chips under the search field. Search lists every match, so the
/// chips hide there.
fn filter_chips(app: &mut App, ui: &mut egui::Ui) {
    if app.show_starred {
        // The starred panel replaces the chat list, so its filters go with it.
        return;
    }
    if !app.locked_folder_open() && !app.search.trim().is_empty() {
        return;
    }
    let palette = app.palette;
    ui.add_space(crate::theme::SPACE_S);
    let output = widgets::chip_scroll_row(ui, "chat-filters", |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
            for filter in ChatFilter::EVERY {
                let count = match filter {
                    ChatFilter::All => 0,
                    _ => app.unread_chats(filter),
                };
                let selected = !app.locked_folder_open()
                    && !app.show_archived
                    && app.label_filter.is_none()
                    && app.chat_filter == filter;
                let chip = widgets::filter_chip(
                    ui,
                    &palette,
                    filter.label(app.locale).as_ref(),
                    count,
                    selected,
                )
                .tab_stop(match filter {
                    ChatFilter::All => Stop::All,
                    ChatFilter::Unread => Stop::Unread,
                    ChatFilter::Private => Stop::Private,
                    ChatFilter::Favorites => Stop::Favorites,
                    ChatFilter::Groups => Stop::Groups,
                    ChatFilter::Channels => Stop::Channels,
                });
                // Store the chip rect for interaction tests.
                ui.ctx()
                    .data_mut(|data| data.insert_temp(filter_chip_id(filter), chip.rect));
                let chip = if filter == ChatFilter::Channels {
                    let now = crate::util::now();
                    let all_muted = app
                        .chats
                        .iter()
                        .filter(|chat| chat.is_channel())
                        .all(|chat| chat.muted(now));
                    chip.context_menu(|ui| {
                        let (icon, label) = if all_muted {
                            (Icon::Bell, "Unmute all channels")
                        } else {
                            (Icon::BellOff, "Mute all channels")
                        };
                        if widgets::menu_item(ui, &palette, Some(icon), label) {
                            app.actions.push(Action::MuteAllChannels(!all_muted));
                            ui.close();
                        }
                    });
                    chip
                } else {
                    chip
                };
                if chip.clicked() {
                    // A second click on the active chip returns to every chat.
                    let next = if selected { ChatFilter::All } else { filter };
                    app.actions.push(Action::SetChatFilter(next));
                }
            }
            ui.add_space(crate::theme::SPACE_XS);
        })
    });
    // Chips cut off at the edge fade into the panel, which says the row
    // scrolls on.
    let hidden = output.content_size.x - output.state.offset.x - output.inner_rect.width();
    if hidden > 0.5 {
        // A translucent sidebar would show the fade as a band; the chips
        // are only cut there.
        if !palette.shows_desktop() {
            widgets::fade_right(ui, chip_band(&output), CHIP_FADE, palette.column());
        }
    }
    labels::chip_row(app, ui, &palette);
}

/// The band a chip row's chips take up. Its scroll area's inner rect can
/// run on down the panel, and a fade over all of it would darken the right
/// edge of every chat row under the chips.
pub(super) fn chip_band<R>(output: &egui::scroll_area::ScrollAreaOutput<R>) -> Rect {
    let rect = output.inner_rect;
    rect.with_max_y(rect.bottom().min(rect.top() + output.content_size.y))
}

/// Stable id for a starred row, so its menu can be addressed by the demo,
/// by tests, and by the row that keeps it open.
pub(crate) fn list_row_id(chat: &str, id: &str) -> egui::Id {
    egui::Id::new(("list-row", chat, id))
}

/// The starred messages, in the place the chat list usually takes. Newest
/// star first, each row naming the chat, the moment, and the message.
fn starred_list(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    if app.starred.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::Star,
            crate::i18n::gettext(app.locale, "No starred messages").as_ref(),
            crate::i18n::gettext(
                app.locale,
                "Right-click a message and choose Star to keep it here.",
            )
            .as_ref(),
        );
        return;
    }
    // Rows read the list from the app while they hand their actions back, so
    // the actions are collected here and applied once the list is drawn.
    let mut actions = Vec::new();
    under_header(ui, |ui, inset| {
        scroll_area(ui, inset).show(ui, |ui| {
            plain_rows(ui);
            ui.add_space(inset);
            for entry in &app.starred {
                if app
                    .chat(&entry.message.chat)
                    .is_some_and(|chat| chat.locked)
                {
                    continue;
                }
                starred_row(
                    app,
                    ui,
                    &palette,
                    &entry.message,
                    entry.starred_at,
                    conversation::Marks { starred: true },
                    &mut actions,
                );
            }
            // The last row scrolls clear of the tab bar.
            ui.add_space(super::tabbar::RESERVE);
        });
    });
    app.actions.append(&mut actions);
}

/// One starred message: where it is, when it was starred, and the message's own
/// words inside a bubble drawn from its side and fill. Clicking opens its chat
/// at the message; right-clicking the bubble opens the chat's own menu.
fn starred_row(
    app: &App,
    ui: &mut egui::Ui,
    palette: &Palette,
    message: &Message,
    marked_at: i64,
    marks: conversation::Marks,
    actions: &mut Vec<Action>,
) {
    let width = ui.available_width();
    let clock = widgets::line(
        ui,
        &crate::util::clock(message.timestamp),
        theme::regular(theme::TEXT_MICRO),
        palette.secondary,
        80.0,
        1,
    );
    let body_width = (width - 32.0 - 24.0 - clock.size().x - 10.0).max(60.0);
    // The message's own words with WhatsApp's markup taken off and mentions
    // named, the way the chat list previews one. Color emoji and wrapping go
    // through the same line helper as chat text.
    let summary = message.content.full_summary_in(app.locale);
    let words = crate::markup::plain(&app.resolve_mention_tokens(&summary), &[]);
    let body = widgets::line(
        ui,
        &words,
        theme::regular(theme::TEXT_CONTROL),
        palette.text,
        body_width,
        12,
    );
    let bubble_height = body.size().y + 12.0;
    let row_id = list_row_id(&message.chat, &message.id);
    let (rect, _) =
        ui.allocate_exact_size(vec2(width, 26.0 + bubble_height + 10.0), Sense::hover());
    let response = ui.interact(rect, row_id, Sense::click());
    theme::reveal_focus(&response);
    // The chat's own title, as the chat list writes it: a group without a
    // subject is named after its members, never "Unknown".
    let name = app.chat_title_of(&message.chat);
    let label = format!("{name}, {words}");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
    });
    if !ui.is_rect_visible(rect) {
        return;
    }
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            theme::RADIUS,
            palette.surface_hover.gamma_multiply(0.5),
        );
    }
    let when = crate::util::moment_stamp(app.locale, marked_at);
    let name_line = widgets::line(
        ui,
        &name,
        theme::medium(theme::TEXT_BODY),
        palette.text,
        width - 140.0,
        1,
    );
    name_line.paint(
        ui,
        pos2(
            rect.left() + 16.0,
            rect.top() + 14.0 - name_line.size().y / 2.0,
        ),
        palette.text,
    );
    ui.painter().text(
        pos2(rect.right() - 16.0, rect.top() + 14.0),
        egui::Align2::RIGHT_CENTER,
        when,
        theme::regular(theme::TEXT_CAPTION),
        palette.secondary,
    );
    // The message itself, as the bubble the chat shows: same fill, same side,
    // with the time in its bottom corner.
    let bubble_width = (body.size().x + 20.0 + clock.size().x + 6.0).min(width - 32.0);
    let left = if message.from_me {
        rect.right() - 16.0 - bubble_width
    } else {
        rect.left() + 16.0
    };
    let bubble = Rect::from_min_size(
        pos2(left, rect.top() + 26.0),
        vec2(bubble_width, bubble_height),
    );
    ui.painter().rect_filled(
        bubble,
        10.0,
        if message.from_me {
            palette.bubble_out
        } else {
            palette.bubble_in
        },
    );
    body.paint(
        ui,
        pos2(bubble.left() + 10.0, bubble.top() + 6.0),
        palette.text,
    );
    clock.paint(
        ui,
        pos2(
            bubble.right() - 10.0 - clock.size().x,
            bubble.bottom() - 6.0 - clock.size().y,
        ),
        palette.secondary,
    );
    // The bubble's rect is what the menu is anchored to, and what a test reads
    // to click it.
    ui.ctx()
        .data_mut(|data| data.insert_temp(row_id.with("bubble"), bubble));
    // Right-click is read from the input because the row's own response cannot
    // tell the bubble from the name line above it. The menu opens only over the
    // bubble, and only when no floating layer covers the chat panel.
    let right_clicked = ui.input(|input| {
        input.pointer.secondary_clicked()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|pos| bubble.contains(pos))
    }) && ui
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|pos| {
            ui.ctx()
                .layer_id_at(pos)
                .is_none_or(|layer| layer == response.layer_id)
        });
    #[cfg(any(test, feature = "demo"))]
    let force_menu = app
        .open_list_menu
        .as_ref()
        .is_some_and(|(chat, id)| chat == &message.chat && id == &message.id);
    #[cfg(not(any(test, feature = "demo")))]
    let force_menu = false;
    let open = if right_clicked || force_menu {
        Some(egui::SetOpenCommand::Bool(true))
    } else if response.clicked() {
        Some(egui::SetOpenCommand::Bool(false))
    } else {
        None
    };
    let popup = egui::Popup::menu(&response)
        .open_memory(open)
        .width(conversation::menu_width(
            ui,
            app.locale,
            &app.settings.reaction_emoji,
            message,
        ));
    let popup = if force_menu {
        popup.at_position(bubble.left_top() + vec2(12.0, 8.0))
    } else {
        popup.at_pointer_fixed()
    };
    popup.show_lifted(ui.ctx(), palette, |ui| {
        let chat = app
            .chat(&message.chat)
            .cloned()
            .unwrap_or_else(|| Chat::new(message.chat.clone(), name.clone()));
        conversation::list_menu(app, ui, &chat, message, marks, actions);
    });
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        actions.push(Action::OpenMessage {
            chat: message.chat.clone(),
            message: message.id.clone(),
        });
    }
}

fn list(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    // Taken here so a send seen while the list is hidden cannot move it later.
    let to_top = std::mem::take(&mut app.scroll_chats_to_top);
    if app.locked_folder_open() {
        locked_list(app, ui);
        return;
    }
    if app.secret_code_matched() {
        // Typing the secret code hides every other result: the folder entry
        // is all the search reveals until it is clicked.
        locked_entry(app, ui);
        return;
    }
    if app.show_starred {
        starred_list(app, ui);
        return;
    }
    if !app.search.trim().is_empty() {
        results(app, ui);
        return;
    }
    // Positions only: the rows drawn below copy their own chat.
    let chats = app.visible_chat_indices();
    if chats.is_empty() {
        // The favorites title is translated, so it is bound here: the tuple
        // below borrows it for this frame, and every other arm stays a literal.
        let favorites_title;
        let (title, body) = if app.show_archived {
            ("Nothing archived", "Archived chats appear here.")
        } else if app.chat_filter != ChatFilter::All {
            let title = match app.chat_filter {
                ChatFilter::Unread => "No unread chats",
                ChatFilter::Private => "No private chats",
                ChatFilter::Channels => "No channels",
                ChatFilter::Favorites => {
                    favorites_title = crate::i18n::gettext(app.locale, "No favorites yet");
                    favorites_title.as_ref()
                }
                _ => "No groups",
            };
            (title, "Choose All to see every chat.")
        } else if app.syncing {
            let label = crate::i18n::gettext(app.locale, "Loading your chats");
            let detail = crate::i18n::gettext(app.locale, "Receiving history from your phone.");
            super::skeleton::list(ui, &palette, label.as_ref(), detail.as_ref());
            return;
        } else {
            (
                "No chats yet",
                "Use New chat to message a contact or yourself.",
            )
        };
        widgets::empty_state(ui, &palette, Icon::MessageCircle, title, body);
        return;
    }
    under_header(ui, |ui, inset| chat_rows(app, ui, &chats, to_top, inset));
}

/// The scrolling rows of `chats`, positions in `app.chats`, the first
/// `inset` points of the scroll area lying under the frosted header.
fn chat_rows(app: &mut App, ui: &mut egui::Ui, chats: &[usize], to_top: bool, inset: f32) {
    // Rows touch: one clickable surface from top to bottom, no gaps or rules.
    ui.spacing_mut().item_spacing.y = 0.0;
    let row_height = theme::ROW_HEIGHT;
    let total = chats.len();
    let mut scroll_area = scroll_area(ui, inset).id_salt("chat-list");
    // The copies a narrow window's slide draws have widget ids of their own:
    // they show the real list's offset instead of keeping one.
    let copy = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(slide_copy_id()))
        .is_some();
    // Chats reordered by new messages keep the rows on screen where they
    // were; a request below to go somewhere else wins.
    let list_id = ui.make_persistent_id(egui::IdSalt::new("chat-list"));
    let view = list_view(app);
    let step = row_height + ui.spacing().item_spacing.y;
    if !copy
        && let Some(offset) = followed_offset(
            ui,
            list_id,
            rows_moved(ui, list_id, view, &app.chats, chats),
            step,
        )
    {
        scroll_area = scroll_area.vertical_scroll_offset(offset);
    }
    if to_top && near_top(ui, "chat-list", row_height) {
        scroll_area = scroll_area.vertical_scroll_offset(0.0);
    }
    let target_row = app.scroll_chat_into_view.as_ref().and_then(|target| {
        chats
            .iter()
            .position(|&index| app.chats[index].id == *target)
    });
    if copy
        && let Some(offset) = ui
            .ctx()
            .data(|data| data.get_temp::<f32>(shared_offset_id()))
    {
        scroll_area = scroll_area.vertical_scroll_offset(offset);
    }
    // A disabled list leaves the request and any carried scroll to the list
    // drawn for real.
    if let Some(target_row) = target_row.filter(|_| ui.is_enabled()) {
        let id = ui.make_persistent_id(egui::IdSalt::new("chat-list"));
        let current = egui::scroll_area::State::load(ui.ctx(), id)
            .unwrap_or_default()
            .offset
            .y;
        // Revealed clear of the header and of the tab bar.
        let offset = row_scroll_offset(
            current,
            ui.available_height() - inset - super::tabbar::RESERVE,
            target_row,
            row_height,
            ui.spacing().item_spacing.y,
        );
        scroll_area = scroll_area.vertical_scroll_offset(offset);
        app.scroll_chat_into_view = None;
    }
    // A scroll gesture that began over the list stays with it (#274).
    let carried = if ui.is_enabled() {
        app.scroll_route.take(crate::app::ScrollPane::Chats)
    } else {
        0.0
    };
    // A touchpad swipe past either end of the list stretches it and it
    // springs back, as the chat's messages do (`overscroll`).
    let list_rect = ui.available_rect_before_wrap();
    let mut overscroll = super::overscroll::Overscroll::load(ui.ctx(), "chat-list");
    if copy || !ui.is_enabled() || crate::motion::reduced(ui.ctx()) {
        overscroll.clear();
    } else {
        let wheel = ui.input(|input| super::overscroll::read_wheel(input, list_rect));
        overscroll.advance(ui.input(|input| input.time), wheel);
    }
    let stretch = overscroll.position();
    let output = show_rows_under(
        scroll_area,
        ui,
        row_height,
        total,
        inset,
        stretch,
        |ui, range| {
            if carried != 0.0 {
                ui.scroll_with_delta_animation(
                    vec2(0.0, carried),
                    egui::style::ScrollAnimation::none(),
                );
            }
            if !copy {
                remember_rows(
                    ui,
                    list_id,
                    view,
                    &app.chats,
                    &chats[range.clone()],
                    range.start,
                );
            }
            let motion = !copy && !crate::motion::reduced(ui.ctx());
            let now = ui.input(|input| input.time);
            let origin = ui.max_rect().top() - inset - range.start as f32 * step;
            let before = if motion {
                ui.ctx()
                    .data(|data| data.get_temp::<RowFlips>(list_id.with("flips")))
                    .filter(|flips| flips.view == view)
            } else {
                None
            };
            let mut flips = RowFlips {
                view,
                born: before.as_ref().map_or(now, |flips| flips.born),
                first: range.start,
                last: range.end,
                rows: std::collections::HashMap::new(),
            };
            let mut animating = false;
            for index in range {
                let Some(chat) = app.chats.get(chats[index]).cloned() else {
                    continue;
                };
                let natural = origin + inset + index as f32 * step;
                let previous = before.as_ref().and_then(|flips| flips.rows.get(&chat.id));
                if motion {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(chat_row_id(&chat.id).with("live"), previous.is_some())
                    });
                }
                let mark = super::swipe::mark(ui);
                // Key by chat so an open menu survives list reordering.
                let response = ui
                    .push_id(("chat", &chat.id), |ui| row(app, ui, &chat))
                    .inner;
                if motion {
                    let mut flip = match previous {
                        Some(flip) => *flip,
                        None => RowFlip {
                            top: natural,
                            origin,
                            from: 0.0,
                            start: f64::NEG_INFINITY,
                            shift: 0.0,
                            born: before
                                .as_ref()
                                .filter(|flips| {
                                    now - flips.born > 0.5
                                        && (flips.first..flips.last).contains(&index)
                                })
                                .map_or(f64::NEG_INFINITY, |_| now),
                        },
                    };
                    // Where the row was last drawn, in this frame's scroll.
                    let drawn = flip.top + (origin - flip.origin) + flip.shift;
                    if previous.is_some() && (drawn - flip.shift - natural).abs() > 0.5 {
                        flip.from = drawn - natural;
                        flip.start = now;
                    }
                    flip.top = natural;
                    flip.origin = origin;
                    flip.shift = row_flip_shift(flip.from, (now - flip.start) as f32);
                    let age = (now - flip.born) as f32;
                    let appear = row_appear(age);
                    if flip.shift != 0.0 || appear.is_some() {
                        let (opacity, scale) = appear.unwrap_or((1.0, 1.0));
                        shift_row(ui, mark, flip.shift, opacity, scale, response.rect.center());
                        animating = true;
                    }
                    flips.rows.insert(chat.id.clone(), flip);
                }
                // Store the row rect for interaction tests.
                ui.ctx()
                    .data_mut(|data| data.insert_temp(chat_row_id(&chat.id), response.rect));
                #[cfg(test)]
                ui.ctx().data_mut(|data| {
                    data.insert_temp(chat_row_id(&chat.id).with("widget"), response.id);
                });
                if response.clicked() && !app.show_archived {
                    app.actions.push(Action::KeepUnread(chat.id.clone()));
                }
            }
            if motion {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(list_id.with("flips"), flips));
                if animating {
                    ui.ctx().request_repaint();
                }
            }
        },
    );
    if !copy {
        ui.ctx()
            .data_mut(|data| data.insert_temp(shared_offset_id(), output.state.offset.y));
        overscroll.set_edges(super::overscroll::Edges::of(
            output.state.offset.y,
            output.inner_rect.height(),
            output.content_size.y,
        ));
        if overscroll.moving() {
            crate::motion::request_frame(ui.ctx());
        }
        overscroll.store(ui.ctx(), "chat-list");
    }
    app.scroll_route
        .place(crate::app::ScrollPane::Chats, output.inner_rect);
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(list_offset_id(), output.state.offset.y);
        data.insert_temp(list_offset_id().with("id"), output.id);
    });
}

/// The chats a list drew last frame, from the first on screen down, and the
/// filter they were drawn under, so [`rows_moved`] can tell where they went.
#[derive(Clone)]
struct ShownRows {
    view: egui::Id,
    pass: u64,
    first: usize,
    ids: Vec<String>,
}

/// Seconds a row takes to glide to its new place in the list.
const ROW_GLIDE: f32 = 0.25;

/// One row's motion: where it was drawn last frame (before any shift),
/// under which scroll origin, and the glide it is in the middle of.
#[derive(Clone, Copy)]
struct RowFlip {
    top: f32,
    origin: f32,
    from: f32,
    start: f64,
    shift: f32,
    born: f64,
}

/// The rows a list drew last frame, for [`RowFlip`]s, and the range of
/// positions they covered: a row missing from it that sits in the range is
/// a new chat, one beyond it only scrolled into view.
#[derive(Clone)]
struct RowFlips {
    view: egui::Id,
    born: f64,
    first: usize,
    last: usize,
    rows: std::collections::HashMap<String, RowFlip>,
}

/// How far a row is still from its place, `from` points off when it began
/// and `elapsed` seconds ago.
fn row_flip_shift(from: f32, elapsed: f32) -> f32 {
    if from == 0.0 || elapsed >= ROW_GLIDE {
        return 0.0;
    }
    from * (1.0 - theme::ease_out(elapsed / ROW_GLIDE))
}

/// Opacity and scale of a new chat's row `age` seconds in, or `None` when it
/// is not appearing (never, or done).
fn row_appear(age: f32) -> Option<(f32, f32)> {
    if !(0.0..theme::MOTION_PANEL).contains(&age) {
        return None;
    }
    let eased = theme::ease_out(age / theme::MOTION_PANEL);
    Some((eased, 0.97 + 0.03 * eased))
}

/// Paints the shapes the row drew since `from` moved down by `shift`, faded
/// to `opacity` and scaled about `pivot`. The clip stays: a gliding row
/// slides inside the list.
fn shift_row(
    ui: &egui::Ui,
    from: egui::layers::ShapeIdx,
    shift: f32,
    opacity: f32,
    scale: f32,
    pivot: egui::Pos2,
) {
    let transform =
        egui::emath::TSTransform::new(pivot.to_vec2() * (1.0 - scale) + vec2(0.0, shift), scale);
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(ui.layer_id());
        for index in from.0..list.next_idx().0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                if opacity < 1.0 {
                    egui::epaint::shape_transform::adjust_colors(&mut clipped.shape, move |c| {
                        if *c != egui::Color32::PLACEHOLDER {
                            *c = c.gamma_multiply(opacity);
                        }
                    });
                }
                clipped.shape.transform(transform);
            });
        }
    });
}

#[cfg(test)]
mod row_motion_tests {
    use super::*;

    #[test]
    fn preview_glyph_follows_the_kind_of_message() {
        let last = |summary: &str, labelled| crate::model::LastMessage {
            from_me: false,
            sender: String::new(),
            sender_name: None,
            summary: summary.to_owned(),
            full: summary.to_owned(),
            labelled,
            status: Default::default(),
        };
        assert_eq!(
            preview_icon(&last("Missed voice call", true)),
            Some((Icon::PhoneMissed, true))
        );
        assert_eq!(
            preview_icon(&last("Missed video call", true)),
            Some((Icon::Video, true))
        );
        assert_eq!(
            preview_icon(&last("Voice call (1:02)", true)),
            Some((Icon::Phone, false))
        );
        assert_eq!(
            preview_icon(&last("Video message", true)),
            Some((Icon::Video, false))
        );
        assert_eq!(
            preview_icon(&last("Voice message (0:12)", true)),
            Some((Icon::Mic, false))
        );
        assert_eq!(
            preview_icon(&last("Location: home", true)),
            Some((Icon::MapPin, false))
        );
        // A message whose own words read like a label gets no glyph.
        assert_eq!(preview_icon(&last("Location: home", false)), None);
        assert_eq!(preview_icon(&last("This message was deleted", true)), None);
    }

    #[test]
    fn glide_runs_from_the_old_place_to_none() {
        assert_eq!(row_flip_shift(80.0, 0.0), 80.0);
        let middle = row_flip_shift(80.0, 0.1);
        assert!(middle > 0.0 && middle < 40.0, "{middle}");
        assert_eq!(row_flip_shift(80.0, ROW_GLIDE), 0.0);
        assert_eq!(row_flip_shift(-80.0, 1.0), 0.0);
    }

    #[test]
    fn marks_pop_in_and_out_then_rest() {
        let (scale, opacity, room) = widgets::mark_pop(true, 0.0).unwrap();
        assert!((scale - 0.6).abs() < 1e-4 && opacity == 0.0 && room == 1.0);
        assert!(widgets::mark_pop(true, 1.0).is_none());
        assert!(widgets::mark_pop(true, f32::NEG_INFINITY).is_none());
        let (scale, opacity, room) = widgets::mark_pop(false, 0.05).unwrap();
        assert!(scale < 1.0 && opacity < 1.0 && room < 1.0);
        assert!(widgets::mark_pop(false, 1.0).is_none());
    }

    #[test]
    fn a_new_row_fades_and_grows_in() {
        assert_eq!(row_appear(0.0), Some((0.0, 0.97)));
        let (opacity, scale) = row_appear(0.1).unwrap();
        assert!(opacity > 0.0 && opacity < 1.0 && scale > 0.97 && scale < 1.0);
        assert_eq!(row_appear(theme::MOTION_PANEL), None);
        assert_eq!(row_appear(f32::NEG_INFINITY.max(-1.0)), None);
    }
}

/// What the list under `view` shows: a change of filter or archive is a new
/// list, not the old one moved.
fn list_view(app: &App) -> egui::Id {
    egui::Id::new((
        std::mem::discriminant(&app.chat_filter),
        &app.label_filter,
        app.show_archived,
        app.locked_folder_open(),
    ))
}

/// How many rows the chats the list of scroll area `id` showed last frame
/// moved since, so its offset can follow them: a message to a chat far
/// down lifts it to the top, and every chat between moves a row down,
/// which scrolled the list under the reader. The first pair of rows that
/// still sit together says it; a chat that moved away from its neighbour
/// is the one that jumped to the top. Nothing moved, or a list drawn under
/// another filter, says zero.
fn rows_moved(ui: &egui::Ui, id: egui::Id, view: egui::Id, all: &[Chat], chats: &[usize]) -> isize {
    let Some(shown) = ui
        .ctx()
        .data(|data| data.get_temp::<ShownRows>(id.with("shown")))
    else {
        return 0;
    };
    let passes = ui.ctx().options(|options| options.max_passes.get()) as u64;
    if shown.view != view || ui.ctx().cumulative_pass_nr() > shown.pass + passes {
        return 0;
    }
    let same = shown.ids.iter().enumerate().all(|(n, id)| {
        chats
            .get(shown.first + n)
            .and_then(|&index| all.get(index))
            .is_some_and(|chat| chat.id == *id)
    });
    if same {
        return 0;
    }
    let position = |id: &str| {
        chats
            .iter()
            .position(|&index| all.get(index).is_some_and(|chat| chat.id == id))
    };
    for (n, pair) in shown.ids.windows(2).enumerate() {
        if let (Some(at), Some(next)) = (position(&pair[0]), position(&pair[1]))
            && next == at + 1
        {
            return at as isize - (shown.first + n) as isize;
        }
    }
    0
}

/// Remembers the chats the list of scroll area `id` drew this frame.
fn remember_rows(
    ui: &egui::Ui,
    id: egui::Id,
    view: egui::Id,
    all: &[Chat],
    chats: &[usize],
    first: usize,
) {
    let shown = ShownRows {
        view,
        pass: ui.ctx().cumulative_pass_nr(),
        first,
        ids: chats
            .iter()
            .filter_map(|&index| all.get(index))
            .map(|chat| chat.id.clone())
            .collect(),
    };
    ui.ctx()
        .data_mut(|data| data.insert_temp(id.with("shown"), shown));
}

/// The offset that keeps the rows a scrolled list showed on screen when
/// `moved` rows arrived above them (or left), or `None` at the top, where
/// a chat lifted there by a new message shows up as it does on the phone.
fn followed_offset(ui: &egui::Ui, id: egui::Id, moved: isize, step: f32) -> Option<f32> {
    let offset = egui::scroll_area::State::load(ui.ctx(), id)?.offset.y;
    (moved != 0 && offset > 0.5).then(|| (offset + moved as f32 * step).max(0.0))
}

/// Whether the list scrolled by `salt` is near enough its top to follow a sent
/// message's chat there: at most one row down, so its first row still shows
/// at least in part. A reader further down is working through older chats, and
/// a send there keeps their place instead of throwing them back to the top.
fn near_top(ui: &egui::Ui, salt: &str, row_height: f32) -> bool {
    let id = ui.make_persistent_id(egui::IdSalt::new(salt));
    let offset = egui::scroll_area::State::load(ui.ctx(), id)
        .unwrap_or_default()
        .offset
        .y;
    offset <= row_height
}

/// Set while a narrow window's slide draws copies of the list.
pub(crate) fn slide_copy_id() -> egui::Id {
    egui::Id::new("chat-list-slide-copy")
}

/// The real list's scroll offset, for the copies a slide draws.
fn shared_offset_id() -> egui::Id {
    egui::Id::new("chat-list-real-offset")
}

/// Where the chat list's scroll offset is kept for tests.
#[cfg(test)]
pub(crate) fn list_offset_id() -> egui::Id {
    egui::Id::new("chat-list-offset")
}

/// The row the secret code reveals: the only thing the search then shows.
fn locked_entry(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let count = app.locked_count();
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            widgets::row_highlight(ui, &palette, rect, palette.surface_hover);
        }
        let icon_rect = Rect::from_center_size(
            pos2(rect.left() + ROW_LEAD + 24.0, rect.center().y),
            Vec2::splat(22.0),
        );
        Icon::LockOpen
            .image(palette.accent, 22.0)
            .paint_at(ui, icon_rect);
        ui.painter().text(
            pos2(rect.left() + ROW_LEAD + 62.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Locked chats",
            theme::medium(theme::TEXT_BODY),
            palette.text,
        );
        ui.painter().text(
            pos2(rect.right() - widgets::ROW_SIDE, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            count.to_string(),
            theme::regular(theme::TEXT_CAPTION),
            palette.accent,
        );
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.actions.push(Action::OpenLockedFolder);
    }
}

/// The authenticated folder, including archived chats and local title search.
fn locked_list(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let chats = app.visible_chat_indices();
    if chats.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::LockOpen,
            "No locked chats",
            "Lock a chat from its context menu to hide it here.",
        );
        return;
    }
    under_header(ui, |ui, inset| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut scroll_area = scroll_area(ui, inset).id_salt("locked-chats");
        let target_row = app.scroll_chat_into_view.as_ref().and_then(|target| {
            chats
                .iter()
                .position(|&index| app.chats[index].id == *target)
        });
        if let Some(target_row) = target_row {
            let id = ui.make_persistent_id(egui::IdSalt::new("locked-chats"));
            let current = egui::scroll_area::State::load(ui.ctx(), id)
                .unwrap_or_default()
                .offset
                .y;
            let offset = row_scroll_offset(
                current,
                ui.available_height() - inset - super::tabbar::RESERVE,
                target_row,
                theme::ROW_HEIGHT,
                ui.spacing().item_spacing.y,
            );
            scroll_area = scroll_area.vertical_scroll_offset(offset);
            app.scroll_chat_into_view = None;
        }
        show_rows_under(
            scroll_area,
            ui,
            theme::ROW_HEIGHT,
            chats.len(),
            inset,
            0.0,
            |ui, range| {
                for &index in &chats[range] {
                    let Some(chat) = app.chats.get(index).cloned() else {
                        continue;
                    };
                    ui.push_id(("chat", &chat.id), |ui| self::row(app, ui, &chat));
                }
            },
        );
    });
}

/// Returns the smallest offset that fully reveals a fixed-height row.
fn row_scroll_offset(
    current: f32,
    viewport_height: f32,
    row: usize,
    row_height: f32,
    spacing: f32,
) -> f32 {
    let top = row as f32 * (row_height + spacing);
    let bottom = top + row_height;
    if top < current {
        top
    } else if bottom > current + viewport_height {
        (bottom - viewport_height).max(0.0)
    } else {
        current
    }
}

/// One row of the search results' flat list.
#[derive(Clone, Copy)]
enum ResultRow {
    Section(&'static str),
    Chat(usize),
    Hit(usize),
    Myself,
    Contact(usize),
}

/// How tall a section title's row is: the space above, the label, the space
/// below.
const SECTION_HEIGHT: f32 = 38.0;

/// The search results laid out as one flat list, built when the query or the
/// data it reads changes and reused by every frame between, so scrolling
/// costs the visible rows alone: no matching, sorting or cloning per frame.
struct ResultView {
    key: u64,
    rows: Vec<ResultRow>,
    /// The top of each row; one more entry ends the last.
    tops: Vec<f32>,
    contacts: Vec<String>,
}

impl ResultView {
    fn height(row: ResultRow) -> f32 {
        match row {
            ResultRow::Section(_) => SECTION_HEIGHT,
            _ => theme::ROW_HEIGHT,
        }
    }

    /// The rows from the first lying below `top` to the last above `bottom`.
    fn visible(&self, top: f32, bottom: f32) -> std::ops::Range<usize> {
        let count = self.rows.len();
        let first = self
            .tops
            .partition_point(|&edge| edge <= top)
            .saturating_sub(1)
            .min(count);
        let last = self.tops.partition_point(|&edge| edge < bottom).min(count);
        first..last.max(first)
    }
}

/// What the results depend on, cheap to compute every frame: the query, the
/// filters, and the fields of each chat the list reads. A change anywhere
/// else (a new message in a chat, a rename) changes one of these.
fn result_key(app: &App) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    app.search.trim().hash(&mut hasher);
    app.open_chat.hash(&mut hasher);
    app.show_archived.hash(&mut hasher);
    app.locked_folder_open().hash(&mut hasher);
    app.chats.len().hash(&mut hasher);
    app.contacts.values().count().hash(&mut hasher);
    app.me.hash(&mut hasher);
    for chat in &app.chats {
        (
            chat.last_activity,
            chat.pinned,
            chat.archived,
            chat.locked,
            chat.name.len(),
            chat.last.as_ref().map(|last| last.summary.len()),
        )
            .hash(&mut hasher);
    }
    app.search_hits.len().hash(&mut hasher);
    if let (Some(first), Some(last)) = (app.search_hits.first(), app.search_hits.last()) {
        (&first.id, &last.id).hash(&mut hasher);
    }
    hasher.finish()
}

fn result_view(app: &App, ctx: &egui::Context) -> std::sync::Arc<ResultView> {
    let key = result_key(app);
    let id = egui::Id::new("search-result-view");
    if let Some(view) = ctx.data(|data| data.get_temp::<std::sync::Arc<ResultView>>(id))
        && view.key == key
    {
        return view;
    }
    let chats = app.visible_chat_indices();
    let contacts: Vec<String> = app
        .matching_contacts()
        .into_iter()
        .map(|contact| contact.id.clone())
        .collect();
    let needle = crate::util::search_key(app.search.trim());
    let offer_self = !needle.is_empty()
        && app.offers_self(&needle)
        && !chats
            .iter()
            .any(|&index| app.me.as_deref() == Some(app.chats[index].id.as_str()));
    let mut rows = Vec::with_capacity(chats.len() + app.search_hits.len() + contacts.len() + 4);
    if !chats.is_empty() {
        rows.push(ResultRow::Section("Chats"));
        rows.extend(chats.into_iter().map(ResultRow::Chat));
    }
    if !app.search_hits.is_empty() {
        rows.push(ResultRow::Section("Messages"));
        rows.extend((0..app.search_hits.len()).map(ResultRow::Hit));
    }
    if !contacts.is_empty() || offer_self {
        rows.push(ResultRow::Section("Contacts"));
        if offer_self {
            rows.push(ResultRow::Myself);
        }
        rows.extend((0..contacts.len()).map(ResultRow::Contact));
    }
    let mut tops = Vec::with_capacity(rows.len() + 1);
    let mut y = 0.0;
    for &row in &rows {
        tops.push(y);
        y += ResultView::height(row);
    }
    tops.push(y);
    let view = std::sync::Arc::new(ResultView {
        key,
        rows,
        tops,
        contacts,
    });
    ctx.data_mut(|data| data.insert_temp(id, view.clone()));
    view
}

/// Search results grouped into chats, messages, and contacts. The rows are
/// one flat list of which only the visible ones are laid out and drawn.
fn results(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let view = result_view(app, ui.ctx());
    if view.rows.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::Search,
            "No results",
            "Try another name, number, or message text.",
        );
        return;
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    under_header(ui, |ui, inset| {
        let mut area = scroll_area(ui, inset).id_salt("search-results");
        // Arrow keys reveal the reached chat: its row's place is known
        // without laying out the rows above it.
        let target = app.scroll_chat_into_view.as_ref().and_then(|target| {
            view.rows.iter().position(|row| {
                matches!(row, ResultRow::Chat(index)
                    if app.chats.get(*index).is_some_and(|chat| chat.id == *target))
            })
        });
        if let Some(position) = target.filter(|_| ui.is_enabled()) {
            let id = ui.make_persistent_id(egui::IdSalt::new("search-results"));
            let current = egui::scroll_area::State::load(ui.ctx(), id)
                .unwrap_or_default()
                .offset
                .y;
            let height = ui.available_height() - inset - super::tabbar::RESERVE;
            let (top, bottom) = (view.tops[position], view.tops[position + 1]);
            let offset = if top < current {
                top
            } else if bottom > current + height {
                (bottom - height).max(0.0)
            } else {
                current
            };
            area = area.vertical_scroll_offset(offset);
            app.scroll_chat_into_view = None;
        }
        let output = area.show_viewport(ui, |ui, viewport| {
            // Room below the last row for it to scroll clear of the tab bar.
            let total = view.tops.last().copied().unwrap_or(0.0) + 8.0 + super::tabbar::RESERVE;
            ui.set_height(inset + total);
            let range = view.visible(viewport.min.y - inset, viewport.max.y - inset);
            let origin = ui.max_rect().top() + inset;
            let x_range = ui.max_rect().x_range();
            for index in range {
                let row = view.rows[index];
                let rect = Rect::from_x_y_ranges(
                    x_range,
                    origin + view.tops[index]..=origin + view.tops[index + 1],
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    plain_colours(ui);
                    ui.skip_ahead_auto_ids(index);
                    match row {
                        ResultRow::Section(label) => section(ui, &palette, label),
                        ResultRow::Chat(position) => {
                            let Some(chat) = app.chats.get(position).cloned() else {
                                return;
                            };
                            ui.push_id(("chat", &chat.id), |ui| self::row(app, ui, &chat));
                        }
                        ResultRow::Hit(position) => {
                            let Some(hit) = app.search_hits.get(position).cloned() else {
                                return;
                            };
                            ui.push_id(("hit", &hit.chat, &hit.id), |ui| hit_row(app, ui, &hit));
                        }
                        ResultRow::Myself => {
                            ui.push_id("self", |ui| self_row(app, ui));
                        }
                        ResultRow::Contact(position) => {
                            let Some(contact) = view
                                .contacts
                                .get(position)
                                .and_then(|id| app.contacts.get(id))
                                .cloned()
                            else {
                                return;
                            };
                            ui.push_id(("contact", &contact.id), |ui| {
                                contact_row(app, ui, &contact, false)
                            });
                        }
                    }
                });
            }
        });
        note_scrolled(ui.ctx(), output.state.offset.y);
    });
}

fn section(ui: &mut egui::Ui, palette: &Palette, label: &str) {
    ui.add_space(8.0);
    Frame::new()
        .inner_margin(Margin {
            left: widgets::ROW_SIDE as i8,
            right: widgets::ROW_SIDE as i8,
            top: 0,
            bottom: 10,
        })
        .show(ui, |ui| {
            theme::text(
                ui,
                label,
                theme::semibold(theme::TEXT_CAPTION),
                palette.accent,
            );
        });
}

/// A message search result. Clicking it opens the chat at that message.
fn hit_row(app: &mut App, ui: &mut egui::Ui, hit: &Message) {
    let palette = app.palette;
    let title = match app.chat(&hit.chat) {
        Some(chat) => app.chat_title(&chat.clone()),
        None => app.display_name_or(&hit.chat, None),
    };
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    theme::reveal_focus(&response);
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            widgets::row_highlight(ui, &palette, rect, palette.surface_hover);
        }
        let avatar_rect = Rect::from_center_size(
            pos2(rect.left() + ROW_LEAD + 24.0, rect.center().y),
            Vec2::splat(48.0),
        );
        let picture = app.avatar(&hit.chat);
        widgets::paint_avatar(
            ui,
            &palette,
            avatar_rect,
            &title,
            &hit.chat,
            picture.as_deref(),
        );
        let left = rect.left() + ROW_LEAD + 62.0;
        let right = rect.right() - widgets::ROW_SIDE;
        let stamp_galley = ui.painter().layout_no_wrap(
            crate::util::chat_stamp(app.locale, hit.timestamp),
            theme::regular(theme::TEXT_CAPTION),
            palette.dim,
        );
        let name_top = rect.top() + 14.0;
        ui.painter().galley(
            pos2(right - stamp_galley.size().x, name_top + 1.0),
            stamp_galley.clone(),
            palette.dim,
        );
        let name_width = (right - stamp_galley.size().x - 8.0 - left).max(0.0);
        let name = widgets::line(
            ui,
            &title,
            theme::medium(theme::TEXT_BODY),
            palette.text,
            name_width,
            1,
        );
        name.paint(ui, pos2(left, name_top), palette.text);
        // Show the sender for group messages.
        let line_y = rect.top() + 38.0;
        let mut x = left;
        if hit.from_me {
            let who = widgets::line(
                ui,
                "You: ",
                theme::regular(theme::TEXT_CONTROL),
                palette.dim,
                (right - x) * 0.5,
                1,
            );
            who.paint(ui, pos2(x, line_y), palette.dim);
            x += who.size().x;
        } else if crate::model::ChatKind::from_id(&hit.chat) == crate::model::ChatKind::Group {
            let sender = app.display_name_or(&hit.sender, hit.sender_name.as_deref());
            let first = app.short_name(&hit.sender, &sender);
            let who = widgets::line(
                ui,
                &format!("{first}: "),
                theme::regular(theme::TEXT_CONTROL),
                palette.dim,
                (right - x) * 0.5,
                1,
            );
            who.paint(ui, pos2(x, line_y), palette.dim);
            x += who.size().x;
        }
        let words = widgets::line(
            ui,
            &app.preview_line(&hit.summary_in(app.locale), hit),
            theme::regular(theme::TEXT_CONTROL),
            palette.dim,
            (right - x).max(0.0),
            1,
        );
        words.paint(ui, pos2(x, line_y), palette.dim);
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        app.actions.push(Action::OpenMessage {
            chat: hit.chat.clone(),
            message: hit.id.clone(),
        });
    }
}

/// A contact without a chat. Clicking starts one.
pub(super) fn contact_row(app: &mut App, ui: &mut egui::Ui, contact: &Contact, selected: bool) {
    let name = app.display_name(&contact.id);
    let detail = crate::model::phone_of(&contact.id).map(crate::util::phone);
    if person_row(app, ui, &contact.id, &name, detail.as_deref(), selected).clicked() {
        app.actions.push(Action::StartChat {
            id: contact.id.clone(),
            name,
            dismiss_dialog: true,
        });
    }
}

/// Offers the chat with ourselves, as "Name (You)" over "Message yourself".
pub(super) fn self_row(app: &mut App, ui: &mut egui::Ui) {
    let Some(me) = app.me.clone() else {
        return;
    };
    let name = app.self_title();
    let detail = crate::i18n::gettext(app.locale, "Message yourself");
    if person_row(app, ui, &me, &name, Some(detail.as_ref()), false).clicked() {
        app.actions.push(Action::MessageYourself);
    }
}

/// The contact whose row New chat last scrolled into view as its first match.
pub(super) fn revealed_row_id() -> egui::Id {
    egui::Id::new("person-row-revealed")
}

/// A row that starts something, drawn like a person's: an icon on the
/// accent colour where the photo would be. New chat's New group and Add
/// contact.
pub(super) fn action_row(app: &App, ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    theme::reveal_focus(&response);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if ui.is_rect_visible(rect) {
        if response.hovered() || response.has_focus() {
            widgets::row_highlight(ui, &palette, rect, palette.surface_hover);
        }
        let circle = Rect::from_center_size(
            pos2(rect.left() + ROW_LEAD + 24.0, rect.center().y),
            Vec2::splat(48.0),
        );
        ui.painter()
            .circle_filled(circle.center(), circle.width() / 2.0, palette.accent_fill);
        theme::paint_icon(ui, icon, circle, 22.0, palette.on_accent);
        let left = rect.left() + ROW_LEAD + 62.0;
        let line = widgets::line(
            ui,
            label,
            theme::medium(theme::TEXT_BODY),
            palette.text,
            rect.right() - widgets::ROW_SIDE - left,
            1,
        );
        line.paint(
            ui,
            pos2(left, rect.center().y - line.size().y / 2.0),
            palette.text,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn person_row(
    app: &mut App,
    ui: &mut egui::Ui,
    id: &str,
    name: &str,
    detail: Option<&str>,
    selected: bool,
) -> egui::Response {
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    theme::reveal_focus(&response);
    // The highlighted row is scrolled into view once each time it changes, so
    // Enter never opens a chat the list has scrolled past. Scrolling again
    // every frame would fight the reader.
    let revealed = revealed_row_id();
    if selected {
        if ui
            .ctx()
            .data(|data| data.get_temp::<String>(revealed))
            .as_deref()
            != Some(id)
        {
            response.scroll_to_me(Some(egui::Align::Center));
            ui.ctx()
                .data_mut(|data| data.insert_temp(revealed, id.to_owned()));
        }
    } else if ui
        .ctx()
        .data(|data| data.get_temp::<String>(revealed))
        .as_deref()
        == Some(id)
    {
        ui.ctx().data_mut(|data| data.remove::<String>(revealed));
    }
    if ui.is_rect_visible(rect) {
        if selected {
            widgets::row_highlight(ui, &palette, rect, palette.surface_active);
        } else if response.hovered() {
            widgets::row_highlight(ui, &palette, rect, palette.surface_hover);
        }
        let avatar_rect = Rect::from_center_size(
            pos2(rect.left() + ROW_LEAD + 24.0, rect.center().y),
            Vec2::splat(48.0),
        );
        let picture = app.avatar(id);
        widgets::paint_avatar(ui, &palette, avatar_rect, name, id, picture.as_deref());
        let left = rect.left() + ROW_LEAD + 62.0;
        let name_line = widgets::line(
            ui,
            name,
            theme::medium(theme::TEXT_BODY),
            palette.text,
            rect.right() - widgets::ROW_SIDE - left,
            1,
        );
        name_line.paint(ui, pos2(left, rect.top() + 14.0), palette.text);
        if let Some(detail) = detail {
            let phone_line = widgets::line(
                ui,
                detail,
                theme::regular(theme::TEXT_CONTROL),
                palette.dim,
                rect.right() - widgets::ROW_SIDE - left,
                1,
            );
            phone_line.paint(ui, pos2(left, rect.top() + 38.0), palette.dim);
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// How quickly a row's hover fill catches the pointer, and how long it
/// takes to leave. The leave is slower, so a fast pass down the list
/// trails the fill behind the pointer instead of blinking each row.
const HOVER_IN: f32 = 0.07;
const HOVER_OUT: f32 = 0.12;

/// Moves `amount` (0 off, 1 on) toward `on` over `dt` seconds.
/// The bool is whether it has arrived.
fn approach_hover(amount: f32, on: bool, dt: f32) -> (f32, bool) {
    let target = if on { 1.0 } else { 0.0 };
    let tau = if on { HOVER_IN } else { HOVER_OUT };
    let step = 1.0 - (-dt.clamp(0.0, 0.05) / tau).exp();
    let next = (amount + (target - amount) * step).clamp(0.0, 1.0);
    if (target - next).abs() < 0.012 {
        (target, true)
    } else {
        (next, false)
    }
}

/// The row's hover fill, eased. It lands at once while motion is reduced,
/// as it is with a screen reader attached, where the animation would
/// otherwise ask for a frame every step of the way.
pub(crate) fn row_hover_amount(ui: &egui::Ui, id: egui::Id, on: bool) -> f32 {
    if crate::motion::reduced(ui.ctx()) {
        return if on { 1.0 } else { 0.0 };
    }
    let id = id.with("hover-fill");
    let previous = ui.data(|data| data.get_temp::<f32>(id)).unwrap_or(0.0);
    let dt = ui.input(|input| input.stable_dt);
    let (amount, settled) = approach_hover(previous, on, dt);
    if amount > 0.0 {
        ui.data_mut(|data| data.insert_temp(id, amount));
    }
    if !settled {
        ui.ctx().request_repaint();
    }
    amount
}

/// Width of the chat row's menu caret, in points.
const ROW_CARET: f32 = 20.0;
/// Room between the time and the caret once the caret has arrived.
const ROW_CARET_GAP: f32 = 4.0;
/// How far to the right the caret starts, before it glides into place.
const ROW_CARET_SLIDE: f32 = 8.0;
/// Seconds the caret takes to arrive. Leaving uses the same curve.
const ROW_CARET_SECONDS: f32 = 0.15;

/// How far the time moves left as the row caret comes in.
/// `progress` is 0 hidden and 1 in place: the caret's width plus [`ROW_CARET_GAP`].
fn row_caret_time_shift(progress: f32) -> f32 {
    progress.clamp(0.0, 1.0) * (ROW_CARET + ROW_CARET_GAP)
}

/// 0 to 1 for the row caret over [`ROW_CARET_SECONDS`], at once when motion
/// is reduced.
fn row_caret_progress(ctx: &egui::Context, id: egui::Id, on: bool) -> f32 {
    ctx.animate_bool_with_time_and_easing(
        id,
        on,
        theme::motion_time(ctx, ROW_CARET_SECONDS),
        egui::emath::easing::cubic_out,
    )
}

fn row(app: &mut App, ui: &mut egui::Ui, chat: &Chat) -> egui::Response {
    let palette = app.palette;
    let title = app.chat_title(chat);
    // While searching, the result reached with the arrows is the selection;
    // before any arrow press it stays the open chat, as a click leaves it.
    let selected = app
        .search_selected
        .as_ref()
        .filter(|_| !app.search.trim().is_empty())
        .or(app.open_chat.as_ref())
        .is_some_and(|selected| *selected == chat.id);
    let now = crate::util::now();
    let muted = chat.muted(now);
    let shapes = super::swipe::mark(ui);
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), theme::ROW_HEIGHT),
        Sense::click(),
    );
    theme::reveal_focus(&response);
    // The preview area and the whole last message, when the row cuts it short.
    let mut full_preview: Option<(Rect, String, String)> = None;
    let mut caret_clicked = false;
    let mut caret_held = false;
    let mut caret_at: Option<egui::Pos2> = None;
    let mut drew_caret = false;
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            unread_announcement(&title, chat),
        )
    });
    // The caret sits on the row, so the pointer over it still counts as the
    // row: otherwise the highlight would drop while the button is under it.
    let over_caret = ui
        .data(|data| data.get_temp::<Rect>(response.id.with("row-caret-rect")))
        .is_some_and(|caret| ui.rect_contains_pointer(caret));
    // While this row's menu is open, from the caret or a right-click, the
    // row keeps its highlight, so the menu's target stays plain to see.
    let menu_open = egui::Popup::is_id_open(ui.ctx(), response.id.with("popup"));
    let hover = row_hover_amount(
        ui,
        response.id,
        !selected && (response.hovered() || over_caret || menu_open),
    );
    if ui.is_rect_visible(rect) {
        if selected {
            widgets::row_highlight(ui, &palette, rect, palette.surface_active);
        } else if hover > 0.0 {
            ui.scope(|ui| {
                ui.multiply_opacity(hover);
                widgets::row_highlight(ui, &palette, rect, palette.surface_hover);
            });
        }
        let avatar_rect = Rect::from_center_size(
            pos2(rect.left() + ROW_LEAD + 24.0, rect.center().y),
            Vec2::splat(48.0),
        );
        let picture = app.avatar(&chat.id);
        widgets::paint_avatar(
            ui,
            &palette,
            avatar_rect,
            &title,
            &chat.id,
            picture.as_deref(),
        );
        if chat.disappearing_timer().is_some() {
            widgets::paint_disappearing_badge(ui, &palette, avatar_rect);
        }

        let left = rect.left() + ROW_LEAD + 62.0;
        let right = rect.right() - widgets::ROW_SIDE;
        let divider = if selected { 0.0 } else { 1.0 - hover };
        widgets::row_divider(ui, &palette, rect, left, divider);
        let stamp = if chat.last_activity > 0 {
            crate::util::chat_stamp(app.locale, chat.last_activity)
        } else {
            String::new()
        };
        let unread = chat.looks_unread();
        let stamp_color = if unread && !muted {
            palette.accent
        } else {
            palette.dim
        };
        let stamp_galley =
            ui.painter()
                .layout_no_wrap(stamp, theme::regular(theme::TEXT_CAPTION), stamp_color);
        let name_top = rect.top() + 14.0;
        let caret_on = ui.rect_contains_pointer(rect) || menu_open;
        let caret_t = row_caret_progress(ui.ctx(), response.id.with("row-caret"), caret_on);
        let time_shift = row_caret_time_shift(caret_t);
        ui.painter().galley(
            pos2(right - stamp_galley.size().x - time_shift, name_top + 1.0),
            stamp_galley.clone(),
            stamp_color,
        );
        let name_width = (right - stamp_galley.size().x - 8.0 - time_shift - left).max(0.0);
        let name_font = if unread {
            theme::semibold(theme::TEXT_BODY)
        } else {
            theme::medium(theme::TEXT_BODY)
        };
        let name = widgets::line(ui, &title, name_font, palette.text, name_width, 1);
        name.paint(ui, pos2(left, name_top), palette.text);
        if caret_t > 0.0 {
            drew_caret = true;
            let slide = (1.0 - caret_t) * ROW_CARET_SLIDE;
            let caret_rect = Rect::from_center_size(
                pos2(
                    right - ROW_CARET * 0.5 + slide,
                    name_top + 1.0 + stamp_galley.size().y * 0.5,
                ),
                Vec2::splat(ROW_CARET),
            );
            ui.data_mut(|data| data.insert_temp(response.id.with("row-caret-rect"), caret_rect));
            let caret = ui
                .interact(
                    caret_rect,
                    response.id.with("row-caret-btn"),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            theme::reveal_focus(&caret);
            caret.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Chat menu")
            });
            ui.scope(|ui| {
                ui.multiply_opacity(caret_t);
                ui.painter()
                    .rect_filled(caret_rect, ROW_CARET * 0.5, palette.surface);
                ui.painter().rect_stroke(
                    caret_rect,
                    ROW_CARET * 0.5,
                    egui::Stroke::new(theme::OUTLINE_STROKE, palette.outline),
                    egui::StrokeKind::Inside,
                );
                Icon::ChevronDown.image(palette.secondary, 14.0).paint_at(
                    ui,
                    Rect::from_center_size(caret_rect.center(), Vec2::splat(14.0)),
                );
            });
            // Hit-testing uses last frame's widgets, so the first hover frame
            // still belongs to the row. A press inside the circle is the caret.
            let pointer_in_caret = ui
                .input(|input| input.pointer.interact_pos())
                .is_some_and(|pos| caret_rect.contains(pos));
            let hit = caret.clicked() || (response.clicked() && pointer_in_caret);
            caret_held = caret.is_pointer_button_down_on()
                || (response.is_pointer_button_down_on() && pointer_in_caret);
            if hit && !menu_open {
                caret_clicked = true;
                let at = caret_rect.left_bottom();
                caret_at = Some(at);
                ui.ctx().data_mut(|data| {
                    data.insert_temp(response.id.with("row-caret-menu"), true);
                    data.insert_temp(response.id.with("row-caret-anchor"), at);
                });
            } else if hit {
                caret_clicked = true;
                ui.ctx().data_mut(|data| {
                    data.remove::<bool>(response.id.with("row-caret-menu"));
                });
            }
        }

        // Leave room for badges beside the latest-message preview.
        let mut badge_right = right;
        let line_y = rect.top() + 38.0;
        let live = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(chat_row_id(&chat.id).with("live")))
            .unwrap_or(false);
        let mark_at = pos2(badge_right - 10.0, line_y + 8.0);
        if chat.unread > 0 {
            let width =
                widgets::badge_animated(ui, ui.id(), &palette, mark_at, chat.unread, muted, live);
            badge_right -= width + 6.0;
        } else {
            widgets::badge_gone(ui, ui.id());
        }
        // The empty dot of a chat marked unread by hand pops in and out.
        let dot = widgets::pop_mark(
            ui,
            ui.id().with("unread-dot"),
            chat.unread == 0 && chat.marked_unread,
            live,
            mark_at,
            |ui| widgets::unread_dot(ui, &palette, mark_at, muted) + 6.0,
        );
        badge_right -= dot;
        // A mention calls even in a muted chat, as on WhatsApp.
        let mention_at = pos2(badge_right - 10.0, line_y + 8.0);
        let mention = widgets::pop_mark(
            ui,
            ui.id().with("mention-badge"),
            chat.unread > 0 && chat.unread_mention.is_some(),
            live,
            mention_at,
            |ui| widgets::mention_badge(ui, &palette, mention_at) + 6.0,
        );
        badge_right -= mention;
        if muted {
            let icon_rect =
                Rect::from_center_size(pos2(badge_right - 8.0, line_y + 8.0), Vec2::splat(15.0));
            Icon::VolumeX
                .image(palette.dim, 15.0)
                .paint_at(ui, icon_rect);
            badge_right -= 20.0;
        }
        if chat.pinned {
            let icon_rect =
                Rect::from_center_size(pos2(badge_right - 8.0, line_y + 8.0), Vec2::splat(14.0));
            Icon::Pin.image(palette.dim, 14.0).paint_at(ui, icon_rect);
            badge_right -= 20.0;
        }
        let mut x = left;
        let typing = app.typing_in(&chat.id);
        let preview_color = if unread && !muted {
            palette.secondary
        } else {
            palette.dim
        };
        let preview = if !typing.is_empty() {
            let who = if chat.is_group() {
                format!("{} is typing…", typing[0].1.trim_start_matches('~'))
            } else {
                "typing…".to_owned()
            };
            widgets::line(
                ui,
                &who,
                theme::medium(theme::TEXT_CONTROL),
                palette.accent,
                badge_right - x,
                1,
            )
        } else if let Some(draft) = app.draft_preview(&chat.id) {
            // Unsent text waits here as in WhatsApp, marked in the accent.
            let label = format!("{} ", crate::i18n::gettext(app.locale, "Draft:"));
            let label = widgets::line(
                ui,
                &label,
                theme::medium(theme::TEXT_CONTROL),
                palette.accent,
                (badge_right - x) * 0.5,
                1,
            );
            let width = label.size().x;
            label.paint(ui, pos2(x, line_y), palette.accent);
            x += width;
            widgets::line(
                ui,
                &draft,
                theme::regular(theme::TEXT_CONTROL),
                preview_color,
                (badge_right - x).max(0.0),
                1,
            )
        } else if let Some(last) = &chat.last {
            let mut prefix = String::new();
            if last.from_me {
                let tick_rect =
                    Rect::from_center_size(pos2(x + 8.0, line_y + 8.0), Vec2::splat(16.0));
                widgets::ticks(
                    ui,
                    &palette,
                    tick_rect,
                    super::conversation::display_delivery(
                        last.status,
                        last.from_me,
                        chat.kind,
                        app.account_receipts_off,
                    ),
                );
                x += 20.0;
            } else if chat.is_group() {
                let sender = app.display_name_or(&last.sender, last.sender_name.as_deref());
                let first = app.short_name(&last.sender, &sender);
                prefix = format!("{first}: ");
                let sender = widgets::line(
                    ui,
                    &prefix,
                    theme::regular(theme::TEXT_CONTROL),
                    preview_color,
                    (badge_right - x) * 0.5,
                    1,
                );
                let width = sender.size().x;
                sender.paint(ui, pos2(x, line_y), preview_color);
                x += width;
            }
            // A small glyph for the kind of message leads the words, after
            // the ticks and the sender, as WhatsApp draws it. A missed call's
            // glyph is red; the words stay grey.
            if let Some((icon, missed)) = preview_icon(last) {
                let color = if missed {
                    palette.danger
                } else {
                    preview_color
                };
                let icon_rect = Rect::from_center_size(
                    pos2(x + PREVIEW_ICON / 2.0, line_y + 8.0),
                    Vec2::splat(PREVIEW_ICON),
                );
                icon.image(color, PREVIEW_ICON).paint_at(ui, icon_rect);
                x += PREVIEW_ICON + PREVIEW_ICON_GAP;
            }
            let words = widgets::line(
                ui,
                &crate::markup::plain(
                    &app.resolve_mention_tokens(&last.summary_in(app.locale)),
                    &[],
                ),
                theme::regular(theme::TEXT_CONTROL),
                preview_color,
                (badge_right - x).max(0.0),
                1,
            );
            // The row shows one line: offer the whole message when that line
            // was cut short or the message has more lines than it.
            if words.galley.elided || last.full.trim_end() != last.summary {
                let area = Rect::from_min_max(
                    pos2(left, line_y - 4.0),
                    pos2(badge_right, line_y + words.size().y.max(16.0) + 4.0),
                );
                full_preview = Some((area, prefix, last.full_in(app.locale).into_owned()));
            }
            words
        } else {
            widgets::line(
                ui,
                "",
                theme::regular(theme::TEXT_CONTROL),
                preview_color,
                1.0,
                1,
            )
        };
        preview.paint(ui, pos2(x, line_y), preview_color);
    }
    if !drew_caret {
        ui.data_mut(|data| data.remove::<Rect>(response.id.with("row-caret-rect")));
    }
    if super::carry::carrying(ui.ctx()) {
        super::carry::target(
            ui,
            rect,
            chat,
            &title,
            pos2(rect.left() + ROW_LEAD + 24.0, rect.center().y),
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if let Some((area, prefix, full)) = full_preview {
        full_preview_tooltip(app, ui, &chat.id, area, &prefix, &full);
    }
    if response.clicked() && !caret_clicked {
        app.actions.push(Action::OpenChat(chat.id.clone()));
    } else if response.is_pointer_button_down_on()
        && !caret_held
        && app
            .conversations
            .get(&chat.id)
            .is_none_or(|conversation| !conversation.requested)
    {
        // The press comes a moment before the click: long enough to read the
        // chat's newest messages, so it opens whole rather than blank.
        app.actions.push(Action::PreloadChat(chat.id.clone()));
    }
    swipe(app, ui, chat, rect, shapes);
    let menu_palette = palette;
    // The favorite item is translated, so its width counts in the reader's
    // language.
    let favorite_label = if chat.favorite {
        crate::i18n::gettext(app.locale, "Remove from favorites")
    } else {
        crate::i18n::gettext(app.locale, "Add to favorites")
    };
    let menu_width = widgets::menu_width(
        ui,
        &[
            "Mark as read",
            "Mark as unread",
            "Pin to top",
            favorite_label.as_ref(),
            "Unarchive",
            &crate::i18n::gettext(app.locale, "Leave group"),
            &crate::i18n::gettext(app.locale, "Leave channel"),
            &crate::i18n::gettext(app.locale, "Delete chat"),
            "Unlock chat",
        ],
        true,
    )
    // Submenu rows also carry a chevron.
    .max(
        widgets::menu_width(
            ui,
            &[
                "Mute",
                &crate::i18n::gettext(app.locale, "More"),
                &crate::i18n::gettext(app.locale, "Labels"),
            ],
            true,
        ) + 24.0,
    )
    .max(190.0);
    if app.settings.split_view {
        super::split::watch(ui, response.rect, &chat.id, || app.chat_title(chat));
    }
    let popup = egui::Popup::context_menu(&response)
        .width(menu_width)
        .frame(widgets::menu_frame(&menu_palette));
    #[cfg(any(test, feature = "demo"))]
    let popup = if app.open_chat_menu.as_deref() == Some(chat.id.as_str()) {
        popup
            .open_memory(Some(egui::SetOpenCommand::Bool(true)))
            .at_position(response.rect.left_top() + vec2(12.0, 8.0))
    } else {
        popup
    };
    // Right-click still opens where the pointer is. A caret click opens the
    // same menu under the button, and keeps that anchor while it stays open.
    if response.secondary_clicked() {
        ui.ctx().data_mut(|data| {
            data.remove::<bool>(response.id.with("row-caret-menu"));
        });
    }
    let from_caret = !response.secondary_clicked()
        && ui.ctx().data(|data| {
            data.get_temp::<bool>(response.id.with("row-caret-menu"))
                .unwrap_or(false)
        });
    let caret_anchor = if from_caret {
        caret_at.or_else(|| {
            ui.ctx()
                .data(|data| data.get_temp(response.id.with("row-caret-anchor")))
        })
    } else {
        None
    };
    let menu_already = egui::Popup::is_id_open(ui.ctx(), response.id.with("popup"));
    let popup = if let Some(at) = caret_anchor {
        let set = if caret_clicked && !menu_already {
            Some(egui::SetOpenCommand::Bool(true))
        } else if response.clicked() {
            Some(egui::SetOpenCommand::Bool(false))
        } else {
            None
        };
        popup.open_memory(set).at_position(at)
    } else {
        popup
    };
    popup.show_compact(ui.ctx(), &menu_palette, |ui| {
        context_menu(app, ui, chat, &menu_palette)
    });
    if !egui::Popup::is_id_open(ui.ctx(), response.id.with("popup")) {
        ui.ctx().data_mut(|data| {
            data.remove::<bool>(response.id.with("row-caret-menu"));
            data.remove::<egui::Pos2>(response.id.with("row-caret-anchor"));
        });
    }
    response
}

/// Two fingers swiped sideways across a chat: the row follows them showing
/// what Settings give that swipe, and letting go past the threshold does it.
fn swipe(app: &mut App, ui: &egui::Ui, chat: &Chat, rect: Rect, shapes: egui::layers::ShapeIdx) {
    let area = Rect::from_x_y_ranges(ui.clip_rect().x_range(), rect.y_range());
    let pull = super::swipe::over(ui.ctx(), area);
    if let Some(pull) = pull.filter(|pull| pull.fired) {
        let action = super::swipe::action(app, false, pull.right);
        let actions = super::swipe::actions(app, action, chat, None);
        app.actions.extend(actions);
    }
    let icon = |right| {
        let action = super::swipe::action(app, false, right);
        super::swipe::available(app, action, chat, false).then(|| super::swipe::icon(action, chat))
    };
    let id = egui::Id::new(("swipe-chat", &chat.id));
    super::swipe::slide_row(ui, id, shapes, rect, pull, icon, &app.palette);
}

/// The widest a chat row's full-message tooltip grows before it wraps.
const FULL_PREVIEW_WIDTH: f32 = 360.0;
/// Lines a full-message tooltip shows before it ends in an ellipsis.
const FULL_PREVIEW_ROWS: usize = 12;
/// Characters of a message the tooltip lays out: far more than its rows
/// hold, so a very long message costs no more than a long one.
const FULL_PREVIEW_CHARS: usize = 2_000;

/// Size of the kind glyph that leads a chat row's preview, in points.
const PREVIEW_ICON: f32 = 14.0;
/// Space between that glyph and the preview words.
const PREVIEW_ICON_GAP: f32 = 4.0;

/// The glyph that leads a chat row's preview for a kind of message, and
/// whether it is drawn in the danger colour (a missed call). Plain text and
/// the other notices have none.
pub fn preview_icon(last: &crate::model::LastMessage) -> Option<(Icon, bool)> {
    if !last.labelled {
        return None;
    }
    // The English label the summary starts with names the kind; the row only
    // keeps the summary, not the message.
    Some(match crate::model::summary_label_of(&last.summary)? {
        "Photo" | "View once photo" => (Icon::Image, false),
        "GIF" => (Icon::Gif, false),
        "Video" | "Video message" | "View once video" => (Icon::Video, false),
        "Voice message" | "View once voice message" => (Icon::Mic, false),
        "Audio" | "View once audio" => (Icon::Music, false),
        "Document" => (Icon::FileText, false),
        "Sticker" | "Sticker pack" => (Icon::Sticker, false),
        "Location" | "Live location" | "Live location ended" => (Icon::MapPin, false),
        "Contact" => (Icon::Contact, false),
        "Poll" => (Icon::ListChecks, false),
        "Missed voice call" => (Icon::PhoneMissed, true),
        "Missed video call" => (Icon::Video, true),
        "Voice call" => (Icon::Phone, false),
        "Video call" => (Icon::Video, false),
        _ => return None,
    })
}

/// Where a chat row's latest-message preview sits, for hover tests.
pub fn preview_id(chat: &str) -> egui::Id {
    egui::Id::new(("chat-preview", chat))
}

/// Shows the whole last message while the pointer rests on a chat row's
/// cut-short preview, as WhatsApp Web does. It keeps out of the way of an
/// open menu and of a drag.
fn full_preview_tooltip(
    app: &App,
    ui: &egui::Ui,
    chat: &str,
    area: Rect,
    prefix: &str,
    full: &str,
) {
    let hover = ui.interact(area, preview_id(chat), Sense::hover());
    if egui::Popup::is_any_open(ui.ctx()) || ui.ctx().dragged_id().is_some() {
        return;
    }
    egui::Tooltip::for_enabled(&hover)
        .width(FULL_PREVIEW_WIDTH)
        .show(|ui| {
            let full: String = full.chars().take(FULL_PREVIEW_CHARS).collect();
            let text = format!(
                "{prefix}{}",
                crate::markup::plain(&app.resolve_mention_tokens(&full), &[])
            );
            let color = ui.visuals().text_color();
            let line = widgets::line(
                ui,
                text.trim_end(),
                theme::regular(theme::TEXT_CONTROL),
                color,
                FULL_PREVIEW_WIDTH,
                FULL_PREVIEW_ROWS,
            );
            let (rect, _) = ui.allocate_exact_size(line.size(), Sense::hover());
            line.paint(ui, rect.min, color);
        });
}

/// Height of one avatar cell in the collapsed list.
const COMPACT_CELL: f32 = 52.0;
/// Avatar size inside a collapsed cell.
const COMPACT_AVATAR: f32 = 36.0;
/// Height of the fade over the rail's avatars where they scroll on.
const COMPACT_FADE: f32 = 16.0;

/// Where one avatar in the collapsed chat list was drawn.
pub fn compact_chat_id(chat: &str) -> egui::Id {
    egui::Id::new(("chat-rail", chat))
}

/// Width of the navigation rail, which stands only where the list is hidden
/// and the chats' avatars fill. On macOS the traffic lights reach past it,
/// over the conversation's header beside it.
pub fn compact_width(_ctx: &egui::Context) -> f32 {
    super::rail::WIDTH
}

/// The rail's avatars while the list is hidden: the chats the full list
/// would show right now, under the same filter, search, archive, and
/// locked-folder state.
pub(super) fn compact_list(app: &mut App, ui: &mut egui::Ui) {
    // The copies a narrow window's slide draws show the real column's
    // offset, and leave its requests and carried scroll to it.
    let copy = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(slide_copy_id()))
        .is_some();
    let to_top = !copy && std::mem::take(&mut app.scroll_chats_to_top);
    if !app.locked_folder_open() && app.secret_code_matched() {
        // As in the full list, the secret code reveals only the way in.
        compact_locked_entry(app, ui);
        return;
    }
    let chats = app.visible_chat_indices();
    // Exactly the height the rail gives it: egui's least scrolled height
    // would reach past the window's bottom in a short one.
    let mut scroll_area = egui::ScrollArea::vertical()
        .id_salt("chat-rail")
        .auto_shrink([false, false])
        .min_scrolled_height(0.0);
    let rail_id = ui.make_persistent_id(egui::IdSalt::new("chat-rail"));
    let view = list_view(app);
    if !copy
        && let Some(offset) = followed_offset(
            ui,
            rail_id,
            rows_moved(ui, rail_id, view, &app.chats, &chats),
            COMPACT_CELL + ui.spacing().item_spacing.y,
        )
    {
        scroll_area = scroll_area.vertical_scroll_offset(offset);
    }
    if to_top && near_top(ui, "chat-rail", COMPACT_CELL) {
        scroll_area = scroll_area.vertical_scroll_offset(0.0);
    }
    if copy
        && let Some(offset) = ui
            .ctx()
            .data(|data| data.get_temp::<f32>(shared_offset_id().with("rail")))
    {
        scroll_area = scroll_area.vertical_scroll_offset(offset);
    }
    // Alt+Up/Down reveals the chat it opens here too.
    let target_row = app
        .scroll_chat_into_view
        .as_ref()
        .filter(|_| !copy)
        .and_then(|target| {
            chats
                .iter()
                .position(|&index| app.chats[index].id == *target)
        });
    if let Some(target_row) = target_row {
        let id = ui.make_persistent_id(egui::IdSalt::new("chat-rail"));
        let current = egui::scroll_area::State::load(ui.ctx(), id)
            .unwrap_or_default()
            .offset
            .y;
        let offset = row_scroll_offset(
            current,
            ui.available_height(),
            target_row,
            COMPACT_CELL,
            ui.spacing().item_spacing.y,
        );
        scroll_area = scroll_area.vertical_scroll_offset(offset);
        app.scroll_chat_into_view = None;
    }
    let pane = crate::app::ScrollPane::Chats;
    let carried = if copy {
        0.0
    } else {
        app.scroll_route.take(pane)
    };
    // Only the avatars on screen are laid out, however many chats there are.
    let output = scroll_area.show_rows(ui, COMPACT_CELL, chats.len(), |ui, range| {
        let mut focused = None;
        if !copy {
            remember_rows(
                ui,
                rail_id,
                view,
                &app.chats,
                &chats[range.clone()],
                range.start,
            );
        }
        if carried != 0.0 {
            ui.scroll_with_delta_animation(
                vec2(0.0, carried),
                egui::style::ScrollAnimation::none(),
            );
        }
        for &index in &chats[range] {
            let Some(chat) = app.chats.get(index).cloned() else {
                continue;
            };
            let response = ui
                .push_id(("chat", &chat.id), |ui| compact_row(app, ui, &chat))
                .inner;
            if response.clicked() && !app.locked_folder_open() && !app.show_archived {
                app.actions.push(Action::KeepUnread(chat.id.clone()));
            }
            if response.has_focus() {
                focused = Some(response);
            }
        }
        focused
    });
    // The inner list scrolls the focused avatar within its own viewport;
    // reveal that same avatar in the rail's outer scroll area too.
    if let Some(focused) = output.inner {
        theme::reveal_focus_in(ui, &focused);
    }
    if !copy {
        ui.ctx().data_mut(|data| {
            data.insert_temp(shared_offset_id().with("rail"), output.state.offset.y);
        });
        app.scroll_route.place(pane, output.inner_rect);
    }
    // Avatars cut off at either end fade into the rail, which says the
    // column scrolls on. A translucent rail would show the fade as a band.
    let palette = app.palette;
    if !palette.shows_desktop() {
        let rect = output.inner_rect;
        let offset = output.state.offset.y;
        if offset > 0.5 {
            widgets::fade_top(ui, rect, COMPACT_FADE, palette.rail());
        }
        if output.content_size.y - offset - rect.height() > 0.5 {
            widgets::fade_bottom(ui, rect, COMPACT_FADE, palette.rail());
        }
    }
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(list_offset_id().with("compact"), output.state.offset.y);
        data.insert_temp(list_offset_id().with("compact-viewport"), output.inner_rect);
        data.insert_temp(list_offset_id().with("compact-id"), output.id);
    });
}

/// The collapsed form of the entry the secret code reveals.
fn compact_locked_entry(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), COMPACT_CELL), Sense::click());
    theme::reveal_focus(&response);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Locked chats")
    });
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(
                Rect::from_center_size(rect.center(), Vec2::splat(COMPACT_CELL - 4.0)),
                egui::CornerRadius::same(theme::RADIUS),
                palette.surface_hover,
            );
        }
        Icon::LockOpen
            .image(palette.accent, 22.0)
            .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(22.0)));
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(compact_locked_id(), response.rect));
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Locked chats")
        .clicked()
    {
        app.actions.push(Action::OpenLockedFolder);
    }
}

/// Where the collapsed locked-chats entry was drawn.
fn compact_locked_id() -> egui::Id {
    egui::Id::new("chat-rail-locked")
}

/// One avatar in the collapsed chat list: a click opens the chat, hovering
/// names it, and unread chats carry the same badge as a full row.
fn compact_row(app: &mut App, ui: &mut egui::Ui, chat: &Chat) -> egui::Response {
    let palette = app.palette;
    let title = app.chat_title(chat);
    let selected = app.open_chat.as_deref() == Some(chat.id.as_str());
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), COMPACT_CELL), Sense::click());
    // A short window leaves the rail's avatars little more than one row.
    theme::reveal_focus_in(ui, &response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            unread_announcement(&title, chat),
        )
    });
    let hover = row_hover_amount(ui, response.id, response.hovered());
    if ui.is_rect_visible(rect) {
        if hover > 0.0 {
            let plate = Rect::from_center_size(rect.center(), Vec2::splat(COMPACT_CELL - 4.0));
            ui.scope(|ui| {
                ui.multiply_opacity(hover);
                ui.painter().rect_filled(
                    plate,
                    egui::CornerRadius::same(theme::RADIUS),
                    palette.surface_hover,
                );
            });
        }
        let avatar_rect = Rect::from_center_size(rect.center(), Vec2::splat(COMPACT_AVATAR));
        let picture = app.avatar(&chat.id);
        widgets::paint_avatar(
            ui,
            &palette,
            avatar_rect,
            &title,
            &chat.id,
            picture.as_deref(),
        );
        if chat.disappearing_timer().is_some() {
            widgets::paint_disappearing_badge(ui, &palette, avatar_rect);
        }
        if selected {
            // A short accent bar stands in for the selected row's background.
            ui.painter().rect_filled(
                Rect::from_min_size(
                    pos2(rect.left() + 1.0, rect.center().y - 11.0),
                    vec2(3.0, 22.0),
                ),
                1.5,
                palette.accent,
            );
        }
        if chat.looks_unread() {
            // Top right, clear of the disappearing-messages timer in the
            // bottom right corner. A muted chat's badge is dimmed, as in the
            // full row.
            widgets::unread_indicator(
                ui,
                &palette,
                compact_badge_center(avatar_rect),
                chat.unread,
                chat.marked_unread,
                chat.muted(crate::util::now()),
            );
        }
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(compact_chat_id(&chat.id), response.rect));
    if super::carry::carrying(ui.ctx()) {
        super::carry::target(ui, rect, chat, &title, rect.center());
    }
    if app.settings.split_view {
        super::split::watch(ui, response.rect, &chat.id, || title.clone());
    }
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(title);
    if response.clicked() {
        app.actions.push(Action::OpenChat(chat.id.clone()));
    }
    response
}

/// Centre of the unread badge on a collapsed avatar. Longer counts widen it
/// to the left, so it still ends inside the rail.
fn compact_badge_center(avatar: Rect) -> egui::Pos2 {
    pos2(avatar.right() - 10.0, avatar.top() + 2.0)
}

fn context_menu(app: &mut App, ui: &mut egui::Ui, chat: &Chat, palette: &Palette) {
    let locale = app.locale;
    if chat.looks_unread()
        && widgets::menu_item(
            ui,
            palette,
            Some(Icon::CheckCheck),
            &crate::i18n::gettext(locale, "Mark as read"),
        )
    {
        app.actions.push(Action::MarkRead(chat.id.clone()));
    }
    if !chat.looks_unread()
        && widgets::menu_item(
            ui,
            palette,
            Some(Icon::MessageCircle),
            &crate::i18n::gettext(locale, "Mark as unread"),
        )
    {
        app.actions.push(Action::MarkUnread(chat.id.clone()));
    }
    let pin_label = if chat.pinned {
        crate::i18n::gettext(locale, "Unpin")
    } else {
        crate::i18n::gettext(locale, "Pin to top")
    };
    if widgets::menu_item(
        ui,
        palette,
        Some(if chat.pinned { Icon::PinOff } else { Icon::Pin }),
        pin_label.as_ref(),
    ) {
        app.actions
            .push(Action::SetPinned(chat.id.clone(), !chat.pinned));
    }
    let now = crate::util::now();
    if chat.muted(now) {
        if widgets::menu_item(
            ui,
            palette,
            Some(Icon::Bell),
            &crate::i18n::gettext(locale, "Unmute"),
        ) {
            app.actions.push(Action::SetMuted(chat.id.clone(), None));
        }
    } else {
        let mute_8 = crate::i18n::gettext(locale, "Mute for 8 hours");
        let mute_week = crate::i18n::gettext(locale, "Mute for a week");
        let mute_forever = crate::i18n::gettext(locale, "Mute indefinitely");
        let mute_label = crate::i18n::gettext(locale, "Mute");
        let options = [
            (mute_8.as_ref(), Some(now + 8 * 3600)),
            (mute_week.as_ref(), Some(now + 7 * 86_400)),
            (mute_forever.as_ref(), Some(0)),
        ];
        let items: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
        widgets::submenu(
            ui,
            palette,
            Icon::BellOff,
            mute_label.as_ref(),
            &items,
            |ui| {
                for (label, until) in options {
                    if widgets::menu_item(ui, palette, None, label) {
                        app.actions.push(Action::SetMuted(chat.id.clone(), until));
                        ui.close();
                    }
                }
            },
        );
    }
    let archive_label = if chat.archived {
        crate::i18n::gettext(locale, "Unarchive")
    } else {
        crate::i18n::gettext(locale, "Archive")
    };
    if widgets::menu_item(ui, palette, Some(Icon::Archive), archive_label.as_ref()) {
        app.actions
            .push(Action::SetArchived(chat.id.clone(), !chat.archived));
    }
    widgets::menu_separator(ui, palette);
    // Channels cannot be favorites, as on the phone.
    if !chat.is_channel() {
        // Bound before the call so the translated text outlives the borrow.
        let favorite_label = if chat.favorite {
            crate::i18n::gettext(locale, "Remove from favorites")
        } else {
            crate::i18n::gettext(locale, "Add to favorites")
        };
        if widgets::menu_item(ui, palette, Some(Icon::Heart), favorite_label.as_ref()) {
            app.actions
                .push(Action::SetFavorite(chat.id.clone(), !chat.favorite));
        }
    }
    labels::chat_menu(app, ui, chat, palette);
    let lock_label = if chat.locked {
        crate::i18n::gettext(locale, "Unlock chat")
    } else {
        crate::i18n::gettext(locale, "Lock chat")
    };
    if widgets::menu_item(
        ui,
        palette,
        Some(if chat.locked {
            Icon::LockOpen
        } else {
            Icon::Lock
        }),
        lock_label.as_ref(),
    ) {
        app.actions.push(if chat.locked {
            Action::SetLocked(chat.id.clone(), false)
        } else {
            Action::ShowDialog(Dialog::ConfirmLockChat(chat.id.clone()))
        });
    }
    if widgets::menu_item(
        ui,
        palette,
        Some(Icon::Info),
        &crate::i18n::gettext(locale, "Info"),
    ) {
        // The panel sits beside the conversation, so the chat opens first.
        app.actions.push(Action::OpenChat(chat.id.clone()));
        app.actions.push(Action::OpenInfo(chat.id.clone()));
    }
    more_menu(app, ui, palette, chat);
    // Leaving and deleting come last, apart and in red.
    let can_leave = chat.can_leave(&app.our_ids());
    // Deleting reaches the phone, so it waits for a connection.
    let connected = matches!(app.link, LinkStatus::Connected);
    widgets::menu_separator(ui, palette);
    // Bound before the call so the translated text outlives the borrow.
    let leave_label = if chat.is_channel() {
        crate::i18n::gettext(locale, "Leave channel")
    } else {
        crate::i18n::gettext(locale, "Leave group")
    };
    if can_leave
        && widgets::menu_item_danger(ui, palette, Some(Icon::LogOut), leave_label.as_ref(), true)
    {
        app.actions
            .push(Action::ShowDialog(Dialog::ConfirmLeaveGroup(
                chat.id.clone(),
            )));
    }
    if widgets::menu_item_danger(
        ui,
        palette,
        Some(Icon::Trash),
        &crate::i18n::gettext(locale, "Delete chat"),
        connected,
    ) {
        app.actions
            .push(Action::ShowDialog(Dialog::ConfirmDeleteChat(
                chat.id.clone(),
            )));
    }
}

/// The chat menu's "More" submenu: the entries used less often, each with
/// its icon like the menu that opens it.
fn more_menu(app: &mut App, ui: &mut egui::Ui, palette: &Palette, chat: &Chat) {
    let more = crate::i18n::gettext(app.locale, "More");
    // Wide enough for the longest of its rows, a submenu's chevron included.
    let rows = [
        crate::i18n::gettext(app.locale, "Notification sound"),
        "Voice message speed".into(),
        crate::i18n::gettext(app.locale, "Copy number"),
        crate::i18n::gettext(app.locale, "Open beside"),
        crate::i18n::gettext(app.locale, "Pop out chat"),
        crate::i18n::gettext(app.locale, "Open as bubble"),
        crate::i18n::gettext(app.locale, "Block contact"),
        crate::i18n::gettext(app.locale, "Unblock contact"),
    ];
    let items: Vec<&str> = rows.iter().map(AsRef::as_ref).collect();
    widgets::submenu(ui, palette, Icon::Ellipsis, more.as_ref(), &items, |ui| {
        let locale = app.locale;
        sound_menu(app, ui, palette, chat);
        voice_speed_menu(app, ui, palette, chat);
        if let Some(phone) = chat.phone()
            && widgets::menu_item(
                ui,
                palette,
                Some(Icon::Copy),
                &crate::i18n::gettext(locale, "Copy number"),
            )
        {
            app.actions.push(Action::CopyText(format!("+{phone}")));
        }
        let beside = app.settings.split_view
            && app.current_chat().is_some_and(|open| open.id != chat.id)
            && !super::narrow(ui.ctx());
        let beside_label = crate::i18n::gettext(locale, "Open beside");
        if beside && widgets::menu_item(ui, palette, Some(Icon::Columns), beside_label.as_ref()) {
            app.actions.push(Action::OpenSplit {
                chat: chat.id.clone(),
                side: crate::app::Side::Right,
            });
        }
        let pop_label = crate::i18n::gettext(locale, "Pop out chat");
        if app.settings.split_view
            && widgets::menu_item(
                ui,
                palette,
                Some(Icon::PictureInPicture),
                pop_label.as_ref(),
            )
        {
            app.actions.push(Action::PopOut(chat.id.clone()));
        }
        let bubble_label = crate::i18n::gettext(locale, "Open as bubble");
        if app.settings.chat_bubbles
            && !chat.locked
            && widgets::menu_item(
                ui,
                palette,
                Some(Icon::MessageCircle),
                bubble_label.as_ref(),
            )
        {
            app.actions.push(Action::Bubble(chat.id.clone()));
        }
        super::blocking::menu(app, ui, &chat.id);
    });
}

/// What a screen reader reads out for a chat's unread state. A chat marked
/// unread by hand has no count to read, so it must not announce zero.
fn unread_announcement(title: &str, chat: &Chat) -> String {
    if chat.marked_unread && chat.unread == 0 {
        format!("{title}, unread")
    } else if chat.unread > 0 && chat.unread_mention.is_some() {
        format!("{title}, {} unread messages, mentioned you", chat.unread)
    } else {
        format!("{title}, {} unread messages", chat.unread)
    }
}

/// A chat's own notification sound, overriding Settings for this chat.
fn sound_menu(app: &mut App, ui: &mut egui::Ui, palette: &Palette, chat: &Chat) {
    use crate::settings::NotificationSound;
    let locale = app.locale;
    let current = chat.notification_sound.clone();
    let default_label = crate::i18n::gettext(locale, "Default");
    let system_label = crate::i18n::gettext(locale, "System sound");
    let none_label = crate::i18n::gettext(locale, "No sound");
    let sounds = [
        (None, default_label),
        (Some(NotificationSound::Receive), "Pidgin".into()),
        (Some(NotificationSound::Alert), "Pidgin alert".into()),
        (Some(NotificationSound::System), system_label),
        (Some(NotificationSound::None), none_label),
    ];
    let custom = match &current {
        Some(NotificationSound::Custom(path)) => Some(path.file_name().map_or_else(
            || crate::i18n::gettext(locale, "Custom").into_owned(),
            |name| name.to_string_lossy().into_owned(),
        )),
        _ => None,
    };
    let choose = crate::i18n::gettext(locale, "Choose a file…");
    let items: Vec<&str> = sounds
        .iter()
        .map(|(_, label)| label.as_ref())
        .chain(custom.as_deref())
        .chain([choose.as_ref()])
        .collect();
    let menu_label = crate::i18n::gettext(locale, "Notification sound");
    widgets::submenu(ui, palette, Icon::Music, &menu_label, &items, |ui| {
        for (sound, label) in &sounds {
            let checked = current == *sound;
            if widgets::menu_item(ui, palette, checked.then_some(Icon::Check), label.as_ref()) {
                app.actions.push(Action::SetChatSound {
                    chat: chat.id.clone(),
                    sound: sound.clone(),
                });
                ui.close();
            }
        }
        if let Some(name) = &custom {
            widgets::menu_item(ui, palette, Some(Icon::Check), name);
        }
        if widgets::menu_item(ui, palette, None, choose.as_ref()) {
            app.actions.push(Action::PickChatSound(chat.id.clone()));
            ui.close();
        }
    });
}

/// A chat's own voice message speed, overriding Settings for this chat.
fn voice_speed_menu(app: &mut App, ui: &mut egui::Ui, palette: &Palette, chat: &Chat) {
    let options: Vec<(Option<f32>, String)> = std::iter::once((
        None,
        format!(
            "Default ({})",
            crate::audio::speed_label(app.settings.voice_speed)
        ),
    ))
    .chain(
        crate::audio::SPEEDS
            .into_iter()
            .map(|speed| (Some(speed), crate::audio::speed_label(speed))),
    )
    .collect();
    let items: Vec<&str> = options.iter().map(|(_, label)| label.as_str()).collect();
    widgets::submenu(
        ui,
        palette,
        Icon::Timer,
        "Voice message speed",
        &items,
        |ui| {
            for (speed, label) in &options {
                let speed = *speed;
                let checked = chat.voice_speed == speed;
                if widgets::menu_item(ui, palette, checked.then_some(Icon::Check), label) {
                    app.actions.push(Action::SetChatVoiceSpeed {
                        chat: chat.id.clone(),
                        speed,
                    });
                    ui.close();
                }
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The time slides left by the caret's width and gap as it arrives,
    /// and no further.
    #[test]
    fn time_shift_follows_the_caret() {
        assert_eq!(row_caret_time_shift(0.0), 0.0);
        assert_eq!(row_caret_time_shift(1.0), ROW_CARET + ROW_CARET_GAP);
        let half = (ROW_CARET + ROW_CARET_GAP) / 2.0;
        assert!((row_caret_time_shift(0.5) - half).abs() < 1e-4);
        assert_eq!(row_caret_time_shift(-1.0), 0.0);
        assert_eq!(row_caret_time_shift(2.0), ROW_CARET + ROW_CARET_GAP);
    }

    #[test]
    fn hover_fill_arrives_quickly_and_leaves_a_trail() {
        let (arrived, _) = approach_hover(0.0, true, 0.07);
        let (left, _) = approach_hover(1.0, false, 0.07);
        assert!(arrived > 0.5, "{arrived}");
        assert!(left > 0.5, "{left}");
        assert!(
            arrived > 1.0 - left,
            "turning on ({arrived}) should outrun turning off ({left})"
        );
    }

    #[test]
    fn hover_fill_settles_at_either_end() {
        let mut amount = 0.0;
        for _ in 0..40 {
            let (next, _) = approach_hover(amount, true, 0.016);
            amount = next;
        }
        assert_eq!(amount, 1.0);
        let (held, settled) = approach_hover(amount, true, 0.016);
        assert!(settled);
        assert_eq!(held, 1.0);
        for _ in 0..80 {
            let (next, _) = approach_hover(amount, false, 0.016);
            amount = next;
        }
        assert_eq!(amount, 0.0);
    }

    /// A header title shows only when it fits before its buttons, whatever
    /// the language makes of it.
    #[test]
    fn header_titles_make_way_for_their_buttons() {
        let spacing = 8.0;
        // Two 30-point buttons take 30 + 8 + 30 = 68 points, and the title
        // keeps 8 from them.
        let two = row_width(2, 30.0, spacing);
        assert_eq!(two, 68.0);
        assert!(title_fits(100.0, 176.0, two, spacing));
        assert!(!title_fits(100.0, 175.0, two, spacing));
        // A third button, the update arrow, needs 38 more.
        let three = row_width(3, 30.0, spacing);
        assert!(!title_fits(100.0, 176.0, three, spacing));
        assert!(title_fits(100.0, 214.0, three, spacing));
        assert_eq!(row_width(0, 30.0, spacing), 0.0);
    }

    /// The list header offers New chat (New call on the Calls tab) and no
    /// button to hide the list: dragging its border past its narrowest width
    /// does that. Calls and Settings are in the tab bar, Starred in the
    /// title's menu.
    /// The list folds only once its border is dragged well past its
    /// narrowest width, and comes back only once the rail's edge is pulled
    /// as far out.
    #[test]
    fn the_border_folds_and_unfolds_the_list_past_a_pull() {
        assert!(!folds_at(0.0, LIST_MIN_WIDTH));
        assert!(!folds_at(0.0, LIST_MIN_WIDTH - FOLD_PULL + 1.0));
        assert!(folds_at(0.0, LIST_MIN_WIDTH - FOLD_PULL - 1.0));
        assert!(folds_at(20.0, 100.0));
        assert!(!unfolds_at(52.0, 52.0 + FOLD_PULL - 1.0));
        assert!(unfolds_at(52.0, 52.0 + FOLD_PULL + 1.0));
    }

    #[test]
    fn header_buttons_have_no_hide_list() {
        use HeaderButton::{NewCall, NewChat};
        assert_eq!(header_buttons(false), [NewChat]);
        assert_eq!(header_buttons(true), [NewCall]);
    }

    /// The Calls tab's header carries New call, as WhatsApp's Calls pane
    /// does; the new-call list's header and the starred messages' have none.
    #[test]
    fn the_calls_tab_header_offers_a_new_call() {
        use HeaderButton::NewCall;
        let root = tempfile::tempdir().unwrap();
        let (mut app, _) = App::headless(
            crate::paths::AppDirs::under(root.path()),
            crate::settings::Settings::default(),
        );
        app.page = Page::Calls;
        assert_eq!(list_header_buttons(&app), [NewCall]);
        app.new_call = true;
        assert!(list_header_buttons(&app).is_empty());
        app.page = Page::Chats;
        app.show_starred = true;
        assert!(list_header_buttons(&app).is_empty());
    }

    /// With the rail gone from beside the list, a waiting release is the
    /// leftmost of the Chats tab's header buttons, in every list the column
    /// shows, and never on the Calls tab's.
    #[test]
    fn a_waiting_release_joins_the_chats_header_buttons() {
        use HeaderButton::{NewCall, NewChat, Update};
        let root = tempfile::tempdir().unwrap();
        let (mut app, _) = App::headless(
            crate::paths::AppDirs::under(root.path()),
            crate::settings::Settings::default(),
        );
        assert!(!list_header_buttons(&app).contains(&Update));
        app.update = Some(crate::updates::Release {
            version: "99.0.0".into(),
            url: "https://example.invalid/release".into(),
        });
        let buttons = list_header_buttons(&app);
        assert_eq!(buttons.first(), Some(&NewChat), "New chat is rightmost");
        assert_eq!(buttons.last(), Some(&Update), "the arrow is leftmost");
        app.show_starred = true;
        assert_eq!(list_header_buttons(&app), [Update]);
        app.show_starred = false;
        app.page = Page::Calls;
        assert_eq!(list_header_buttons(&app), [NewCall]);
    }

    /// The title's menu lists Chats, Starred and Archived, and Locked chats
    /// only while chats are locked; the title names the list on screen, and
    /// picking the archive from the locked folder leaves the folder.
    #[test]
    fn the_title_menu_offers_locked_chats_only_while_there_are_any() {
        use Destination::{Archived, Chats, Locked, Starred};
        let (_directory, mut app, _ids, _ctx) = rail_app(2);
        assert_eq!(title_menu_entries(&app), [Chats, Starred, Archived]);
        assert_eq!(shown_list(&app), Chats);
        app.chats[0].locked = true;
        assert!(Locked.offered(&app));
        assert_eq!(title_menu_entries(&app), [Chats, Starred, Archived, Locked]);
        app.chats[0].locked = false;
        assert_eq!(title_menu_entries(&app), [Chats, Starred, Archived]);

        app.show_archived = true;
        assert_eq!(shown_list(&app), Archived);
        app.show_starred = true;
        assert_eq!(shown_list(&app), Starred, "starred covers it");
        app.show_starred = false;
        app.show_archived = false;
        app.locked_folder = true;
        assert_eq!(shown_list(&app), Locked);
        assert!(
            title_menu_entries(&app).contains(&Locked),
            "the list on screen is always offered"
        );
        let archived = title_menu_actions(&app, Archived);
        assert!(archived.contains(&Action::CloseLockedFolder));
        assert!(archived.contains(&Action::ShowArchived(true)));
        let starred = title_menu_actions(&app, Starred);
        assert!(starred.contains(&Action::CloseLockedFolder));
        assert_eq!(
            title_menu_actions(&app, Chats)
                .iter()
                .filter(|action| **action == Action::CloseLockedFolder)
                .count(),
            1,
            "Chats leaves the folder once"
        );
    }

    /// The header spans the rows' hover and selection cards: the search
    /// field and the buttons on the right end where a card does, the field
    /// starts where it does, the tab bar keeps within them, centred on them,
    /// and the title starts on the avatars' line (or right of the macOS
    /// traffic lights).
    #[test]
    fn the_header_controls_end_where_the_row_cards_do() {
        for macos in [false, true] {
            let (_directory, mut app, ids, ctx) = rail_app(3);
            if macos {
                crate::theme::preview_macos(&ctx);
            }
            app.open_chat = Some(ids[0].clone());
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(1180.0, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| show(&mut app, ui),
                );
                output.textures_delta.clear();
            }
            let row = ctx
                .data(|data| data.get_temp::<Rect>(chat_row_id(&ids[0])))
                .expect("the first row was laid out");
            let card = row.shrink2(vec2(widgets::ROW_CARD_X, 0.0));
            let header = ctx
                .data(|data| data.get_temp::<Rect>(header_row_id()))
                .expect("the header row was laid out");
            let field = ctx
                .read_response(egui::Id::new("chat-search").with("pill"))
                .expect("the search field was drawn")
                .rect;
            let new_chat = super::super::focus::control(&ctx, Stop::NewChat)
                .and_then(|id| ctx.read_response(id))
                .expect("New chat was drawn")
                .rect;
            for (what, right) in [
                ("header row", header.right()),
                ("search field", field.right()),
                ("New chat", new_chat.right()),
            ] {
                assert!(
                    (right - card.right()).abs() < 0.5,
                    "macOS={macos}: the {what} ends at {right}, the card at {}",
                    card.right()
                );
            }
            assert!(
                (field.left() - card.left()).abs() < 0.5,
                "macOS={macos}: the search field starts at {}, the card at {}",
                field.left(),
                card.left()
            );
            let pill = ctx
                .data(|data| data.get_temp::<Rect>(super::super::tabbar::rect_id()))
                .expect("the tab bar was drawn");
            assert!(
                card.expand(0.5).x_range().contains(pill.left())
                    && card.expand(0.5).x_range().contains(pill.right())
                    && (pill.center().x - card.center().x).abs() < 0.5,
                "macOS={macos}: the tab bar {pill:?} is not centred within the card {card:?}"
            );
            let title = ctx
                .read_response(title_menu_id())
                .expect("the title is the menu's button")
                .rect;
            let text = title.left() + TITLE_INSET;
            let avatars = row.left() + ROW_LEAD;
            assert!(
                text >= avatars - 0.5,
                "macOS={macos}: {text} before {avatars}"
            );
        }
    }

    use crate::paths::AppDirs;
    use crate::settings::Settings;

    #[test]
    fn chat_context_menu_stays_compact_in_wide_windows() {
        for width in [360.0, 1180.0, 2000.0] {
            let directory = tempfile::tempdir().unwrap();
            let (mut app, _events) =
                App::headless(AppDirs::under(directory.path()), Settings::default());
            let chat = Chat::new("fixture@g.us".into(), "Fixture group".into());
            let ctx = egui::Context::default();
            app.attach(&ctx);
            let mut frame = |events| {
                let mut response = None;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(width, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| response = Some(row(&mut app, ui, &chat)),
                );
                output.textures_delta.clear();
                response.unwrap()
            };
            let response = frame(vec![]);
            let position = response.rect.left_center() + vec2(20.0, 0.0);
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            for _ in 0..3 {
                frame(vec![]);
            }
            let popup = response.id.with("popup");
            assert!(egui::Popup::is_id_open(&ctx, popup));
            let rect = ctx.read_response(popup).expect("chat context menu").rect;
            assert!(
                (190.0..=240.0).contains(&rect.width()),
                "a {width}-point window produced a {}-point menu",
                rect.width()
            );
        }
    }

    #[test]
    fn alt_navigation_scrolls_the_destination_chat_into_view() {
        let root = std::env::temp_dir().join(format!(
            "zapfast-chat-list-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
        let mut first = String::new();
        let mut last = String::new();
        for index in 0..24 {
            let id = format!("49170000{index:04}@s.whatsapp.net");
            let mut chat = Chat::new(id.clone(), format!("Chat {index:02}"));
            chat.last_activity = 100 - i64::from(index);
            if index == 0 {
                first.clone_from(&id);
            }
            last.clone_from(&id);
            app.chats.push(chat);
        }
        app.open_chat = Some(first);

        let ctx = egui::Context::default();
        app.attach(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 240.0))),
            events: vec![egui::Event::Key {
                key: egui::Key::ArrowUp,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::ALT,
            }],
            ..Default::default()
        };
        let mut offset = 0.0;
        let mut output = ctx.run_ui(input, |ui| {
            super::super::keys::handle(&mut app, ui.ctx());
            let scroll_id = ui.make_persistent_id(egui::IdSalt::new("chat-list"));
            list(&mut app, ui);
            offset = egui::scroll_area::State::load(ui.ctx(), scroll_id)
                .expect("chat-list scroll state")
                .offset
                .y;
        });
        output.textures_delta.clear();

        assert!(
            app.actions.contains(&Action::OpenChat(last)),
            "Alt+Up wraps to the last visible chat"
        );
        assert!(app.scroll_chat_into_view.is_none(), "reveal was consumed");
        assert!(offset > 0.0, "the list moved down to reveal the last row");
    }

    /// Moves a drawn list (`id` names where its scroll area's id was kept)
    /// to `offset`, as a reader's scrolling would.
    fn scroll_list_to(ctx: &egui::Context, id: egui::Id, offset: f32) {
        let id = ctx
            .data(|data| data.get_temp::<egui::Id>(id))
            .expect("the list was drawn");
        let mut state = egui::scroll_area::State::load(ctx, id).expect("scroll state");
        state.offset.y = offset;
        state.store(ctx, id);
    }

    /// A send follows its chat to the top only from a list that is within a
    /// row of the top; further down, the reader keeps their place.
    #[test]
    fn a_sent_message_scrolls_the_chat_list_to_the_top_only_from_near_it() {
        let (_directory, mut app, ids, ctx) = rail_app(24);
        let frame = |app: &mut App| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 240.0))),
                    ..Default::default()
                },
                |ui| list(app, ui),
            );
            output.textures_delta.clear();
            ctx.data(|data| data.get_temp::<f32>(list_offset_id()))
                .expect("chat-list offset")
        };
        app.scroll_chat_into_view = ids.last().cloned();
        let scrolled = frame(&mut app);
        assert!(scrolled > theme::ROW_HEIGHT, "the list starts far down");
        app.scroll_chats_to_top = true;
        assert_eq!(
            frame(&mut app),
            scrolled,
            "a reader far down keeps their place"
        );
        assert!(!app.scroll_chats_to_top, "the request was consumed");

        // Just over a row down still counts as working down the list.
        scroll_list_to(&ctx, list_offset_id().with("id"), theme::ROW_HEIGHT + 1.0);
        assert_eq!(frame(&mut app), theme::ROW_HEIGHT + 1.0);
        app.scroll_chats_to_top = true;
        assert_eq!(frame(&mut app), theme::ROW_HEIGHT + 1.0);

        scroll_list_to(&ctx, list_offset_id().with("id"), theme::ROW_HEIGHT / 2.0);
        assert_eq!(frame(&mut app), theme::ROW_HEIGHT / 2.0);
        app.scroll_chats_to_top = true;
        assert_eq!(frame(&mut app), 0.0, "the list is back at the top");
        assert_eq!(frame(&mut app), 0.0, "the list stays at the top");
    }

    #[test]
    fn a_send_while_search_results_show_does_not_move_the_list_later() {
        let (_directory, mut app, ids, ctx) = rail_app(24);
        let frame = |app: &mut App| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 240.0))),
                    ..Default::default()
                },
                |ui| list(app, ui),
            );
            output.textures_delta.clear();
            ctx.data(|data| data.get_temp::<f32>(list_offset_id()))
                .expect("chat-list offset")
        };
        app.scroll_chat_into_view = ids.last().cloned();
        let scrolled = frame(&mut app);
        assert!(scrolled > 0.0, "the list starts scrolled down");

        app.search = "Chat".into();
        app.scroll_chats_to_top = true;
        frame(&mut app);
        assert!(!app.scroll_chats_to_top, "the request was dropped");
        app.search.clear();
        assert_eq!(frame(&mut app), scrolled, "the list keeps its place");
    }

    #[test]
    fn a_search_finds_only_names_that_contain_the_text() {
        let (_directory, mut app, _ids, _ctx) = rail_app(0);
        for (id, name, activity) in [
            ("1@s.whatsapp.net", "John Smith", 1),
            ("2@s.whatsapp.net", "Alice", 2),
            ("3@s.whatsapp.net", "Jhn Doe", 0),
        ] {
            let mut chat = Chat::new(id.into(), name.into());
            chat.last_activity = activity;
            app.chats.push(chat);
        }
        // No fuzzy matching: "jhn" is not found inside "John Smith".
        app.search = "jhn".into();
        let titles: Vec<String> = app
            .visible_chats()
            .into_iter()
            .map(|chat| chat.name.clone())
            .collect();
        assert_eq!(titles, ["Jhn Doe"]);
    }

    #[test]
    fn a_frosted_header_leaves_the_first_row_below_it_and_rows_scroll_under_it() {
        let (_directory, mut app, ids, ctx) = rail_app(24);
        app.palette.bubbles = crate::theme::BubbleStyle::Nothing;
        let frame = |app: &mut App| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 400.0))),
                    ..Default::default()
                },
                |ui| column(app, ui),
            );
            output.textures_delta.clear();
        };
        frame(&mut app);
        let header = ctx
            .data(|data| data.get_temp::<Rect>(header_row_id()))
            .expect("the header row was laid out");
        let first = ctx
            .data(|data| data.get_temp::<Rect>(chat_row_id(&ids[0])))
            .expect("the first row was laid out");
        // The search field and the chips lie between the title row and the
        // list: the rows start under all of them, not under the title.
        assert!(
            (header.bottom() + 40.0..header.bottom() + 160.0).contains(&first.top()),
            "first row at {}, header row ends at {}",
            first.top(),
            header.bottom()
        );

        // Scrolled down, rows keep being drawn up under the header: a row
        // above where the first one rests can only have been laid out now.
        app.scroll_chat_into_view = ids.last().cloned();
        frame(&mut app);
        frame(&mut app);
        let under = ids.iter().any(|id| {
            ctx.data(|data| data.get_temp::<Rect>(chat_row_id(id)))
                .is_some_and(|row| row.top() < first.top() - 1.0 && row.bottom() > header.top())
        });
        assert!(under, "no row reached up under the header");
    }

    #[test]
    fn numbered_shortcuts_reveal_locked_chats_and_consume_the_scroll_request() {
        let (_directory, mut app, ids, ctx) = rail_app(24);
        for chat in &mut app.chats {
            chat.locked = true;
        }
        app.page = Page::Chats;
        app.settings.set_chat_lock_code(Some("fixture-code"));
        app.actions
            .push(Action::UnlockLockedFolder("fixture-code".into()));
        app.background_frame(&ctx);
        assert!(app.locked_folder_open());

        let mut frame = |key: Option<Key>| {
            let mut offset = 0.0;
            let mut height = 0.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 240.0))),
                    events: key
                        .into_iter()
                        .map(|key| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::COMMAND,
                        })
                        .collect(),
                    ..Default::default()
                },
                |ui| {
                    super::super::keys::handle(&mut app, ui.ctx());
                    let scroll_id = ui.make_persistent_id(egui::IdSalt::new("locked-chats"));
                    height = ui.available_height();
                    list(&mut app, ui);
                    offset = egui::scroll_area::State::load(ui.ctx(), scroll_id)
                        .expect("locked-chats scroll state")
                        .offset
                        .y;
                },
            );
            output.textures_delta.clear();
            (offset, height)
        };
        assert_eq!(frame(None).0, 0.0);
        let (offset, height) = frame(Some(Key::Num9));
        assert!(offset > 0.0, "the ninth locked chat starts off screen");
        assert!(8.0 * theme::ROW_HEIGHT >= offset);
        assert!(9.0 * theme::ROW_HEIGHT <= offset + height + 0.5);
        let (offset, _) = frame(Some(Key::Num1));
        assert_eq!(offset, 0.0, "navigating back reveals the first row");
        assert!(app.actions.contains(&Action::OpenChat(ids[8].clone())));
        assert!(app.actions.contains(&Action::OpenChat(ids[0].clone())));
        assert!(app.scroll_chat_into_view.is_none(), "reveal was consumed");
    }

    #[test]
    fn the_collapsed_list_is_narrow_and_opens_a_chat_on_click() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _events) =
            App::headless(AppDirs::under(directory.path()), Settings::default());
        let mut first = Chat::new("491700000001@s.whatsapp.net".into(), "Alice".into());
        first.unread = 3;
        first.last_activity = 20;
        let second = Chat::new("491700000002@s.whatsapp.net".into(), "Bob".into());
        app.chats.push(first.clone());
        app.chats.push(second.clone());
        app.open_chat = Some(second.id.clone());
        let ctx = egui::Context::default();
        app.attach(&ctx);
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 400.0))),
                    events,
                    ..Default::default()
                },
                |ui| super::super::rail::show(&mut app, ui, true),
            );
            output.textures_delta.clear();
        };
        frame(vec![]);
        let rect = ctx
            .data(|data| data.get_temp::<Rect>(compact_chat_id(&first.id)))
            .expect("the rail draws every visible chat");
        assert!(
            ctx.data(|data| data.get_temp::<Rect>(compact_chat_id(&second.id)))
                .is_some(),
            "the rail draws the second chat too"
        );
        assert!(
            (rect.width() - compact_width(&ctx)).abs() <= 1.5,
            "an avatar cell is one panel wide, not {} points",
            rect.width()
        );
        let position = rect.center();
        for pressed in [true, false] {
            frame(vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert!(
            app.actions.contains(&Action::OpenChat(first.id.clone())),
            "clicking an avatar opens that chat: {:?}",
            app.actions
        );
    }

    /// Both chip rows overflow in a narrow window (#444). They show no
    /// scroll bar, at rest or hovered, and a plain vertical wheel scrolls them.
    #[test]
    fn narrow_chip_rows_hide_their_scroll_bar_and_follow_the_wheel() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _events) =
            App::headless(AppDirs::under(directory.path()), Settings::default());
        app.labels = (0..8)
            .map(|index| crate::model::Label {
                id: index.to_string(),
                name: format!("Label {index}"),
                color_hex: "#25d366".into(),
                created_at: 0,
            })
            .collect();
        let ctx = egui::Context::default();
        app.attach(&ctx);
        // Narrow enough that both rows scroll.
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(220.0, 400.0))),
                    events,
                    ..Default::default()
                },
                |ui| filter_chips(&mut app, ui),
            );
            output.textures_delta.clear();
            output.shapes
        };
        frame(vec![]);
        let all = |ctx: &egui::Context| {
            ctx.data(|data| data.get_temp::<Rect>(filter_chip_id(ChatFilter::All)))
                .expect("the filter chips are drawn")
        };
        let first_label = ctx
            .data(|data| data.get_temp::<Rect>(super::labels::chip_id("0")))
            .expect("the first label chip is drawn");

        /// Collects every visible thin rectangle in `shape` into `bars`. A
        /// scroll bar is a thin rectangle; chips are taller.
        fn thin_rects(shape: &egui::Shape, bars: &mut Vec<Rect>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    shapes.iter().for_each(|shape| thin_rects(shape, bars));
                }
                egui::Shape::Rect(rect)
                    if rect.rect.height() < 12.0
                        && rect.rect.width() > 12.0
                        && rect.fill.a() > 0 =>
                {
                    bars.push(rect.rect);
                }
                _ => {}
            }
        }
        let bars = |shapes: &[egui::epaint::ClippedShape]| {
            let mut bars = Vec::new();
            for clipped in shapes {
                thin_rects(&clipped.shape, &mut bars);
            }
            bars
        };

        // At rest, away from the rows, neither shows a bar.
        let mut shapes = Vec::new();
        for _ in 0..5 {
            shapes = frame(vec![egui::Event::PointerMoved(egui::pos2(110.0, 380.0))]);
        }
        let resting = bars(&shapes);
        assert!(
            resting.is_empty(),
            "the chip rows show bars at rest: {resting:?}"
        );

        // Hovering a row shows no bar either: the fade at the cut edge says
        // the row scrolls on.
        for pointer in [all(&ctx).center(), first_label.center()] {
            let mut shapes = Vec::new();
            for _ in 0..5 {
                shapes = frame(vec![egui::Event::PointerMoved(pointer)]);
            }
            let shown = bars(&shapes);
            assert!(shown.is_empty(), "a hovered chip row shows bars: {shown:?}");
        }

        let before = all(&ctx).left();
        // The middle of the row.
        let pointer = egui::pos2(110.0, all(&ctx).center().y);
        frame(vec![
            egui::Event::PointerMoved(pointer),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -60.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            },
        ]);
        frame(vec![egui::Event::PointerMoved(pointer)]);
        assert!(
            all(&ctx).left() < before - 1.0,
            "a vertical wheel left the filter chips at {before}, now {}",
            all(&ctx).left()
        );
    }

    /// An app with `count` chats, newest first, and a context to draw it in.
    fn rail_app(count: usize) -> (tempfile::TempDir, App, Vec<String>, egui::Context) {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, _events) =
            App::headless(AppDirs::under(directory.path()), Settings::default());
        let mut ids = Vec::new();
        for index in 0..count {
            let id = format!("49170000{index:04}@s.whatsapp.net");
            let mut chat = Chat::new(id.clone(), format!("Chat {index:03}"));
            chat.last_activity = 10_000 - index as i64;
            app.chats.push(chat);
            ids.push(id);
        }
        let ctx = egui::Context::default();
        app.attach(&ctx);
        (directory, app, ids, ctx)
    }

    fn rail_frame(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 400.0))),
                events,
                ..Default::default()
            },
            |ui| {
                super::super::keys::handle(app, ui.ctx());
                super::super::rail::show(app, ui, true);
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn the_collapsed_list_lays_out_only_the_avatars_on_screen() {
        let (_directory, mut app, ids, ctx) = rail_app(2_000);
        rail_frame(&mut app, &ctx, vec![]);
        let drawn = ids
            .iter()
            .filter(|id| ctx.data(|data| data.get_temp::<Rect>(compact_chat_id(id)).is_some()))
            .count();
        assert!(drawn > 0, "the first avatars are drawn");
        assert!(
            drawn < 20,
            "a 400-point window has room for a few avatars, yet {drawn} were laid out"
        );
    }

    /// A backlog reply keeps the collapsed sidebar scrolled down even when
    /// normal activity ordering moves the recipient to the top.
    #[test]
    fn sending_far_down_leaves_the_collapsed_list_scrolled_down() {
        let (_directory, mut app, ids, ctx) = rail_app(40);
        let (backend, mut commands) = crate::backend::Backend::recording();
        app.backend = backend;
        app.open_chat = Some(ids[30].clone());
        app.scroll_chat_into_view = Some(ids[30].clone());
        rail_frame(&mut app, &ctx, vec![]);
        rail_frame(&mut app, &ctx, vec![]);
        let offset = || {
            ctx.data(|data| data.get_temp::<f32>(list_offset_id().with("compact")))
                .expect("the collapsed list was drawn")
        };
        let before = offset();
        assert!(before > COMPACT_CELL, "the rail starts far down");
        app.actions.push(Action::SendText {
            chat: ids[30].clone(),
            text: "Backlog reply fixture".into(),
            quoting: None,
        });
        app.background_frame(&ctx);
        assert!(
            std::iter::from_fn(|| commands.try_recv().ok()).any(|command| matches!(
                command,
                crate::backend::Command::SendText { chat, .. } if chat == ids[30]
            ))
        );
        // Normal activity ordering still moves the replied-to chat up.
        app.chats
            .iter_mut()
            .find(|chat| chat.id == ids[30])
            .unwrap()
            .last_activity = 20_000;
        rail_frame(&mut app, &ctx, vec![]);
        rail_frame(&mut app, &ctx, vec![]);
        assert_eq!(offset(), before, "sending keeps the sidebar offset");
        assert_eq!(app.visible_chats()[0].id, ids[30]);
    }

    #[test]
    fn a_sent_message_scrolls_the_collapsed_list_to_the_top_only_from_near_it() {
        let (_directory, mut app, ids, ctx) = rail_app(40);
        let offset = || {
            ctx.data(|data| data.get_temp::<f32>(list_offset_id().with("compact")))
                .expect("the collapsed list was drawn")
        };
        app.scroll_chat_into_view = Some(ids[39].clone());
        rail_frame(&mut app, &ctx, vec![]);
        rail_frame(&mut app, &ctx, vec![]);
        let scrolled = offset();
        assert!(
            scrolled > COMPACT_CELL,
            "the rail starts scrolled to the end"
        );
        app.scroll_chats_to_top = true;
        rail_frame(&mut app, &ctx, vec![]);
        assert!(!app.scroll_chats_to_top, "the request was consumed");
        assert_eq!(offset(), scrolled, "a reader far down keeps their place");

        scroll_list_to(
            &ctx,
            list_offset_id().with("compact-id"),
            COMPACT_CELL / 2.0,
        );
        rail_frame(&mut app, &ctx, vec![]);
        assert_eq!(offset(), COMPACT_CELL / 2.0);
        app.scroll_chats_to_top = true;
        rail_frame(&mut app, &ctx, vec![]);
        assert_eq!(offset(), 0.0, "the rail is back at the top");
    }

    #[test]
    fn alt_navigation_scrolls_the_collapsed_list_too() {
        let (_directory, mut app, ids, ctx) = rail_app(40);
        app.open_chat = Some(ids[0].clone());
        rail_frame(&mut app, &ctx, vec![]);
        assert!(
            ctx.data(|data| data.get_temp::<Rect>(compact_chat_id(&ids[39])))
                .is_none(),
            "the last avatar starts off screen"
        );
        let alt_up = egui::Event::Key {
            key: egui::Key::ArrowUp,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };
        rail_frame(&mut app, &ctx, vec![alt_up]);
        assert!(app.actions.contains(&Action::OpenChat(ids[39].clone())));
        assert!(app.scroll_chat_into_view.is_none(), "reveal was consumed");
        rail_frame(&mut app, &ctx, vec![]);
        let rect = ctx
            .data(|data| data.get_temp::<Rect>(compact_chat_id(&ids[39])))
            .expect("the destination avatar is laid out");
        assert!(
            rect.bottom() <= 400.0 + 0.5,
            "the destination avatar is on screen: {rect:?}"
        );
    }

    #[test]
    fn the_secret_code_shows_only_the_way_into_the_locked_folder() {
        let (_directory, mut app, ids, ctx) = rail_app(3);
        app.chats[0].locked = true;
        // A code that also matches an ordinary chat's number.
        app.settings.set_chat_lock_code(Some("0001"));
        app.search = "0001".into();
        rail_frame(&mut app, &ctx, vec![]);
        for id in &ids {
            assert!(
                ctx.data(|data| data.get_temp::<Rect>(compact_chat_id(id)))
                    .is_none(),
                "no chat shows while the code is typed"
            );
        }
        let entry = ctx
            .data(|data| data.get_temp::<Rect>(compact_locked_id()))
            .expect("the locked entry is drawn");
        let position = entry.center();
        for pressed in [true, false] {
            rail_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(app.actions.contains(&Action::OpenLockedFolder));
    }

    #[test]
    fn collapsing_and_expanding_keeps_the_saved_list_width() {
        let (_directory, mut app, _ids, ctx) = rail_app(3);
        app.settings.sidebar_width = 400.0;
        let frame = |app: &mut App, collapsed: bool| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 600.0))),
                    ..Default::default()
                },
                |ui| {
                    if collapsed {
                        super::super::rail::show(app, ui, true);
                    } else {
                        show(app, ui);
                    }
                },
            );
            output.textures_delta.clear();
        };
        frame(&mut app, false);
        frame(&mut app, true);
        frame(&mut app, false);
        assert!(
            (app.settings.sidebar_width - 400.0).abs() < 1.0,
            "the full list came back {} points wide",
            app.settings.sidebar_width
        );
        assert!(!app.actions.contains(&Action::SettingsChanged));
    }

    #[test]
    fn the_widest_badge_stays_inside_the_collapsed_list() {
        let (_directory, _app, _ids, ctx) = rail_app(0);
        let rail = Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(super::super::rail::WIDTH, COMPACT_CELL),
        );
        let avatar = Rect::from_center_size(rail.center(), Vec2::splat(COMPACT_AVATAR));
        let mut width = 0.0;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let palette = Palette::dark();
            width = widgets::badge(ui, &palette, compact_badge_center(avatar), 12_345, true);
        });
        output.textures_delta.clear();
        let center = compact_badge_center(avatar);
        assert!(width > 20.0, "a long count widens the badge");
        assert!(center.x + 10.0 <= rail.right(), "{width} too wide");
        assert!(center.y - 10.0 >= rail.top(), "the badge fits its cell");
    }
}
