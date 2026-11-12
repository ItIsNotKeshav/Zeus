//! Zeus's own parts of the composer and the strips above it.

use super::*;

/// The composer of the other pane of a split: a quiet field with what is
/// drafted there, or "Type a message". It takes nothing itself: a press
/// anywhere in the pane makes its chat the open one, and the real composer
/// takes its place with the keyboard.
pub(super) fn quiet_composer(app: &mut App, ui: &mut egui::Ui, chat: &Chat) {
    let palette = app.palette;
    let draft = app.draft_preview(&chat.id);
    egui::Panel::bottom(ui.id().with("composer"))
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(Color32::TRANSPARENT)
                .inner_margin(Margin {
                    left: 8,
                    right: 8,
                    top: 12,
                    bottom: 12,
                }),
        )
        .show(ui, |ui| {
            if !chat.can_send() {
                return;
            }
            let line_height = ui
                .painter()
                .layout_no_wrap("x".to_owned(), theme::regular(BODY_SIZE), palette.text)
                .size()
                .y;
            let line = (line_height + COMPOSER_PADDING)
                .round()
                .max(COMPOSER_CONTROL);
            let pill = composer_glass(ui, &palette, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), line), Sense::hover());
                let (text, color) = match &draft {
                    Some(draft) => (
                        format!("{} {draft}", crate::i18n::gettext(app.locale, "Draft:")),
                        palette.secondary,
                    ),
                    None => (
                        crate::i18n::gettext(app.locale, "Type a message").into_owned(),
                        palette.dim,
                    ),
                };
                let inset = f32::from(COMPOSER_RADIUS) * 0.7;
                let label = widgets::line(
                    ui,
                    &text,
                    theme::regular(BODY_SIZE),
                    color,
                    (rect.width() - inset * 2.0).max(0.0),
                    1,
                );
                label.paint(
                    ui,
                    pos2(rect.left() + inset, rect.center().y - label.size().y / 2.0),
                    color,
                );
                response
            });
            ui.ctx()
                .data_mut(|data| data.insert_temp(composer_pill_id(), pill.response.rect));
        });
}

/// The pop-out window's transcript.
pub(in crate::ui) fn pop_out_transcript(app: &mut App, ui: &mut egui::Ui, chat: &Chat) {
    messages_in(app, ui, chat, 0.0, 0.0, Pane::PopOut);
}

/// An open bubble's transcript.
pub(in crate::ui) fn bubble_transcript(app: &mut App, ui: &mut egui::Ui, chat: &Chat) {
    messages_in(app, ui, chat, 0.0, 0.0, Pane::Bubble);
}

/// The pop-out window's reply field: one line, sent with Enter to its own
/// chat, never touching the open chat's composer.
pub(in crate::ui) fn pop_out_composer(app: &mut App, ui: &mut egui::Ui, chat: &Chat) {
    if !chat.can_send() {
        return;
    }
    let palette = app.palette;
    let hint = crate::i18n::gettext(app.locale, "Type a message").into_owned();
    egui::Panel::bottom(ui.id().with("pop-out-composer"))
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.panel.to_opaque())
                .inner_margin(Margin::same(10)),
        )
        .show(ui, |ui| {
            let Some(pop) = app.pop_out.as_mut() else {
                return;
            };
            let field = egui::TextEdit::singleline(&mut pop.text)
                .hint_text(hint)
                .desired_width(f32::INFINITY)
                .margin(Margin::symmetric(12, 8));
            let response = ui.add(field);
            if std::mem::take(&mut pop.focus) {
                response.request_focus();
            }
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                let text = std::mem::take(&mut pop.text);
                response.request_focus();
                if !text.trim().is_empty() {
                    app.actions.push(Action::PopOutSend {
                        chat: chat.id.clone(),
                        text,
                    });
                }
            }
        });
}

/// Space between the composer's field and the chat card's sides and bottom.
/// The card's corners are this much rounder than the field's, so the two
/// curves run parallel (see `chats::CARD_RADIUS`).
pub(crate) const COMPOSER_MARGIN: i8 = 12;
/// Space the send button keeps on every side inside its band, so it sits
/// clear of the field's corner while the field stays one line thick.
pub(super) const SEND_CLEARANCE: f32 = 4.0;
/// The composer's field over blurred glass: what scrolls on under it is
/// frosted within its rounded ends, under its see-through fill.
pub(super) fn composer_glass<R>(
    ui: &mut egui::Ui,
    palette: &Palette,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let glass = ui.painter().add(egui::Shape::Noop);
    let shown = composer_pill(palette).show(ui, add);
    if palette.vibrant() {
        ui.painter().set(
            glass,
            crate::ui::glass::sheet_rounded(shown.response.rect, f32::from(COMPOSER_RADIUS)),
        );
    }
    shown
}

/// Draws the link preview's card, the gap under it included, and returns
/// whether its X was clicked.
pub(super) fn link_preview_card(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: crate::i18n::Locale,
    shown: &crate::app::ComposerPreview,
) -> bool {
    let palette = *palette;
    let mut dismiss = false;
    let strip = match &shown.state {
        LinkPreviewState::Dismissed => return false,
        LinkPreviewState::Loading => {
            let loading = crate::i18n::gettext(locale, "Loading preview…");
            strip_frame(&palette).show(ui, |ui| {
                ui.set_width(ui.available_width().max(0.0));
                ui.horizontal(|ui| {
                    let (bar, _) = ui.allocate_exact_size(vec2(3.0, 18.0), Sense::hover());
                    ui.painter().rect_filled(bar, 2.0, palette.accent);
                    theme::text(ui, loading, theme::regular(12.5), palette.secondary);
                });
            })
        }
        LinkPreviewState::Ready(composed) => {
            let remove = crate::i18n::gettext(locale, "Remove link preview");
            // The card has no message: its picture is named by its chat and
            // link instead, the same each frame.
            let image = composer_thumbnail_id(&composed.preview.url);
            let thumbnail = composed
                .thumbnail
                .as_deref()
                .map(|bytes| (shown.chat.as_str(), image.as_str(), bytes));
            let frame = strip_frame(&palette).inner_margin(Margin::same(10));
            frame.show(ui, |ui| {
                ui.set_width(ui.available_width().max(0.0));
                // Top-aligned: a centred row would start the card partway down
                // its own first line and leave the margin above it too deep.
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let right = ui.max_rect().right();
                    let (bar, _) = ui.allocate_exact_size(vec2(3.0, 0.0), Sense::hover());
                    let width = (ui.available_width() - 46.0).max(0.0);
                    let card = preview_layout(ui, &palette, &composed.preview, thumbnail, width);
                    ui.painter().rect_filled(
                        Rect::from_x_y_ranges(bar.x_range(), card.y_range()),
                        2.0,
                        palette.accent,
                    );
                    let room = Rect::from_x_y_ranges(card.right()..=right, card.y_range());
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(room)
                            .layout(Layout::right_to_left(Align::Center)),
                        |ui| {
                            let button = theme::icon_button(
                                ui,
                                Icon::X,
                                16.0,
                                palette.secondary,
                                palette.text,
                                remove.as_ref(),
                            );
                            dismiss = button.clicked();
                        },
                    );
                });
            })
        }
    };
    ui.ctx()
        .data_mut(|data| data.insert_temp(link_preview_strip_id(), strip.response.rect));
    strip_gap(ui);
    dismiss
}

/// How long a refused voice message of `samples` lasts, in whole seconds.
pub(super) fn unsent_seconds(samples: usize) -> u32 {
    (samples as f64 / f64::from(crate::voice::RATE))
        .round()
        .max(1.0) as u32
}

/// What the reply strip over the composer shows.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum ReplyCard {
    /// Who wrote the message being replied to, and a line of what it says.
    Quote { who: String, summary: String },
    /// A private reply whose group message is not on this computer.
    Unavailable,
}

impl ReplyCard {
    pub(super) fn of(app: &App, quoted: &Message) -> Self {
        let who = if quoted.from_me {
            "You".to_owned()
        } else {
            app.display_name_or(&quoted.sender, quoted.sender_name.as_deref())
        };
        let summary = markup::plain(&quoted.summary_in(app.locale), &app.mention_list(quoted));
        Self::Quote { who, summary }
    }
}

/// The spring a strip over the composer opens on: critically damped, since
/// its room moves the field and the messages, and a reversal mid-way
/// carries on from where it is. A strip that grows or shrinks while open,
/// and an attachment tile sliding to a new place, move on it too.
pub(super) const STRIP_OPEN: crate::motion::Spring = crate::motion::Spring::new(0.35, 0.0);
/// The spring it closes on, quicker, as surfaces close.
pub(super) const STRIP_CLOSE: crate::motion::Spring =
    crate::motion::Spring::new(0.35 * theme::CLOSE as f64, 0.0);
/// The share of the way open by which a strip is fully opaque: it fades in
/// over the start of its rise and out over the end of its fall.
pub(super) const STRIP_FADE: f32 = 0.4;
/// How long an open strip takes to trade its card for another, as when a
/// second message is double-clicked for a reply while one is quoted: the
/// old card fades out rising [`SWAP_RISE`] points as the new one fades in
/// rising into its place.
pub(super) const SWAP_SECONDS: f32 = 0.2;
pub(super) const SWAP_RISE: f32 = 6.0;

/// Where one strip over the composer stands as it opens and closes. Kept
/// in the context's memory under the strip's own id and brought up to date
/// once a pass by [`reveal_strip`].
#[derive(Clone)]
pub(super) struct Reveal<T> {
    pub(super) chat: String,
    /// What the strip shows: the open one's, or the last while it closes.
    pub(super) card: Option<std::sync::Arc<T>>,
    /// The card it showed before, while it fades out, and when it began to.
    pub(super) outgoing: Option<std::sync::Arc<T>>,
    pub(super) swapped: f64,
    /// How far open the strip is, 0 closed and 1 open, with its speed per
    /// second.
    pub(super) shown: f32,
    pub(super) velocity: f32,
    /// The strip's height with the gap under it, as last laid out.
    pub(super) height: f32,
    /// The height its room follows on [`STRIP_OPEN`], with its speed, so a
    /// strip that grows or shrinks while open moves the messages smoothly.
    pub(super) tall: f32,
    pub(super) tall_velocity: f32,
    /// When it was last brought up to date, and the pass it was.
    pub(super) time: f64,
    pub(super) pass: u64,
}

/// The room the strips over a chat's composer took in a pass.
#[derive(Clone, Copy, Default)]
pub(super) struct StripRooms {
    pub(super) pass: u64,
    pub(super) total: f32,
}

pub(super) fn strip_rooms_id(chat: &str) -> egui::Id {
    egui::Id::new(("composer-strip-rooms", chat))
}

/// The room the strips over `chat`'s composer take in this pass, while they
/// open, close or show: 0 when none was drawn. The messages rise and sink
/// with it exactly.
pub(super) fn strip_room(ctx: &egui::Context, chat: &str) -> f32 {
    ctx.data(|data| data.get_temp::<StripRooms>(strip_rooms_id(chat)))
        .filter(|rooms| rooms.pass == ctx.cumulative_pass_nr())
        .map_or(0.0, |rooms| rooms.total)
}

/// Moves a spring at `position` with `velocity` toward `target` over `dt`
/// seconds, landing on it once it is all but there.
pub(super) fn spring_toward(
    spring: crate::motion::Spring,
    position: f32,
    velocity: f32,
    target: f32,
    dt: f64,
) -> (f32, f32) {
    let (offset, velocity) = spring.advance(position - target, velocity, dt);
    if offset.abs() <= 1e-3 && velocity.abs() <= 1e-2 {
        (target, 0.0)
    } else {
        (target + offset, velocity)
    }
}

/// A strip over `chat`'s composer that shows `card`, or `None` while there
/// is nothing to show. It opens out of the field: its room grows on
/// [`STRIP_OPEN`] while the strip rises from behind the field and fades in,
/// lifting the messages with it, and it sinks back the same way on
/// [`STRIP_CLOSE`] once `card` goes, drawing the last card meanwhile and
/// turning around from where it is. A strip that changes height while open
/// grows out from behind the field or settles down onto it. With `swap`, a
/// new card in an open strip crossfades with the old one. `draw` lays the
/// card out, the gap under it included. When `calm` (a screen reader, a
/// background window) or in a chat just opened, it is in place at once.
/// Returns whether anything was drawn.
pub(super) fn reveal_strip<T: Clone + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    calm: bool,
    id: egui::Id,
    chat: &str,
    card: Option<&T>,
    swap: bool,
    mut draw: impl FnMut(&mut egui::Ui, &T),
) -> bool {
    let ctx = ui.ctx().clone();
    let id = id.with(chat);
    let now = ctx.input(|input| input.time);
    let pass = ctx.cumulative_pass_nr();
    let open = card.is_some();
    let target = if open { 1.0 } else { 0.0 };
    let mut moving = false;
    let mut reveal = match ctx.data(|data| data.get_temp::<Reveal<T>>(id)) {
        // Drawn in the pass before for the same chat: it moves on.
        Some(mut reveal) if !calm && reveal.chat == chat && reveal.pass + 1 >= pass => {
            let spring = if open { STRIP_OPEN } else { STRIP_CLOSE };
            let dt = (now - reveal.time).max(0.0);
            (reveal.shown, reveal.velocity) =
                spring_toward(spring, reveal.shown, reveal.velocity, target, dt);
            (reveal.tall, reveal.tall_velocity) = spring_toward(
                STRIP_OPEN,
                reveal.tall,
                reveal.tall_velocity,
                reveal.height,
                dt,
            );
            moving = true;
            reveal
        }
        previous => {
            let height = previous.map_or(0.0, |reveal| reveal.height);
            Reveal {
                chat: chat.to_owned(),
                card: None,
                outgoing: None,
                swapped: now,
                shown: target,
                velocity: 0.0,
                height,
                tall: height,
                tall_velocity: 0.0,
                time: now,
                pass,
            }
        }
    };
    // A strip opening from nothing takes its height at once: its room grows
    // on the opening spring alone.
    let from_nothing = reveal.card.is_none() || !moving;
    match card {
        Some(card) if reveal.card.as_deref() != Some(card) => {
            if swap && !from_nothing && reveal.shown > 0.0 {
                reveal.outgoing = reveal.card.take();
                reveal.swapped = now;
            }
            reveal.card = Some(std::sync::Arc::new(card.clone()));
        }
        Some(_) => {}
        None if reveal.shown == 0.0 && reveal.velocity == 0.0 => reveal.card = None,
        None => {}
    }
    let swap_time = theme::motion_time(&ctx, SWAP_SECONDS);
    let swapped = if calm || swap_time <= 0.0 {
        1.0
    } else {
        (((now - reveal.swapped) as f32) / swap_time).clamp(0.0, 1.0)
    };
    if swapped >= 1.0 {
        reveal.outgoing = None;
    }
    let lift = if reveal.outgoing.is_some() {
        SWAP_RISE * (1.0 - theme::ease_out(swapped))
    } else {
        0.0
    };
    let mut room = 0.0;
    let drawn = reveal.card.clone();
    if let Some(card) = &drawn {
        let top = ui.cursor().min;
        let shown = reveal.shown.clamp(0.0, 1.0);
        // A strip shorter than its room rests on the field, its top
        // settling down; a taller one is laid out where it rests below the
        // top of its room and cut off where the field starts, so it rises
        // from behind it.
        let tall = if from_nothing {
            reveal.height
        } else {
            reveal.tall
        };
        let settle = (tall - reveal.height).max(0.0) * shown;
        let mut strip = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(
                    ui.available_rect_before_wrap()
                        .translate(vec2(0.0, settle + lift)),
                )
                .layout(Layout::top_down(Align::Min)),
        );
        // One in place at once has no room to keep to yet: its height is
        // measured as it is drawn.
        if !(from_nothing && shown >= 1.0) {
            let mut clip = strip.clip_rect();
            clip.max.y = clip.max.y.min(top.y + tall * shown);
            strip.set_clip_rect(clip);
        }
        if shown < 1.0 {
            strip.multiply_opacity((shown / STRIP_FADE).min(1.0));
        }
        if let Some(old) = reveal.outgoing.clone() {
            // Beneath the new card, only painting: it rises away as it fades.
            let gone = theme::ease_out(swapped);
            let mut leaving = strip.new_child(
                egui::UiBuilder::new()
                    .id_salt("outgoing-card")
                    .max_rect(
                        strip
                            .max_rect()
                            .translate(vec2(0.0, -lift - SWAP_RISE * gone)),
                    )
                    .layout(Layout::top_down(Align::Min)),
            );
            leaving.disable();
            leaving.multiply_opacity(1.0 - gone);
            draw(&mut leaving, &*old);
            strip.multiply_opacity(gone);
        }
        draw(&mut strip, &**card);
        reveal.height = strip.cursor().min.y - top.y - settle - lift;
        if from_nothing {
            (reveal.tall, reveal.tall_velocity) = (reveal.height, 0.0);
        }
        room = reveal.tall * shown;
        ui.add_space(room);
    }
    if reveal.shown != target
        || reveal.velocity != 0.0
        || reveal.outgoing.is_some()
        || (drawn.is_some() && (reveal.tall != reveal.height || reveal.tall_velocity != 0.0))
    {
        crate::motion::request_frame(&ctx);
    }
    reveal.time = now;
    reveal.pass = pass;
    ctx.data_mut(|data| {
        data.insert_temp(id, reveal);
        let rooms = data.get_temp_mut_or_default::<StripRooms>(strip_rooms_id(chat));
        if rooms.pass != pass {
            *rooms = StripRooms { pass, total: 0.0 };
        }
        rooms.total += room;
    });
    drawn.is_some()
}

/// Draws the reply strip's card in `ui` and returns where it lies.
pub(super) fn reply_card(app: &mut App, ui: &mut egui::Ui, card: &ReplyCard) -> Rect {
    let palette = app.palette;
    let locale = app.locale;
    let strip = strip_frame(&palette).show(ui, |ui| {
        ui.set_width(ui.available_width().max(0.0));
        ui.horizontal(|ui| {
            match card {
                ReplyCard::Quote { who, summary } => {
                    let (bar, _) = ui.allocate_exact_size(vec2(3.0, 34.0), Sense::hover());
                    ui.painter().rect_filled(bar, 2.0, palette.accent);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.set_max_width((ui.available_width() - 40.0).max(0.0));
                        widgets::rich_text(
                            ui,
                            &format!("Replying to {who}"),
                            theme::semibold(theme::TEXT_CAPTION),
                            palette.accent,
                        );
                        widgets::rich_text(
                            ui,
                            summary,
                            theme::regular(theme::TEXT_CAPTION),
                            palette.secondary,
                        );
                    });
                }
                ReplyCard::Unavailable => {
                    ui.vertical(|ui| {
                        ui.set_max_width((ui.available_width() - 40.0).max(0.0));
                        widgets::rich_text(
                            ui,
                            &crate::i18n::gettext(locale, "Original group message unavailable"),
                            theme::regular(theme::TEXT_CAPTION),
                            palette.secondary,
                        );
                    });
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(
                    ui,
                    Icon::X,
                    16.0,
                    palette.secondary,
                    palette.text,
                    "Cancel reply (Esc)",
                )
                .clicked()
                {
                    app.actions.push(Action::CancelReply);
                }
            });
        });
    });
    strip.response.rect
}
