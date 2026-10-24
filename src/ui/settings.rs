//! The settings page.

use std::borrow::Cow;

use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, Vec2, pos2, vec2,
};

use super::controls;
use crate::app::App;
use crate::chat_themes::{Accent, ChatTheme, Mode, Style};
use crate::i18n::Locale;
use crate::model::{Action, Dialog, Page};
use crate::privacy::{PrivacyChoice, PrivacyKind};
use crate::settings::{DOODLE_INTENSITY, DoodleColor, DoodleStyle, Settings, WallpaperColor};
use crate::theme::{self, Icon, Palette};
use crate::wallpaper;

use super::widgets;

/// The settings search field, which Ctrl+F focuses on this page.
pub const SEARCH_ID: &str = "settings-search";

/// A settings string with its English source, so the search finds a
/// translated setting by either wording.
#[derive(Clone, Debug, Default)]
struct Text {
    shown: Cow<'static, str>,
    source: Cow<'static, str>,
}

impl From<&'static str> for Text {
    fn from(text: &'static str) -> Self {
        Self {
            shown: text.into(),
            source: text.into(),
        }
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Self {
            source: text.clone().into(),
            shown: text.into(),
        }
    }
}

/// Translates a settings string and keeps its English source for the search.
fn translated(locale: Locale, source: &'static str) -> Text {
    Text {
        shown: crate::i18n::gettext(locale, source),
        source: source.into(),
    }
}

/// The words a settings row is found by: its title and description, and for
/// rows that draw themselves, the labels of what they contain.
#[derive(Debug, Default)]
struct Row {
    title: Text,
    description: Text,
    keywords: Vec<Text>,
}

/// The settings search, folded like chat search so case and accents do not
/// matter.
struct Filter {
    needle: String,
}

impl Filter {
    fn new(query: &str) -> Self {
        Self {
            needle: crate::util::search_key(query.trim()),
        }
    }

    fn is_empty(&self) -> bool {
        self.needle.is_empty()
    }

    fn finds(&self, text: &Text) -> bool {
        crate::util::search_key(&text.shown).contains(&self.needle)
            || crate::util::search_key(&text.source).contains(&self.needle)
    }

    /// Whether a row shows. A match on the section title keeps the whole
    /// section, as a match on the row keeps the row.
    fn shows(&self, section: &Text, row: &Row) -> bool {
        self.is_empty()
            || self.finds(section)
            || self.finds(&row.title)
            || self.finds(&row.description)
            || row.keywords.iter().any(|keyword| self.finds(keyword))
    }
}

type Draw = Box<dyn FnOnce(&mut egui::Ui, &mut App)>;

enum Control {
    /// The control beside a titled row.
    Row(Draw),
    /// A switch bound to a setting.
    Toggle(fn(&mut Settings) -> &mut bool),
    /// A switch bound to the active WhatsApp account.
    AccountToggle(fn(&mut crate::settings::AccountSettings) -> &mut bool),
    /// Something that lays itself out, like the account card.
    Block(Draw),
}

/// A titled card of settings, drawn only when the search leaves a row in it.
struct Section {
    title: Text,
    entries: Vec<(Row, Control)>,
}

impl Section {
    fn new(title: Text) -> Self {
        Self {
            title,
            entries: Vec::new(),
        }
    }

    fn row(
        &mut self,
        title: impl Into<Text>,
        description: impl Into<Text>,
        control: impl FnOnce(&mut egui::Ui, &mut App) + 'static,
    ) {
        let row = Row {
            title: title.into(),
            description: description.into(),
            keywords: Vec::new(),
        };
        self.entries.push((row, Control::Row(Box::new(control))));
    }

    fn toggle(
        &mut self,
        title: impl Into<Text>,
        description: impl Into<Text>,
        field: fn(&mut Settings) -> &mut bool,
    ) {
        let row = Row {
            title: title.into(),
            description: description.into(),
            keywords: Vec::new(),
        };
        self.entries.push((row, Control::Toggle(field)));
    }

    fn account_toggle(
        &mut self,
        title: impl Into<Text>,
        description: impl Into<Text>,
        field: fn(&mut crate::settings::AccountSettings) -> &mut bool,
    ) {
        let row = Row {
            title: title.into(),
            description: description.into(),
            keywords: Vec::new(),
        };
        self.entries.push((row, Control::AccountToggle(field)));
    }

    fn block(&mut self, keywords: Vec<Text>, draw: impl FnOnce(&mut egui::Ui, &mut App) + 'static) {
        let row = Row {
            keywords,
            ..Row::default()
        };
        self.entries.push((row, Control::Block(Box::new(draw))));
    }

    /// The rows the search leaves, in order.
    fn visible(self, filter: &Filter) -> (Text, Vec<(Row, Control)>) {
        let entries = self
            .entries
            .into_iter()
            .filter(|(row, _)| filter.shows(&self.title, row))
            .collect();
        (self.title, entries)
    }

    /// Draws the section, returning whether anything in it matched.
    fn show(self, ui: &mut egui::Ui, app: &mut App, filter: &Filter, titled: bool) -> bool {
        let (title, entries) = self.visible(filter);
        if entries.is_empty() {
            return false;
        }
        let palette = app.palette;
        let title = if titled { title.shown } else { "".into() };
        // Room between settings, more on the open page than on a card.
        let gap = if titled { ROW_GAP } else { PAGE_ROW_GAP };
        section(ui, &palette, &title, |ui| {
            for (index, (row, control)) in entries.into_iter().enumerate() {
                if index > 0 {
                    ui.add_space(gap);
                }
                match control {
                    Control::Row(control) => widgets::setting_row(
                        ui,
                        &palette,
                        &row.title.shown,
                        &row.description.shown,
                        |ui| control(ui, app),
                    ),
                    Control::Toggle(field) => {
                        toggle(ui, app, &row.title.shown, &row.description.shown, field);
                    }
                    Control::AccountToggle(field) => {
                        account_toggle(ui, app, &row.title.shown, &row.description.shown, field);
                    }
                    Control::Block(draw) => draw(ui, app),
                }
            }
        });
        true
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    let beside = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(super::side_columns_id()))
        .unwrap_or(false);
    egui::ScrollArea::vertical()
        .id_salt("settings")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The column keeps a readable width and sits in the middle of a
            // wide window instead of leaving the space on its right empty.
            let full = ui.available_width();
            let width = (full - 2.0 * SIDE_MARGIN).clamp(0.0, COLUMN_WIDTH);
            let column = Rect::from_min_size(
                ui.cursor().min + vec2((full - width) / 2.0, TOP_MARGIN),
                vec2(width, ui.available_height()),
            );
            ui.ctx()
                .data_mut(|data| data.insert_temp(column_id(), column.x_range()));
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(column)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.set_width(width);
                    let all = sections(app);
                    let searching = !app.settings_search.trim().is_empty();
                    let section = app.settings_section.min(all.len().saturating_sub(1));
                    // Beside its categories the page shows the one picked,
                    // under its own name, or what a search finds.
                    let one = beside.then_some(section).filter(|_| !searching);
                    let heading = match one {
                        Some(index) => all[index].title.shown.clone(),
                        None => crate::i18n::gettext(locale, "Settings"),
                    };
                    ui.horizontal(|ui| {
                        if theme::icon_button(
                            ui,
                            Icon::ArrowLeft,
                            20.0,
                            palette.secondary,
                            palette.text,
                            "Back (Esc)",
                        )
                        .clicked()
                        {
                            app.actions.push(Action::Open(Page::Chats));
                        }
                        theme::text(ui, heading.as_ref(), theme::bold(24.0), palette.text);
                    });
                    ui.add_space(14.0);
                    if !beside {
                        search(app, ui);
                        ui.add_space(6.0);
                    }
                    let filter = Filter::new(&app.settings_search);
                    let mut found = false;
                    for (index, section) in all.into_iter().enumerate() {
                        if one.is_some_and(|one| one != index) {
                            continue;
                        }
                        found |= section.show(ui, app, &filter, one.is_none());
                    }
                    if !found {
                        ui.add_space(24.0);
                        theme::paragraph(
                            ui,
                            crate::i18n::gettext(locale, "No settings match “{query}”.")
                                .replace("{query}", app.settings_search.trim()),
                            theme::regular(14.0),
                            palette.secondary,
                        );
                    }
                },
            );
            ui.add_space(TOP_MARGIN);
        });
}

/// How tall a category row in the settings list is.
const CATEGORY_ROW: f32 = 44.0;

/// The icon beside a settings category, by its English name.
fn category_icon(source: &str) -> Icon {
    match source {
        "Appearance" => {
            if theme::halloween() {
                Icon::Ghost
            } else {
                Icon::Sun
            }
        }
        "Chats" => theme::chats_icon(),
        "Calls and voice" => Icon::Phone,
        "Notifications" => Icon::Bell,
        "Privacy" => Icon::Lock,
        "System" => Icon::Monitor,
        "Account" => Icon::User,
        "Files" => Icon::Download,
        "About" => Icon::Info,
        _ => Icon::Settings,
    }
}

/// The settings categories in the list column, where the chats are on the
/// Chats tab, with the settings search above them.
pub fn sidebar(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let panel = egui::Panel::left("settings-categories")
        .resizable(false)
        .exact_size(app.settings.sidebar_width)
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.rail()).inner_margin(Margin {
            left: 4,
            right: 8,
            top: super::chats::HEADER_DROP as i8,
            bottom: 0,
        }));
    panel.show(ui, |ui| {
        Frame::new()
            .inner_margin(Margin {
                left: 14,
                right: 14,
                top: 8,
                bottom: 8,
            })
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    vec2(ui.available_width(), super::conversation::HEADER_ROW),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        theme::text(
                            ui,
                            crate::i18n::gettext(app.locale, "Settings"),
                            theme::bold(20.0),
                            palette.text,
                        );
                    },
                );
                ui.add_space(8.0);
                search(app, ui);
            });
        ui.add_space(4.0);
        let searching = !app.settings_search.trim().is_empty();
        let titles: Vec<Text> = sections(app)
            .into_iter()
            .map(|section| section.title)
            .collect();
        let current = app.settings_section.min(titles.len().saturating_sub(1));
        egui::ScrollArea::vertical()
            .id_salt("settings-categories")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Rows line up with the search field above them.
                ui.spacing_mut().item_spacing.y = 4.0;
                let inner = ui.available_rect_before_wrap();
                let inner = Rect::from_x_y_ranges(
                    inner.left() + 8.0..=inner.right() - 4.0,
                    inner.y_range(),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    for (index, title) in titles.iter().enumerate() {
                        let selected = !searching && index == current;
                        let (rect, response) = ui.allocate_exact_size(
                            vec2(ui.available_width(), CATEGORY_ROW),
                            Sense::click(),
                        );
                        theme::reveal_focus(&response);
                        theme::focus_outline(ui, response.id, rect, 12.0);
                        if selected {
                            ui.painter().rect_filled(rect, 12.0, palette.surface);
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 12.0, palette.surface_hover);
                        }
                        let color = if selected {
                            palette.text
                        } else {
                            palette.secondary
                        };
                        theme::paint_icon(
                            ui,
                            category_icon(&title.source),
                            Rect::from_center_size(
                                pos2(rect.left() + 26.0, rect.center().y),
                                Vec2::splat(20.0),
                            ),
                            20.0,
                            color,
                        );
                        ui.painter().text(
                            pos2(rect.left() + 50.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            title.shown.as_ref(),
                            theme::medium(15.0),
                            if selected { palette.text } else { color },
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                ui.is_enabled(),
                                selected,
                                title.shown.as_ref(),
                            )
                        });
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            app.actions.push(Action::OpenSettingsSection(index));
                        }
                    }
                });
            });
    });
}

/// Extra room between settings on a card.
const ROW_GAP: f32 = 8.0;
/// Extra room between settings on the page beside the list of categories.
const PAGE_ROW_GAP: f32 = 18.0;

/// Widest the settings column grows.
const COLUMN_WIDTH: f32 = 640.0;
/// Least space beside the settings column.
const SIDE_MARGIN: f32 = 32.0;
/// Space above and below the settings column.
const TOP_MARGIN: f32 = 24.0;

/// Where the settings column was laid out, for layout tests.
pub fn column_id() -> egui::Id {
    egui::Id::new("settings-column")
}

/// The search field above the settings. Ctrl+F focuses it.
fn search(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let mut text = app.settings_search.clone();
    let response = widgets::search_field(
        ui,
        &palette,
        egui::Id::new(SEARCH_ID),
        &mut text,
        &crate::i18n::gettext(app.locale, "Search settings"),
        ui.available_width(),
    );
    if text != app.settings_search {
        app.actions.push(Action::SearchSettings(text));
    }
    if app.focus_settings_search {
        app.focus_settings_search = false;
        response.request_focus();
        response.scroll_to_me(Some(Align::Min));
    }
}

/// Every settings section, in page order, with the rows this setup offers.
fn sections(app: &App) -> Vec<Section> {
    let locale = app.locale;
    let palette = app.palette;

    let mut appearance = Section::new(translated(locale, "Appearance"));
    // The language first: it is what a reader who cannot read the rest
    // looks for.
    appearance.row(translated(locale, "Language"), "", language_picker);
    appearance.row(
        translated(locale, "Halloween"),
        translated(
            locale,
            "An orange theme with a night wallpaper, pumpkin icons, spooky doodles and sounds. Off puts back the doodles and sounds it changed.",
        ),
        halloween_switch,
    );
    if app.settings.halloween {
        for part in crate::settings::HalloweenPart::ALL {
            appearance.row(translated(locale, part.label()), "", move |ui, app| {
                halloween_part_switch(ui, app, part)
            });
        }
    }
    appearance.block(
        vec![translated(locale, "Preview"), translated(locale, "Theme")],
        chat_preview,
    );
    appearance.row(
        translated(locale, "Colours"),
        translated(
            locale,
            "Make your own from a look, a mode and an accent, or pick a ready-made theme.",
        ),
        style_picker,
    );
    match app.settings.style() {
        Style::Custom => {
            appearance.block(
                vec![translated(locale, "Look"), translated(locale, "Theme")],
                look_cards,
            );
            appearance.row(
                translated(locale, "Mode"),
                if app.custom_themes.follows_omarchy() {
                    translated(locale, "Follow system uses your Omarchy colours.")
                } else {
                    translated(locale, "Light, dark, or as the system is.")
                },
                mode_picker,
            );
            appearance.block(
                vec![translated(locale, "Accent"), translated(locale, "Colour")],
                accent_picker,
            );
        }
        Style::Themes => {
            appearance.row(
                translated(locale, "Mode"),
                translated(locale, "Each theme comes in light and dark."),
                mode_picker,
            );
            appearance.block(
                vec![
                    translated(locale, "Themes"),
                    translated(locale, "Theme"),
                    "Agent Orchestrator".into(),
                ],
                theme_cards,
            );
        }
    }
    appearance.toggle(
        translated(locale, "Blur"),
        translated(
            locale,
            "Frosted glass under the chat header and the message field, and the desktop through the window where the system allows. Off keeps every surface solid.",
        ),
        |settings| &mut settings.blur,
    );
    appearance.toggle(
        translated(locale, "Always show time"),
        translated(
            locale,
            "The time and ticks inside every message, as WhatsApp shows them. Off, Zeus's look shows them below a message you click.",
        ),
        |settings| &mut settings.always_show_time,
    );
    appearance.row(
        translated(locale, "Font"),
        translated(locale, "Choose the desktop font."),
        font_picker,
    );
    appearance.row(
        translated(locale, "Wallpaper"),
        Text::default(),
        move |ui, app| {
            let label = if app.account().settings.wallpaper_image.is_some() {
                crate::i18n::gettext(app.locale, "Image").into_owned()
            } else {
                wallpaper_label(app.locale, app.settings.wallpaper_color_for(palette.dark))
            };
            // What is chosen shows in the accent, as a selected chip does.
            if theme::soft_button(ui, &palette, Some(Icon::ChevronRight), &label, true).clicked() {
                app.actions.push(Action::Open(Page::Wallpaper));
            }
        },
    );
    appearance.block(
        vec![
            translated(locale, "Doodles"),
            translated(locale, "Wallpaper"),
        ],
        doodle_cards,
    );
    appearance.row(
        translated(locale, "Zoom"),
        keyed(translated(
            locale,
            "Ctrl+Plus and Ctrl+Minus work anywhere, and Ctrl+0 resets it.",
        )),
        move |ui, app| {
            if theme::icon_button(
                ui,
                Icon::Plus,
                16.0,
                palette.secondary,
                palette.text,
                &crate::i18n::gettext(app.locale, "Larger"),
            )
            .clicked()
            {
                app.actions.push(Action::ZoomBy(0.1));
            }
            theme::text(
                ui,
                format!("{:.0}%", app.settings.zoom * 100.0),
                theme::medium(13.5),
                palette.text,
            );
            if theme::icon_button(
                ui,
                Icon::Minus,
                16.0,
                palette.secondary,
                palette.text,
                &crate::i18n::gettext(app.locale, "Smaller"),
            )
            .clicked()
            {
                app.actions.push(Action::ZoomBy(-0.1));
            }
        },
    );

    let mut chats = Section::new(translated(locale, "Chats"));
    chats.toggle(
        translated(locale, "Enter sends"),
        keyed(translated(locale, "When off, Ctrl+Enter sends.")),
        |settings| &mut settings.enter_sends,
    );
    chats.account_toggle(
        translated(locale, "Download files automatically"),
        translated(
            locale,
            "Files up to 64 MiB download as they come into view.",
        ),
        |settings| &mut settings.auto_download,
    );
    chats.account_toggle(
        translated(locale, "Fetch older history"),
        translated(
            locale,
            "Asks your phone in the background for older messages in the open, pinned and ten most recent chats.",
        ),
        |settings| &mut settings.history_prefetch,
    );
    // macOS has no public API to pause other apps' media.
    if crate::media_pause::SUPPORTED {
        chats.toggle(
            translated(locale, "Pause other media while recording or playing"),
            translated(locale, "Music and videos in other apps resume afterwards."),
            |settings| &mut settings.pause_other_media,
        );
    }
    chats.row(
        translated(locale, "Keep chats archived"),
        translated(
            locale,
            "When off, a new message brings an archived chat back to the list.",
        ),
        move |ui, app| {
            let mut keep = app.settings.keep_chats_archived;
            let response = widgets::switch(ui, &palette, &mut keep);
            theme::reveal_focus(&response);
            let label = crate::i18n::gettext(app.locale, "Keep chats archived");
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Checkbox,
                    ui.is_enabled(),
                    keep,
                    label.as_ref(),
                )
            });
            if response.changed() {
                app.actions.push(Action::SetKeepChatsArchived(keep));
            }
        },
    );
    chats.row(
        translated(locale, "Locked chats code"),
        translated(
            locale,
            "Opens the Locked tab on this computer. It hides chats, it does not encrypt them.",
        ),
        move |ui, app| {
            // The buffer lives in egui memory: the hash is the only
            // stored form, so there is nothing to read it back from.
            let code_id = egui::Id::new("settings-chat-lock-code");
            let mut code: String = ui.data_mut(|data| data.get_temp(code_id).unwrap_or_default());
            let hint = crate::i18n::gettext(app.locale, "Secret code");
            let response = controls::Field::new(&mut code)
                .hint(&hint)
                .password()
                .show(ui, &palette);
            if response.changed() {
                let trimmed = code.trim().to_owned();
                app.actions.push(Action::SetChatLockCode(Some(trimmed)));
            }
            if app.settings.chat_lock_code_hash.is_some()
                && ui
                    .small_button(crate::i18n::gettext(app.locale, "Clear"))
                    .clicked()
            {
                code.clear();
                app.actions.push(Action::SetChatLockCode(None));
            }
            ui.data_mut(|data| data.insert_temp(code_id, code));
        },
    );

    let mut notifications = Section::new(translated(locale, "Notifications"));
    notifications.account_toggle(
        translated(locale, "Desktop notifications"),
        translated(
            locale,
            "For chats you are not looking at. Muted chats stay quiet.",
        ),
        |settings| &mut settings.notifications,
    );
    if app.account().settings.notifications {
        let (title, description) = sound_text(locale, false);
        notifications.toggle(
            translated(locale, "Show message content in notifications"),
            translated(locale, "When off, they say only “New message”."),
            |settings| &mut settings.notification_content,
        );
        notifications.row(title, description, |ui, app| sound_control(ui, app, false));
        notifications.row(
            translated(locale, "Limit repeat notifications"),
            translated(
                locale,
                "After one notification, a chat stays quiet for 10 minutes. Opening the chat resets this. In groups, mentions and replies to you still notify.",
            ),
            |ui, app| {
                let palette = app.palette;
                let locale = app.locale;
                let selected = app.account().settings.limit_notifications;
                let mut picked = None;
                controls::dropdown(
                    ui,
                    &palette,
                    "limit_notifications",
                    &translated(locale, "Limit repeat notifications").shown,
                    selected.label(locale).as_ref(),
                    controls::WIDTH,
                    |ui| {
                        for limit in crate::settings::NotificationLimit::ALL {
                            if controls::option(
                                ui,
                                &palette,
                                limit.label(locale).as_ref(),
                                limit == selected,
                            ) {
                                picked = Some(limit);
                            }
                        }
                    },
                );
                if let Some(limit) = picked {
                    app.account_mut().settings.limit_notifications = limit;
                    app.account_mut().mark_settings_dirty();
                }
            },
        );
        notifications.toggle(
            translated(locale, "Play sounds for group messages"),
            translated(
                locale,
                "When off, only mentions and replies to you make a sound.",
            ),
            |settings| &mut settings.group_sounds,
        );
        let (title, description) = sound_text(locale, true);
        notifications.row(title, description, |ui, app| sound_control(ui, app, true));
    }

    let voice = voice_section(app);

    let mut privacy = Section::new(translated(locale, "Privacy"));
    let receipts_note = if app.account_receipts_off {
        translated(locale, "Off for your account, so only groups get them.")
    } else {
        translated(locale, "Let people see when you read their messages.")
    };
    privacy.account_toggle(
        translated(locale, "Send read receipts"),
        receipts_note,
        |settings| &mut settings.send_read_receipts,
    );
    privacy.account_toggle(
        translated(locale, "Show when you are typing"),
        "",
        |settings| &mut settings.send_typing,
    );
    app_lock_rows(app, &mut privacy);
    // The account values live on the phone: they are shown once fetched and
    // edited only while connected with a fresh snapshot.
    let editable = app.is_connected() && app.account_privacy.editable();
    let note = if app.account_privacy.fetch_failed {
        Some(crate::i18n::gettext(
            locale,
            "Could not load your account privacy. Trying again when Zeus reconnects.",
        ))
    } else if !app.is_connected() {
        Some(crate::i18n::gettext(
            locale,
            "Connect to WhatsApp to change your account privacy.",
        ))
    } else if !app.account_privacy.loaded {
        Some(crate::i18n::gettext(
            locale,
            "Loading your account privacy…",
        ))
    } else {
        None
    };
    if let Some(note) = note {
        privacy.block(Vec::new(), move |ui, _app| {
            widgets::rich_text(ui, &note, theme::regular(12.5), palette.secondary);
            ui.add_space(10.0);
        });
    }
    for kind in PrivacyKind::ALL {
        let Some(current) = app.account_privacy.get(kind) else {
            continue;
        };
        let title = Text {
            shown: kind.label(locale),
            source: kind.label(Locale::English),
        };
        let mut description = Text {
            shown: kind.hint(locale),
            source: kind.hint(Locale::English),
        };
        // The people an Except list leaves out are chosen on the phone.
        if current == PrivacyChoice::Except {
            let note = translated(locale, "Change who is excluded on your phone.");
            let join = |hint: &str, note: &str| {
                if hint.is_empty() {
                    note.to_owned()
                } else {
                    format!("{hint} {note}")
                }
            };
            description = Text {
                shown: join(&description.shown, &note.shown).into(),
                source: join(&description.source, &note.source).into(),
            };
        }
        privacy.row(title, description, move |ui, app| {
            ui.add_enabled_ui(editable, |ui| privacy_control(ui, app, kind));
        });
    }

    let mut system = Section::new(translated(locale, "System"));
    system.toggle(
        translated(locale, "Keep running when the window closes"),
        keyed(translated(locale, "Quit from the tray or with Ctrl+Q.")),
        |settings| &mut settings.keep_running_in_background,
    );
    if app.start_with_system.is_some() {
        system.row(
            translated(locale, "Start at login"),
            translated(locale, "Starts in the tray, without a window."),
            move |ui, app| {
                let Some(mut enabled) = app.start_with_system else {
                    return;
                };
                let response = widgets::switch(ui, &palette, &mut enabled);
                theme::reveal_focus(&response);
                let label = crate::i18n::gettext(app.locale, "Start at login");
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Checkbox,
                        ui.is_enabled(),
                        enabled,
                        label.as_ref(),
                    )
                });
                if response.changed() {
                    app.actions.push(Action::SetStartWithSystem(enabled));
                }
            },
        );
    }
    // Only a build with a release feed of its own has anything to check.
    if crate::brand::UPDATES {
        system.toggle(
            translated(locale, "Check for updates"),
            translated(
                locale,
                "Asks GitHub once a day, sending only the Zeus version.",
            ),
            |settings| &mut settings.check_for_updates,
        );
        system.toggle(
            translated(locale, "Download updates automatically"),
            translated(
                locale,
                "You still choose when to restart. Package managers and Flatpak update Zeus themselves.",
            ),
            |settings| &mut settings.download_updates_automatically,
        );
    }
    let environment = app
        .settings
        .proxy
        .is_empty()
        .then(crate::proxy::for_whatsapp)
        .flatten();
    let description = match environment {
        Some(proxy) => {
            let redacted = proxy.redacted();
            let text = translated(locale, "Using {proxy} from the environment.");
            Text {
                shown: text.shown.replace("{proxy}", &redacted).into(),
                source: text.source.replace("{proxy}", &redacted).into(),
            }
        }
        None => translated(
            locale,
            "For WhatsApp, media, and updates. Empty uses ALL_PROXY or HTTPS_PROXY.",
        ),
    };
    system.row(translated(locale, "Proxy"), description, move |ui, app| {
        let id = egui::Id::new("settings-proxy-draft");
        let mut draft = ui
            .data(|data| data.get_temp::<String>(id))
            .unwrap_or_else(|| app.settings.proxy.clone());
        let response = controls::Field::new(&mut draft)
            .hint("socks5h://127.0.0.1:9050")
            .show(ui, &palette);
        if response.lost_focus() {
            app.actions.push(Action::SetProxy(draft.clone()));
            ui.data_mut(|data| data.remove::<String>(id));
        } else if response.has_focus() {
            ui.data_mut(|data| data.insert_temp(id, draft));
        }
    });
    system.row(
        translated(locale, "GIPHY API key"),
        if crate::settings::BUILT_IN_GIPHY_KEY.is_some() {
            translated(
                locale,
                "For GIF search. Replaces the key this build includes.",
            )
        } else {
            translated(
                locale,
                "For GIF search. Get a free key at developers.giphy.com.",
            )
        },
        move |ui, app| {
            let response = controls::Field::new(&mut app.settings.giphy_key)
                .hint("GIPHY API key")
                .show(ui, &palette);
            if response.changed() {
                app.actions.push(Action::SettingsChanged);
            }
        },
    );

    let mut account_section = Section::new(translated(locale, "Account"));
    account_section.block(
        vec![
            translated(locale, "Your name"),
            Text {
                shown: crate::i18n::pgettext(locale, "profile", "About"),
                source: "About".into(),
            },
            translated(locale, "Change profile picture"),
            translated(locale, "Unlink this computer"),
            translated(locale, "Add account"),
            translated(locale, "Remove this account"),
        ],
        |ui, app| account(app, ui),
    );

    let mut files = Section::new(translated(locale, "Files"));
    // The account on screen: each number keeps its own archive and media.
    let state = app.account().dirs.state.clone();
    let open_folder = crate::i18n::gettext(locale, "Open folder");
    files.row(
        translated(locale, "Message archive"),
        app.account().dirs.archive_db().display().to_string(),
        {
            let open_folder = open_folder.clone();
            move |ui, app| {
                if theme::soft_button(ui, &palette, Some(Icon::ExternalLink), &open_folder, false)
                    .clicked()
                {
                    app.actions.push(Action::OpenFolder(state));
                }
            }
        },
    );
    let custom = app.settings.download_folder.clone();
    let media = custom
        .clone()
        .unwrap_or_else(|| app.account().dirs.media_cache_dir());
    files.row(
        translated(locale, "Downloads"),
        media.display().to_string(),
        move |ui, app| {
            if theme::soft_button(ui, &palette, Some(Icon::ExternalLink), &open_folder, false)
                .clicked()
            {
                let _ = std::fs::create_dir_all(&media);
                app.actions.push(Action::OpenFolder(media.clone()));
            }
            if theme::soft_button(
                ui,
                &palette,
                None,
                &crate::i18n::gettext(app.locale, "Change…"),
                false,
            )
            .clicked()
            {
                app.actions.push(Action::PickDownloadFolder);
            }
            if custom.is_some()
                && theme::soft_button(
                    ui,
                    &palette,
                    None,
                    &crate::i18n::gettext(app.locale, "Use default"),
                    false,
                )
                .clicked()
            {
                app.actions.push(Action::SetDownloadFolder(None));
            }
        },
    );
    files.block(
        vec![
            translated(locale, "Storage used"),
            translated(locale, "Media cache"),
            translated(locale, "Images"),
            translated(locale, "Videos and GIFs"),
            translated(locale, "Stickers"),
            translated(locale, "Other"),
            translated(locale, "Clear…"),
        ],
        storage_usage,
    );
    let log = app.dirs.log_file();
    files.row(
        translated(locale, "Log"),
        log.display().to_string(),
        move |ui, app| {
            if theme::soft_button(
                ui,
                &palette,
                Some(Icon::FileText),
                &crate::i18n::gettext(app.locale, "Open"),
                false,
            )
            .clicked()
            {
                app.actions.push(Action::OpenLog(log));
            }
        },
    );

    let mut about_section = Section::new(translated(locale, "About"));
    about_section.block(
        vec![
            "Zeus".into(),
            translated(locale, "Keyboard shortcuts"),
            translated(locale, "Source code"),
        ],
        |ui, app| about(app, ui),
    );

    vec![
        appearance,
        chats,
        notifications,
        voice,
        privacy,
        system,
        account_section,
        files,
        about_section,
    ]
}

/// The microphone calls and voice messages use, a check of how it hears the
/// room, and push to talk.
fn voice_section(app: &App) -> Section {
    let locale = app.locale;
    let mut voice = Section::new(translated(locale, "Calls and voice"));
    // A picked microphone the last scan did not find is named, so the reader
    // knows why calls use another one. An empty scan proves nothing.
    let missing = app.settings.microphone.as_ref().filter(|device| {
        app.microphones
            .found
            .as_ref()
            .is_some_and(|found| !found.is_empty() && !found.iter().any(|(id, _)| *id == device.id))
    });
    let description = match missing {
        Some(device) => {
            let text = translated(
                locale,
                "“{name}” is not connected. Zeus uses the system default until it is.",
            );
            Text {
                shown: text.shown.replace("{name}", &device.name).into(),
                source: text.source.replace("{name}", &device.name).into(),
            }
        }
        None => translated(locale, "The input device for calls and voice messages."),
    };
    voice.row(
        translated(locale, "Microphone"),
        description,
        microphone_picker,
    );
    voice.row(
        translated(locale, "Check your microphone"),
        translated(
            locale,
            "Speak and watch the level. Nothing is recorded or sent.",
        ),
        microphone_check,
    );
    voice.row(
        translated(locale, "Push to talk"),
        translated(
            locale,
            "During calls your microphone stays silent until you hold a key, or the talk button on the call screen.",
        ),
        |ui, app| {
            let palette = app.palette;
            let mut on = app.settings.push_to_talk;
            let response = widgets::switch(ui, &palette, &mut on);
            theme::reveal_focus(&response);
            let label = crate::i18n::gettext(app.locale, "Push to talk");
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Checkbox,
                    ui.is_enabled(),
                    on,
                    label.as_ref(),
                )
            });
            if response.changed() {
                app.actions.push(Action::SetPushToTalk(on));
            }
        },
    );
    if app.settings.push_to_talk {
        voice.row(
            translated(locale, "Push to talk key"),
            translated(
                locale,
                "Works while Zeus is in front. While you type a message, use an F key or open the call screen.",
            ),
            push_to_talk_key,
        );
    }
    voice
}

/// The microphone list, with the system default first.
fn microphone_picker(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let default_label = crate::i18n::gettext(locale, "System default").into_owned();
    let saved = app.settings.microphone.clone();
    let selected = match &saved {
        Some(device) => app
            .microphones
            .label(&device.id)
            .unwrap_or(&device.name)
            .to_owned(),
        None => default_label.clone(),
    };
    let found = app.microphones.found.clone().unwrap_or_default();
    let mut picked = None;
    controls::dropdown(
        ui,
        &palette,
        "settings-microphone",
        "Microphone",
        &selected,
        controls::WIDTH,
        |ui| {
            if controls::option(ui, &palette, &default_label, saved.is_none()) {
                picked = Some(None);
            }
            for (id, label) in &found {
                let current = saved.as_ref().is_some_and(|device| device.id == *id);
                if controls::option(ui, &palette, label, current) {
                    picked = Some(Some(crate::settings::SavedDevice {
                        id: id.clone(),
                        name: label.clone(),
                    }));
                }
            }
        },
    );
    let scanning = app.microphones.scanning();
    let refresh = ui
        .add_enabled_ui(!scanning, |ui| {
            theme::icon_button(
                ui,
                Icon::Refresh,
                15.0,
                palette.secondary,
                palette.text,
                &crate::i18n::gettext(locale, "Look for microphones again"),
            )
        })
        .inner;
    if refresh.clicked() {
        app.actions.push(Action::RefreshMicrophones);
    }
    if let Some(device) = picked
        && device != saved
    {
        app.actions.push(Action::SetMicrophone(device));
    }
}

/// Listens to the picked microphone and shows how loud it hears the room.
fn microphone_check(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let in_call = app.call.as_ref().is_some_and(|call| call.phase.is_live());
    let checking = app.mic_check.is_some();
    let label = if checking {
        crate::i18n::gettext(locale, "Stop")
    } else {
        crate::i18n::gettext(locale, "Check")
    };
    let button = ui
        .add_enabled_ui(!in_call, |ui| {
            theme::soft_button(ui, &palette, Some(Icon::Mic), &label, checking)
        })
        .inner
        .on_disabled_hover_text(crate::i18n::gettext(
            locale,
            "The call is using the microphone.",
        ));
    if button.clicked() {
        app.actions.push(Action::CheckMicrophone(!checking));
    }
    let Some(check) = &app.mic_check else {
        return;
    };
    if check.failure().is_some() {
        theme::text(
            ui,
            crate::i18n::gettext(locale, "Could not hear this microphone"),
            theme::medium(12.5),
            palette.danger,
        );
        return;
    }
    if !check.listening() {
        theme::text(
            ui,
            crate::i18n::gettext(locale, "Opening…"),
            theme::medium(12.5),
            palette.secondary,
        );
        return;
    }
    level_meter(ui, &palette, check.level());
}

/// A horizontal bar filled to `level`, from 0 to 1.
fn level_meter(ui: &mut egui::Ui, palette: &Palette, level: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(120.0, 6.0), Sense::hover());
    let radius = CornerRadius::same(3);
    ui.painter()
        .rect_filled(rect, radius, palette.surface_active);
    if level > 0.0 {
        let filled = Rect::from_min_size(rect.min, vec2(rect.width() * level, rect.height()));
        ui.painter().rect_filled(filled, radius, palette.accent);
    }
    let percent = (level * 100.0).round();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::ProgressIndicator,
            true,
            format!("Microphone level {percent}%"),
        )
    });
}

/// The key push to talk listens for; clicked, it waits for the next key.
fn push_to_talk_key(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let capturing = app.push_to_talk.capturing;
    let label = if capturing {
        crate::i18n::gettext(app.locale, "Press a key…").into_owned()
    } else {
        crate::voice_input::key_label(app.settings.push_to_talk_key())
    };
    let response = theme::soft_button(ui, &palette, Some(Icon::Keyboard), &label, capturing);
    theme::reveal_focus(&response);
    if response.clicked() {
        // A flag of this view's own: the next key press is read in `keys`.
        app.push_to_talk.capturing = !capturing;
    }
}

/// The app lock: a password, how long ZapFast may go unused, and the form
/// that sets, changes, or removes the password.
fn app_lock_rows(app: &App, privacy: &mut Section) {
    use crate::app_lock::FormMode;
    let locale = app.locale;
    let palette = app.palette;
    let enabled = app.settings.app_lock_hash.is_some();
    privacy.row(
        translated(locale, "App lock"),
        translated(
            locale,
            "Asks for a password at start and after a while unused. It keeps people using this computer out of your chats and encrypts nothing more. A forgotten password means unlinking.",
        ),
        move |ui, app| {
            use crate::i18n::gettext;
            // The row lays its controls out from the right.
            let modes = if enabled {
                vec![
                    (FormMode::TurnOff, gettext(app.locale, "Turn off…")),
                    (FormMode::Change, gettext(app.locale, "Change password…")),
                ]
            } else {
                vec![(FormMode::Set, gettext(app.locale, "Set password…"))]
            };
            for (mode, label) in modes {
                if theme::soft_button(ui, &palette, None, &label, false).clicked() {
                    app.actions.push(Action::AppLockForm(Some(mode)));
                }
            }
        },
    );
    if app.app_lock.form.is_some() {
        // Found by the same words as its row, so a search keeps them together.
        privacy.block(
            vec![
                translated(locale, "App lock"),
                translated(locale, "Lock after"),
            ],
            app_lock_form,
        );
    }
    if enabled {
        privacy.row(
            translated(locale, "Lock after"),
            keyed(translated(
                locale,
                "Time without using Zeus, also counted while it is in the tray. Ctrl+Shift+L locks it at once.",
            )),
            move |ui, app| {
                let selected = app.settings.app_lock_after;
                let locale = app.locale;
                controls::dropdown(
                    ui,
                    &palette,
                    "app_lock_after",
                    &translated(locale, "Lock after").shown,
                    selected.label(locale).as_ref(),
                    controls::WIDTH,
                    |ui| {
                        for after in crate::settings::AutoLock::ALL {
                            if controls::option(
                                ui,
                                &palette,
                                after.label(locale).as_ref(),
                                after == selected,
                            ) {
                                app.actions.push(Action::SetAutoLock(after));
                            }
                        }
                    },
                );
            },
        );
    }
}

/// The password form under the app lock row. Its fields belong to the view;
/// the passwords leave them only for the checking thread.
fn app_lock_form(ui: &mut egui::Ui, app: &mut App) {
    use crate::app_lock::{FormError, FormMode, MIN_PASSWORD_CHARS};
    use crate::i18n::gettext;
    let palette = app.palette;
    let locale = app.locale;
    let Some(form) = app.app_lock.form.as_mut() else {
        return;
    };
    let mode = form.mode;
    let busy = form.busy;
    let mut submit = false;
    ui.add_space(4.0);
    theme::text(
        ui,
        match mode {
            FormMode::Set => gettext(locale, "Set an app lock password"),
            FormMode::Change => gettext(locale, "Change the app lock password"),
            FormMode::TurnOff => gettext(locale, "Turn off the app lock"),
        },
        theme::semibold(14.0),
        palette.text,
    );
    let count = MIN_PASSWORD_CHARS.to_string();
    let mut fields: Vec<(&mut String, String, &'static str)> = Vec::new();
    if mode != FormMode::Set {
        fields.push((
            &mut form.current,
            gettext(locale, "Current password").into_owned(),
            "app-lock-current",
        ));
    }
    if mode != FormMode::TurnOff {
        fields.push((
            &mut form.new,
            gettext(locale, "New password, at least {count} characters").replace("{count}", &count),
            "app-lock-new",
        ));
        fields.push((
            &mut form.confirm,
            gettext(locale, "Type the new password again").into_owned(),
            "app-lock-confirm",
        ));
    }
    let focus_first = ui.memory(|memory| memory.focused().is_none());
    for (index, (text, hint, id)) in fields.into_iter().enumerate() {
        let id = egui::Id::new(id);
        // TextEdit surrenders focus on Enter; take the key before drawing it.
        submit |= ui.memory(|memory| memory.has_focus(id))
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let response = ui
            .add_enabled_ui(!busy, |ui| {
                controls::Field::new(text)
                    .id(id)
                    .password()
                    .hint(&hint)
                    .width(320.0)
                    .show(ui, &palette)
            })
            .inner;
        if index == 0 && focus_first && !busy {
            response.request_focus();
        }
    }
    let error = form.error.map(|error| match error {
        FormError::TooShort => gettext(locale, "The password needs at least {count} characters.")
            .replace("{count}", &count),
        FormError::Mismatch => gettext(locale, "The two new passwords are different.").into_owned(),
        FormError::WrongCurrent => gettext(locale, "Wrong password. Try again.").into_owned(),
    });
    if let Some(error) = error {
        widgets::rich_text(ui, &error, theme::regular(12.5), palette.danger);
    }
    ui.horizontal(|ui| {
        let confirm = match mode {
            FormMode::Set => gettext(locale, "Turn on"),
            FormMode::Change => gettext(locale, "Change password"),
            FormMode::TurnOff => gettext(locale, "Turn off"),
        };
        submit |= ui
            .add_enabled_ui(!busy, |ui| theme::pill_button(ui, &palette, &confirm, true))
            .inner
            .clicked();
        if busy {
            theme::spinner(ui, 16.0, palette.accent);
        } else if theme::pill_button(ui, &palette, &gettext(locale, "Cancel"), false).clicked() {
            app.actions.push(Action::AppLockForm(None));
        }
    });
    ui.add_space(10.0);
    if submit && !busy {
        app.actions.push(Action::SubmitAppLockForm);
    }
}

/// A translated description that names keys, with Cmd and Option on macOS.
/// The search still finds it by its English source.
fn keyed(text: Text) -> Text {
    Text {
        shown: super::keys::label(&text.shown).into(),
        source: text.source,
    }
}

/// A title and description over a control laid out below them, for settings
/// too wide to sit beside their name.
fn block_heading(ui: &mut egui::Ui, palette: &Palette, title: &str, description: &str) {
    if !title.is_empty() {
        widgets::rich_text(ui, title, theme::medium(14.0), palette.text);
    }
    if !description.is_empty() {
        let description = widgets::line(
            ui,
            description,
            theme::regular(12.5),
            palette.secondary,
            ui.available_width(),
            usize::MAX,
        );
        let (rect, _) = ui.allocate_exact_size(description.size(), Sense::hover());
        if ui.is_rect_visible(rect) {
            description.paint(ui, rect.min, palette.secondary);
        }
    }
    ui.add_space(10.0);
}

/// How tall the live preview of a conversation is.
const PREVIEW_HEIGHT: f32 = 248.0;
/// The room between the preview's edge and what is drawn in it.
const PREVIEW_PAD: f32 = 20.0;

/// A small conversation drawn in the colours on screen, so a change of
/// theme, mode, accent or blur shows here as it does in the chats.
fn chat_preview(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), PREVIEW_HEIGHT), Sense::hover());
    ui.add_space(14.0);
    if !ui.is_rect_visible(rect) {
        return;
    }
    let text = |source: &'static str| crate::i18n::gettext(locale, source).into_owned();
    let painter = ui.painter_at(rect);
    let radius = f32::from(super::chats::CARD_RADIUS);
    painter.rect_filled(rect, radius, app.wallpaper().color);
    // The header, as the chat draws it: solid, or a veil over the messages
    // easing out under it.
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 58.0));
    let header_fill = if palette.vibrant() {
        palette.vibrant_header(palette.chat)
    } else {
        palette.panel
    };
    painter.rect_filled(
        header,
        CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: 0,
            se: 0,
        },
        header_fill,
    );
    let avatar = pos2(header.left() + PREVIEW_PAD + 14.0, header.center().y);
    painter.circle_filled(avatar, 14.0, palette.avatar(0.58));
    painter.text(
        avatar,
        egui::Align2::CENTER_CENTER,
        "A",
        theme::semibold(13.0),
        Color32::WHITE,
    );
    painter.text(
        pos2(avatar.x + 22.0, header.center().y - 8.0),
        egui::Align2::LEFT_CENTER,
        "Ava",
        theme::semibold(13.5),
        palette.text,
    );
    painter.text(
        pos2(avatar.x + 22.0, header.center().y + 8.0),
        egui::Align2::LEFT_CENTER,
        text("online"),
        theme::regular(11.5),
        palette.secondary,
    );

    // Messages: theirs on the left, ours on the right with its ticks.
    let bubble = |top: f32, label: String, own: bool| -> f32 {
        let colours = palette.on_bubble(own);
        let galley = painter.layout_no_wrap(label, theme::regular(13.0), colours.text);
        let meta = if own { 30.0 } else { 0.0 };
        let size = galley.size() + vec2(28.0 + meta, 18.0);
        let left = if own {
            rect.right() - PREVIEW_PAD - size.x
        } else {
            rect.left() + PREVIEW_PAD
        };
        let shape = Rect::from_min_size(pos2(left, top), size);
        let fill = if own {
            palette.bubble_out
        } else {
            palette.bubble_in
        };
        painter.rect_filled(shape, theme::RADIUS, fill);
        painter.galley(shape.min + vec2(14.0, 9.0), galley, colours.text);
        if own {
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
        shape.bottom()
    };
    let mut y = header.bottom() + 16.0;
    y = bubble(y, text("Did you see the new themes?"), false) + 6.0;
    y = bubble(y, text("Trying them all right now"), true) + 6.0;
    bubble(y, text("Pick one with a light mode too"), false);

    // The message field, on glass or a solid pill.
    let field = Rect::from_min_max(
        pos2(
            rect.left() + PREVIEW_PAD - 4.0,
            rect.bottom() - PREVIEW_PAD - 30.0,
        ),
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
        egui::Align2::LEFT_CENTER,
        text("Message"),
        theme::regular(13.0),
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

/// Seconds the pill's fill takes to reach a new segment.
const SLIDE_SECONDS: f64 = 0.22;

/// Where the fill is, in segments, `target` being where it is going: it
/// leaves where it was drawn last and eases out to `target`.
fn slide_position(ui: &egui::Ui, id: egui::Id, target: f32) -> f32 {
    let now = ui.input(|input| input.time);
    let (from, to, since) = ui
        .data(|data| data.get_temp::<(f32, f32, f64)>(id))
        .unwrap_or((target, target, now - SLIDE_SECONDS));
    let progress = (((now - since) / SLIDE_SECONDS) as f32).clamp(0.0, 1.0);
    let here = from + (to - from) * egui::emath::easing::cubic_out(progress);
    let (from, to, since) = if to == target {
        (from, to, since)
    } else {
        (here, target, now)
    };
    ui.data_mut(|data| data.insert_temp(id, (from, to, since)));
    if progress < 1.0 || to != here {
        ui.ctx().request_repaint();
    }
    if to == target && progress >= 1.0 {
        target
    } else {
        from + (to - from) * egui::emath::easing::cubic_out(((now - since) / SLIDE_SECONDS) as f32)
    }
}

/// Options as segments of one pill, the selected one filled with the
/// accent. Returns the option clicked.
fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    palette: &Palette,
    salt: &str,
    options: &[(T, std::borrow::Cow<'_, str>)],
    selected: Option<T>,
    width: f32,
) -> Option<T> {
    let segment = vec2(width, 30.0);
    let (outer, _) = ui.allocate_exact_size(
        vec2(segment.x * options.len() as f32 + 6.0, segment.y + 6.0),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(outer, outer.height() / 2.0, palette.surface);
    // The accent fill glides to the chosen segment.
    let slide = selected
        .and_then(|value| options.iter().position(|(option, _)| *option == value))
        .map(|index| slide_position(ui, ui.id().with((salt, "slide")), index as f32));
    if let Some(position) = slide {
        let rect = Rect::from_min_size(outer.min + vec2(3.0 + segment.x * position, 3.0), segment);
        ui.painter()
            .rect_filled(rect, rect.height() / 2.0, palette.accent);
    }
    let mut clicked = None;
    for (index, (value, label)) in options.iter().enumerate() {
        let rect = Rect::from_min_size(
            outer.min + vec2(3.0 + segment.x * index as f32, 3.0),
            segment,
        );
        let response = ui.interact(rect, ui.id().with((salt, index)), Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
        let on = selected == Some(*value);
        let color = if slide.is_some_and(|position| (position - index as f32).abs() < 0.5) {
            palette.on_accent
        } else if response.hovered() {
            palette.text
        } else {
            palette.secondary
        };
        if !on && response.hovered() {
            ui.painter()
                .rect_filled(rect, rect.height() / 2.0, palette.surface_hover);
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label.as_ref(),
            theme::medium(13.0),
            color,
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                on,
                label.as_ref(),
            )
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            clicked = Some(*value);
        }
    }
    clicked
}

/// The Halloween look's switch. The colours change from where it was clicked,
/// as a theme card's do.
fn halloween_switch(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let label = crate::i18n::gettext(app.locale, "Halloween").into_owned();
    let mut on = app.settings.halloween;
    let response = widgets::switch(ui, &palette, &mut on);
    theme::reveal_focus(&response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), on, &label)
    });
    if response.changed() {
        if let Some(origin) = ui.ctx().pointer_interact_pos() {
            app.actions.push(Action::RevealThemeFrom(origin));
        }
        app.actions.push(Action::SetHalloween(on));
    }
}

/// The switch of one part of the Halloween look.
fn halloween_part_switch(ui: &mut egui::Ui, app: &mut App, part: crate::settings::HalloweenPart) {
    let palette = app.palette;
    let label = crate::i18n::gettext(app.locale, part.label()).into_owned();
    let parts = app.settings.halloween_parts;
    let mut on = match part {
        crate::settings::HalloweenPart::Colors => parts.colors,
        crate::settings::HalloweenPart::Wallpaper => parts.wallpaper,
        crate::settings::HalloweenPart::Doodles => parts.doodles,
        crate::settings::HalloweenPart::Icons => parts.icons,
        crate::settings::HalloweenPart::Sounds => parts.sounds,
    };
    let response = widgets::switch(ui, &palette, &mut on);
    theme::reveal_focus(&response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), on, &label)
    });
    if response.changed() {
        if let Some(origin) = ui.ctx().pointer_interact_pos() {
            app.actions.push(Action::RevealThemeFrom(origin));
        }
        app.actions.push(Action::SetHalloweenPart(part, on));
    }
}

/// Custom or Themes: a look made your own, or a ready-made theme.
fn style_picker(ui: &mut egui::Ui, app: &mut App) {
    let options: Vec<_> = Style::ALL
        .into_iter()
        .map(|style| (style, crate::i18n::gettext(app.locale, style.label())))
        .collect();
    let selected = Some(app.settings.style());
    if let Some(style) = segmented(ui, &app.palette, "style", &options, selected, 88.0) {
        if let Some(origin) = ui.ctx().pointer_interact_pos() {
            app.actions.push(Action::RevealThemeFrom(origin));
        }
        app.actions.push(Action::SetStyle(style));
    }
}

/// Light, dark or system, as three segments of one control.
fn mode_picker(ui: &mut egui::Ui, app: &mut App) {
    let custom = app.settings.custom_theme.is_some();
    let selected = (!custom).then_some(app.settings.mode);
    let options: Vec<_> = Mode::ALL
        .into_iter()
        .map(|mode| (mode, crate::i18n::gettext(app.locale, mode.label())))
        .collect();
    if let Some(mode) = segmented(ui, &app.palette, "mode", &options, selected, 72.0) {
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

/// How wide an accent swatch is.
const SWATCH: f32 = 26.0;

/// The accent swatches: the theme's own, then the colours.
fn accent_picker(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let chat_theme = app.settings.chat_theme;
    let custom = app.settings.custom_theme.is_some();
    let description = if !chat_theme.takes_accent() {
        crate::i18n::gettext(locale, "Messages keeps the system blue.")
    } else {
        crate::i18n::gettext(
            locale,
            "Buttons, links, ticks and your own messages take this colour.",
        )
    };
    block_heading(
        ui,
        &palette,
        &crate::i18n::gettext(locale, "Accent"),
        &description,
    );
    let enabled = !custom && chat_theme.takes_accent();
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for accent in Accent::ALL {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(SWATCH + 6.0), Sense::click());
                theme::reveal_focus(&response);
                theme::focus_outline(ui, response.id, rect, rect.height() / 2.0);
                let on = enabled && app.settings.accent == accent;
                // The theme's own accent, in the mode on screen.
                let color = accent.rgb().map_or_else(
                    || chat_theme.palette(palette.dark, Accent::Theme).accent,
                    |[r, g, b]| Color32::from_rgb(r, g, b),
                );
                let color = if enabled {
                    color
                } else {
                    color.gamma_multiply(0.4)
                };
                let center = rect.center();
                if on {
                    ui.painter().circle_stroke(
                        center,
                        SWATCH / 2.0 + 2.5,
                        Stroke::new(2.0, palette.text),
                    );
                } else if response.hovered() {
                    ui.painter().circle_stroke(
                        center,
                        SWATCH / 2.0 + 2.5,
                        Stroke::new(1.0, palette.secondary),
                    );
                }
                ui.painter().circle_filled(center, SWATCH / 2.0, color);
                if accent == Accent::Theme {
                    // The theme's own: its accent ringed round its canvas.
                    let own = chat_theme.palette(palette.dark, Accent::Theme);
                    ui.painter().circle_filled(center, SWATCH * 0.22, own.panel);
                    ui.painter().circle_stroke(
                        center,
                        SWATCH / 2.0,
                        Stroke::new(1.0, palette.outline),
                    );
                }
                if on && accent != Accent::Theme {
                    let tick = theme::readable_on(color, Color32::WHITE, Color32::BLACK, 3.0);
                    theme::paint_icon(
                        ui,
                        Icon::Check,
                        Rect::from_center_size(center, Vec2::splat(14.0)),
                        14.0,
                        tick,
                    );
                }
                let label = crate::i18n::gettext(locale, accent.label());
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
                    app.actions.push(Action::SetAccent(accent));
                }
            }
        });
    });
    ui.add_space(16.0);
}

/// The least width of a theme card.
const CARD_WIDTH: f32 = 128.0;
/// How tall the coloured part of a theme card is.
const CARD_PREVIEW: f32 = 62.0;
/// How tall the name under it is.
const CARD_LABEL: f32 = 30.0;
/// The space between cards.
const CARD_GAP: f32 = 10.0;

/// The chat theme cards for `choices`, each in the mode on screen with the
/// accent it would take.
fn chat_theme_cards(ui: &mut egui::Ui, app: &mut App, salt: &str, choices: &[ChatTheme]) {
    let palette = app.palette;
    let custom = app.settings.custom_theme.is_some();
    let cards: Vec<(String, Palette, bool, Action)> = choices
        .iter()
        .map(|&choice| {
            (
                choice.label().to_owned(),
                choice.palette(palette.dark, app.settings.accent),
                !custom && app.settings.chat_theme == choice,
                Action::SetChatTheme(choice),
            )
        })
        .collect();
    card_grid(ui, app, salt, cards);
}

/// The looks the Custom style starts from.
fn look_cards(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    block_heading(
        ui,
        &palette,
        &crate::i18n::gettext(app.locale, "Look"),
        &crate::i18n::gettext(
            app.locale,
            "Zeus's monochrome glass, WhatsApp's greens, or the bubbles of Messages.",
        ),
    );
    ui.add_space(2.0);
    chat_theme_cards(ui, app, "look", &ChatTheme::LOOKS);
    ui.add_space(4.0);
}

/// The ready-made themes as cards, each in the mode on screen.
fn theme_cards(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    block_heading(
        ui,
        &palette,
        &crate::i18n::gettext(app.locale, "Themes"),
        &crate::i18n::gettext(
            app.locale,
            "Ported from Agent Orchestrator, each with its own accent.",
        ),
    );
    ui.add_space(2.0);
    let themes: Vec<ChatTheme> = ChatTheme::ALL
        .into_iter()
        .filter(|choice| !choice.is_look())
        .collect();
    chat_theme_cards(ui, app, "chat-theme", &themes);
    ui.add_space(4.0);
}

/// Theme cards in as many columns as fit, widened to fill the row.
fn card_grid(
    ui: &mut egui::Ui,
    app: &mut App,
    salt: &str,
    cards: Vec<(String, Palette, bool, Action)>,
) {
    let width = ui.available_width();
    // A short row, like the three looks, widens to fill the line.
    let columns = (((width + CARD_GAP) / (CARD_WIDTH + CARD_GAP)).floor() as usize)
        .min(cards.len())
        .max(1);
    let card = vec2(
        (width - CARD_GAP * (columns - 1) as f32) / columns as f32,
        CARD_PREVIEW + CARD_LABEL,
    );
    let rows = cards.len().div_ceil(columns);
    let (area, _) = ui.allocate_exact_size(
        vec2(
            width,
            rows as f32 * card.y + rows.saturating_sub(1) as f32 * CARD_GAP,
        ),
        Sense::hover(),
    );
    ui.add_space(12.0);
    for (index, (label, colours, selected, action)) in cards.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            area.min
                + vec2(
                    (index % columns) as f32 * (card.x + CARD_GAP),
                    (index / columns) as f32 * (card.y + CARD_GAP),
                ),
            card,
        );
        let response = ui.interact(rect, ui.id().with((salt, index)), Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, f32::from(theme::RADIUS_CARD));
        if ui.is_rect_visible(rect) {
            theme_card(
                ui,
                &app.palette,
                rect,
                &label,
                &colours,
                selected,
                response.hovered(),
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                selected,
                &label,
            )
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.actions.push(Action::RevealThemeFrom(rect.center()));
            app.actions.push(action);
        }
    }
}

/// One theme card: its conversation with a bubble each way, over its name
/// on its own list colour, with its accent beside the name.
fn theme_card(
    ui: &egui::Ui,
    palette: &Palette,
    rect: Rect,
    label: &str,
    colours: &Palette,
    selected: bool,
    hovered: bool,
) {
    let painter = ui.painter_at(rect.expand(3.0));
    let radius = f32::from(theme::RADIUS_CARD);
    let r = radius as u8;
    let preview = Rect::from_min_size(rect.min, vec2(rect.width(), CARD_PREVIEW));
    let footer = Rect::from_min_max(pos2(rect.left(), preview.bottom()), rect.max);
    painter.rect_filled(
        preview,
        CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        colours.chat,
    );
    painter.rect_filled(
        footer,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: r,
            se: r,
        },
        colours.panel,
    );
    let inner = preview.shrink2(vec2(10.0, 11.0));
    let line = 12.0;
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
    let dot = pos2(footer.right() - 14.0, footer.center().y);
    painter.circle_filled(dot, 5.0, colours.accent);
    let name = widgets::line(
        ui,
        label,
        theme::medium(12.5),
        colours.text,
        footer.width() - 36.0,
        1,
    );
    name.paint(
        ui,
        pos2(
            footer.left() + 10.0,
            footer.center().y - name.size().y / 2.0,
        ),
        colours.text,
    );
    let edge = if selected {
        Stroke::new(2.0, palette.accent)
    } else if hovered {
        Stroke::new(1.0, palette.secondary)
    } else {
        Stroke::new(1.0, palette.outline)
    };
    painter.rect_stroke(rect, radius, edge, egui::StrokeKind::Outside);
}

/// The interface language menu.
fn font_picker(ui: &mut egui::Ui, app: &mut App) {
    use crate::settings::FontChoice;
    let palette = app.palette;
    let selected = app.settings.font;
    // Font family names stay in their own branding.
    let label = |choice: FontChoice| match choice {
        FontChoice::System => crate::i18n::gettext(app.locale, choice.label()).into_owned(),
        _ => choice.label().to_owned(),
    };
    controls::dropdown(
        ui,
        &palette,
        "interface_font",
        "Font",
        &label(selected),
        controls::WIDTH,
        |ui| {
            for choice in FontChoice::ALL {
                if controls::option(ui, &palette, &label(choice), selected == choice) {
                    app.actions.push(Action::SetFont(choice));
                }
            }
        },
    );
}

fn language_picker(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let selected = app.settings.interface_language;
    let label = match selected {
        Some(locale) => locale.label().to_owned(),
        None => crate::i18n::gettext(app.locale, "Auto").into_owned(),
    };
    controls::dropdown(
        ui,
        &palette,
        "interface_language",
        "Language",
        &label,
        controls::WIDTH,
        |ui| {
            if controls::option(
                ui,
                &palette,
                crate::i18n::gettext(app.locale, "Auto").as_ref(),
                selected.is_none(),
            ) {
                app.actions.push(Action::SetInterfaceLanguage(None));
            }
            for locale in Locale::ALL {
                if controls::option(ui, &palette, locale.label(), selected == Some(locale)) {
                    app.actions.push(Action::SetInterfaceLanguage(Some(locale)));
                }
            }
        },
    );
}

/// Wallpaper colour picker and live preview.
pub fn wallpaper_show(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let body_height = ui.available_height().max(0.0);
    ui.with_layout(
        Layout::left_to_right(Align::Min).with_main_align(Align::Min),
        |ui| {
            const HEADER_HEIGHT: f32 = 52.0;
            const MIN_PREVIEW_WIDTH: f32 = 180.0;
            let total_width = ui.available_width();
            let left_width = (total_width * 0.42)
                .clamp(220.0, 520.0)
                .min((total_width - MIN_PREVIEW_WIDTH).max(0.0));
            let palette_width = (left_width - 40.0).max(0.0);
            let preview_width = (total_width - left_width).max(0.0);
            ui.allocate_ui_with_layout(
                vec2(left_width, body_height),
                Layout::top_down(Align::Min).with_main_align(Align::Min),
                |ui| {
                    let section = ui.max_rect();
                    let header = Rect::from_min_size(
                        section.left_top(),
                        vec2(left_width, HEADER_HEIGHT),
                    );
                    ui.painter().rect_filled(header, 0.0, palette.panel);
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(header)
                            .layout(
                                Layout::left_to_right(Align::Center)
                                    .with_main_align(Align::Min),
                            ),
                        |ui| {
                            ui.add_space(24.0);
                            if theme::icon_button(
                                ui,
                                Icon::ArrowLeft,
                                20.0,
                                palette.secondary,
                                palette.text,
                                "Back to settings",
                            )
                            .clicked()
                            {
                                app.actions.push(Action::Open(Page::Settings));
                            }
                            theme::text(ui, "Set chat wallpaper", theme::bold(18.0), palette.text);
                        },
                    );

                    let palette_rect = Rect::from_min_size(
                        pos2(section.left() + 20.0, header.bottom() + 20.0),
                        vec2(palette_width, (body_height - HEADER_HEIGHT - 20.0).max(0.0)),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(palette_rect)
                            .layout(Layout::top_down(Align::Min).with_main_align(Align::Min)),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("wallpaper-palette")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    block_heading(
                                        ui,
                                        &palette,
                                        &crate::i18n::gettext(app.locale, "Image"),
                                        &crate::i18n::gettext(
                                            app.locale,
                                            "Your own picture replaces the colour and doodles.",
                                        ),
                                    );
                                    ui.add_space(8.0);
                                    image_buttons(app, ui);
                                    ui.add_space(22.0);
                                    block_heading(
                                        ui,
                                        &palette,
                                        &crate::i18n::gettext(app.locale, "Colour"),
                                        &crate::i18n::gettext(
                                            app.locale,
                                            "Pick the background. Doodles are chosen under Appearance.",
                                        ),
                                    );
                                    ui.add_space(10.0);
                                    let selected = app.settings.wallpaper_color_for(palette.dark);
                                    let gap = 10.0;
                                    let columns = ((palette_width + gap) / (COLOR_SWATCH + gap))
                                        .floor()
                                        .max(1.0)
                                        as usize;
                                    let choices = WallpaperColor::choices(palette.dark);
                                    let rows = choices.len().div_ceil(columns);
                                    let (area, _) = ui.allocate_exact_size(
                                        vec2(
                                            palette_width,
                                            rows as f32 * (COLOR_SWATCH + gap) - gap,
                                        ),
                                        Sense::hover(),
                                    );
                                    for (index, color) in choices.iter().enumerate() {
                                        let rect = Rect::from_min_size(
                                            area.min
                                                + vec2(
                                                    (index % columns) as f32 * (COLOR_SWATCH + gap),
                                                    (index / columns) as f32 * (COLOR_SWATCH + gap),
                                                ),
                                            Vec2::splat(COLOR_SWATCH),
                                        );
                                        if wallpaper_color_button(
                                            ui,
                                            &palette,
                                            app.locale,
                                            rect,
                                            *color,
                                            selected,
                                        ) {
                                            app.actions.push(Action::SetWallpaperColor(*color));
                                        }
                                    }
                                    ui.add_space(20.0);
                                });
                        },
                    );
                },
            );
            let divider_x = ui.cursor().left();
            let divider_top = ui.cursor().top();
            ui.allocate_ui_with_layout(
                vec2(preview_width, body_height),
                Layout::top_down(Align::Min).with_main_align(Align::Min),
                |ui| {
                    let section = ui.max_rect();
                    let header = Rect::from_min_size(
                        section.left_top(),
                        vec2(preview_width, HEADER_HEIGHT),
                    );
                    ui.painter().rect_filled(header, 0.0, palette.panel);
                    ui.painter().text(
                        header.center(),
                        egui::Align2::CENTER_CENTER,
                        "Wallpaper preview",
                        theme::bold(18.0),
                        palette.text,
                    );
                    let preview = Rect::from_min_size(
                        header.left_bottom(),
                        vec2(preview_width, (body_height - HEADER_HEIGHT).max(0.0)),
                    );
                    // The same look the chat draws, image and theme colour included.
                    wallpaper::paint_rect(ui, preview, &app.wallpaper());
                    wallpaper_sample_bubbles(ui, &palette, preview);
                },
            );
            ui.painter().line_segment(
                [
                    pos2(divider_x, divider_top),
                    pos2(divider_x, divider_top + body_height),
                ],
                Stroke::new(1.0, palette.outline),
            );
        },
    );
}

/// "Choose image…", and "Remove image" while one is set. An image replaces
/// the colour and doodles in the chat.
fn image_buttons(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let choose = crate::i18n::gettext(app.locale, "Choose image…");
    let remove = crate::i18n::gettext(app.locale, "Remove image");
    let has_image = app.account().settings.wallpaper_image.is_some();
    ui.horizontal(|ui| {
        if theme::soft_button(ui, &palette, Some(Icon::Image), &choose, has_image).clicked() {
            app.actions.push(Action::PickWallpaperImage);
        }
        if has_image
            && theme::soft_button(ui, &palette, Some(Icon::Trash), &remove, false).clicked()
        {
            app.actions.push(Action::RemoveWallpaperImage);
        }
    });
}

/// The two sample bubbles the wallpaper preview shows over the wallpaper.
fn wallpaper_sample_bubbles(ui: &egui::Ui, palette: &Palette, preview: Rect) {
    let painter = ui.painter_at(preview);
    let radius = f32::from(theme::RADIUS_MENU);
    let width = (preview.width() * 0.5).clamp(110.0, 260.0);
    let theirs = Rect::from_min_size(preview.left_top() + vec2(24.0, 28.0), vec2(width, 40.0));
    painter.rect_filled(theirs, radius, palette.bubble_in);
    let ours = Rect::from_min_size(
        pos2(preview.right() - 24.0 - width * 0.8, theirs.bottom() + 10.0),
        vec2(width * 0.8, 40.0),
    );
    painter.rect_filled(ours, radius, palette.bubble_out);
    for (bubble, fraction) in [(theirs, 0.6), (ours, 0.5)] {
        let line = Rect::from_center_size(bubble.center(), vec2(bubble.width() * fraction, 6.0));
        painter.rect_filled(line, 3.0, palette.text.gamma_multiply(0.35));
    }
}

/// How wide and tall a wallpaper colour swatch is.
const COLOR_SWATCH: f32 = 68.0;

/// A wallpaper colour's name in the interface language.
fn wallpaper_label(locale: Locale, color: WallpaperColor) -> String {
    match color {
        WallpaperColor::Theme => {
            crate::i18n::pgettext(locale, "wallpaper colour", "Theme").into_owned()
        }
        color => color.label().to_owned(),
    }
}

/// One colour swatch in `rect`, ringed in the accent with a tick once chosen.
fn wallpaper_color_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: Locale,
    rect: Rect,
    color: WallpaperColor,
    selected: WallpaperColor,
) -> bool {
    let fill = color.color32(palette);
    let label = wallpaper_label(locale, color);
    let chosen = color == selected;
    let response = ui.interact(
        rect,
        ui.id().with(("wallpaper-swatch", color.label())),
        Sense::click(),
    );
    theme::reveal_focus(&response);
    let radius = f32::from(theme::RADIUS_CARD);
    theme::focus_outline(ui, response.id, rect, radius);
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_with_time_and_easing(
            response.id,
            chosen,
            0.16,
            egui::emath::easing::quadratic_out,
        );
        let painter = ui.painter_at(rect.expand(6.0));
        painter.rect_filled(rect, radius, fill);
        // Theme follows the palette, so its swatch names itself.
        if color == WallpaperColor::Theme {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &label,
                theme::medium(13.0),
                palette.text,
            );
        }
        // Otherwise a swatch of the panel's own colour vanishes into it.
        let edge = if response.hovered() {
            Stroke::new(1.5, palette.secondary)
        } else {
            Stroke::new(1.0, palette.outline)
        };
        painter.rect_stroke(rect, radius, edge, egui::StrokeKind::Inside);
        if t > 0.0 {
            painter.rect_stroke(
                rect.expand(3.0),
                radius + 3.0,
                Stroke::new(2.5, palette.accent.gamma_multiply(t)),
                egui::StrokeKind::Outside,
            );
            let badge = Rect::from_center_size(
                rect.right_bottom() - vec2(14.0, 14.0),
                Vec2::splat(22.0 * t),
            );
            painter.circle_filled(badge.center(), badge.width() / 2.0, palette.accent);
            if t > 0.6 {
                theme::paint_icon(ui, Icon::Check, badge, 14.0, palette.on_accent);
            }
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            chosen,
            label.as_str(),
        )
    });
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label.as_str())
        .clicked()
}

/// The doodle patterns, as tiles of the wallpaper each draws, and none.
fn doodle_cards(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    block_heading(
        ui,
        &palette,
        &crate::i18n::gettext(app.locale, "Doodles"),
        &crate::i18n::gettext(
            app.locale,
            "The pattern drawn over the wallpaper colour in your chats.",
        ),
    );
    ui.add_space(2.0);
    let look = app.wallpaper();
    let shown = app.settings.show_wallpaper;
    let current = app.settings.doodle_style;
    let width = ui.available_width();
    let gap = 10.0;
    let tiles = DoodleStyle::ALL.len() + 1;
    let columns = (((width + gap) / (DOODLE_TILE + gap)).floor() as usize).clamp(1, tiles);
    let tile = vec2(
        (width - gap * (columns - 1) as f32) / columns as f32,
        DOODLE_PREVIEW + CARD_LABEL,
    );
    let rows = tiles.div_ceil(columns);
    ui.add_space(12.0);
    let (area, _) = ui.allocate_exact_size(
        vec2(width, rows as f32 * (tile.y + gap) - gap),
        Sense::hover(),
    );
    let radius = f32::from(theme::RADIUS_CARD);
    for index in 0..tiles {
        // The first tile is none; the rest follow DoodleStyle::ALL.
        let style = index.checked_sub(1).map(|index| DoodleStyle::ALL[index]);
        let chosen = match style {
            None => !shown,
            Some(style) => shown && style == current,
        };
        let label = match style {
            None => crate::i18n::pgettext(app.locale, "doodles", "None").into_owned(),
            Some(style) => crate::i18n::gettext(app.locale, style.label()).into_owned(),
        };
        let rect = Rect::from_min_size(
            area.min
                + vec2(
                    (index % columns) as f32 * (tile.x + gap),
                    (index / columns) as f32 * (tile.y + gap),
                ),
            tile,
        );
        let response = ui.interact(rect, ui.id().with(("doodle-tile", index)), Sense::click());
        theme::reveal_focus(&response);
        theme::focus_outline(ui, response.id, rect, radius);
        if ui.is_rect_visible(rect) {
            let t = ui.ctx().animate_bool_with_time_and_easing(
                response.id,
                chosen,
                0.16,
                egui::emath::easing::quadratic_out,
            );
            let painter = ui.painter_at(rect.expand(6.0));
            let r = radius as u8;
            let preview = Rect::from_min_size(rect.min, vec2(rect.width(), DOODLE_PREVIEW));
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
            // The wallpaper's own colour, with this pattern over it.
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
            if let Some(style) = style {
                wallpaper::paint_doodles(ui, preview, &look, style);
            } else {
                theme::paint_icon(
                    ui,
                    Icon::Ban,
                    Rect::from_center_size(preview.center(), Vec2::splat(26.0)),
                    24.0,
                    palette.secondary,
                );
            }
            let name = widgets::line(
                ui,
                &label,
                theme::medium(12.5),
                if chosen { palette.accent } else { palette.text },
                footer.width() - 20.0,
                1,
            );
            name.paint(
                ui,
                pos2(
                    footer.center().x - name.size().x / 2.0,
                    footer.center().y - name.size().y / 2.0,
                ),
                if chosen { palette.accent } else { palette.text },
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
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                chosen,
                &label,
            )
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            match style {
                None => app.actions.push(Action::SetWallpaperDoodles(false)),
                Some(style) => app.actions.push(Action::SetDoodleStyle(style)),
            }
        }
    }
    ui.add_space(14.0);
    doodle_ink_controls(ui, app);
    ui.add_space(4.0);
}

/// The doodles' colour, grey or the accent, and how strongly they show.
fn doodle_ink_controls(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let current = app.settings.doodle_color;
    let locale = app.locale;
    let options: Vec<_> = DoodleColor::ALL
        .into_iter()
        .map(|color| (color, crate::i18n::gettext(locale, color.label())))
        .collect();
    let actions = &mut app.actions;
    widgets::setting_row(
        ui,
        &palette,
        &crate::i18n::gettext(locale, "Doodle colour"),
        &crate::i18n::gettext(
            locale,
            "Gray stands out softly from the wallpaper; Accent tints the doodles with your accent colour.",
        ),
        |ui| {
            if let Some(color) =
                segmented(ui, &palette, "doodle-color", &options, Some(current), 88.0)
            {
                actions.push(Action::SetDoodleColor(color));
            }
        },
    );
    ui.add_space(10.0);
    let range = DOODLE_INTENSITY;
    let intensity = app
        .settings
        .doodle_intensity
        .clamp(*range.start(), *range.end());
    ui.horizontal(|ui| {
        theme::text(
            ui,
            crate::i18n::gettext(app.locale, "Doodle intensity").into_owned(),
            theme::medium(13.5),
            palette.text,
        );
        ui.add_space(6.0);
        // The slider shows a share of the strongest doodles, not a multiple
        // of the default, so it ends at 100%.
        let mut percent = intensity / range.end();
        let value = format!("{:.0}%", percent * 100.0);
        let value_width = 48.0;
        let width = ui.available_width() - value_width - ui.spacing().item_spacing.x;
        let slider = widgets::glass_slider(
            ui,
            &palette,
            &mut percent,
            range.start() / range.end()..=1.0,
            width,
        );
        theme::text(ui, value, theme::medium(13.0), palette.secondary);
        if slider.changed() {
            app.actions
                .push(Action::SetDoodleIntensity(percent * range.end()));
        }
    });
}

/// Least width of a doodle tile before the grid drops a column.
const DOODLE_TILE: f32 = 128.0;
/// How tall the doodled part of a tile is.
const DOODLE_PREVIEW: f32 = 84.0;

/// The space this account's media cache takes, split by kind, with the
/// button that empties it.
fn storage_usage(ui: &mut egui::Ui, app: &mut App) {
    let palette = app.palette;
    let locale = app.locale;
    let stats = app.storage_stats.unwrap_or_default();
    let measured = app.storage_stats.is_some();
    let total = stats.bytes_total();
    ui.horizontal(|ui| {
        theme::text(
            ui,
            crate::i18n::gettext(locale, "Storage used"),
            theme::medium(14.0),
            palette.text,
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let clearing = app.storage_clearing;
            let label = crate::i18n::gettext(locale, "Clear…");
            let clear = ui
                .add_enabled_ui(measured && total > 0 && !clearing, |ui| {
                    theme::soft_button(ui, &palette, Some(Icon::Trash), &label, false)
                })
                .inner;
            if clear.clicked() {
                let id = app.account().id.clone();
                app.actions
                    .push(Action::ShowDialog(Dialog::ConfirmClearMediaCache(id)));
            }
            // A reading that never arrived shows no size rather than zero.
            if measured {
                theme::text(
                    ui,
                    crate::util::bytes(total),
                    theme::semibold(14.0),
                    palette.text,
                );
            }
        });
    });
    ui.add_space(2.0);
    // Each number has a cache of its own: say whose this is.
    let account = app.account().display_label(locale);
    let hint = widgets::line(
        ui,
        &crate::i18n::gettext(
            locale,
            "Photos, videos, stickers and files {account} downloaded to this computer. Clearing them keeps your messages; attachments download again when you open them.",
        )
        .replace("{account}", &account),
        theme::regular(12.5),
        palette.secondary,
        ui.available_width(),
        usize::MAX,
    );
    let (rect, _) = ui.allocate_exact_size(hint.size(), Sense::hover());
    if ui.is_rect_visible(rect) {
        hint.paint(ui, rect.min, palette.secondary);
    }
    if !measured {
        return;
    }
    ui.add_space(10.0);
    // Other only shows when there is something in it.
    for (label, count, bytes, always) in [
        (
            crate::i18n::gettext(locale, "Images"),
            stats.images,
            stats.image_bytes,
            true,
        ),
        (
            crate::i18n::gettext(locale, "Videos and GIFs"),
            stats.videos,
            stats.video_bytes,
            true,
        ),
        (
            crate::i18n::gettext(locale, "Stickers"),
            stats.stickers,
            stats.sticker_bytes,
            true,
        ),
        (
            crate::i18n::gettext(locale, "Other"),
            stats.other,
            stats.other_bytes,
            false,
        ),
    ] {
        if always || count > 0 {
            storage_bar_row(ui, &palette, &label, count, bytes, total);
        }
    }
}

/// One labelled bar: the kind, how many files and how much they weigh, and
/// the share of the total they take.
fn storage_bar_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    count: u64,
    bytes: u64,
    total: u64,
) {
    ui.horizontal(|ui| {
        theme::text(ui, label, theme::medium(14.0), palette.text);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            theme::text(
                ui,
                format!("{count} · {}", crate::util::bytes(bytes)),
                theme::regular(12.5),
                palette.secondary,
            );
        });
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, rect.height() / 2.0, palette.outline);
        let width = crate::model::StorageStats::bar_width(bytes, total, rect.width());
        if width > 0.5 {
            ui.painter().rect_filled(
                Rect::from_min_size(rect.min, vec2(width, rect.height())),
                rect.height() / 2.0,
                palette.accent,
            );
        }
    }
    ui.add_space(8.0);
}

/// A titled group of settings on a rounded card, or untitled on the page.
fn section(
    ui: &mut egui::Ui,
    palette: &Palette,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    // A section alone under its own name, beside the list of them, needs
    // no card to set it apart: its rows sit straight on the page.
    let frame = if title.is_empty() {
        Frame::new().inner_margin(Margin {
            left: 0,
            right: 0,
            top: 8,
            bottom: 6,
        })
    } else {
        ui.add_space(12.0);
        theme::text(ui, title, theme::bold(18.0), palette.text);
        ui.add_space(8.0);
        Frame::new()
            .fill(theme::blend(palette.panel, palette.surface, 0.5))
            .stroke(Stroke::new(1.0, palette.outline))
            .corner_radius(CornerRadius::same(theme::RADIUS_CARD))
            // Every row ends with its own gap, so the bottom margin is smaller.
            .inner_margin(Margin {
                left: 20,
                right: 20,
                top: 16,
                bottom: 6,
            })
    };
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add_contents(ui);
    });
    ui.add_space(8.0);
}

/// Our WhatsApp profile, which can be edited in place, and unlinking.
fn account(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let name = app.me_name.clone().unwrap_or_default();
    let me = app.me.clone().unwrap_or_default();
    let phone = crate::model::phone_of(&me)
        .map(crate::util::phone)
        .unwrap_or_else(|| me.clone());
    // The name and About being edited; `None` while not editing.
    let draft_id = ui.id().with("profile_draft");
    let mut draft: Option<(String, String)> = ui.data_mut(|data| data.get_temp(draft_id));
    let mut submitted = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let picture = app.avatar_full(&me).or_else(|| app.avatar(&me));
        let change_picture = crate::i18n::gettext(app.locale, "Change profile picture");
        if widgets::clickable_avatar(
            ui,
            &palette,
            &name,
            &me,
            56.0,
            picture.as_deref(),
            &change_picture,
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(change_picture.as_ref())
        .clicked()
        {
            app.actions.push(Action::PickProfilePicture);
        }
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 380.0).max(160.0));
            if let Some((draft_name, draft_about)) = &mut draft {
                submitted |= profile_field(
                    ui,
                    &palette,
                    draft_name,
                    &crate::i18n::gettext(app.locale, "Your name"),
                    25,
                );
                ui.add_space(6.0);
                submitted |= profile_field(
                    ui,
                    &palette,
                    draft_about,
                    &crate::i18n::pgettext(app.locale, "profile", "About"),
                    139,
                );
                return;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let shown = if name.is_empty() {
                    crate::i18n::gettext(app.locale, "Linked device")
                } else {
                    name.clone().into()
                };
                theme::text(ui, shown, theme::semibold(16.0), palette.text);
                if theme::icon_button(
                    ui,
                    Icon::Pencil,
                    15.0,
                    palette.secondary,
                    palette.text,
                    &crate::i18n::gettext(app.locale, "Edit your name and About"),
                )
                .clicked()
                {
                    draft = Some((name.clone(), app.me_about.clone().unwrap_or_default()));
                }
            });
            theme::text(ui, &phone, theme::regular(13.0), palette.secondary);
            if let Some(about) = &app.me_about {
                theme::paragraph(ui, about, theme::regular(13.0), palette.secondary);
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // With other numbers here, unlinking this one also takes it off
            // the switcher; the last one stays, waiting to be linked again.
            if app.has_several_accounts() {
                if theme::soft_button(
                    ui,
                    &palette,
                    Some(Icon::LogOut),
                    &crate::i18n::gettext(app.locale, "Remove this account"),
                    false,
                )
                .clicked()
                {
                    let id = app.account().id.clone();
                    app.actions
                        .push(Action::ShowDialog(Dialog::ConfirmRemoveAccount(id)));
                }
            } else if theme::soft_button(
                ui,
                &palette,
                Some(Icon::LogOut),
                &crate::i18n::gettext(app.locale, "Unlink this computer"),
                false,
            )
            .clicked()
            {
                app.actions.push(Action::ShowDialog(Dialog::ConfirmUnlink));
            }
            if theme::soft_button(
                ui,
                &palette,
                Some(Icon::Plus),
                &crate::i18n::gettext(app.locale, "Add account"),
                false,
            )
            .clicked()
            {
                app.actions.push(Action::AddAccount);
            }
        });
    });
    if let Some((draft_name, draft_about)) = &draft {
        ui.add_space(10.0);
        let mut done = false;
        ui.horizontal(|ui| {
            ui.add_space(56.0 + 14.0);
            let name = draft_name.trim();
            let about = draft_about.trim();
            let changed_name =
                (!name.is_empty() && app.me_name.as_deref() != Some(name)).then(|| name.to_owned());
            let changed_about =
                (app.me_about.as_deref().unwrap_or_default() != about).then(|| about.to_owned());
            let save = crate::i18n::gettext(app.locale, "Save");
            if theme::pill_button(ui, &palette, &save, true).clicked() || submitted {
                if changed_name.is_some() || changed_about.is_some() {
                    app.actions.push(Action::SetProfile {
                        name: changed_name,
                        about: changed_about,
                    });
                }
                done = true;
            }
            let cancel = crate::i18n::gettext(app.locale, "Cancel");
            if theme::pill_button(ui, &palette, &cancel, false).clicked() {
                done = true;
            }
        });
        if done {
            draft = None;
        }
    }
    ui.add_space(10.0);
    ui.data_mut(|data| {
        if let Some(draft) = draft {
            data.insert_temp(draft_id, draft);
        } else {
            data.remove::<(String, String)>(draft_id);
        }
    });
}

/// A single-line profile text field with its caption above it. Returns
/// whether Enter submitted it.
fn profile_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    value: &mut String,
    label: &str,
    limit: usize,
) -> bool {
    theme::text(ui, label, theme::medium(12.5), palette.secondary);
    let response = controls::Field::new(value)
        .char_limit(limit)
        .width(f32::INFINITY)
        .show(ui, palette);
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

/// The version, links to more about ZapFast, and who made it.
fn about(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let (logo, _) = ui.allocate_exact_size(Vec2::splat(44.0), egui::Sense::hover());
        theme::mark(ui, logo.center(), 44.0);
        ui.vertical(|ui| {
            theme::text(
                ui,
                format!("Zeus {}", env!("CARGO_PKG_VERSION")),
                theme::semibold(16.0),
                palette.text,
            );
            theme::paragraph(
                ui,
                crate::i18n::gettext(
                    app.locale,
                    "A native WhatsApp client built with Rust, egui, and whatsapp-rust.",
                ),
                theme::regular(13.0),
                palette.secondary,
            );
        });
    });
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        if let Some(update) = &app.update {
            let label = crate::i18n::gettext(app.locale, "Update to {version}")
                .replace("{version}", &update.version);
            if theme::soft_button(ui, &palette, Some(Icon::Download), &label, false).clicked() {
                app.actions.push(Action::ShowUpdate);
            }
        } else if crate::brand::UPDATES {
            check_for_updates(app, ui);
        }
        if theme::soft_button(
            ui,
            &palette,
            Some(Icon::Keyboard),
            &crate::i18n::gettext(app.locale, "Keyboard shortcuts"),
            false,
        )
        .clicked()
        {
            app.actions.push(Action::ShowDialog(Dialog::Shortcuts));
        }
        if theme::soft_button(
            ui,
            &palette,
            Some(Icon::Info),
            &crate::i18n::gettext(app.locale, "About"),
            false,
        )
        .clicked()
        {
            app.actions.push(Action::ShowDialog(Dialog::About));
        }
        if theme::soft_button(
            ui,
            &palette,
            Some(Icon::ExternalLink),
            &crate::i18n::gettext(app.locale, "Source code"),
            false,
        )
        .clicked()
        {
            app.actions
                .push(Action::OpenUrl(env!("CARGO_PKG_REPOSITORY").to_owned()));
        }
    });
    if app.update.is_none() {
        let status = match &app.update_check {
            crate::updates::Check::UpToDate => Some((
                crate::i18n::gettext(app.locale, "Zeus is up to date.").to_string(),
                palette.secondary,
            )),
            crate::updates::Check::Failed(error) => Some((
                crate::i18n::gettext(app.locale, "Could not check for updates: {error}")
                    .replace("{error}", error),
                palette.danger,
            )),
            crate::updates::Check::Idle | crate::updates::Check::Checking => None,
        };
        if let Some((text, color)) = status {
            ui.add_space(10.0);
            theme::paragraph(ui, text, theme::regular(13.0), color);
        }
    }
    if crate::brand::UPDATES {
        ui.add_space(12.0);
        update_channel_picker(app, ui);
    }
    ui.add_space(14.0);
    if let Some(url) = widgets::credit(ui, &palette, app.locale) {
        app.actions.push(Action::OpenUrl(url.to_owned()));
    }
}

/// Stable or Beta: which releases About offers.
fn update_channel_picker(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    let locale = app.locale;
    let options: Vec<_> = crate::updates::Channel::ALL
        .into_iter()
        .map(|channel| (channel, crate::i18n::gettext(locale, channel.label())))
        .collect();
    let selected = Some(app.settings.update_channel);
    ui.horizontal(|ui| {
        theme::text(
            ui,
            crate::i18n::gettext(locale, "Update channel"),
            theme::medium(13.0),
            palette.secondary,
        );
        ui.add_space(6.0);
        if let Some(channel) = segmented(ui, &palette, "channel", &options, selected, 88.0) {
            app.actions.push(Action::SetUpdateChannel(channel));
        }
    });
    let note = match app.settings.update_channel {
        crate::updates::Channel::Stable => "Tested releases only.",
        crate::updates::Channel::Beta => "Early builds first. They may have rough edges.",
    };
    ui.add_space(6.0);
    theme::paragraph(
        ui,
        crate::i18n::gettext(locale, note),
        theme::regular(12.0),
        palette.secondary,
    );
}

/// Asks GitHub for a newer release now; a newer one opens the update window.
fn check_for_updates(app: &mut App, ui: &mut egui::Ui) {
    let palette = app.palette;
    if app.update_check == crate::updates::Check::Checking {
        ui.horizontal(|ui| {
            ui.set_min_height(30.0);
            theme::spinner(ui, 15.0, palette.accent);
            theme::text(
                ui,
                crate::i18n::gettext(app.locale, "Checking for updates…"),
                theme::medium(13.0),
                palette.secondary,
            );
        });
    } else if theme::soft_button(
        ui,
        &palette,
        Some(Icon::Refresh),
        &crate::i18n::gettext(app.locale, "Check for updates"),
        false,
    )
    .clicked()
    {
        app.actions.push(Action::CheckForUpdates);
    }
}

fn toggle(
    ui: &mut egui::Ui,
    app: &mut App,
    label: &str,
    description: &str,
    field: impl Fn(&mut crate::settings::Settings) -> &mut bool,
) {
    let palette = app.palette;
    let mut value = *field(&mut app.settings);
    let mut changed = false;
    widgets::setting_row(ui, &palette, label, description, |ui| {
        let response = widgets::switch(ui, &palette, &mut value);
        theme::reveal_focus(&response);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), value, label)
        });
        changed = response.changed();
    });
    if changed {
        *field(&mut app.settings) = value;
        app.actions.push(Action::SettingsChanged);
    }
}

fn account_toggle(
    ui: &mut egui::Ui,
    app: &mut App,
    label: &str,
    description: &str,
    field: impl Fn(&mut crate::settings::AccountSettings) -> &mut bool,
) {
    let palette = app.palette;
    let mut value = *field(&mut app.account_mut().settings);
    let mut changed = false;
    widgets::setting_row(ui, &palette, label, description, |ui| {
        let response = widgets::switch(ui, &palette, &mut value);
        theme::reveal_focus(&response);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), value, label)
        });
        changed = response.changed();
    });
    if changed {
        *field(&mut app.account_mut().settings) = value;
        app.account_mut().mark_settings_dirty();
    }
}

/// Title and description of the sound for new messages or for mentions and
/// replies to us, as Pidgin plays one sound for messages and alerts when a
/// chat says your name.
fn sound_text(locale: Locale, mention: bool) -> (Text, Text) {
    if mention {
        (
            translated(locale, "Mention sound"),
            translated(locale, "When a group mentions you or replies to you."),
        )
    } else {
        (translated(locale, "Message sound"), Text::default())
    }
}

/// The sound menu, with a button that plays the current choice.
fn sound_control(ui: &mut egui::Ui, app: &mut App, mention: bool) {
    use crate::i18n::{gettext, pgettext};
    use crate::settings::NotificationSound;
    let palette = app.palette;
    let locale = app.locale;
    let current = if mention {
        app.settings.mention_sound.clone()
    } else {
        app.settings.message_sound.clone()
    };
    let choices = [
        (NotificationSound::Spooky, gettext(locale, "Spooky")),
        (
            NotificationSound::Witching,
            gettext(locale, "Witching hour"),
        ),
        (NotificationSound::Receive, "Pidgin".into()),
        (NotificationSound::Alert, gettext(locale, "Pidgin alert")),
        (NotificationSound::System, gettext(locale, "System default")),
        // Translators: no notification sound.
        (NotificationSound::None, pgettext(locale, "sound", "None")),
    ];
    let selected = match &current {
        NotificationSound::Custom(path) => path.file_name().map_or_else(
            || "Custom".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        ),
        sound => choices
            .iter()
            .find(|(choice, _)| choice == sound)
            .map(|(_, name)| name.to_string())
            .unwrap_or_default(),
    };
    controls::dropdown(
        ui,
        &palette,
        ("notification-sound", mention),
        "Sound",
        &selected,
        controls::WIDTH,
        |ui| {
            for (sound, name) in choices {
                if controls::option(ui, &palette, &name, current == sound) {
                    app.actions
                        .push(Action::SetNotificationSound { mention, sound });
                }
            }
            if controls::option(ui, &palette, &gettext(locale, "Choose a file…"), false) {
                app.actions.push(Action::PickNotificationSound { mention });
            }
        },
    );
    if !matches!(current, NotificationSound::System | NotificationSound::None)
        && theme::icon_button(
            ui,
            Icon::Play,
            16.0,
            palette.secondary,
            palette.text,
            &gettext(locale, "Play"),
        )
        .clicked()
    {
        app.actions.push(Action::PreviewSound(current.clone()));
    }
}

/// One account privacy category's picker: what the phone holds, and the
/// values it takes. While a write is in flight it is disabled, so a second
/// pick cannot race the first.
fn privacy_control(ui: &mut egui::Ui, app: &mut App, kind: PrivacyKind) {
    let palette = app.palette;
    let current = app.account_privacy.get(kind);
    let locale = app.locale;
    let selected = current.map_or(Cow::Borrowed("\u{2014}"), |choice| choice.label(locale));
    let pending = app.account_privacy.pending(kind);
    ui.add_enabled_ui(!pending, |ui| {
        controls::dropdown(
            ui,
            &palette,
            format!("privacy_{}", kind.wire_name()),
            &kind.label(locale),
            &selected,
            controls::WIDTH,
            |ui| {
                for choice in kind.choices() {
                    if controls::option(
                        ui,
                        &palette,
                        &choice.label(locale),
                        current == Some(*choice),
                    ) {
                        app.actions.push(Action::SetAccountPrivacy {
                            kind,
                            choice: *choice,
                        });
                    }
                }
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(section: Section, filter: &Filter) -> Vec<String> {
        let (_, rows) = section.visible(filter);
        rows.into_iter()
            .map(|(row, _)| row.title.shown.into_owned())
            .collect()
    }

    fn window(locale: Locale) -> Section {
        let mut window = Section::new(translated(locale, "Notifications"));
        window.toggle(
            "Check for updates",
            "Ask GitHub every two hours.",
            |settings| &mut settings.check_for_updates,
        );
        let (title, description) = sound_text(locale, false);
        window.row(title, description, |_, _| {});
        window.toggle(
            translated(locale, "Play sounds for group messages"),
            "",
            |settings| &mut settings.group_sounds,
        );
        window
    }

    #[test]
    fn an_empty_search_shows_every_row() {
        let filter = Filter::new("  ");
        assert!(filter.is_empty());
        assert_eq!(window(Locale::English).visible(&filter).1.len(), 3);
    }

    #[test]
    fn the_search_matches_titles_and_descriptions_ignoring_case_and_accents() {
        let rows = |query| titles(window(Locale::English), &Filter::new(query));
        assert_eq!(
            rows("SOUND"),
            ["Message sound", "Play sounds for group messages"]
        );
        assert_eq!(rows("github"), ["Check for updates"]);
        assert!(rows("proxy").is_empty(), "an empty section is left out");
        // German folds its umlauts, so "tone" finds "Töne".
        let german = |query| titles(window(Locale::German), &Filter::new(query));
        assert_eq!(german("tone"), ["Töne für Gruppennachrichten abspielen"]);
    }

    #[test]
    fn a_translated_setting_is_found_in_english_too() {
        let german = |query| titles(window(Locale::German), &Filter::new(query));
        assert_eq!(german("Nachrichtenton"), ["Nachrichtenton"]);
        assert_eq!(german("message sound"), ["Nachrichtenton"]);
    }

    #[test]
    fn a_matching_section_title_keeps_all_its_rows() {
        let rows = titles(window(Locale::German), &Filter::new("notifications"));
        assert_eq!(rows.len(), 3);
        let rows = titles(window(Locale::German), &Filter::new("benachrichtigungen"));
        assert_eq!(rows.len(), 3);
        let rows = titles(window(Locale::Turkish), &Filter::new("bildirimler"));
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn blocks_are_found_by_their_keywords() {
        let mut account = Section::new(translated(Locale::English, "Account"));
        account.block(
            vec![translated(Locale::English, "Unlink this computer")],
            |_, _| {},
        );
        assert_eq!(account.visible(&Filter::new("unlink")).1.len(), 1);
        let mut account = Section::new(translated(Locale::English, "Account"));
        account.block(Vec::new(), |_, _| {});
        assert!(account.visible(&Filter::new("unlink")).1.is_empty());
    }
}
