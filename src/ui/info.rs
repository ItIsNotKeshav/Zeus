//! The info panel beside the open chat: a contact's, group's or channel's
//! picture, name and actions, its members, and the pictures, videos,
//! documents and links shared in it. It opens as a rounded card floating at
//! the right of the chat, like the chat's own card, easing in as it appears,
//! and the window grows to make room; in the narrow window mode it is a page
//! of its own over the chat, with Back.

use egui::{Align, Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Vec2};
use egui::{pos2, vec2};

use super::widgets::FrostedPopup;
use crate::app::{App, InfoFocus};
use crate::model::{Action, ChatMedia, Content, Dialog, InfoView, MediaTab, Message};
use crate::theme::{self, Icon};

use super::focus::{Stop, TabStop};
use super::pane;
use super::widgets;

/// The panel's side margin.
const MARGIN: f32 = 18.0;
/// Space between the cells of the picture grid and strip.
const GAP: f32 = 4.0;
/// A document or link row.
const ROW_HEIGHT: f32 = 76.0;
/// A member's row.
const MEMBER_HEIGHT: f32 = 48.0;
/// The card's width, the column it takes with its gap, and how far it eases
/// in from the right while it appears.
const CARD_WIDTH: f32 = 440.0;
const CARD_COLUMN: f32 = CARD_WIDTH + super::chats::CARD_GAP;
const POPUP_RISE: f32 = 36.0;
/// Seconds the popup takes to appear, and to leave: a little quicker.
const POPUP_SECONDS: f64 = 0.24;
const POPUP_LEAVE_SECONDS: f64 = 0.16;
/// The members shown without a search field above them.
const SEARCH_FROM: usize = 8;
/// The members listed before "Show all".
const MEMBERS_SHOWN: usize = 10;
/// The panel's Tab stops after its close button, in reading order: the
/// group photo, the rename pencil, the name editor's check, the action
/// buttons, the media row, the media view's tabs, Leave, and Try again.
const PHOTO_STOP: u8 = 0;
const PENCIL_STOP: u8 = 1;
const NAME_CHECK_STOP: u8 = 2;
const ACTION_STOP: u8 = 3;
pub const MEDIA_ROW_STOP: u8 = 10;
const TAB_STOP: u8 = 20;
const LEAVE_STOP: u8 = 30;
const RETRY_STOP: u8 = 31;

/// The media view's title, and the overview row that opens it.
fn media_title(app: &App) -> String {
    crate::i18n::gettext(app.locale, "Media, links and docs").into_owned()
}

/// The texture name a listed picture's preview is drawn under: the panel's
/// own, so freeing it when the panel closes leaves the bubbles' alone.
pub(crate) fn thumbnail_uri(chat: &str, id: &str) -> String {
    format!(
        "bytes://info-{}-{}",
        chat.chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>(),
        id
    )
}

/// Frees the pictures the panel no longer lists. Called on every frame,
/// the lock and login screens included, so a closed panel's pictures do
/// not wait for the chats to be drawn again.
pub fn release_thumbnails(app: &mut App, ctx: &egui::Context) {
    for uri in std::mem::take(&mut app.info_released) {
        app.info_textures.remove(&uri);
        crate::image_cache::forget(ctx, &uri);
    }
}

/// Draws the panel as a floating card at the right of the chat when the
/// window is wide, and returns the rect of the chat it covers as a page of
/// its own, for [`show_overlay`], when the window is narrow. Called before
/// the conversation is laid out, so the card takes its own column.
pub fn show(app: &mut App, ui: &mut egui::Ui) -> Option<Rect> {
    let ctx = ui.ctx().clone();
    if !app.info_visible() {
        ctx.data_mut(|data| {
            data.remove::<bool>(docked_id());
            data.remove::<f64>(popup_start_id());
            data.remove::<f64>(popup_leave_id());
            data.remove::<(String, String)>(member_filter_id());
            data.remove::<String>(members_open_id());
        });
        // The window goes back to the width it had before the card.
        widen_window(&ctx, false);
        // The thumbnails go with the panel, not with the resident window.
        for uri in std::mem::take(&mut app.info_textures) {
            crate::image_cache::forget(&ctx, &uri);
        }
        return None;
    }
    if super::narrow(&ctx) {
        return Some(ui.available_rect_before_wrap());
    }
    widen_window(&ctx, true);
    side_card(app, ui);
    None
}

/// The panel as a page over the whole conversation in a narrow window.
pub fn show_overlay(app: &mut App, ctx: &egui::Context, region: Rect) {
    let width = region.width();
    pane::overlay(app, ctx, "chat-info-overlay", width, region, |app, ui| {
        contents(app, ui, true, false)
    });
    ctx.data_mut(|data| data.insert_temp(docked_id(), false));
}

/// The width the window grew by for the card, while it has.
fn widened_id() -> egui::Id {
    egui::Id::new("chat-info-widened")
}

/// Grows the window by the card's column while the card is open, and shrinks
/// it back after, so the chat keeps its width. A maximized or full-screen
/// window cannot grow: the chat gives way instead.
fn widen_window(ctx: &egui::Context, open: bool) {
    let widened = ctx.data(|data| data.get_temp::<f32>(widened_id()));
    let size = ctx.input(|input| {
        let viewport = input.viewport();
        let fixed = viewport.maximized == Some(true) || viewport.fullscreen == Some(true);
        let inner = viewport
            .inner_rect
            .map_or_else(|| input.content_rect().size(), |rect| rect.size());
        (!fixed).then_some(inner)
    });
    match (open, widened, size) {
        (true, None, Some(size)) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                size + vec2(CARD_COLUMN, 0.0),
            ));
            ctx.data_mut(|data| data.insert_temp(widened_id(), CARD_COLUMN));
        }
        (false, Some(by), Some(size)) => {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size - vec2(by, 0.0)));
            ctx.data_mut(|data| data.remove::<f32>(widened_id()));
        }
        // The window was maximized meanwhile: nothing to give back.
        (false, Some(_), None) => ctx.data_mut(|data| data.remove::<f32>(widened_id())),
        _ => {}
    }
}

/// The card in its own column at the right of the chat, the chat card's
/// twin: the same rounding and gaps, easing in from the right.
fn side_card(app: &mut App, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let palette = app.palette;
    let now = ctx.input(|input| input.time);
    let start = ctx.data_mut(|data| *data.get_temp_mut_or_insert_with(popup_start_id(), || now));
    let leaving = ctx.data(|data| data.get_temp::<f64>(popup_leave_id()));
    let reduced = crate::motion::screen_reader_attached(&ctx);
    let progress = match leaving {
        Some(began) => {
            let left = ((now - began) / POPUP_LEAVE_SECONDS).clamp(0.0, 1.0) as f32;
            if left >= 1.0 || reduced {
                // Gone: the panel closes for good.
                ctx.data_mut(|data| data.remove::<f64>(popup_leave_id()));
                app.actions.push(Action::CloseInfo);
                return;
            }
            ctx.request_repaint();
            1.0 - egui::emath::easing::cubic_in(left)
        }
        None => {
            let arrived = ((now - start) / POPUP_SECONDS).clamp(0.0, 1.0) as f32;
            // A screen reader's window is not asked for a frame per refresh.
            if reduced {
                1.0
            } else {
                if arrived < 1.0 {
                    ctx.request_repaint();
                }
                egui::emath::easing::cubic_out(arrived)
            }
        }
    };
    let gap = super::chats::CARD_GAP;
    egui::Panel::right("chat-info-side")
        .resizable(false)
        .default_size(CARD_COLUMN)
        .size_range(CARD_COLUMN..=CARD_COLUMN)
        .show_separator_line(false)
        .frame(Frame::NONE)
        .show(ui, |ui| {
            let column = ui.max_rect();
            let rest = Rect::from_min_max(
                pos2(column.left(), column.top() + gap),
                pos2(column.right() - gap, column.bottom() - gap),
            );
            let card = rest.translate(vec2((1.0 - progress) * POPUP_RISE, 0.0));
            let fill = super::central_frame(app).fill;
            let radius = f32::from(super::chats::CARD_RADIUS);
            let painter = ui.painter().clone();
            let faded = |color: egui::Color32| color.gamma_multiply(progress);
            painter.rect(
                card,
                radius,
                faded(fill),
                Stroke::new(1.25, faded(palette.outline)),
                egui::StrokeKind::Inside,
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
                ui.set_clip_rect(card.intersect(ui.clip_rect()));
                ui.multiply_opacity(progress);
                // The lists end above the rounded corners.
                Frame::new()
                    .inner_margin(Margin {
                        bottom: 12,
                        ..Margin::ZERO
                    })
                    .show(ui, |ui| {
                        ui.set_min_size(card.size() - vec2(0.0, 12.0));
                        ui.set_max_size(card.size() - vec2(0.0, 12.0));
                        contents(app, ui, true, true);
                    });
            });
        });
    ctx.data_mut(|data| data.insert_temp(docked_id(), true));
}

/// Starts the popup's exit: it eases out, then closes the panel.
fn leave(ctx: &egui::Context) {
    let now = ctx.input(|input| input.time);
    ctx.data_mut(|data| {
        data.get_temp_mut_or_insert_with(popup_leave_id(), || now);
    });
    ctx.request_repaint();
}

/// When the popup began to leave, while it does.
fn popup_leave_id() -> egui::Id {
    egui::Id::new("chat-info-popup-leave")
}

/// When the popup opened, so it eases in once.
fn popup_start_id() -> egui::Id {
    egui::Id::new("chat-info-popup-start")
}

/// The chat whose whole member list is open.
fn members_open_id() -> egui::Id {
    egui::Id::new("chat-info-members-open")
}

/// What is typed in the member search, for the chat shown.
fn member_filter_id() -> egui::Id {
    egui::Id::new("chat-info-member-filter")
}

/// Whether the panel was a popup over the chat (`true`) or a page of its own
/// (`false`) this frame, for interaction tests.
pub fn docked_id() -> egui::Id {
    egui::Id::new("chat-info-docked")
}

/// The panel's contents. Over the conversation (`overlay`) an item that goes
/// to a message closes the panel, so the message can be seen. As a `popup`
/// the header ends in a close button, and the cards float on the glass.
fn contents(app: &mut App, ui: &mut egui::Ui, overlay: bool, popup: bool) {
    let (chat, view) = match &app.info {
        Some(info) => (info.chat.clone(), info.view),
        None => return,
    };
    match view {
        InfoView::Overview => overview(app, ui, &chat, overlay, popup),
        // A locked chat's media is never drawn: the overview shows instead.
        InfoView::Media(_) if !app.info_media_allowed() => {
            app.actions.push(Action::ShowInfo(InfoView::Overview));
            overview(app, ui, &chat, overlay, popup);
        }
        InfoView::Media(tab) => media_view(app, ui, &chat, tab, overlay, popup),
    }
}

/// The header: a button at the left, then the title. `focus` says whether
/// the keyboard moves to the button this frame. As a `popup` an overview has
/// no button at the left; a close button ends every header instead.
fn header(
    app: &mut App,
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    title: &str,
    action: Action,
    focus: bool,
    popup: bool,
) {
    let palette = app.palette;
    let margin = if popup {
        Margin {
            left: MARGIN as i8,
            right: MARGIN as i8,
            top: 20,
            bottom: 8,
        }
    } else {
        pane::header_margin(ui.ctx())
    };
    // The close button of a popup's overview is the one the keyboard reaches.
    let leading = !popup || icon != Icon::X;
    Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            if leading {
                let button =
                    theme::icon_button(ui, icon, 18.0, palette.secondary, palette.text, label)
                        .tab_stop(Stop::InfoClose);
                if focus {
                    button.request_focus();
                }
                if button.clicked() {
                    app.actions.push(action.clone());
                }
                ui.add_space(6.0);
            }
            theme::text(
                ui,
                title,
                theme::semibold(if popup { 17.0 } else { 16.0 }),
                palette.text,
            );
            if popup {
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let close = crate::i18n::gettext(app.locale, "Close");
                    let mut button = theme::icon_button(
                        ui,
                        Icon::X,
                        18.0,
                        palette.secondary,
                        palette.text,
                        &close,
                    );
                    if !leading {
                        button = button.tab_stop(Stop::InfoClose);
                        if focus {
                            button.request_focus();
                        }
                    }
                    if button.clicked() {
                        leave(ui.ctx());
                    }
                });
            }
        });
    });
    // A page keeps its rule under the header; a popup is spaced instead.
    if !popup {
        ui.painter().hline(
            ui.max_rect().x_range(),
            ui.cursor().top(),
            Stroke::new(1.0, palette.outline),
        );
    }
}

/// A section of the overview: a card in a popup, a plain block on a page.
fn section<R>(
    ui: &mut egui::Ui,
    palette: &theme::Palette,
    popup: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    if popup {
        card(ui, palette, add)
    } else {
        ui.add_space(8.0);
        add(ui)
    }
}

/// A rounded card on the glass, as wide as the panel, for a section.
fn card<R>(ui: &mut egui::Ui, palette: &theme::Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .fill(palette.surface)
        .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            add(ui)
        })
        .inner
}

/// What the overview shows of the chat, read once per frame from the chat
/// list without copying its member list.
struct Shown {
    id: String,
    /// The name the chat list shows.
    name: String,
    /// The group's subject as the phone named it, for the rename editor.
    subject: String,
    subject_known: bool,
    has_chat: bool,
    group: bool,
    channel: bool,
    phone: Option<String>,
    members: usize,
    can_leave: bool,
    can_edit_info: bool,
    muted_until: Option<i64>,
    muted: bool,
    pinned: bool,
    archived: bool,
    /// A group's description, when it has one.
    description: Option<String>,
    /// The disappearing-message timer line, when the timer is known.
    timer: Option<String>,
}

impl Shown {
    fn of(app: &App, id: &str) -> Self {
        // Group members may not have an existing chat.
        let fallback;
        let (chat, has_chat) = match app.chat(id) {
            Some(chat) => (chat, true),
            None => {
                fallback = crate::model::Chat::new(id.to_owned(), app.display_name(id));
                (&fallback, false)
            }
        };
        let ours = app.our_ids();
        Self {
            id: chat.id.clone(),
            name: app.chat_title(chat),
            subject: chat.name.clone(),
            subject_known: chat.group_subject_known,
            has_chat,
            group: chat.is_group(),
            channel: chat.is_channel(),
            phone: chat.phone().map(str::to_owned),
            members: chat.participants.len(),
            can_leave: chat.can_leave(&ours),
            can_edit_info: chat.can_edit_info(),
            muted_until: chat.muted_until,
            muted: chat.muted(crate::util::now()),
            pinned: chat.pinned,
            archived: chat.archived,
            description: chat
                .group_description
                .clone()
                .filter(|text| chat.is_group() && !text.trim().is_empty()),
            timer: crate::util::disappearing_line(app.locale, chat.ephemeral_expiration),
        }
    }
}

/// The chat's picture, name, details, actions and members.
fn overview(app: &mut App, ui: &mut egui::Ui, id: &str, overlay: bool, popup: bool) {
    let shown = Shown::of(app, id);
    let title = if shown.group {
        crate::i18n::gettext(app.locale, "Group info")
    } else if shown.channel {
        crate::i18n::gettext(app.locale, "Channel info")
    } else {
        crate::i18n::gettext(app.locale, "Contact info")
    };
    // As a page of its own, the panel goes back to the chat it covers.
    let (icon, close) = if overlay && !popup {
        (Icon::ChevronLeft, crate::i18n::gettext(app.locale, "Back"))
    } else {
        (Icon::X, crate::i18n::gettext(app.locale, "Close"))
    };
    header(
        app,
        ui,
        icon,
        &close,
        &title,
        Action::CloseInfo,
        false,
        popup,
    );
    egui::ScrollArea::vertical()
        .id_salt(("chat-info", id))
        .auto_shrink([false, false])
        .max_height(ui.available_height())
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(MARGIN as i8, if popup { 16 } else { 12 }))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = if popup { 18.0 } else { 8.0 };
                    identity(app, ui, &shown, overlay, popup);
                });
        });
}

/// The picture, name, details, actions and members, top to bottom.
fn identity(app: &mut App, ui: &mut egui::Ui, shown: &Shown, overlay: bool, popup: bool) {
    let palette = app.palette;
    let id = shown.id.as_str();
    let name = shown.name.as_str();
    // The photo takes the panel's width, up to a portrait size.
    let photo = if popup {
        96.0
    } else {
        (ui.available_width() * 0.55).clamp(120.0, 200.0)
    };
    let picture = app.avatar_full(id).or_else(|| app.avatar(id));
    let mine = app.me.as_deref() == Some(id);
    let editable = shown.phone.is_some() && !mine;
    // Saving or cancelling leaves the editor buffer checked out.
    let mut editing = app.contact_edit.take().filter(|_| editable);
    let mut saved = None;
    let mut leave = false;
    // A group's name and photo, when WhatsApp lets us change them. Nothing is
    // offered while a change is on its way.
    let saving = app.group_saving.contains(id);
    let group_editable = shown.can_edit_info && !saving;
    let mut renaming = app.group_name_edit.take().filter(|_| group_editable);
    let mut group_action = None;
    ui.vertical_centered(|ui| {
        if popup {
            ui.spacing_mut().item_spacing.y = 10.0;
        }
        if group_editable {
            group_action = group_photo(app, ui, name, id, photo, picture.as_deref());
        } else {
            widgets::avatar(ui, &palette, name, id, photo, picture.as_deref());
        }
        ui.add_space(6.0);
        if let Some(draft) = renaming.as_mut() {
            match group_name_field(app, ui, draft) {
                Some(true) => {
                    let typed = draft.trim();
                    // An empty or unchanged name closes the editor and sends nothing.
                    group_action = Some(if typed.is_empty() || typed == shown.subject {
                        Action::CloseGroupName
                    } else {
                        Action::SetGroupName {
                            chat: id.to_owned(),
                            name: typed.to_owned(),
                        }
                    });
                }
                Some(false) => group_action = Some(Action::CloseGroupName),
                None => {}
            }
        } else if group_editable {
            let edit = crate::i18n::gettext(app.locale, "Edit group name");
            // Center the name and its pencil together.
            let button = 15.0 + 12.0;
            let spacing = ui.spacing().item_spacing.x;
            let available = ui.available_width();
            let text_width = widgets::line(
                ui,
                name,
                theme::bold(19.0),
                palette.text,
                (available - button - spacing).max(40.0),
                1,
            )
            .size()
            .x;
            ui.horizontal(|ui| {
                ui.add_space(((available - text_width - spacing - button) / 2.0).max(0.0));
                ui.allocate_ui(vec2(text_width + 1.0, 30.0), |ui| {
                    widgets::selectable_rich_text(ui, name, theme::bold(19.0), palette.text);
                });
                let pencil = theme::icon_button(
                    ui,
                    Icon::Pencil,
                    15.0,
                    palette.secondary,
                    palette.text,
                    &edit,
                )
                .tab_stop(Stop::InfoControl(PENCIL_STOP));
                ui.ctx()
                    .data_mut(|data| data.insert_temp(group_name_button_id(), pencil.rect));
                if pencil.clicked() {
                    // The field appears next frame; egui keeps a focus
                    // request that long.
                    ui.memory_mut(|memory| memory.request_focus(group_name_field_id()));
                    // An unnamed group starts empty rather than from its
                    // members' summary, the title `chat_title` falls back to.
                    let unnamed = shown.subject.trim().is_empty()
                        || (shown.subject == "Group" && !shown.subject_known);
                    group_action = Some(Action::EditGroupName(if unnamed {
                        String::new()
                    } else {
                        shown.subject.clone()
                    }));
                }
            });
        } else if let Some((first, last)) = editing.as_mut() {
            let mut submit = false;
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 288.0).max(0.0) / 2.0);
                let name_field = |ui: &mut egui::Ui, buffer: &mut String, salt: &str, hint| {
                    let format = egui::TextFormat::simple(theme::semibold(15.0), palette.text);
                    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                        crate::bidi::layout_field(ui, text.as_str(), &format, wrap)
                    };
                    let align = if crate::bidi::base_rtl(buffer) {
                        Align::RIGHT
                    } else {
                        Align::LEFT
                    };
                    let field = Frame::new()
                        .fill(palette.surface)
                        .corner_radius(CornerRadius::same(theme::RADIUS))
                        .inner_margin(Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(buffer)
                                    .id(egui::Id::new(salt))
                                    .hint_text(
                                        egui::RichText::new(hint)
                                            .color(palette.dim)
                                            .font(theme::semibold(15.0)),
                                    )
                                    .font(theme::semibold(15.0))
                                    .text_color(palette.text)
                                    .frame(Frame::NONE)
                                    .desired_width(108.0)
                                    .horizontal_align(align)
                                    .layouter(&mut layouter),
                            )
                        });
                    theme::focus_outline(
                        ui,
                        field.inner.id,
                        field.response.rect,
                        f32::from(theme::RADIUS),
                    );
                    field.inner
                };
                let first_field = name_field(ui, first, "contact-first", "First name");
                let last_field = name_field(ui, last, "contact-last", "Surname");
                if ui.memory(|memory| memory.focused().is_none()) {
                    first_field.request_focus();
                }
                submit = (first_field.lost_focus() || last_field.lost_focus())
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if theme::icon_button(
                    ui,
                    Icon::Check,
                    18.0,
                    palette.secondary,
                    palette.accent,
                    "Save name (Enter)",
                )
                .tab_stop(Stop::InfoControl(NAME_CHECK_STOP))
                .clicked()
                {
                    submit = true;
                }
            });
            if submit && !(first.trim().is_empty() && last.trim().is_empty()) {
                saved = Some((first.trim().to_owned(), last.trim().to_owned()));
            }
        } else {
            widgets::selectable_rich_text(ui, name, theme::bold(19.0), palette.text);
        }
        if let Some(phone) = &shown.phone {
            theme::selectable_text(
                ui,
                crate::util::phone(phone),
                theme::regular(13.5),
                palette.secondary,
            );
        }
        if saving {
            theme::text(
                ui,
                crate::i18n::gettext(app.locale, "Saving…"),
                theme::regular(12.5),
                palette.dim,
            );
        }
        if shown.group && shown.members > 0 {
            theme::text(
                ui,
                crate::i18n::ngettext(app.locale, "{} member", "{} members", shown.members as u32)
                    .replace("{}", &shown.members.to_string()),
                theme::regular(13.5),
                palette.secondary,
            );
        }
        if let Some(presence) = app.presence.get(id) {
            let status = if presence.online {
                "online".to_owned()
            } else if let Some(seen) = presence.last_seen {
                crate::util::last_seen(app.locale, seen)
            } else {
                String::new()
            };
            if !status.is_empty() {
                theme::text(ui, status, theme::regular(12.5), palette.dim);
            }
        }
        if let Some(until) = shown.muted_until {
            theme::text(
                ui,
                if until == 0 {
                    "Muted".to_owned()
                } else {
                    format!("Muted until {}", crate::util::chat_stamp(app.locale, until))
                },
                theme::regular(12.5),
                palette.secondary,
            );
        }
        // The chat's disappearing-message timer, read-only. A timer WhatsApp
        // turned off reads as "Off", which is a fact; a chat whose timer
        // nobody has mentioned draws nothing rather than guessing.
        // Recorded every frame, drawn or not, so a test reading the marker
        // after the row went away sees nothing instead of the last frame.
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                super::dialogs::disappearing_timer_id(),
                shown.timer.as_deref().map(|text| super::dialogs::TimerRow {
                    text: text.to_owned(),
                    rect: Rect::NOTHING,
                }),
            )
        });
        if let Some(text) = shown.timer.as_deref() {
            let drawn = super::dialogs::disappearing_timer_row(ui, &palette, text);
            ui.ctx().data_mut(|data| {
                data.insert_temp(super::dialogs::disappearing_timer_id(), Some(drawn))
            });
        }
    });
    if let Some((first, last)) = saved {
        editing = None;
        app.actions.push(Action::SaveContact {
            id: id.to_owned(),
            first,
            last,
        });
    }
    app.contact_edit = editing;
    app.group_name_edit = renaming;
    if let Some(action) = group_action {
        app.actions.push(action);
    }
    ui.add_space(if popup { 6.0 } else { 4.0 });
    actions(app, ui, shown, mine, editable);
    if let Some(description) = shown.description.as_deref() {
        section(ui, &palette, popup, |ui| {
            theme::text(
                ui,
                crate::i18n::gettext(app.locale, "Description"),
                theme::medium(12.5),
                palette.secondary,
            );
            ui.add_space(4.0);
            // A long description scrolls in place, so the media and the members
            // stay close under it.
            egui::ScrollArea::vertical()
                .id_salt("group-description")
                .max_height(120.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    theme::selectable_text(ui, description, theme::regular(13.5), palette.text);
                });
        });
    }
    // A locked chat's media stays in the locked folder: the section is not
    // there, rather than empty.
    if app.info_media_allowed() {
        section(ui, &palette, popup, |ui| {
            media_row(app, ui, id, overlay, popup)
        });
    }
    if shown.group && shown.members > 0 {
        section(ui, &palette, popup, |ui| members(app, ui, id));
    }
    if shown.can_leave {
        ui.add_space(4.0);
        let leave_label = if shown.channel {
            crate::i18n::gettext(app.locale, "Leave channel")
        } else {
            crate::i18n::gettext(app.locale, "Leave group")
        };
        ui.vertical_centered(|ui| {
            if super::dialogs::danger_button(ui, app, leave_label.as_ref())
                .tab_stop(Stop::InfoControl(LEAVE_STOP))
                .clicked()
            {
                leave = true;
            }
        });
    }
    if leave {
        app.actions
            .push(Action::ShowDialog(Dialog::ConfirmLeaveGroup(id.to_owned())));
    }
}

/// The actions the chat offers, as many per row as fit, centred.
fn actions(app: &mut App, ui: &mut egui::Ui, shown: &Shown, mine: bool, editable: bool) {
    let palette = app.palette;
    let id = shown.id.as_str();
    let mut buttons: Vec<(Icon, &str, Vec<Action>)> = Vec::new();
    if !shown.group && !mine {
        buttons.push((
            Icon::MessageCircle,
            "Message",
            vec![
                Action::StartChat {
                    id: id.to_owned(),
                    name: shown.name.clone(),
                    // The panel is not a dialog: whatever dialog is up stays.
                    dismiss_dialog: false,
                },
                Action::CloseInfo,
            ],
        ));
    }
    if editable {
        let known = app
            .contacts
            .get(id)
            .and_then(|contact| contact.full_name.as_deref())
            .is_some_and(|full| !full.is_empty());
        buttons.push((
            Icon::User,
            if known { "Rename" } else { "Add to contacts" },
            vec![Action::EditContact {
                id: id.to_owned(),
                name: shown.name.trim_start_matches('~').to_owned(),
            }],
        ));
    }
    if let Some(phone) = &shown.phone {
        buttons.push((
            Icon::Copy,
            "Copy number",
            vec![Action::CopyText(format!("+{phone}"))],
        ));
    }
    if shown.has_chat {
        buttons.push(if shown.muted {
            (
                Icon::Bell,
                "Unmute",
                vec![Action::SetMuted(id.to_owned(), None)],
            )
        } else {
            (
                Icon::BellOff,
                "Mute",
                vec![Action::SetMuted(id.to_owned(), Some(0))],
            )
        });
        buttons.push((
            if shown.pinned {
                Icon::PinOff
            } else {
                Icon::Pin
            },
            if shown.pinned { "Unpin" } else { "Pin" },
            vec![Action::SetPinned(id.to_owned(), !shown.pinned)],
        ));
        buttons.push((
            Icon::Archive,
            if shown.archived {
                "Unarchive"
            } else {
                "Archive"
            },
            vec![
                Action::SetArchived(id.to_owned(), !shown.archived),
                Action::CloseInfo,
            ],
        ));
    }
    let spacing = ui.spacing().item_spacing.x;
    let available = ui.available_width();
    let mut fired: Option<Vec<Action>> = None;
    let mut start = 0;
    while start < buttons.len() {
        // Fit and center as many buttons as each row allows.
        let mut end = start;
        let mut total = 0.0;
        while end < buttons.len() {
            let width = theme::soft_button_width(ui, buttons[end].1, true);
            let grown = if end == start {
                width
            } else {
                total + spacing + width
            };
            if end > start && grown > available {
                break;
            }
            total = grown;
            end += 1;
        }
        ui.horizontal(|ui| {
            ui.add_space((available - total).max(0.0) / 2.0);
            for (index, (icon, label, actions)) in buttons[start..end].iter().enumerate() {
                if theme::soft_button(ui, &palette, Some(*icon), label, false)
                    .tab_stop(Stop::InfoControl(ACTION_STOP + (start + index) as u8))
                    .clicked()
                {
                    fired = Some(actions.clone());
                }
            }
        });
        start = end;
    }
    if let Some(actions) = fired {
        app.actions.extend(actions);
    }
}

/// The group's members, all of them, each opening their own panel. They are
/// sorted once per change of the chat list or the contacts, not every frame;
/// a large group gets a search field, and only the rows on screen are drawn.
fn members(app: &mut App, ui: &mut egui::Ui, id: &str) {
    let palette = app.palette;
    if app.info_members.as_ref().is_none_or(|(chat, _)| chat != id) {
        let list = app
            .chat(id)
            .map(|chat| app.participant_list(chat))
            .unwrap_or_default();
        app.info_members = Some((id.to_owned(), list));
    }
    // Taken for the frame rather than cloned: the rows only push actions.
    let Some((_, members)) = app.info_members.take() else {
        return;
    };
    let (_, mut query) = ui
        .ctx()
        .data(|data| data.get_temp::<(String, String)>(member_filter_id()))
        .filter(|(chat, _)| chat == id)
        .unwrap_or_default();
    ui.horizontal(|ui| {
        theme::text(ui, "Members", theme::semibold(14.0), palette.text);
        theme::text(
            ui,
            members.len().to_string(),
            theme::regular(13.0),
            palette.dim,
        );
    });
    if members.len() > SEARCH_FROM {
        ui.add_space(4.0);
        let width = ui.available_width();
        widgets::search_field(
            ui,
            &palette,
            egui::Id::new(("member-search", id)),
            &mut query,
            "Search members",
            width,
        );
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(member_filter_id(), (id.to_owned(), query.clone())));
    let needle = query.trim().to_lowercase();
    let shown: Vec<&(String, String)> = members
        .iter()
        .filter(|(member, name)| {
            needle.is_empty()
                || name.to_lowercase().contains(&needle)
                || crate::model::phone_of(member).is_some_and(|phone| phone.contains(&needle))
        })
        .collect();
    ui.add_space(4.0);
    // Ten are listed; the rest are a click away. A search looks through all.
    let all = ui
        .ctx()
        .data(|data| data.get_temp::<String>(members_open_id()))
        .is_some_and(|chat| chat == id);
    let total = shown.len();
    let capped = needle.is_empty() && !all && total > MEMBERS_SHOWN;
    let listed = if capped { MEMBERS_SHOWN } else { total };
    let mut open = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    for (member, name) in &shown[..listed] {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), MEMBER_HEIGHT),
            egui::Sense::click(),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        if ui.is_rect_visible(rect) {
            let mine = app.me.as_deref() == Some(member.as_str());
            if response.hovered() && !mine {
                ui.painter().rect_filled(
                    rect.expand2(vec2(6.0, 0.0)),
                    theme::RADIUS_CONTROL,
                    palette.surface_hover,
                );
            }
            let picture = app.avatar(member);
            let avatar = Rect::from_center_size(
                pos2(rect.left() + 20.0, rect.center().y),
                Vec2::splat(38.0),
            );
            widgets::paint_avatar(
                ui,
                &palette,
                avatar,
                name.trim_start_matches('~'),
                member,
                picture.as_deref(),
            );
            let left = rect.left() + 48.0;
            let width = rect.right() - left;
            let subtitle = crate::model::phone_of(member)
                .filter(|_| !name.starts_with('+'))
                .map(crate::util::phone);
            let line = widgets::line(ui, name, theme::medium(14.0), palette.text, width, 1);
            let top = if subtitle.is_some() {
                rect.center().y - line.size().y + 1.0
            } else {
                rect.center().y - line.size().y / 2.0
            };
            line.paint(ui, pos2(left, top), palette.text);
            if let Some(subtitle) = subtitle {
                let detail =
                    widgets::line(ui, &subtitle, theme::regular(12.0), palette.dim, width, 1);
                detail.paint(ui, pos2(left, rect.center().y + 2.0), palette.dim);
            }
        }
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            open = Some(member.clone());
        }
    }
    if needle.is_empty() && total > MEMBERS_SHOWN {
        ui.add_space(8.0);
        let label = if capped {
            format!("Show all {total} members")
        } else {
            "Show fewer".to_owned()
        };
        ui.vertical_centered(|ui| {
            if theme::soft_button(ui, &palette, None, &label, false)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                ui.ctx().data_mut(|data| {
                    if capped {
                        data.insert_temp(members_open_id(), id.to_owned());
                    } else {
                        data.remove::<String>(members_open_id());
                    }
                });
            }
        });
    }
    if shown.is_empty() {
        theme::text(ui, "No members found", theme::regular(13.0), palette.dim);
    }
    app.info_members = Some((id.to_owned(), members));
    if let Some(member) = open
        && Some(member.as_str()) != app.me.as_deref()
    {
        app.actions.push(Action::OpenInfo(member));
    }
}

/// The "Media, links and docs" row: its count once the listing is in, a
/// chevron, and the newest pictures under it. The row opens the media view;
/// a picture goes to its message.
fn media_row(app: &mut App, ui: &mut egui::Ui, chat: &str, overlay: bool, popup: bool) {
    let palette = app.palette;
    // Taken for the frame rather than cloned: the cells only push actions.
    let listing = app.info_media.take();
    let uris = std::mem::take(&mut app.info_uris);
    let shown = listing.as_ref().filter(|media| media.chat == chat);
    let count = shown.map(|media| {
        (
            media.media.len() + media.docs.len() + media.links.len(),
            media.truncated(),
        )
    });
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    let response = response.tab_stop(Stop::InfoControl(MEDIA_ROW_STOP));
    if app.info_focus == Some(InfoFocus::MediaRow) {
        app.info_focus = None;
        response.request_focus();
    }
    theme::reveal_focus(&response);
    theme::focus_outline(ui, response.id, rect, 6.0);
    let title = media_title(app);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &title));
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            // The label sits on the panel's margin, in line with the members
            // header and the pictures under it; the highlight reaches past.
            ui.painter().rect_filled(
                rect.expand2(vec2(6.0, 0.0)),
                theme::RADIUS_SMALL,
                palette.surface_hover,
            );
        }
        ui.painter().text(
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            &title,
            if popup {
                theme::semibold(14.0)
            } else {
                theme::regular(13.5)
            },
            if popup {
                palette.text
            } else {
                palette.secondary
            },
        );
        let chevron = Rect::from_center_size(
            pos2(rect.right() - 15.0, rect.center().y),
            Vec2::splat(18.0),
        );
        theme::paint_icon(ui, Icon::ChevronRight, chevron, 18.0, palette.dim);
        if let Some((count, cut)) = count {
            // A "+" says the lists were cut, so the count is a floor.
            let count = if cut {
                crate::i18n::gettext(app.locale, "{}+").replace("{}", &count.to_string())
            } else {
                count.to_string()
            };
            ui.painter().text(
                pos2(chevron.left() - 2.0, rect.center().y),
                Align2::RIGHT_CENTER,
                count,
                theme::regular(13.0),
                palette.dim,
            );
        }
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.actions
            .push(Action::ShowInfo(InfoView::Media(MediaTab::Media)));
    }
    if app.info_media_failed {
        failure_line(app, ui);
    }
    if let Some(media) = shown
        && !media.media.is_empty()
    {
        let cell = ((ui.available_width() - 2.0 * GAP) / 3.0).min(130.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for (index, message) in media.media.iter().take(3).enumerate() {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                media_cell(
                    app,
                    ui,
                    rect,
                    response,
                    message,
                    uris.get(index).map(String::as_str),
                    overlay,
                );
            }
        });
    }
    app.info_uris = uris;
    app.info_media = listing;
}

/// Says the listing could not be made, with a button that asks again.
fn failure_line(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    ui.horizontal_wrapped(|ui| {
        theme::text(
            ui,
            crate::i18n::gettext(app.locale, "Could not list the media."),
            theme::regular(12.5),
            palette.dim,
        );
        if theme::soft_button(
            ui,
            &palette,
            Some(Icon::Refresh),
            &crate::i18n::gettext(app.locale, "Try again"),
            false,
        )
        .tab_stop(Stop::InfoControl(RETRY_STOP))
        .clicked()
        {
            app.actions.push(Action::RetryInfoMedia);
        }
    });
}

/// The media view: a back header, the three tabs, and the tab's list.
fn media_view(
    app: &mut App,
    ui: &mut egui::Ui,
    chat: &str,
    tab: MediaTab,
    overlay: bool,
    popup: bool,
) {
    let palette = app.palette;
    let back = crate::i18n::gettext(app.locale, "Back");
    let title = media_title(app);
    let focus_back = app.info_focus == Some(InfoFocus::Back);
    if focus_back {
        app.info_focus = None;
    }
    header(
        app,
        ui,
        Icon::ChevronLeft,
        &back,
        &title,
        Action::ShowInfo(InfoView::Overview),
        focus_back,
        popup,
    );

    // Taken for the frame rather than cloned: the rows only push actions.
    let listing = app.info_media.take();
    let uris = std::mem::take(&mut app.info_uris);
    let shown = listing.as_ref().filter(|media| media.chat == chat);
    Frame::new()
        .inner_margin(Margin::symmetric(MARGIN as i8, if popup { 14 } else { 10 }))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (index, choice) in [MediaTab::Media, MediaTab::Docs, MediaTab::Links]
                    .into_iter()
                    .enumerate()
                {
                    let count = shown.map_or(0, |media| match choice {
                        MediaTab::Media => media.media.len(),
                        MediaTab::Docs => media.docs.len(),
                        MediaTab::Links => media.links.len(),
                    });
                    let label = tab_label(app, choice);
                    if widgets::filter_chip(ui, &palette, &label, count, choice == tab)
                        .tab_stop(Stop::InfoControl(TAB_STOP + index as u8))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        app.actions.push(Action::ShowInfo(InfoView::Media(choice)));
                    }
                }
            });
        });
    if !popup {
        ui.painter().hline(
            ui.max_rect().x_range(),
            ui.cursor().top(),
            Stroke::new(1.0, palette.outline),
        );
    }
    match shown {
        Some(media) => {
            let cut = match tab {
                MediaTab::Media => media.media_truncated,
                MediaTab::Docs => media.docs_truncated,
                MediaTab::Links => media.links_truncated,
            };
            if cut {
                truncated_note(app, ui);
            }
            match tab {
                MediaTab::Media => media_grid(app, ui, media, &uris, overlay),
                MediaTab::Docs => docs_list(app, ui, media, overlay),
                MediaTab::Links => links_list(app, ui, media),
            }
        }
        None if app.info_media_failed => {
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.add_space(MARGIN);
                failure_line(app, ui);
            });
        }
        None if app.info_media_pending => {
            ui.add_space(32.0);
            ui.vertical_centered(|ui| theme::spinner(ui, 22.0, palette.secondary));
        }
        None => {}
    }
    app.info_uris = uris;
    app.info_media = listing;
}

/// The chat's pictures and videos, newest first, in square cells as wide as
/// the panel allows. Only the rows on screen are laid out, so only their
/// thumbnails are decoded.
fn media_grid(
    app: &mut App,
    ui: &mut egui::Ui,
    listing: &ChatMedia,
    uris: &[String],
    overlay: bool,
) {
    let palette = app.palette;
    if listing.media.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::Image,
            &crate::i18n::gettext(app.locale, "No media"),
            &crate::i18n::gettext(
                app.locale,
                "Photos and videos shared in this chat will show here.",
            ),
        );
        return;
    }
    let width = ui.available_width() - 2.0 * MARGIN;
    let columns = if width >= 4.0 * 84.0 + 3.0 * GAP {
        4
    } else {
        3
    };
    let cell = (width - (columns - 1) as f32 * GAP) / columns as f32;
    let rows = listing.media.len().div_ceil(columns);
    ui.add_space(6.0);
    // `show_rows` reads the spacing from outside its closure.
    ui.spacing_mut().item_spacing = vec2(GAP, GAP);
    egui::ScrollArea::vertical()
        .id_salt(("info-media-grid", listing.chat.as_str()))
        .auto_shrink([false, false])
        .show_rows(ui, cell + GAP, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.add_space(MARGIN);
                    for column in 0..columns {
                        let index = row * columns + column;
                        let Some(message) = listing.media.get(index) else {
                            break;
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                        media_cell(
                            app,
                            ui,
                            rect,
                            response,
                            message,
                            uris.get(index).map(String::as_str),
                            overlay,
                        );
                    }
                });
            }
        });
}

/// One square picture or video: its preview cropped to the square, with a
/// play mark and the length on a video. Clicking it goes to the message.
fn media_cell(
    app: &mut App,
    ui: &mut egui::Ui,
    rect: Rect,
    response: egui::Response,
    message: &Message,
    uri: Option<&str>,
    overlay: bool,
) {
    let palette = app.palette;
    let video = matches!(message.content, Content::Video { .. });
    let seconds = match &message.content {
        Content::Video { seconds, .. } => *seconds,
        _ => None,
    };
    let stamp = crate::util::chat_stamp(app.locale, message.timestamp);
    response.widget_info(|| {
        let what = if video {
            crate::i18n::gettext(app.locale, "Video")
        } else {
            crate::i18n::gettext(app.locale, "Photo")
        };
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{what}, {stamp}"))
    });
    if ui.is_rect_visible(rect) {
        if !thumbnail(ui, &palette, rect, message, uri) {
            ui.painter()
                .rect_filled(rect, theme::RADIUS_CONTROL, palette.surface);
            let icon = if video { Icon::Video } else { Icon::Image };
            theme::paint_icon(ui, icon, rect, 22.0, palette.dim);
        }
        if video {
            let disc = Rect::from_center_size(rect.center(), Vec2::splat(30.0));
            ui.painter()
                .circle_filled(disc.center(), 15.0, Color32::from_black_alpha(140));
            theme::paint_icon(ui, Icon::Play, disc, 14.0, Color32::WHITE);
            if let Some(seconds) = seconds {
                let galley = ui.painter().layout_no_wrap(
                    crate::util::duration(seconds),
                    theme::medium(11.0),
                    Color32::WHITE,
                );
                let pad = vec2(4.0, 2.0);
                let label = Rect::from_min_max(
                    rect.max - galley.size() - pad * 2.0 - Vec2::splat(4.0),
                    rect.max - Vec2::splat(4.0),
                );
                ui.painter()
                    .rect_filled(label, 3.0, Color32::from_black_alpha(140));
                ui.painter().galley(label.min + pad, galley, Color32::WHITE);
            }
        }
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, theme::RADIUS_CONTROL, Color32::from_white_alpha(24));
        }
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        go_to(app, message, overlay);
    }
}

/// Goes to a listed message. Over a narrow chat the panel would hide it,
/// so the panel closes, as the search pane folds.
fn go_to(app: &mut App, message: &Message, overlay: bool) {
    app.actions.push(Action::OpenMessage {
        chat: message.chat.clone(),
        message: message.id.clone(),
    });
    if overlay {
        app.actions.push(Action::CloseInfo);
    }
}

/// Paints the message's preview cropped to fill `rect`, and reports whether
/// there was one. The archived JPEG preview needs no download; a downloaded
/// picture without one goes through egui's file loader, as in the bubble. A
/// video without one keeps its placeholder: its file is not a picture.
fn thumbnail(
    ui: &egui::Ui,
    palette: &theme::Palette,
    rect: Rect,
    message: &Message,
    uri: Option<&str>,
) -> bool {
    let image = match (&message.thumbnail, uri, &message.content) {
        (Some(bytes), Some(uri), _) => {
            crate::image_cache::include(ui.ctx(), uri, bytes);
            egui::Image::new(uri.to_owned())
        }
        (_, _, Content::Image { media, .. }) if media.path.is_some() => {
            let path = media.path.as_deref().expect("checked");
            widgets::file_image(ui, path)
        }
        _ => return false,
    };
    cover(ui, palette, rect, image);
    true
}

/// Paints `image` cropped to fill `rect`.
fn cover(ui: &egui::Ui, palette: &theme::Palette, rect: Rect, image: egui::Image<'_>) {
    // The texture's shape is known once it is decoded; until then the cell
    // is a plain surface rather than a stretched placeholder.
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ui.ctx(), rect.size())
    else {
        ui.painter()
            .rect_filled(rect, theme::RADIUS_CONTROL, palette.surface);
        return;
    };
    // Cover the square: the longer side is cropped at both ends.
    let (width, height) = (texture.size.x.max(1.0), texture.size.y.max(1.0));
    let uv = if width > height {
        let cut = (1.0 - height / width) / 2.0;
        Rect::from_min_max(pos2(cut, 0.0), pos2(1.0 - cut, 1.0))
    } else {
        let cut = (1.0 - width / height) / 2.0;
        Rect::from_min_max(pos2(0.0, cut), pos2(1.0, 1.0 - cut))
    };
    image
        .uv(uv)
        .corner_radius(theme::RADIUS_CONTROL)
        .paint_at(ui, rect);
}

/// The chat's documents, newest first: a file icon, the name, and its
/// pages, size and day. A row goes to its message.
fn docs_list(app: &mut App, ui: &mut egui::Ui, listing: &ChatMedia, overlay: bool) {
    let palette = app.palette;
    if listing.docs.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::FileText,
            &crate::i18n::gettext(app.locale, "No documents"),
            &crate::i18n::gettext(app.locale, "Files shared in this chat will show here."),
        );
        return;
    }
    // `show_rows` reads the spacing from outside its closure.
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .id_salt(("info-docs", listing.chat.as_str()))
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, listing.docs.len(), |ui, range| {
            for message in &listing.docs[range] {
                let Content::Document {
                    media,
                    file_name,
                    pages,
                    ..
                } = &message.content
                else {
                    continue;
                };
                let mut detail = Vec::new();
                if let Some(pages) = pages {
                    detail.push(
                        crate::i18n::ngettext(app.locale, "{} page", "{} pages", *pages)
                            .replace("{}", &pages.to_string()),
                    );
                }
                detail.push(crate::util::bytes(media.size));
                detail.push(crate::util::chat_stamp(app.locale, message.timestamp));
                let row = Row {
                    icon: Icon::FileText,
                    title: file_name,
                    description: None,
                    detail: &detail.join(" · "),
                    picture: None,
                };
                if item_row(app, ui, row).clicked() {
                    go_to(app, message, overlay);
                }
            }
        });
}

/// The web links written in the chat, newest first: the preview's title or
/// the address, and the site. A row opens the address in the browser.
fn links_list(app: &mut App, ui: &mut egui::Ui, listing: &ChatMedia) {
    let palette = app.palette;
    if listing.links.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::ExternalLink,
            &crate::i18n::gettext(app.locale, "No links"),
            &crate::i18n::gettext(app.locale, "Links shared in this chat will show here."),
        );
        return;
    }
    // `show_rows` reads the spacing from outside its closure.
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .id_salt(("info-links", listing.chat.as_str()))
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, listing.links.len(), |ui, range| {
            for link in &listing.links[range] {
                let title = link.title.as_deref().unwrap_or(&link.url);
                let detail = format!(
                    "{} · {}",
                    domain(&link.url),
                    crate::util::chat_stamp(app.locale, link.timestamp)
                );
                let uri = link_uri(&listing.chat, link);
                let picture = link.thumbnail.as_deref().zip(uri.as_deref());
                let row = Row {
                    icon: Icon::ExternalLink,
                    title,
                    description: link.description.as_deref(),
                    detail: &detail,
                    picture,
                };
                if item_row(app, ui, row).clicked() {
                    app.actions.push(Action::OpenUrl(link.url.clone()));
                }
            }
        });
}

/// The texture name a link's preview picture is drawn under, when it has one.
pub(crate) fn link_uri(chat: &str, link: &crate::model::ChatLink) -> Option<String> {
    link.thumbnail
        .as_ref()
        .map(|_| thumbnail_uri(chat, &format!("link-{}", link.message)))
}

/// What a document or link row shows.
struct Row<'a> {
    icon: Icon,
    title: &'a str,
    /// A line between the title and the detail, such as a link's summary.
    description: Option<&'a str>,
    detail: &'a str,
    /// A picture in place of the icon: its bytes and the texture name.
    picture: Option<(&'a [u8], &'a str)>,
}

/// One document or link row: a tile with an icon or a picture, a title, and
/// the lines under it. A pointer and accessibility target, like a chat row,
/// not a Tab stop.
fn item_row(app: &mut App, ui: &mut egui::Ui, row: Row<'_>) -> egui::Response {
    let palette = app.palette;
    let Row {
        icon,
        title,
        description,
        detail,
        picture,
    } = row;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{title}, {detail}"))
    });
    if ui.is_rect_visible(rect) {
        if response.hovered() {
            ui.painter().rect_filled(
                rect.shrink2(vec2(MARGIN - 8.0, 4.0)),
                theme::RADIUS_CONTROL,
                palette.surface_hover,
            );
        }
        let tile = Rect::from_center_size(
            pos2(rect.left() + MARGIN + 24.0, rect.center().y),
            Vec2::splat(48.0),
        );
        match picture {
            Some((bytes, uri)) => {
                crate::image_cache::include(ui.ctx(), uri, bytes);
                cover(ui, &palette, tile, egui::Image::new(uri.to_owned()));
            }
            None => {
                ui.painter()
                    .rect_filled(tile, theme::RADIUS_CONTROL, palette.surface);
                theme::paint_icon(ui, icon, tile, 20.0, palette.secondary);
            }
        }
        let left = tile.right() + 14.0;
        let width = rect.right() - MARGIN - left;
        let mut lines = vec![(title, theme::medium(14.0), palette.text, 18.0)];
        if let Some(description) = description {
            lines.push((description, theme::regular(12.5), palette.secondary, 17.0));
        }
        lines.push((detail, theme::regular(12.0), palette.dim, 16.0));
        let gap = 3.0;
        let total: f32 =
            lines.iter().map(|line| line.3).sum::<f32>() + gap * (lines.len() - 1) as f32;
        let mut top = rect.center().y - total / 2.0;
        for (text, font, color, height) in lines {
            let line = widgets::line(ui, text, font, color, width, 1);
            line.paint(ui, pos2(left, top), color);
            top += height + gap;
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Says the open tab's list was cut, above it, when the archive held more
/// than it carries.
fn truncated_note(app: &App, ui: &mut egui::Ui) {
    let palette = app.palette;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(MARGIN);
        let line = widgets::line(
            ui,
            &crate::i18n::gettext(app.locale, "Only the newest items are listed."),
            theme::regular(12.0),
            palette.dim,
            ui.available_width() - MARGIN,
            2,
        );
        let (rect, _) = ui.allocate_exact_size(line.size(), Sense::hover());
        line.paint(ui, rect.min, palette.dim);
    });
    ui.add_space(4.0);
}

/// A tab's label. The words are short, so each carries the context that
/// tells a translator where it sits.
pub fn tab_label(app: &App, tab: MediaTab) -> String {
    match tab {
        MediaTab::Media => crate::i18n::pgettext(app.locale, "info tab", "Media"),
        MediaTab::Docs => crate::i18n::pgettext(app.locale, "info tab", "Docs"),
        MediaTab::Links => crate::i18n::pgettext(app.locale, "info tab", "Links"),
    }
    .into_owned()
}

/// The host an address opens, without a leading `www.`. It is parsed, not
/// sliced, so userinfo such as `bank.example@` cannot pose as the site.
pub fn domain(url: &str) -> String {
    let parsed = reqwest::Url::parse(url)
        .or_else(|_| reqwest::Url::parse(&format!("https://{url}")))
        .ok();
    match parsed.as_ref().and_then(reqwest::Url::host_str) {
        Some(host) => host.trim_start_matches("www.").to_owned(),
        None => url.to_owned(),
    }
}

/// Where the pencil that renames a group was drawn, for interaction tests.
pub fn group_name_button_id() -> egui::Id {
    egui::Id::new("group-name-edit")
}

/// Where the group photo that opens its menu was drawn, for interaction tests.
pub fn group_photo_id() -> egui::Id {
    egui::Id::new("group-photo")
}

/// The group's big photo, which opens a menu to change or remove it.
fn group_photo(
    app: &App,
    ui: &mut egui::Ui,
    name: &str,
    id: &str,
    size: f32,
    picture: Option<&std::path::Path>,
) -> Option<Action> {
    let palette = app.palette;
    let label = crate::i18n::gettext(app.locale, "Change group photo");
    let response = widgets::clickable_avatar(ui, &palette, name, id, size, picture, &label)
        .tab_stop(Stop::InfoControl(PHOTO_STOP))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label.as_ref());
    ui.ctx()
        .data_mut(|data| data.insert_temp(group_photo_id(), response.rect));
    let change = crate::i18n::gettext(app.locale, "Change photo");
    let remove = crate::i18n::gettext(app.locale, "Remove photo");
    let width = widgets::menu_width(ui, &[change.as_ref(), remove.as_ref()], true);
    let mut chosen = None;
    egui::Popup::menu(&response)
        .width(width)
        .frame(widgets::menu_frame(&palette))
        .show_frosted(ui.ctx(), |ui| {
            if widgets::menu_item(ui, &palette, Some(Icon::Image), &change) {
                chosen = Some(Action::PickGroupPicture(id.to_owned()));
                ui.close();
            }
            // Only a photo that is there can be removed.
            if picture.is_some() && widgets::menu_item(ui, &palette, Some(Icon::Trash), &remove) {
                chosen = Some(Action::RemoveGroupPicture(id.to_owned()));
                ui.close();
            }
        });
    chosen
}

/// The group name being typed. Returns `Some(true)` when Enter or the check
/// submits it, `Some(false)` when the cross cancels it.
fn group_name_field(app: &App, ui: &mut egui::Ui, draft: &mut String) -> Option<bool> {
    let palette = app.palette;
    let mut outcome = None;
    ui.horizontal(|ui| {
        let buttons = 2.0 * (18.0 + 12.0) + 2.0 * ui.spacing().item_spacing.x;
        let width = 240.0_f32.min(ui.available_width() - buttons);
        ui.add_space(((ui.available_width() - width - buttons) / 2.0).max(0.0));
        let format = egui::TextFormat::simple(theme::semibold(15.0), palette.text);
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
            crate::bidi::layout_field(ui, text.as_str(), &format, wrap)
        };
        let align = if crate::bidi::base_rtl(draft) {
            Align::RIGHT
        } else {
            Align::LEFT
        };
        let hint = crate::i18n::gettext(app.locale, "Group name");
        let field = Frame::new()
            .fill(palette.surface)
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(Margin::symmetric(10, 5))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(draft)
                        .id(group_name_field_id())
                        .hint_text(
                            egui::RichText::new(hint.as_ref())
                                .color(palette.dim)
                                .font(theme::semibold(15.0)),
                        )
                        .font(theme::semibold(15.0))
                        .text_color(palette.text)
                        .frame(Frame::NONE)
                        // WhatsApp refuses longer names.
                        .char_limit(crate::model::GROUP_NAME_LIMIT)
                        .desired_width(width - 20.0)
                        .horizontal_align(align)
                        .layouter(&mut layouter),
                )
            });
        theme::focus_outline(
            ui,
            field.inner.id,
            field.response.rect,
            f32::from(theme::RADIUS),
        );
        // Read before focus is requested again below: egui answers
        // `lost_focus` from the current focus, not from this frame's input.
        if field.inner.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            outcome = Some(true);
        } else if ui.memory(|memory| memory.focused().is_none()) {
            field.inner.request_focus();
        }
        if theme::icon_button(
            ui,
            Icon::Check,
            18.0,
            palette.secondary,
            palette.accent,
            &crate::i18n::gettext(app.locale, "Save name (Enter)"),
        )
        .clicked()
        {
            outcome = Some(true);
        }
        if theme::icon_button(
            ui,
            Icon::X,
            18.0,
            palette.secondary,
            palette.text,
            &crate::i18n::gettext(app.locale, "Cancel (Escape)"),
        )
        .clicked()
        {
            outcome = Some(false);
        }
    });
    outcome
}

/// The group name editor's text field.
pub fn group_name_field_id() -> egui::Id {
    egui::Id::new("group-name-field")
}

#[cfg(test)]
mod tests {
    use super::domain;

    #[test]
    fn a_links_site_drops_the_scheme_the_www_and_the_path() {
        assert_eq!(domain("https://spotifast.rocks/"), "spotifast.rocks");
        assert_eq!(
            domain("https://www.rust-lang.org/learn?x=1"),
            "rust-lang.org"
        );
        assert_eq!(domain("engine.rocks#top"), "engine.rocks");
    }

    #[test]
    fn a_links_site_is_the_host_the_browser_opens_not_its_userinfo() {
        assert_eq!(
            domain("https://bank.example@attacker.example/path"),
            "attacker.example"
        );
        assert_eq!(
            domain("https://user:pass@www.attacker.example:8080/"),
            "attacker.example"
        );
    }
}
