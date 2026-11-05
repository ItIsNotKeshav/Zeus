//! The media viewer: a full-window photo and video view with the chat's
//! album along the bottom, in the shape WhatsApp's own viewer uses, and the
//! card a sticker is shown on by itself.

use egui::{
    Align, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, UiBuilder, Vec2, pos2, vec2,
};

use std::path::Path;

use super::widgets::FrostedPopup;
use crate::app::App;
use crate::image_preview::PreviewState;
use crate::model::MediaItem;
use crate::model::{Action, Dialog};
use crate::theme::{self, Icon};

use super::conversation::{album_tile_id, bubble_id, thumbnail_uri, video_flight_id};

/// Height of the header across the top.
const HEADER: f32 = 56.0;
/// Height of the album strip along the bottom.
const STRIP: f32 = 78.0;
const THUMB: f32 = 56.0;
const STRIP_GAP: f32 = 6.0;
const STRIP_PAD: f32 = 4.0;

/// How long a picture takes to fly between its bubble and the viewer.
const FLIGHT_SECONDS: f32 = 0.4;
/// The spring's bounce: a shade, so the picture settles instead of stopping.
const FLIGHT_BOUNCE: f64 = 0.12;
/// The picture's corner radius in its bubble, which the viewer squares off.
const FLIGHT_RADIUS: f32 = theme::RADIUS as f32;

/// When the viewer opened and the pass it was last drawn in, to tell a new
/// opening from a later frame or a step to another picture.
#[derive(Clone, Copy)]
struct Opened {
    start: f64,
    pass: u64,
}

/// Where a flight starts: the bubble's rect, how far along it is, and the
/// corner radius the picture has in its bubble.
type Flight = (Rect, f32, f32);

/// What the picture flying back to its bubble is made of.
#[derive(Clone)]
enum GhostPicture {
    /// A photo, read from its file.
    File(std::path::PathBuf),
    /// The frame a video had decoded.
    Frame(egui::TextureHandle),
    /// A video's poster, by its image URI.
    Poster(String),
}

/// The message whose picture is in the air, until `ends`, so its bubble keeps
/// that picture empty instead of showing it twice.
#[derive(Clone)]
struct Flying {
    chat: String,
    message: String,
    ends: f64,
}

fn flying_id() -> egui::Id {
    egui::Id::new("image-preview-flying")
}

/// Whether the message's picture is flying between its bubble and the viewer
/// right now.
pub fn source_hidden(ctx: &egui::Context, chat: &str, message: &str) -> bool {
    let now = ctx.input(|input| input.time);
    ctx.data(|data| data.get_temp::<Flying>(flying_id()))
        .is_some_and(|flying| flying.chat == chat && flying.message == message && now < flying.ends)
}

/// Says the message's picture is in the air for another `seconds`.
fn mark_flying(ctx: &egui::Context, chat: &str, message: &str, seconds: f32) {
    let ends = ctx.input(|input| input.time) + f64::from(seconds);
    ctx.data_mut(|data| {
        data.insert_temp(
            flying_id(),
            Flying {
                chat: chat.to_owned(),
                message: message.to_owned(),
                ends,
            },
        );
    });
}

/// The corner radius a message's picture has in its bubble: a round video
/// note is a circle, everything else the bubble's own corner.
fn source_radius(ctx: &egui::Context, chat: &str, message: &str, from: Rect) -> f32 {
    if ctx.data(|data| data.get_temp::<bool>(video_note_id(chat, message))) == Some(true) {
        from.width() / 2.0
    } else {
        FLIGHT_RADIUS
    }
}

/// Marks a message's video as a round note, which flies as a circle.
pub fn video_note_id(chat: &str, message: &str) -> egui::Id {
    video_flight_id(chat, message).with("note")
}

/// The picture the viewer last settled on, kept so the viewer can fly it back
/// to its bubble after the app has dropped the preview.
#[derive(Clone)]
struct Ghost {
    picture: GhostPicture,
    chat: String,
    message: String,
    /// Where the picture sat in the viewer.
    rect: Rect,
    /// When the viewer closed, once it has.
    closed: Option<f64>,
}

fn opened_id() -> egui::Id {
    egui::Id::new("image-preview-opened")
}

fn ghost_id() -> egui::Id {
    egui::Id::new("image-preview-ghost")
}

/// Where the message's picture was last drawn in its bubble, when that is on
/// screen.
fn source_rect(ctx: &egui::Context, chat: &str, message: &str) -> Option<Rect> {
    let rect = ctx.data(|data| {
        data.get_temp::<Rect>(video_flight_id(chat, message))
            .or_else(|| data.get_temp::<Rect>(album_tile_id(chat, message)))
            .or_else(|| data.get_temp::<Rect>(bubble_id(chat, message).with("picture")))
    })?;
    (rect.is_positive() && ctx.content_rect().intersects(rect)).then_some(rect)
}

/// `a` moved `t` of the way to `b`; `t` may overshoot for a spring.
fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min.lerp(b.min, t), a.max.lerp(b.max, t))
}

/// Paints a picture in the air at `rect`, cropped to cover it instead of
/// stretched to it. A bubble may crop its picture (a round note to a square,
/// an album to square tiles) while the viewer shows the whole of it, so the
/// crop opens out on the way and the picture keeps its own shape throughout.
fn paint_flying(
    ui: &egui::Ui,
    image: egui::Image<'_>,
    rect: Rect,
    radius: f32,
    tint: egui::Color32,
) {
    let uv = image
        .load_for_size(ui.ctx(), rect.size())
        .ok()
        .and_then(|poll| poll.size())
        .map_or(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), |size| {
            crate::wallpaper::cover_uv(size, rect.size())
        });
    image
        .fit_to_exact_size(rect.size())
        .uv(uv)
        .corner_radius(radius)
        .tint(tint)
        .paint_at(ui, rect);
}

/// How far the opening flight has come at `elapsed` seconds, on the spring.
fn flight_progress(elapsed: f32) -> f32 {
    if elapsed <= 0.0 {
        return 0.0;
    }
    if elapsed >= FLIGHT_SECONDS {
        return 1.0;
    }
    let (stiffness, damping) =
        spring_from_duration(f64::from(FLIGHT_SECONDS) * 1000.0, FLIGHT_BOUNCE);
    spring_unit(f64::from(elapsed) * 1000.0, stiffness, damping) as f32
}

/// Seconds since the viewer opened, restarting when it is new this pass.
fn open_elapsed(ctx: &egui::Context) -> f32 {
    let now = ctx.input(|input| input.time);
    let pass = ctx.cumulative_pass_nr();
    let opened = ctx.data_mut(|data| {
        let opened = match data.get_temp::<Opened>(opened_id()) {
            Some(opened) if opened.pass + 1 >= pass => Opened {
                start: opened.start,
                pass,
            },
            _ => Opened { start: now, pass },
        };
        data.insert_temp(opened_id(), opened);
        opened
    });
    (now - opened.start) as f32
}

/// Stiffness and damping for Motion's `{ duration, bounce }` spring, as the
/// conversation uses for a message arriving. `duration_ms` is milliseconds.
fn spring_from_duration(duration_ms: f64, bounce: f64) -> (f64, f64) {
    const SAFE_MIN: f64 = 0.001;
    const ROOT_STEPS: i32 = 12;
    let mass = 1.0;
    let velocity = 0.0;
    let damping_ratio = (1.0 - bounce).clamp(0.05, 1.0);
    let duration = (duration_ms / 1000.0).clamp(0.01, 10.0);
    let angular = |undamped: f64| undamped * (1.0 - damping_ratio * damping_ratio).sqrt();
    let envelope = |undamped: f64| {
        let decay = undamped * damping_ratio;
        let delta = decay * duration;
        let a = decay - velocity;
        let b = angular(undamped);
        SAFE_MIN - (a / b) * (-delta).exp()
    };
    let derivative = |undamped: f64| {
        let decay = undamped * damping_ratio;
        let delta = decay * duration;
        let d = delta * velocity + velocity;
        let e = damping_ratio.powi(2) * undamped.powi(2) * duration;
        let f = (-delta).exp();
        let g = angular(undamped.powi(2));
        let factor = if -envelope(undamped) + SAFE_MIN > 0.0 {
            -1.0
        } else {
            1.0
        };
        (factor * ((d - e) * f)) / g
    };
    let mut undamped = 5.0 / duration;
    for _ in 1..ROOT_STEPS {
        let slope = derivative(undamped);
        if slope.abs() < 1e-9 {
            break;
        }
        undamped -= envelope(undamped) / slope;
    }
    let stiffness = undamped * undamped * mass;
    let damping = damping_ratio * 2.0 * (mass * stiffness).sqrt();
    (stiffness, damping)
}

/// Underdamped spring from 0 to 1 at `t_ms`, the form Motion samples.
fn spring_unit(t_ms: f64, stiffness: f64, damping: f64) -> f64 {
    let mass = 1.0;
    let damping_ratio = damping / (2.0 * (stiffness * mass).sqrt());
    let undamped = (stiffness / mass).sqrt() / 1000.0;
    let angular = undamped * (1.0 - damping_ratio * damping_ratio).sqrt();
    let envelope = (-damping_ratio * undamped * t_ms).exp();
    1.0 - envelope
        * ((damping_ratio * undamped) / angular * (angular * t_ms).sin() + (angular * t_ms).cos())
}

/// Plays the picture back into its bubble once the app has dropped the
/// viewer: the picture flies to where it was drawn in the chat, or fades and
/// shrinks when that is off screen, while the field fades out.
fn fly_back(ctx: &egui::Context, palette: &crate::theme::Palette) {
    let Some(mut ghost) = ctx.data(|data| data.get_temp::<Ghost>(ghost_id())) else {
        return;
    };
    let now = ctx.input(|input| input.time);
    let closed = *ghost.closed.get_or_insert(now);
    let seconds = FLIGHT_SECONDS * theme::CLOSE;
    let linear = if crate::motion::screen_reader_attached(ctx) || seconds <= 0.0 {
        1.0
    } else {
        ((now - closed) as f32 / seconds).clamp(0.0, 1.0)
    };
    if linear >= 1.0 {
        ctx.data_mut(|data| data.remove::<Ghost>(ghost_id()));
        return;
    }
    ctx.data_mut(|data| data.insert_temp(ghost_id(), ghost.clone()));
    let t = theme::ease_out(linear);
    let screen = ctx.content_rect();
    let (rect, radius, opacity) = match source_rect(ctx, &ghost.chat, &ghost.message) {
        Some(target) => {
            mark_flying(
                ctx,
                &ghost.chat,
                &ghost.message,
                seconds - (now - closed) as f32,
            );
            let end = source_radius(ctx, &ghost.chat, &ghost.message, target);
            (lerp_rect(ghost.rect, target, t), end * t, 1.0)
        }
        None => {
            let scale = 1.0 - (1.0 - theme::CLOSE_SCALE) * t;
            (
                Rect::from_center_size(ghost.rect.center(), ghost.rect.size() * scale),
                0.0,
                1.0 - t,
            )
        }
    };
    let back = 1.0 - t;
    egui::Area::new(egui::Id::new("image-preview-ghost-area"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(false)
        .show(ctx, |ui| {
            ui.painter()
                .rect_filled(screen, 0.0, palette.shadow.gamma_multiply(back));
            ui.painter()
                .rect_filled(screen, 0.0, palette.overlay.gamma_multiply(back));
            let image = match &ghost.picture {
                GhostPicture::File(path) => crate::ui::widgets::full_size_file_image(ui, path),
                GhostPicture::Frame(frame) => {
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(frame))
                }
                GhostPicture::Poster(uri) => egui::Image::new(uri.clone()),
            };
            paint_flying(
                ui,
                image,
                rect,
                radius,
                egui::Color32::WHITE.gamma_multiply(opacity),
            );
        });
    ctx.request_repaint();
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    // The album carries every thumbnail of the chat, so it is moved out for
    // the frame and put back at the end instead of being copied: a copy per
    // repaint is megabytes of bytes nobody reads. The preview holds two ids and
    // two strings, and it is moved the same way so the drawing functions can
    // borrow it mutably while `app` is borrowed too.
    let Some(mut preview) = app.image_preview.take() else {
        // Nothing on screen: the strip centres on the current item again the
        // next time the viewer opens, whatever it was left on.
        ctx.data_mut(|data| {
            data.remove::<(String, String, Option<usize>)>(egui::Id::new("viewer-strip-centred"))
        });
        fly_back(ctx, &app.palette);
        return;
    };
    let album = std::mem::take(&mut app.viewer_media);
    let palette = app.palette;
    // The album the viewer browses, and where this picture sits in it.
    let position = preview.position(&album);
    let item = position.and_then(|index| album.get(index));
    let message = preview.message().to_owned();
    let chat = preview.chat().to_owned();
    let thumbnail = item.and_then(|item| item.thumbnail.as_deref());
    // With no album item yet, the file's own name still says whether this is a
    // clip, so a clip is never handed to the image decoder.
    let clip = preview.shows_video(&album);
    let strip = album.len() > 1;
    let mut actions: Vec<Action> = Vec::new();

    let mut backdrop_clicked = false;
    // The viewer fills the window instead of floating a dialog over the chat:
    // the header across the top, the picture on the field, and the album along
    // the bottom, which is the shape WhatsApp's own viewer uses. It stays a
    // modal, so it keeps the keyboard: Tab walks its own controls and Enter
    // cannot reach the chat behind it. The field and every control over it come
    // from the palette, so the light theme reads as well as the dark one.
    let screen = ctx.content_rect();
    // The picture flies out of its bubble: the field fades in over the panel
    // time, the picture follows the spring, and the controls arrive after
    // 60% of the flight. A screen reader gets the viewer at once.
    let elapsed = if crate::motion::screen_reader_attached(ctx) {
        FLIGHT_SECONDS
    } else {
        open_elapsed(ctx)
    };
    let linear = (elapsed / FLIGHT_SECONDS).clamp(0.0, 1.0);
    let fade = theme::ease_out((elapsed / theme::MOTION_PANEL).clamp(0.0, 1.0));
    let chrome = ((linear - 0.6) / 0.4).clamp(0.0, 1.0);
    let flight = (linear < 1.0)
        .then(|| source_rect(ctx, &chat, &message))
        .flatten()
        .map(|from| {
            (
                from,
                flight_progress(elapsed),
                source_radius(ctx, &chat, &message, from),
            )
        });
    if flight.is_some() {
        mark_flying(ctx, &chat, &message, FLIGHT_SECONDS * (1.0 - linear));
    }
    if linear < 1.0 {
        ctx.request_repaint();
    }
    let area = egui::Area::new(egui::Id::new("image-preview"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min);
    let response = egui::Modal::new(egui::Id::new("image-preview"))
        .area(area)
        .frame(egui::Frame::new().fill(palette.overlay.gamma_multiply(fade)))
        .backdrop_color(palette.shadow.gamma_multiply(fade))
        .show(ctx, |ui| {
            // Sized to the window with no margin around it, so the field runs
            // edge to edge.
            ui.set_min_size(screen.size());
            let window = Rect::from_min_size(screen.min, screen.size());
            ui.painter()
                .rect_filled(window, 0.0, palette.overlay.gamma_multiply(fade));
            // Claimed first, so every control drawn over it wins the click.
            // What reaches this is a click on the empty field, which closes the
            // viewer, as it does on the phone.
            let (_, backdrop) = ui.allocate_exact_size(window.size(), Sense::click());
            let (header_rect, stage, strip_rect) = panes(window, strip);

            // Inset from the window's edges, so the title does not touch the
            // left one and the close button keeps clear of the right.
            let inset = header_rect.shrink2(vec2(14.0, 0.0));
            let mut head = ui.new_child(UiBuilder::new().max_rect(inset));
            head.set_clip_rect(header_rect);
            head.set_opacity(chrome);
            header(
                app,
                &mut head,
                &preview,
                item,
                chat.as_str(),
                clip,
                &mut actions,
            );

            let mut body = ui.new_child(UiBuilder::new().max_rect(stage));
            body.set_clip_rect(stage);
            // The file on screen is the preview's, not the album item's: a clip
            // opened from the message menu is shown before the album arrives,
            // and the album may never hold that message at all.
            // A motion photo's clip plays over its photo, which stays the item.
            let shown = if clip {
                preview.clip_path()
            } else {
                preview.path()
            }
            .map(Path::to_path_buf);
            let drawn = if clip {
                video(
                    app,
                    &mut body,
                    shown.as_deref(),
                    thumbnail,
                    stage,
                    &chat,
                    &message,
                    flight,
                    &mut actions,
                )
            } else {
                still(
                    app,
                    &mut body,
                    &mut preview,
                    shown.as_deref(),
                    thumbnail,
                    clip,
                    &chat,
                    &message,
                    stage,
                    flight,
                    &mut actions,
                )
            };
            if strip {
                navigation_overlay(
                    ui,
                    stage,
                    drawn.unwrap_or(stage),
                    &palette,
                    app.locale,
                    clip,
                    &mut actions,
                );
                let mut bar = ui.new_child(UiBuilder::new().max_rect(strip_rect));
                bar.set_clip_rect(strip_rect);
                bar.set_opacity(chrome);
                strip_bar(
                    app,
                    &mut bar,
                    &album,
                    &chat,
                    (&message, preview.card()),
                    strip_rect,
                    &mut actions,
                );
            }
            backdrop_clicked = backdrop.clicked();
        });
    if response.should_close() || backdrop_clicked {
        actions.push(Action::CloseImagePreview);
    }
    app.image_preview = Some(preview);
    app.viewer_media = album;
    app.actions.extend(actions);
}

/// Header, stage and album strip across the window, top to bottom. Without an
/// album the strip is empty and the stage runs to the bottom edge.
fn panes(window: Rect, strip: bool) -> (Rect, Rect, Rect) {
    let header = Rect::from_min_max(window.min, pos2(window.max.x, window.min.y + HEADER));
    let strip_rect = if strip {
        Rect::from_min_max(pos2(window.min.x, window.max.y - STRIP), window.max)
    } else {
        Rect::from_min_max(pos2(window.max.x, window.max.y), window.max)
    };
    let stage = Rect::from_min_max(
        pos2(window.min.x, header.max.y),
        pos2(window.max.x, strip_rect.min.y),
    );
    (header, stage, strip_rect)
}

/// Title, the actions that reach the message, and the zoom controls.
fn header(
    app: &mut App,
    ui: &mut egui::Ui,
    preview: &PreviewState,
    item: Option<&MediaItem>,
    chat: &str,
    clip: bool,
    actions: &mut Vec<Action>,
) {
    let palette = app.palette;
    // The chat's name is the useful title once the viewer browses an album;
    // the file name is all there is for a picture opened on its own.
    let title = app
        .chat(chat)
        .map(|known| app.chat_title(known))
        .unwrap_or_else(|| {
            preview
                .path()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .map_or_else(
                    || crate::i18n::gettext(app.locale, "Image").into_owned(),
                    str::to_owned,
                )
        });
    let message = preview.message().to_owned();
    // The file already on the computer, when the item has one: the header
    // offers Save as for it, as the message menu does, instead of fetching it
    // again and leaving the viewer.
    let saved = item.and_then(|item| item.path.clone());
    let has_message = item.is_some();
    // A motion photo's clip, when the photo on screen has one.
    let motion = app
        .conversations
        .get(chat)
        .and_then(|conversation| conversation.message(&message))
        .and_then(|row| match &row.content {
            crate::model::Content::Image {
                motion: Some(motion),
                ..
            } => Some(motion.clone()),
            _ => None,
        });
    // The row fills the header and centers its controls vertically alongside
    // the window caption buttons. Right-to-left places actions at the right;
    // the title takes the remaining width on the left.
    ui.allocate_ui_with_layout(
        ui.available_size(),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if theme::icon_button(
                    ui,
                    Icon::X,
                    18.0,
                    palette.secondary,
                    palette.text,
                    crate::i18n::gettext(app.locale, "Close preview (Esc)").as_ref(),
                )
                .clicked()
                {
                    actions.push(Action::CloseImagePreview);
                }
                if let Some(path) = preview.path() {
                    if theme::icon_button(
                        ui,
                        Icon::ExternalLink,
                        18.0,
                        palette.secondary,
                        palette.text,
                        crate::i18n::gettext(app.locale, "Open in another app").as_ref(),
                    )
                    .clicked()
                    {
                        actions.push(Action::OpenFile(path.to_owned()));
                    }
                    let copy_hint = format!(
                        "{} ({})",
                        crate::i18n::gettext(app.locale, "Copy image"),
                        super::keys::label("Ctrl+C"),
                    );
                    // A clip is played, never decoded as an image, so copying it
                    // could only fail: the action is left out here, and the
                    // shortcut is gated on the same rule in `keys`.
                    if !clip
                        && theme::icon_button(
                            ui,
                            Icon::Copy,
                            18.0,
                            palette.secondary,
                            palette.text,
                            &copy_hint,
                        )
                        .clicked()
                    {
                        actions.push(Action::CopyImage(path.to_owned()));
                    }
                }
                if let Some(motion) = &motion {
                    motion_button(app, ui, preview, motion, chat, &message, actions);
                }
                // Zoom is for a picture: a clip fills the stage at its own shape,
                // so the controls are left out while one is on screen.
                if !clip {
                    ui.add_space(8.0);
                    // Right to left: zoom in, the current scale, zoom out.
                    if theme::icon_button(
                        ui,
                        Icon::Plus,
                        18.0,
                        palette.secondary,
                        palette.text,
                        crate::i18n::gettext(app.locale, "Zoom in").as_ref(),
                    )
                    .clicked()
                    {
                        actions.push(Action::ZoomImageIn);
                    }
                    // One control shows the scale and switches between fitting the
                    // window and the original size.
                    let (label, hint, action) = if preview.is_fit() {
                        (
                            crate::i18n::gettext(app.locale, "Fit").into_owned(),
                            crate::i18n::gettext(app.locale, "Show at original size"),
                            Action::ImageActualSize,
                        )
                    } else {
                        (
                            format!("{:.0}%", preview.zoom() * 100.0),
                            crate::i18n::gettext(app.locale, "Fit to the window (0)"),
                            Action::FitImage,
                        )
                    };
                    if theme::soft_button(ui, &palette, None, &label, false)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        actions.push(action);
                    }
                    if theme::icon_button(
                        ui,
                        Icon::Minus,
                        18.0,
                        palette.secondary,
                        palette.text,
                        crate::i18n::gettext(app.locale, "Zoom out").as_ref(),
                    )
                    .clicked()
                    {
                        actions.push(Action::ZoomImageOut);
                    }
                }
                // What the message itself can do, when the viewer knows which one
                // it is. A picture opened on its own has nothing to act on.
                if has_message {
                    ui.add_space(8.0);
                    if theme::icon_button(
                        ui,
                        Icon::Reply,
                        18.0,
                        palette.secondary,
                        palette.text,
                        crate::i18n::gettext(app.locale, "Reply").as_ref(),
                    )
                    .clicked()
                    {
                        actions.push(Action::CloseImagePreview);
                        actions.push(Action::Reply(message.clone()));
                    }
                    if theme::icon_button(
                        ui,
                        Icon::Smile,
                        18.0,
                        palette.secondary,
                        palette.text,
                        crate::i18n::gettext(app.locale, "React").as_ref(),
                    )
                    .clicked()
                    {
                        // The viewer draws after the picker, so it has to go first:
                        // otherwise the picker opens behind it and cannot be used.
                        actions.push(Action::CloseImagePreview);
                        actions.push(Action::OpenReactionPicker {
                            chat: chat.to_owned(),
                            message: message.clone(),
                            beside_menu: false,
                        });
                    }
                    if theme::icon_button(
                        ui,
                        Icon::Forward,
                        18.0,
                        palette.secondary,
                        palette.text,
                        crate::i18n::gettext(app.locale, "Forward").as_ref(),
                    )
                    .clicked()
                    {
                        actions.push(Action::CloseImagePreview);
                        actions.push(Action::ShowDialog(Dialog::Forward {
                            chat: chat.to_owned(),
                            messages: vec![message.clone()],
                        }));
                    }
                    match saved {
                        // Already on the computer: hand it to the save dialog, the
                        // way the message menu does.
                        Some(path) => {
                            let name = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("attachment")
                                .to_owned();
                            if theme::icon_button(
                                ui,
                                Icon::Download,
                                18.0,
                                palette.secondary,
                                palette.text,
                                crate::i18n::gettext(app.locale, "Save as…").as_ref(),
                            )
                            .clicked()
                            {
                                actions.push(Action::CloseImagePreview);
                                actions.push(Action::SaveAttachmentAs { path, name });
                            }
                        }
                        // Not here yet: fetch it, and return to the chat with it.
                        None => {
                            if theme::icon_button(
                                ui,
                                Icon::Download,
                                18.0,
                                palette.secondary,
                                palette.text,
                                crate::i18n::gettext(app.locale, "Download").as_ref(),
                            )
                            .clicked()
                            {
                                actions.push(Action::CloseImagePreview);
                                actions.push(Action::Download {
                                    // Asked for in the viewer.
                                    manual: true,
                                    card: None,
                                    chat: chat.to_owned(),
                                    message: message.clone(),
                                });
                            }
                        }
                    }
                    if theme::icon_button(
                        ui,
                        Icon::MessageCircle,
                        18.0,
                        palette.secondary,
                        palette.text,
                        crate::i18n::gettext(app.locale, "Show in the chat").as_ref(),
                    )
                    .clicked()
                    {
                        // Closing leaves the chat where it was, so the message the
                        // viewer was showing is brought into view. `ScrollTo` also
                        // pages older archive history toward it when it is not in
                        // the transcript yet, which is the usual case after
                        // stepping back through the album.
                        actions.push(Action::CloseImagePreview);
                        actions.push(Action::ScrollTo(message.clone()));
                    }
                    ui.add_space(8.0);
                }
                // Last, so it takes what the actions left, scrolling if it needs
                // more rather than drawn under them.
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    crate::ui::widgets::scrolling_text(
                        ui,
                        &title,
                        theme::semibold(14.0),
                        palette.text,
                        palette.overlay,
                    );
                });
            });
        },
    );
}

/// Plays a motion photo's clip over the photo, or goes back to the photo
/// while it plays. A clip not downloaded yet is fetched first, and a failed
/// download says why; a click tries again.
fn motion_button(
    app: &App,
    ui: &mut egui::Ui,
    preview: &PreviewState,
    motion: &crate::model::Motion,
    chat: &str,
    message: &str,
    actions: &mut Vec<Action>,
) {
    let palette = app.palette;
    if matches!(motion.state, crate::model::MediaState::Downloading) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
        theme::paint_spinner(ui, rect, 18.0, palette.secondary);
        return;
    }
    let playing = preview.motion().is_some();
    let (icon, hint) = match &motion.state {
        crate::model::MediaState::Failed(error) if !playing => (
            Icon::CircleAlert,
            std::borrow::Cow::Borrowed(error.as_str()),
        ),
        _ if playing => (
            Icon::Image,
            crate::i18n::gettext(app.locale, "Show the photo"),
        ),
        _ => (
            Icon::Play,
            crate::i18n::gettext(app.locale, "Play motion photo"),
        ),
    };
    if !theme::icon_button(ui, icon, 18.0, palette.secondary, palette.text, &hint).clicked() {
        return;
    }
    actions.push(match (playing, &motion.path) {
        (true, _) => Action::ViewImage {
            message: message.to_owned(),
            card: None,
        },
        (false, Some(path)) => Action::ExpandVideo {
            message: message.to_owned(),
            path: path.clone(),
        },
        (false, None) => Action::DownloadMotion {
            chat: chat.to_owned(),
            message: message.to_owned(),
        },
    });
}

/// The picture itself, zoomed and panned exactly as before.
#[allow(clippy::too_many_arguments)]
fn still(
    app: &mut App,
    ui: &mut egui::Ui,
    preview: &mut PreviewState,
    path: Option<&Path>,
    thumbnail: Option<&[u8]>,
    video: bool,
    chat: &str,
    message: &str,
    stage: Rect,
    flight: Option<Flight>,
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    let palette = app.palette;
    let canvas = stage.size();
    // An item whose attachment is not here yet is shown for what it is, with
    // its thumbnail and a way to fetch it, instead of standing in for another
    // picture.
    let Some(path) = path else {
        let idle = crate::model::MediaState::Idle;
        let state = app.viewer_media_state.get(message).unwrap_or(&idle);
        pending(
            app, ui, thumbnail, video, message, chat, stage, state, actions,
        );
        return None;
    };
    // Registered with the image cache like every other draw site, so a sweep
    // never releases the picture while it is on screen.
    let image = crate::ui::widgets::file_image(ui, path);
    match image.load_for_size(ui.ctx(), canvas) {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            let size = display_size(texture.size, canvas, preview.is_fit(), preview.zoom());
            if preview.is_fit() && texture.size.x > 0.0 {
                preview.set_fit_scale(size.x / texture.size.x);
            }
            // Flying out of the bubble: the picture is painted by itself on
            // its way, and the scroll area takes over once it has landed.
            if let Some((from, progress, radius)) = flight {
                let to = Rect::from_center_size(stage.center(), size);
                let rect = lerp_rect(from, to, progress);
                paint_flying(
                    ui,
                    image,
                    rect,
                    radius * (1.0 - progress).max(0.0),
                    egui::Color32::WHITE,
                );
                return Some(to);
            }
            let scroll_id = ui.make_persistent_id(egui::IdSalt::new("image-preview-scroll"));
            let trackpad = app.scrolling.from_trackpad();
            // Read before the scroll area, which would otherwise take the
            // wheel. The zoom lands on the preview the viewer will put back,
            // so the next frame lays out the new size.
            let zoom = zoom_input(ui, stage, trackpad).map(|(factor, pointer)| {
                preview.zoom_by(factor);
                let zoomed = display_size(texture.size, canvas, preview.is_fit(), preview.zoom());
                (pointer, zoomed)
            });
            let output = egui::ScrollArea::both()
                .id_salt("image-preview-scroll")
                .auto_shrink([false, false])
                // egui drags only on touch screens by default.
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: egui::scroll_area::DragScroll::Always,
                    ..Default::default()
                })
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_drag_cursor(egui::CursorIcon::Grabbing)
                .show(ui, |ui| {
                    ui.allocate_ui_with_layout(
                        canvas.max(size),
                        Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            let image_response =
                                ui.add(image.fit_to_exact_size(size).sense(egui::Sense::click()));
                            // The same three actions the header offers, on the
                            // picture itself: copy it, save it, or hand it to
                            // another app.
                            let copy_label = crate::i18n::gettext(app.locale, "Copy image");
                            let save_label = crate::i18n::gettext(app.locale, "Save as…");
                            let open_label =
                                crate::i18n::gettext(app.locale, "Open in another app");
                            let menu_width = crate::ui::widgets::menu_width(
                                ui,
                                &[&copy_label, &save_label, &open_label],
                                true,
                            )
                            .max(180.0);
                            egui::Popup::context_menu(&image_response)
                                .width(menu_width)
                                .frame(crate::ui::widgets::menu_frame(&palette))
                                .show_frosted(ui.ctx(), |ui| {
                                    if crate::ui::widgets::menu_item(
                                        ui,
                                        &palette,
                                        Some(Icon::Copy),
                                        &copy_label,
                                    ) {
                                        app.actions.push(Action::CopyImage(path.to_owned()));
                                    }
                                    if crate::ui::widgets::menu_item(
                                        ui,
                                        &palette,
                                        Some(Icon::Download),
                                        &save_label,
                                    ) {
                                        let name = path
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or("image.png")
                                            .to_owned();
                                        app.actions.push(Action::SaveAttachmentAs {
                                            path: path.to_owned(),
                                            name,
                                        });
                                    }
                                    if crate::ui::widgets::menu_item(
                                        ui,
                                        &palette,
                                        Some(Icon::ExternalLink),
                                        &open_label,
                                    ) {
                                        app.actions.push(Action::OpenFile(path.to_owned()));
                                    }
                                });
                            (
                                ui.layout()
                                    .align_size_within_rect(size, image_response.rect),
                                image_response
                                    .interact_pointer_pos()
                                    .filter(|_| image_response.double_clicked()),
                            )
                        },
                    )
                    .inner
                });
            let (picture, double_clicked) = output.inner;
            if preview.is_fit() {
                let rect = Rect::from_center_size(stage.center(), size);
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        ghost_id(),
                        Ghost {
                            picture: GhostPicture::File(path.to_owned()),
                            chat: chat.to_owned(),
                            message: message.to_owned(),
                            rect,
                            closed: None,
                        },
                    );
                });
            }
            // Stored after the scroll area, which clamps its offset to this
            // frame's size; the next frame lays out the zoomed size with the
            // pointed-at pixel still under the pointer.
            if let Some((pointer, zoomed)) = zoom {
                let mut scroll = output.state;
                scroll.offset = crate::image_preview::anchored_offset(
                    canvas,
                    size,
                    zoomed,
                    output.state.offset,
                    pointer,
                    pointer,
                );
                scroll.store(ui.ctx(), scroll_id);
            }
            // A double click switches between fitting the window and the
            // original size. The original size opens with the clicked point in
            // the middle: the offset is stored for the next frame, which lays
            // out the new size.
            if let Some(pos) = double_clicked {
                if preview.is_fit() {
                    let mut scroll = output.state;
                    scroll.offset = crate::image_preview::anchored_offset(
                        canvas,
                        size,
                        texture.size,
                        output.state.offset,
                        pos - stage.min,
                        canvas / 2.0,
                    );
                    scroll.store(ui.ctx(), scroll_id);
                    preview.actual_size();
                } else {
                    preview.fit();
                }
            }
            // The part of the picture the scroll area leaves on screen.
            Some(picture.intersect(output.inner_rect))
        }
        Ok(egui::load::TexturePoll::Pending { .. }) => {
            theme::paint_spinner(ui, stage, 28.0, palette.accent);
            None
        }
        Err(_) => {
            let path = path.to_owned();
            ui.allocate_ui_with_layout(
                canvas,
                Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.label(crate::i18n::gettext(
                        app.locale,
                        "This image could not be displayed in Zeus.",
                    ));
                    if ui
                        .button(crate::i18n::gettext(app.locale, "Open externally"))
                        .clicked()
                    {
                        app.actions.push(Action::OpenFile(path.clone()));
                    }
                },
            );
            None
        }
    }
}

/// An item whose attachment has not been downloaded: its own thumbnail, the
/// clip or picture it is, and the button that fetches it. Nothing here stands
/// in for another file.
#[allow(clippy::too_many_arguments)]
fn pending(
    app: &App,
    ui: &mut egui::Ui,
    thumbnail: Option<&[u8]>,
    video: bool,
    message: &str,
    chat: &str,
    stage: Rect,
    state: &crate::model::MediaState,
    actions: &mut Vec<Action>,
) {
    let palette = app.palette;
    let disc = Rect::from_center_size(stage.center(), Vec2::splat(220.0));
    if let Some(bytes) = thumbnail {
        egui::Image::new(thumbnail_uri(ui.ctx(), chat, message, bytes))
            .fit_to_exact_size(disc.size())
            .corner_radius(theme::RADIUS)
            .paint_at(ui, disc);
    } else {
        theme::paint_icon(
            ui,
            if video { Icon::Video } else { Icon::Image },
            disc,
            48.0,
            palette.secondary,
        );
    }
    // A download that is running says so, and one that failed shows the
    // worker's own message, the way the chat bubble does: a button that
    // looks dead is what the review asked to avoid.
    if matches!(state, crate::model::MediaState::Downloading) {
        theme::paint_spinner(ui, disc, 28.0, palette.text);
        return;
    }
    let button = Rect::from_center_size(
        pos2(stage.center().x, disc.bottom() + 34.0),
        vec2(160.0, 32.0),
    );
    if let crate::model::MediaState::Failed(error) = state {
        ui.painter().text(
            pos2(stage.center().x, disc.bottom() + 12.0),
            egui::Align2::CENTER_CENTER,
            error,
            theme::regular(12.5),
            palette.danger,
        );
    }
    let response = ui
        .interact(
            button,
            egui::Id::new("viewer-pending-download"),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter()
        .rect_filled(button, theme::RADIUS_CONTROL, palette.surface_hover);
    ui.painter().text(
        button.center(),
        egui::Align2::CENTER_CENTER,
        crate::i18n::gettext(app.locale, "Download"),
        theme::regular(13.5),
        palette.text,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            crate::i18n::gettext(app.locale, "Download").as_ref(),
        )
    });
    theme::reveal_focus(&response);
    if response.clicked() {
        actions.push(Action::Download {
            // Asked for in the viewer.
            manual: true,
            card: None,
            chat: chat.to_owned(),
            message: message.to_owned(),
        });
    }
}

/// A video plays in place, through the same player the chat bubble uses, so it
/// carries sound and its position is the one the player reports.
#[allow(clippy::too_many_arguments)]
fn video(
    app: &mut App,
    ui: &mut egui::Ui,
    path: Option<&Path>,
    thumbnail: Option<&[u8]>,
    stage: Rect,
    chat: &str,
    message: &str,
    flight: Option<Flight>,
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    let palette = app.palette;
    // A clip keeps its own shape: the decoded frame knows it, and before the
    // first frame the poster does, so a poster that flies in lands at the
    // clip's shape. 16:9 is only for a clip with neither.
    let status = app.video.status(message);
    let poster_size = || {
        let bytes = thumbnail?;
        egui::Image::new(thumbnail_uri(ui.ctx(), chat, message, bytes))
            .load_for_size(ui.ctx(), stage.size())
            .ok()?
            .size()
    };
    let positive = |size: &Vec2| size.x > 0.0 && size.y > 0.0;
    let aspect = status
        .as_ref()
        .and_then(|status| status.frame.as_ref())
        .map(|frame| vec2(frame.size()[0] as f32, frame.size()[1] as f32))
        .filter(positive)
        .or_else(|| poster_size().filter(positive))
        .unwrap_or_else(|| vec2(16.0, 9.0));
    let size = fit(aspect, stage.size());
    let media = Rect::from_center_size(stage.center(), size);
    // The player pauses a clip it has not been told about for 1.5 s, so the
    // viewer has to say it is still on screen, as the chat renderer does.
    app.video.saw(message);
    // What flies back to the bubble when the viewer closes: the frame on
    // screen, else the poster.
    let ghost = match (
        status.as_ref().and_then(|status| status.frame.clone()),
        thumbnail,
    ) {
        (Some(frame), _) => Some(GhostPicture::Frame(frame)),
        (None, Some(bytes)) => Some(GhostPicture::Poster(thumbnail_uri(
            ui.ctx(),
            chat,
            message,
            bytes,
        ))),
        (None, None) => None,
    };
    ui.ctx().data_mut(|data| match &ghost {
        Some(picture) => {
            data.insert_temp(
                ghost_id(),
                Ghost {
                    picture: picture.clone(),
                    chat: chat.to_owned(),
                    message: message.to_owned(),
                    rect: media,
                    closed: None,
                },
            );
        }
        None => {
            data.remove::<Ghost>(ghost_id());
        }
    });
    // Flying out of the bubble: the frame is painted by itself on its way,
    // and the player's controls arrive once it has landed.
    if let Some((from, progress, radius)) = flight {
        let rect = lerp_rect(from, media, progress);
        let radius = radius * (1.0 - progress).max(0.0);
        match ghost {
            Some(GhostPicture::Frame(frame)) => {
                paint_flying(
                    ui,
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(&frame)),
                    rect,
                    radius,
                    egui::Color32::WHITE,
                );
            }
            Some(GhostPicture::Poster(uri)) => {
                paint_flying(
                    ui,
                    egui::Image::new(uri),
                    rect,
                    radius,
                    egui::Color32::WHITE,
                );
            }
            _ => {
                ui.painter().rect_filled(rect, radius, palette.shadow);
            }
        }
        return Some(media);
    }
    match status.as_ref().and_then(|status| status.frame.clone()) {
        Some(frame) => {
            ui.painter().image(
                frame.id(),
                media,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            ui.painter().rect_filled(media, 8.0, palette.shadow);
            if let Some(bytes) = thumbnail {
                egui::Image::new(thumbnail_uri(ui.ctx(), chat, message, bytes))
                    .fit_to_exact_size(media.size())
                    .corner_radius(theme::RADIUS)
                    .paint_at(ui, media);
            }
        }
    }
    // Clicking the frame plays or pauses, as in the bubble. A clip that is not
    // here yet is offered for download instead.
    let response = ui
        .interact(media, egui::Id::new("viewer-video"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let downloading = path.is_none()
        && matches!(
            app.viewer_media_state.get(message),
            Some(crate::model::MediaState::Downloading)
        );
    let failed = match (path, app.viewer_media_state.get(message)) {
        (None, Some(crate::model::MediaState::Failed(error))) => Some(error.as_str()),
        _ => None,
    };
    let label = if path.is_some() {
        crate::i18n::gettext(app.locale, "Play or pause the video")
    } else {
        crate::i18n::gettext(app.locale, "Download")
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label.as_ref())
    });
    theme::reveal_focus(&response);
    if response.clicked() && !downloading {
        match path {
            Some(path) => actions.push(Action::PlayVideo {
                message: message.to_owned(),
                path: path.to_owned(),
            }),
            None => actions.push(Action::Download {
                // Asked for in the viewer.
                manual: true,
                card: None,
                chat: chat.to_owned(),
                message: message.to_owned(),
            }),
        }
    }
    // A double click puts a video back in its message, as it did when it
    // covered the window on its own.
    if response.double_clicked() && path.is_some() {
        actions.push(Action::CloseImagePreview);
    }
    if let (Some(status), Some(path)) = (status.as_ref(), path) {
        // Loaded in the player: the bubble's own controls, along the bottom of
        // the frame (play, the time, the seek bar, the sound, and back to the
        // message), inside the frame, since the body is clipped to the stage.
        if status.frame.is_none() {
            let disc = Rect::from_center_size(media.center(), Vec2::splat(64.0));
            theme::paint_spinner(ui, disc, 28.0, palette.text);
        }
        super::conversation::video_controls(
            ui,
            &super::conversation::VideoControls {
                player: &app.video,
                locale: app.locale,
                accent: palette.accent,
                expanded: true,
            },
            message,
            path,
            media,
            status,
            actions,
        );
    } else if downloading {
        let disc = Rect::from_center_size(media.center(), Vec2::splat(64.0));
        ui.painter()
            .circle_filled(disc.center(), 32.0, palette.shadow);
        theme::paint_spinner(ui, disc, 28.0, palette.text);
    } else {
        let disc = Rect::from_center_size(media.center(), Vec2::splat(64.0));
        ui.painter()
            .circle_filled(disc.center(), 32.0, palette.shadow);
        // A control painted over a picture, so it stays light on its own scrim
        // in either theme, exactly as the bubble paints the same button.
        theme::paint_icon(ui, Icon::Play, disc, 28.0, palette.text);
        if let Some(error) = failed {
            // Inside the frame. A wide window makes the 16:9 rectangle as
            // tall as the stage, and the body is clipped to the stage, so a
            // line below the frame is never seen.
            ui.painter().text(
                pos2(media.center().x, media.bottom() - 18.0),
                egui::Align2::CENTER_CENTER,
                error,
                theme::regular(12.5),
                palette.danger,
            );
        }
    }
    Some(media)
}

/// The album along the bottom: a thumbnail per item, the current one ringed.
#[allow(clippy::too_many_arguments)]
fn strip_bar(
    app: &App,
    ui: &mut egui::Ui,
    album: &[MediaItem],
    chat: &str,
    current: (&str, Option<usize>),
    rect: Rect,
    actions: &mut Vec<Action>,
) {
    let palette = app.palette;
    let is_current = |item: &MediaItem| (item.id.as_str(), item.card) == current;
    ui.painter().rect_filled(rect, 0.0, palette.shadow);
    let inner = rect.shrink2(vec2(STRIP_PAD, STRIP_PAD));
    // Only the room the row cannot fill becomes padding. Adding half the width
    // on each side whatever the row needs made the content wider than the
    // strip, so the bar showed even for three items.
    let row = album.len() as f32 * THUMB + album.len().saturating_sub(1) as f32 * STRIP_GAP;
    let pad = ((inner.width() - row) * 0.5).max(0.0);
    // The strip is centred on the current item once, when the viewer moves to
    // another one. Asking for it every frame fights the wheel and the bar: egui
    // pulls the strip back to the current thumbnail as soon as the user scrolls
    // it by hand, so the album could never be browsed with the mouse.
    // The key carries the chat as well as the message: ids are scoped to a
    // chat, so the same id in another chat is another item and has to be
    // centred on its own.
    let centred = egui::Id::new("viewer-strip-centred");
    let last: Option<(String, String, Option<usize>)> =
        ui.ctx().data(|data| data.get_temp(centred));
    let key = (chat.to_owned(), current.0.to_owned(), current.1);
    let moved = last.as_ref() != Some(&key);
    if moved {
        ui.ctx().data_mut(|data| data.insert_temp(centred, key));
    }
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner));
    // The app style floats scrollbars over content. This strip needs the bar
    // under the thumbs, with a handle that is not the same colour as its rail.
    {
        let scroll = &mut child.spacing_mut().scroll;
        scroll.floating = false;
        scroll.bar_width = 6.0;
        scroll.bar_inner_margin = 3.0;
        scroll.bar_outer_margin = 2.0;
        scroll.foreground_color = true;
    }
    egui::ScrollArea::horizontal()
        .id_salt("viewer-strip")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.set_min_height(THUMB);
                ui.add_space(pad);
                for (index, item) in album.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(STRIP_GAP);
                    }
                    let (thumb_rect, response) =
                        ui.allocate_exact_size(Vec2::splat(THUMB), Sense::click());
                    if moved && is_current(item) {
                        // Keep the picture being looked at in the middle of the
                        // strip as the album is stepped through.
                        ui.scroll_to_rect_animation(
                            thumb_rect,
                            Some(egui::Align::Center),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                    if ui.is_rect_visible(thumb_rect) {
                        thumb(ui, &palette, chat, item, thumb_rect, is_current(item));
                    }
                    // Custom-painted and clickable, so it needs the same focus
                    // reveal and label every other custom control registers.
                    // The kind is looked up on its own so both literals stay
                    // direct arguments of `gettext` and the extractor emits
                    // them; picking between them inside the call hides both.
                    let kind = if item.video {
                        crate::i18n::gettext(app.locale, "Video")
                    } else {
                        crate::i18n::gettext(app.locale, "Photo")
                    };
                    let label = format!("{} {}", kind, index + 1);
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            is_current(item),
                            &label,
                        )
                    });
                    theme::reveal_focus(&response);
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        actions.push(Action::ViewImage {
                            message: item.id.clone(),
                            card: item.card,
                        });
                    }
                }
                ui.add_space(pad);
            });
        });
}

/// Where a strip tile's picture comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tile {
    /// The thumbnail the archive stored with the message: already the 56 px the
    /// strip wants.
    Stored,
    /// The message's kind icon. A tile is never drawn from the file itself: a
    /// 56 px square is not worth decoding a whole photo on the interface
    /// thread, nor holding its full-size texture for a strip that scrolls.
    Icon,
}

fn tile_of(item: &MediaItem) -> Tile {
    if item.thumbnail.is_some() {
        Tile::Stored
    } else {
        Tile::Icon
    }
}

#[cfg(test)]
mod tile_tests {
    use super::*;
    use crate::model::MediaItem;
    use std::path::PathBuf;

    /// The strip draws the thumbnail the archive stored, and the message's
    /// kind icon when there is none: decoding the file for a 56 px tile
    /// would run on the interface thread and hold its full-size texture.
    #[test]
    fn a_tile_is_drawn_from_the_stored_thumbnail_or_not_at_all() {
        let stored = MediaItem {
            card: None,
            id: "m1".into(),
            timestamp: 0,
            video: false,
            path: Some(PathBuf::from("/photos/a.jpg")),
            thumbnail: Some(vec![1, 2, 3]),
        };
        assert_eq!(tile_of(&stored), Tile::Stored);
        let file_only = MediaItem {
            thumbnail: None,
            ..stored.clone()
        };
        assert_eq!(tile_of(&file_only), Tile::Icon);
        let clip = MediaItem {
            video: true,
            ..file_only
        };
        assert_eq!(tile_of(&clip), Tile::Icon);
    }
}

fn thumb(
    ui: &egui::Ui,
    palette: &crate::theme::Palette,
    chat: &str,
    item: &MediaItem,
    rect: Rect,
    current: bool,
) {
    ui.painter()
        .rect_filled(rect, theme::RADIUS_SMALL, palette.surface_hover);
    match (tile_of(item), item.thumbnail.as_deref()) {
        // A message id is only unique inside its chat, and the image cache
        // keeps the first bytes for a URI, so the chat has to be part of it.
        (Tile::Stored, Some(bytes)) => {
            // A carousel's cards share their message's id, and each has its
            // own preview, so the card is part of the key too.
            let key = match item.card {
                Some(card) => format!("{}#{card}", item.id),
                None => item.id.clone(),
            };
            egui::Image::new(thumbnail_uri(ui.ctx(), chat, &key, bytes))
                .fit_to_exact_size(rect.size())
                .corner_radius(theme::RADIUS_SMALL)
                .paint_at(ui, rect);
        }
        _ => {
            theme::paint_icon(
                ui,
                if item.video { Icon::Video } else { Icon::Image },
                rect,
                22.0,
                palette.secondary,
            );
        }
    }
    if item.video {
        let disc = Rect::from_center_size(rect.center(), Vec2::splat(18.0));
        ui.painter()
            .circle_filled(disc.center(), 9.0, palette.shadow);
        // Over the thumbnail, so it stays light in either theme.
        theme::paint_icon(ui, Icon::Play, disc, 12.0, palette.text);
    }
    if current {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(theme::RADIUS_SMALL),
            Stroke::new(2.0, palette.accent),
            egui::StrokeKind::Inside,
        );
    }
}

/// Previous and next buttons over the visible part of the picture or clip,
/// shown while the pointer is over it. A picture too narrow for both gets a
/// band just wide enough, centred on it, so the buttons sit at its edges
/// instead of on top of each other. Over a clip the arrow keys jump through
/// it, so the tooltips name no key there.
fn navigation_overlay(
    ui: &mut egui::Ui,
    area: Rect,
    image: Rect,
    palette: &theme::Palette,
    locale: crate::i18n::Locale,
    clip: bool,
    actions: &mut Vec<Action>,
) {
    const DIAMETER: f32 = 40.0;
    const INSET: f32 = 12.0;
    let min_width = 2.0 * (DIAMETER + INSET) + DIAMETER;
    let mut band = image;
    if band.width() < min_width {
        band = Rect::from_center_size(band.center(), vec2(min_width, band.height()));
    }
    let band = band.intersect(area);
    if band.height() < DIAMETER || band.width() < 2.0 * DIAMETER {
        return;
    }
    if ui.input(|input| input.pointer.hover_pos()).is_none() || !ui.rect_contains_pointer(band) {
        return;
    }
    let inset = INSET.min((band.width() - 2.0 * DIAMETER) / 2.0);
    let (previous, next) = if clip {
        ("Previous", "Next")
    } else {
        ("Previous (Left arrow)", "Next (Right arrow)")
    };
    for (right, icon, hint, step) in [
        (false, Icon::ChevronLeft, previous, -1i8),
        (true, Icon::ChevronRight, next, 1i8),
    ] {
        let x = if right {
            band.right() - inset - DIAMETER / 2.0
        } else {
            band.left() + inset + DIAMETER / 2.0
        };
        let rect = Rect::from_center_size(pos2(x, band.center().y), Vec2::splat(DIAMETER));
        ui.scope_builder(
            UiBuilder::new()
                .id_salt(("image-preview-navigation", right))
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                if theme::circle_button(
                    ui,
                    icon,
                    DIAMETER,
                    palette.overlay.gamma_multiply(0.85),
                    palette.surface_hover,
                    palette.text,
                    crate::i18n::gettext(locale, hint).as_ref(),
                )
                .clicked()
                {
                    actions.push(Action::ViewerStep(step));
                }
            },
        );
    }
}

/// Height kept below a sticker for its favorite button and the card margins.
const STICKER_BUTTON_ROOM: f32 = 110.0;
/// A sticker's side in the viewer, as in WhatsApp's: a card, not the window.
/// Animated frames are decoded at most 320 pixels wide, so this stays sharp.
const STICKER_VIEW_SIDE: f32 = 300.0;

/// Shows a sticker on a square card over the chat. A click on the sticker,
/// the backdrop, or Escape closes it.
pub fn show_sticker(app: &mut App, ctx: &egui::Context) {
    let Some(preview) = app.sticker_view.clone() else {
        return;
    };
    let palette = app.palette;
    let favorite = preview.favorite(&app.stickers_saved).cloned();
    let side = sticker_side(ctx.content_rect().size());
    let card = Frame::new()
        .fill(palette.overlay)
        .stroke(Stroke::new(1.0, palette.outline))
        .corner_radius(CornerRadius::same(theme::RADIUS_DIALOG))
        .inner_margin(Margin::same(20))
        .shadow(palette.modal_shadow());
    let response = egui::Modal::new(egui::Id::new("sticker-preview"))
        .frame(card)
        .backdrop_color(palette.shadow)
        .show(ctx, |ui| {
            ui.set_width(side);
            ui.vertical_centered(|ui| {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(side), egui::Sense::click());
                paint_sticker(ui, &preview, rect, &palette);
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    app.actions.push(Action::CloseStickerPreview);
                }
                ui.add_space(16.0);
                let (icon, label, action) = match favorite {
                    Some(saved) => (
                        Icon::StarOff,
                        "Remove from favorites",
                        Action::ForgetSticker(saved),
                    ),
                    None => (
                        Icon::Star,
                        "Add to favorites",
                        Action::SaveSticker(preview.path().to_owned()),
                    ),
                };
                let label = crate::i18n::gettext(app.locale, label);
                let button = theme::soft_button(ui, &palette, Some(icon), &label, false);
                ui.ctx()
                    .data_mut(|data| data.insert_temp(sticker_favorite_id(), button.rect));
                if button.clicked() {
                    app.actions.push(action);
                }
            });
        });
    if response.should_close() {
        app.actions.push(Action::CloseStickerPreview);
    }
}

/// Where the sticker preview's favorite button was drawn, for tests.
pub fn sticker_favorite_id() -> egui::Id {
    egui::Id::new("sticker-preview-favorite")
}

/// Side of the square a sticker is shown in, shrunk only when the window
/// cannot fit the card.
fn sticker_side(viewport: Vec2) -> f32 {
    ((viewport.y - STICKER_BUTTON_ROOM).min(viewport.x - 48.0)).clamp(64.0, STICKER_VIEW_SIDE)
}

fn paint_sticker(
    ui: &mut egui::Ui,
    preview: &crate::image_preview::StickerPreview,
    rect: Rect,
    palette: &crate::theme::Palette,
) {
    let full = Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
    let loaded = if preview.animated() {
        match crate::animation::frame(ui, preview.path(), rect, true) {
            crate::animation::Frame::Ready(texture) => {
                ui.painter().image(
                    texture.id(),
                    fit_within(texture.size_vec2(), rect),
                    full,
                    egui::Color32::WHITE,
                );
                Some(true)
            }
            crate::animation::Frame::Pending => None,
            crate::animation::Frame::Unavailable => Some(false),
        }
    } else {
        let image = crate::ui::widgets::file_image(ui, preview.path());
        match image.load_for_size(ui.ctx(), rect.size()) {
            Ok(egui::load::TexturePoll::Ready { texture }) => {
                image.paint_at(ui, fit_within(texture.size, rect));
                Some(true)
            }
            Ok(egui::load::TexturePoll::Pending { .. }) => None,
            Err(_) => Some(false),
        }
    };
    match loaded {
        Some(true) => {}
        Some(false) => theme::paint_icon(ui, Icon::Sticker, rect, 64.0, palette.secondary),
        None => theme::paint_spinner(ui, rect, 28.0, palette.accent),
    }
}

/// The largest rect of `size`'s shape centred in `rect`. Stickers are small,
/// so unlike photos they are enlarged to fill it.
fn fit_within(size: Vec2, rect: Rect) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return rect;
    }
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    Rect::from_center_size(rect.center(), size * scale)
}

/// Zoom factor the wheel or a pinch asks for over the preview area, with the
/// anchor point relative to the picture area. Each wheel notch is one header
/// zoom step. A plain mouse wheel zooms instead of scrolling, so its delta is
/// taken from the scroll area. Windows and X11 report touchpads as wheel
/// lines, so two fingers zoom there; only scrolling reported in points
/// (macOS, Wayland) keeps moving the picture.
fn zoom_input(ui: &mut egui::Ui, area: Rect, trackpad: bool) -> Option<(f32, Vec2)> {
    let (touch, hover) = ui.input(|input| {
        (
            input.multi_touch().map(|touch| touch.center_pos),
            input.pointer.hover_pos(),
        )
    });
    let anchor = touch.or(hover)?;
    if !area.contains(anchor) || touch.is_none() && !ui.rect_contains_pointer(area) {
        return None;
    }
    let (zoom_speed, line_speed) = ui.ctx().options(|options| {
        let input = &options.input_options;
        (input.scroll_zoom_speed, input.line_scroll_speed)
    });
    let step = crate::image_preview::PreviewState::ZOOM_STEP;
    let factor = ui.input_mut(|input| {
        let pinch = input.multi_touch().is_some()
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Zoom(_)));
        let mut factor = input.zoom_delta();
        if !pinch {
            // Without a pinch the zoom came from Ctrl/Cmd+wheel (Windows
            // touchpad pinches included), to which egui applies its own curve,
            // exp(scroll_zoom_speed * points). It keeps the modifiers from the
            // start of the gesture, so this checks the source, not the keys.
            factor = factor.powf(step.ln() / (zoom_speed * line_speed));
        }
        // Shift and Alt turn the wheel sideways; Ctrl pressed during a plain
        // notch must not hand the rest of it to the scroll area.
        if !trackpad
            && !input.modifiers.shift
            && !input.modifiers.alt
            && input.smooth_scroll_delta.y != 0.0
        {
            factor *= step.powf(input.smooth_scroll_delta.y / line_speed);
            input.smooth_scroll_delta.y = 0.0;
        }
        factor
    });
    (factor != 1.0).then_some((factor, anchor - area.min))
}

/// Size the image is drawn at from the texture's intrinsic pixel dimensions:
/// fitted into the canvas, or scaled by the preview's zoom factor. Zoom is
/// applied here only. The size hint passed when loading does not change the
/// texture: egui decodes raster formats (all the preview accepts) once at full
/// resolution and reports the source size, whatever size is asked for.
fn display_size(original: Vec2, canvas: Vec2, fit: bool, zoom: f32) -> Vec2 {
    let (width, height) = if fit {
        crate::image_preview::fit_size(original.x, original.y, canvas.x, canvas.y)
    } else {
        crate::image_preview::zoomed_size(original.x, original.y, zoom)
    };
    vec2(width, height)
}

/// Scales `size` to fit inside `max`, keeping its aspect ratio.
fn fit(size: Vec2, max: Vec2) -> Vec2 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return max;
    }
    let scale = (max.x / size.x).min(max.y / size.y);
    size * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flight_runs_from_the_bubble_to_the_stage() {
        let from = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 50.0));
        let to = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        assert_eq!(lerp_rect(from, to, 0.0), from);
        assert_eq!(lerp_rect(from, to, 1.0), to);
        let half = lerp_rect(from, to, 0.5);
        assert_eq!(half.min, pos2(5.0, 10.0));
        assert_eq!(half.max, pos2(255.0, 185.0));
        assert_eq!(flight_progress(0.0), 0.0);
        assert_eq!(flight_progress(FLIGHT_SECONDS), 1.0);
        let middle = flight_progress(FLIGHT_SECONDS / 2.0);
        assert!(middle > 0.5 && middle < 1.2, "{middle}");
    }

    #[test]
    fn a_flying_picture_hides_its_bubbles_picture_until_it_lands() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            assert!(!source_hidden(ctx, "chat", "one"));
            mark_flying(ctx, "chat", "one", 0.4);
            assert!(source_hidden(ctx, "chat", "one"));
            assert!(!source_hidden(ctx, "chat", "two"));
            assert!(!source_hidden(ctx, "other", "one"));
        });
        output.textures_delta.clear();
        // Once its time is up the bubble shows the picture again.
        let input = egui::RawInput {
            time: Some(1.0),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx();
            assert!(!source_hidden(ctx, "chat", "one"));
        });
        output.textures_delta.clear();
    }

    #[test]
    fn a_video_note_flies_as_a_circle_and_a_clip_with_the_bubbles_corner() {
        let ctx = egui::Context::default();
        let from = Rect::from_min_size(pos2(10.0, 10.0), vec2(180.0, 180.0));
        assert_eq!(source_radius(&ctx, "chat", "clip", from), FLIGHT_RADIUS);
        ctx.data_mut(|data| data.insert_temp(video_note_id("chat", "note"), true));
        assert_eq!(source_radius(&ctx, "chat", "note", from), 90.0);
    }

    #[test]
    fn a_video_flies_from_where_its_bubble_drew_it() {
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(pos2(20.0, 30.0), vec2(200.0, 120.0));
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx();
            ctx.data_mut(|data| data.insert_temp(video_flight_id("chat", "clip"), rect));
            assert_eq!(source_rect(ctx, "chat", "clip"), Some(rect));
            assert_eq!(source_rect(ctx, "chat", "other"), None);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn a_picture_in_the_air_is_cropped_to_its_shape_not_stretched() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            ..Default::default()
        };
        let texture = ctx.load_texture(
            "wide",
            egui::ColorImage::filled([200, 100], egui::Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let square = Rect::from_min_size(pos2(10.0, 10.0), vec2(100.0, 100.0));
        let wide = Rect::from_min_size(pos2(10.0, 10.0), vec2(200.0, 100.0));
        let uv_painted = |rect: Rect| {
            let output = ctx.run_ui(input.clone(), |ui| {
                paint_flying(
                    ui,
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(&texture)),
                    rect,
                    0.0,
                    egui::Color32::WHITE,
                );
            });
            output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Rect(shape) => shape.brush.as_ref().map(|brush| brush.uv),
                    _ => None,
                })
                .expect("the picture is painted")
        };
        // Leaving a round note's square, the wide frame shows its middle half.
        let crop = uv_painted(square);
        assert!((crop.min.x - 0.25).abs() < 1e-4 && (crop.max.x - 0.75).abs() < 1e-4);
        assert_eq!((crop.min.y, crop.max.y), (0.0, 1.0));
        // Landed at its own shape, the whole frame shows.
        assert_eq!(
            uv_painted(wide),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
        );
    }

    #[test]
    fn stickers_show_on_a_card_that_fits_small_windows() {
        assert_eq!(sticker_side(vec2(1180.0, 780.0)), STICKER_VIEW_SIDE);
        assert_eq!(
            sticker_side(vec2(1180.0, 300.0)),
            300.0 - STICKER_BUTTON_ROOM
        );
        assert_eq!(sticker_side(vec2(10.0, 10.0)), 64.0);
        let square = Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(600.0));
        assert_eq!(fit_within(vec2(512.0, 512.0), square), square);
        assert_eq!(
            fit_within(vec2(512.0, 256.0), square),
            Rect::from_min_max(egui::pos2(0.0, 150.0), egui::pos2(600.0, 450.0))
        );
        assert_eq!(fit_within(Vec2::ZERO, square), square);
    }

    #[test]
    fn fitted_images_keep_aspect_ratio_inside_the_canvas() {
        assert_eq!(
            display_size(vec2(1600.0, 1200.0), vec2(800.0, 700.0), true, 1.0),
            vec2(800.0, 600.0)
        );
        assert_eq!(
            display_size(vec2(320.0, 240.0), vec2(800.0, 700.0), true, 1.0),
            vec2(320.0, 240.0)
        );
        assert_eq!(
            display_size(vec2(320.0, 240.0), vec2(800.0, 700.0), false, 2.0),
            vec2(640.0, 480.0)
        );
    }

    #[test]
    fn the_panes_fill_the_window_top_to_bottom() {
        let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 900.0));
        let (header, stage, strip) = panes(window, true);
        assert_eq!(header.min, window.min, "the header starts at the top edge");
        assert_eq!(header.height(), HEADER);
        assert_eq!(strip.max, window.max, "the strip ends at the bottom edge");
        assert_eq!(strip.height(), STRIP);
        assert_eq!(stage.min.y, header.max.y, "no gap under the header");
        assert_eq!(stage.max.y, strip.min.y, "no gap over the strip");
        assert_eq!(stage.width(), window.width(), "the stage is full width");
        // Without an album the stage keeps the room the strip would have taken.
        let (_, stage, strip) = panes(window, false);
        assert!(strip.height() < f32::EPSILON);
        assert_eq!(stage.max.y, window.max.y);
    }

    #[test]
    fn a_video_frame_keeps_its_aspect_ratio_inside_the_stage() {
        assert_eq!(
            fit(vec2(1920.0, 1080.0), vec2(800.0, 600.0)),
            vec2(800.0, 450.0)
        );
        assert_eq!(fit(vec2(0.0, 0.0), vec2(800.0, 600.0)), vec2(800.0, 600.0));
    }

    /// The row of thumbnails never asks for more room than it has, so the strip
    /// keeps its bar for an album that overflows and not for one that fits.
    #[test]
    fn the_strip_pads_only_the_room_the_row_does_not_use() {
        let inner = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 70.0));
        let row = |count: usize| count as f32 * THUMB + count.saturating_sub(1) as f32 * STRIP_GAP;
        let pad = |count: usize| ((inner.width() - row(count)) * 0.5).max(0.0);
        // Three thumbnails fit, so the padding centres them and the content is
        // exactly the width of the strip.
        assert_eq!(row(3) + 2.0 * pad(3), inner.width());
        // More than the strip holds: no padding at all, and the bar appears
        // because the row is wider than the strip.
        assert_eq!(pad(40), 0.0);
        assert!(row(40) > inner.width());
    }
}
