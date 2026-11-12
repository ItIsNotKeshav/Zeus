//! Zeus's uploads in flight, sticker flights and the pending strip.

use super::*;

/// How long an upload takes to rise into place when it is sent.
pub(super) const UPLOAD_ENTRANCE: f32 = theme::MOTION_PANEL;
/// How far below its place an upload starts its entrance.
pub(super) const UPLOAD_RISE: f32 = 14.0;
/// The widest an upload's picture is drawn.
pub(super) const UPLOAD_PICTURE_WIDTH: f32 = 260.0;
/// The progress ring over a picture: its radius and stroke.
pub(super) const UPLOAD_RING: f32 = 22.0;
pub(super) const UPLOAD_RING_STROKE: f32 = 3.0;

/// Attachments on their way, as your own messages at the end of the chat:
/// a picture with a progress ring over it, or a file card whose icon is
/// ringed by its progress, each rising into place when it is sent and
/// replaced by its message once that is in the archive.
pub(super) fn upload_rows(ui: &mut egui::Ui, palette: &Palette, uploads: &[crate::model::Upload]) {
    if uploads.is_empty() {
        return;
    }
    // The ring turns while anything is on its way.
    ui.ctx().request_repaint();
    let cap = (ui.available_width() * WHATSAPP_WIDTH_SHARE).min(CARD_WIDTH);
    for upload in uploads {
        let id = ui.id().with(("upload", upload.id));
        let shown = ui.ctx().animate_bool_with_time(
            id,
            true,
            theme::motion_time(ui.ctx(), UPLOAD_ENTRANCE),
        );
        // Eased out, so it settles softly.
        let eased = theme::ease_out(shown);
        let fraction = upload.fraction().map(|fraction| {
            theme::animate_value(ui.ctx(), id.with("progress"), fraction, theme::MOTION_PANEL)
        });
        let picture = upload.preview.as_ref().filter(|_| {
            matches!(
                upload.kind,
                crate::model::UploadKind::Image | crate::model::UploadKind::Video
            )
        });
        ui.add_space(RUN_GAP);
        if upload.kind == crate::model::UploadKind::Sticker
            && let Some(crate::model::UploadPreview::File(path)) = &upload.preview
        {
            upload_sticker(ui, palette, upload.id, path, eased);
            continue;
        }
        match picture {
            Some(preview) => upload_picture(ui, palette, preview, fraction, cap, eased),
            None => upload_card(ui, palette, upload, fraction, cap, eased),
        }
    }
}

/// How high, at most, a sticker flying from the picker arcs over its
/// straight path.
pub(super) const STICKER_ARC: f32 = 64.0;
/// How long a sticker takes to fly from the picker, as the drag-carry
/// flights do.
pub(super) const STICKER_FLIGHT: f32 = theme::MOTION_SCREEN;
/// The side of the "sending" chip under a sticker on its way.
pub(super) const STICKER_CHIP: f32 = 22.0;
/// How much a sticker on its way is faded, until its message arrives.
pub(super) const STICKER_WAITING_FADE: f32 = 0.25;

/// A sticker flying from its picker tile to its place in the chat.
#[derive(Clone)]
pub(super) struct StickerFlight {
    pub(super) upload: crate::model::UploadId,
    /// The tile it left, in screen coordinates.
    pub(super) from: Rect,
    pub(super) start: f64,
    /// Where it lands and what it shows, as its row last laid it out; it
    /// flies nowhere until the row has.
    pub(super) to: Option<(Rect, egui::TextureId)>,
}

pub(super) fn flights_id() -> egui::Id {
    egui::Id::new("sticker-flights")
}

/// Sends the sticker of `upload` flying from `from`, its tile in the picker,
/// to its place at the end of the chat. Without motion it is simply there.
pub fn launch_sticker(ctx: &egui::Context, upload: crate::model::UploadId, from: Rect) {
    if crate::motion::reduced(ctx) {
        return;
    }
    let start = ctx.input(|input| input.time);
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<StickerFlight>>(flights_id())
            .push(StickerFlight {
                upload,
                from,
                start,
                to: None,
            });
    });
    ctx.request_repaint();
}

/// How far a flight that began at `start` has got, from 0 to 1.
pub(super) fn flight_progress(start: f64, now: f64) -> f32 {
    ((now - start) / f64::from(STICKER_FLIGHT)).clamp(0.0, 1.0) as f32
}

/// Where a sticker flying from `from` to `to` is, `t` of the way: eased
/// out, growing to its size, along a low arc.
pub(super) fn flight_rect(from: Rect, to: Rect, t: f32) -> Rect {
    let eased = theme::ease_out(t.clamp(0.0, 1.0));
    let center = from.center().lerp(to.center(), eased);
    let size = from.size() + (to.size() - from.size()) * eased;
    let arc = (from.center().distance(to.center()) * 0.25).min(STICKER_ARC);
    let lift = arc * (std::f32::consts::PI * eased).sin();
    Rect::from_center_size(center - vec2(0.0, lift), size)
}

/// Draws the stickers in flight over everything, after the chat has laid
/// out where they land, and forgets the ones that have.
pub fn paint_sticker_flights(ctx: &egui::Context) {
    let Some(mut flights) = ctx.data(|data| data.get_temp::<Vec<StickerFlight>>(flights_id()))
    else {
        return;
    };
    let now = ctx.input(|input| input.time);
    flights.retain(|flight| flight_progress(flight.start, now) < 1.0);
    for flight in &flights {
        let Some((to, texture)) = flight.to else {
            continue;
        };
        let t = flight_progress(flight.start, now);
        let rect = flight_rect(flight.from, to, t);
        // It settles to the faded look of a sticker still on its way.
        let tint = Color32::WHITE.gamma_multiply(1.0 - STICKER_WAITING_FADE * theme::ease_out(t));
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            flights_id().with(flight.upload),
        ))
        .image(
            texture,
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            tint,
        );
    }
    if flights.is_empty() {
        ctx.data_mut(|data| data.remove::<Vec<StickerFlight>>(flights_id()));
    } else {
        ctx.data_mut(|data| data.insert_temp(flights_id(), flights));
        ctx.request_repaint();
    }
}

/// The sticker's texture and natural size: an animation's first frame, or
/// the picture.
pub(super) fn sticker_texture(
    ui: &egui::Ui,
    path: &Path,
    rect: Rect,
) -> Option<(egui::TextureId, Vec2)> {
    if crate::ui::picker::moves(ui.ctx(), path) {
        return match animation::frame(ui, path, rect, false) {
            animation::Frame::Ready(texture) => Some((texture.id(), texture.size_vec2())),
            _ => None,
        };
    }
    let poll = widgets::fitted_sticker(ui, path, rect.size())
        .load_for_size(ui.ctx(), rect.size())
        .ok()?;
    Some((poll.texture_id()?, poll.size()?))
}

/// A sticker on its way: drawn where its message will be, at the right edge
/// without a bubble, faded a little with a turning ring beside it. One sent
/// from the picker flies there first, and rests once it lands.
pub(super) fn upload_sticker(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: crate::model::UploadId,
    path: &Path,
    eased: f32,
) {
    let square = Vec2::splat(STICKER_SIDE);
    let texture = sticker_texture(ui, path, Rect::from_min_size(ui.cursor().min, square));
    let size = texture.map_or(square, |(_, natural)| fit_sticker(natural.x, natural.y));
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let rest = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size);
    let now = ui.input(|input| input.time);
    // The flight aims at the sticker's place, or at the bottom of the view
    // while that is still below it.
    let clip = ui.clip_rect();
    let target = rest.translate(vec2(0.0, (clip.bottom() - rest.bottom()).min(0.0)));
    let flying = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<StickerFlight>>(flights_id())
            .iter_mut()
            .find(|flight| flight.upload == upload && flight_progress(flight.start, now) < 1.0)
            .map(|flight| {
                flight.to = texture.map(|(texture, _)| (target, texture));
            })
            .is_some()
    });
    if flying {
        return;
    }
    // One that did not fly rises into place as other uploads do.
    let rect = rest.translate(vec2(0.0, UPLOAD_RISE * (1.0 - eased)));
    let painter = ui.painter_at(row.expand(UPLOAD_RISE));
    match texture {
        Some((texture, _)) => {
            painter.image(
                texture,
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE.gamma_multiply((1.0 - STICKER_WAITING_FADE) * eased),
            );
        }
        None => {
            painter.rect_filled(rect, theme::RADIUS, palette.surface.gamma_multiply(eased));
            theme::paint_icon(
                ui,
                Icon::Sticker,
                Rect::from_center_size(rect.center(), Vec2::splat(28.0)),
                28.0,
                palette.secondary.gamma_multiply(eased),
            );
        }
    }
    // Waiting for the phone's servers, so a ring turns without a number.
    let chip = Rect::from_min_size(
        rect.right_bottom() - Vec2::splat(STICKER_CHIP + 4.0),
        Vec2::splat(STICKER_CHIP),
    );
    painter.circle_filled(
        chip.center(),
        STICKER_CHIP / 2.0,
        Color32::from_black_alpha((120.0 * eased) as u8),
    );
    progress_ring(
        ui,
        chip.center(),
        STICKER_CHIP / 2.0 - 5.0,
        None,
        Color32::WHITE.gamma_multiply(eased),
    );
}

/// The bubble of an upload, at the right edge, `eased` of the way into place.
pub(super) fn upload_bubble(
    ui: &mut egui::Ui,
    palette: &Palette,
    size: Vec2,
    eased: f32,
) -> (Rect, f32) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let rect = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size)
        .translate(vec2(0.0, UPLOAD_RISE * (1.0 - eased)));
    let fill = palette.bubble_out.gamma_multiply(eased);
    if palette.bubbles == theme::BubbleStyle::Nothing {
        ui.painter()
            .add(widgets::grouped_bubble_shape(rect, fill, true, true, true));
    } else {
        ui.painter()
            .rect_filled(rect, f32::from(widgets::BUBBLE_RADIUS), fill);
    }
    (rect, eased)
}

/// A picture on its way: the picture itself, dimmed, under its progress.
pub(super) fn upload_picture(
    ui: &mut egui::Ui,
    palette: &Palette,
    preview: &crate::model::UploadPreview,
    fraction: Option<f32>,
    cap: f32,
    eased: f32,
) {
    let width = UPLOAD_PICTURE_WIDTH.min(cap);
    let (texture, natural) = match preview {
        crate::model::UploadPreview::Pixels {
            width: w,
            height: h,
            texture,
            ..
        } => (
            texture.as_ref().map(egui::TextureHandle::id),
            Some(vec2(*w as f32, *h as f32)),
        ),
        crate::model::UploadPreview::File(path) => {
            match widgets::fitted_image(ui, path, Vec2::splat(width))
                .load_for_size(ui.ctx(), Vec2::splat(width))
            {
                Ok(poll) => (poll.texture_id(), poll.size()),
                Err(_) => (None, None),
            }
        }
    };
    let aspect = natural
        .filter(|size| size.x > 0.0 && size.y > 0.0)
        .map_or(0.75, |size| size.y / size.x);
    let margin = 4.0;
    let picture_size = vec2(width, (width * aspect).clamp(120.0, 320.0));
    let (bubble, eased) =
        upload_bubble(ui, palette, picture_size + Vec2::splat(margin * 2.0), eased);
    let picture = bubble.shrink(margin);
    let radius = if palette.bubbles == theme::BubbleStyle::Nothing {
        12.0
    } else {
        6.0
    };
    let painter = ui.painter_at(bubble);
    match texture {
        Some(texture) => {
            // Cover the frame, cropping the longer side.
            let uv = natural.map_or(Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), |size| {
                cover_uv(size, picture.size())
            });
            painter.add(
                egui::epaint::RectShape::filled(
                    picture,
                    CornerRadius::same(radius as u8),
                    Color32::WHITE.gamma_multiply(eased),
                )
                .with_texture(texture, uv),
            );
        }
        None => {
            painter.rect_filled(picture, radius, palette.surface);
            theme::paint_icon(
                ui,
                Icon::Image,
                Rect::from_center_size(picture.center(), Vec2::splat(28.0)),
                28.0,
                palette.secondary,
            );
        }
    }
    painter.rect_filled(
        picture,
        radius,
        Color32::from_black_alpha((90.0 * eased) as u8),
    );
    let ring = Rect::from_center_size(picture.center(), Vec2::splat(UPLOAD_RING * 2.0 + 12.0));
    painter.circle_filled(
        ring.center(),
        UPLOAD_RING + 6.0,
        Color32::from_black_alpha((110.0 * eased) as u8),
    );
    progress_ring(
        ui,
        ring.center(),
        UPLOAD_RING,
        fraction,
        Color32::WHITE.gamma_multiply(eased),
    );
    theme::paint_icon(
        ui,
        Icon::ArrowUp,
        Rect::from_center_size(ring.center(), Vec2::splat(18.0)),
        18.0,
        Color32::WHITE.gamma_multiply(eased),
    );
    if let Some(fraction) = fraction {
        let label = format!("{:.0}%", fraction * 100.0);
        let galley =
            painter.layout_no_wrap(label, theme::medium(theme::TEXT_CAPTION), Color32::WHITE);
        let chip = Rect::from_min_size(
            picture.left_bottom() + vec2(8.0, -8.0 - galley.size().y - 6.0),
            galley.size() + vec2(14.0, 6.0),
        );
        painter.rect_filled(
            chip,
            chip.height() / 2.0,
            Color32::from_black_alpha((130.0 * eased) as u8),
        );
        painter.galley(
            chip.min + vec2(7.0, 3.0),
            galley,
            Color32::WHITE.gamma_multiply(eased),
        );
    }
}

/// A file on its way, laid out as its message will be: the file card with
/// its icon ringed by the progress, its name, and how much has gone.
pub(super) fn upload_card(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: &crate::model::Upload,
    fraction: Option<f32>,
    cap: f32,
    eased: f32,
) {
    let colours = palette.on_bubble(true);
    let (margin, inset) = attachment_insets(palette);
    let card = vec2(cap - margin.x * 2.0, ATTACHMENT_ROW);
    let (bubble, eased) = upload_bubble(ui, palette, card + inset * 2.0 + margin * 2.0, eased);
    let card = Rect::from_min_size(bubble.min + margin, card + inset * 2.0);
    let painter = ui.painter_at(bubble);
    painter.rect_filled(
        card,
        attachment_radius(palette),
        palette.window.gamma_multiply(0.35 * eased),
    );
    let content = card.shrink2(inset);
    let disc = pos2(content.left() + ATTACHMENT_DISC / 2.0, content.center().y);
    painter.circle_filled(
        disc,
        ATTACHMENT_DISC / 2.0,
        colours.accent.gamma_multiply(0.25 * eased),
    );
    progress_ring(
        ui,
        disc,
        ATTACHMENT_DISC / 2.0 - 1.5,
        fraction,
        colours.accent.gamma_multiply(eased),
    );
    let icon = match upload.kind {
        crate::model::UploadKind::Image => Icon::Image,
        crate::model::UploadKind::Video => Icon::Video,
        crate::model::UploadKind::Audio => Icon::Music,
        crate::model::UploadKind::Document => Icon::FileText,
        crate::model::UploadKind::Sticker => Icon::Sticker,
    };
    theme::paint_icon(
        ui,
        icon,
        Rect::from_center_size(disc, Vec2::splat(18.0)),
        18.0,
        colours.accent.gamma_multiply(eased),
    );
    let text_left = disc.x + ATTACHMENT_DISC / 2.0 + 12.0;
    let text_width = (content.right() - text_left).max(0.0);
    let name = widgets::line(
        ui,
        if upload.name.is_empty() {
            "Photo"
        } else {
            &upload.name
        },
        theme::medium(theme::TEXT_BODY),
        colours.text,
        text_width,
        1,
    );
    let detail = match fraction {
        Some(fraction) if fraction >= 1.0 => "Sending…".to_owned(),
        Some(fraction) => format!(
            "{} of {} · {:.0}%",
            crate::util::bytes((fraction as f64 * upload.total as f64) as u64),
            crate::util::bytes(upload.total),
            fraction * 100.0
        ),
        None => "Preparing…".to_owned(),
    };
    let detail = painter.layout_no_wrap(
        detail,
        theme::regular(theme::TEXT_CAPTION),
        colours.secondary,
    );
    let gap = 2.0;
    let top = content.center().y - (name.size().y + gap + detail.size().y) / 2.0;
    name.paint(ui, pos2(text_left, top), colours.text.gamma_multiply(eased));
    painter.galley(
        pos2(text_left, top + name.size().y + gap),
        detail,
        colours.secondary.gamma_multiply(eased),
    );
}

/// A ring of `fraction` round from the top, clockwise, over a faint track;
/// without one, a quarter turning, as nothing is known yet.
pub(super) fn progress_ring(
    ui: &egui::Ui,
    center: egui::Pos2,
    radius: f32,
    fraction: Option<f32>,
    color: Color32,
) {
    let painter = ui.painter();
    let stroke = UPLOAD_RING_STROKE.min(radius / 5.0).max(2.0);
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(stroke, color.gamma_multiply(0.25)),
    );
    let time = ui.input(|input| input.time) as f32;
    let (start, sweep) = match fraction {
        Some(fraction) => (
            -std::f32::consts::FRAC_PI_2,
            fraction.max(0.02) * std::f32::consts::TAU,
        ),
        None => (time * 4.0, std::f32::consts::FRAC_PI_2),
    };
    let steps = ((sweep / std::f32::consts::TAU) * 64.0).ceil().max(2.0) as usize;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|step| {
            let angle = start + sweep * step as f32 / steps as f32;
            center + Vec2::angled(angle) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(stroke, color)));
}

/// A video's poster frame for the composer, read off the UI thread.
pub(super) enum PendingPoster {
    Loading,
    Decoded(Option<egui::ColorImage>),
    Ready(egui::TextureHandle),
    Failed,
}

/// The poster of a staged video, started on first sight and picked up
/// when the decode finishes. `None` while it is still loading or when the
/// clip has no picture the app can decode.
pub(super) fn pending_poster(
    ctx: &egui::Context,
    path: &std::path::Path,
) -> Option<egui::TextureHandle> {
    let id = egui::Id::new(("pending-poster", path));
    let slot = ctx
        .data_mut(|data| data.get_temp::<std::sync::Arc<std::sync::Mutex<PendingPoster>>>(id))
        .unwrap_or_else(|| {
            let slot = std::sync::Arc::new(std::sync::Mutex::new(PendingPoster::Loading));
            ctx.data_mut(|data| data.insert_temp(id, slot.clone()));
            let (worker, ctx, path) = (slot.clone(), ctx.clone(), path.to_path_buf());
            std::thread::spawn(move || {
                let picture = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| crate::animation::poster(&bytes))
                    .and_then(|poster| poster.picture)
                    .map(|picture| {
                        let picture = image::DynamicImage::ImageRgb8(picture);
                        let picture = if picture.width().max(picture.height()) > 1024 {
                            picture.resize(1024, 1024, image::imageops::FilterType::Triangle)
                        } else {
                            picture
                        };
                        let rgba = picture.to_rgba8();
                        egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            &rgba,
                        )
                    });
                if let Ok(mut slot) = worker.lock() {
                    *slot = PendingPoster::Decoded(picture);
                }
                ctx.request_repaint();
            });
            slot
        });
    let mut state = slot.lock().ok()?;
    if let PendingPoster::Decoded(picture) = &mut *state {
        *state = match picture.take() {
            Some(image) => PendingPoster::Ready(ctx.load_texture(
                format!("pending-poster-{}", path.display()),
                image,
                egui::TextureOptions::LINEAR,
            )),
            None => PendingPoster::Failed,
        };
    }
    match &*state {
        PendingPoster::Ready(handle) => Some(handle.clone()),
        _ => None,
    }
}

/// Lets go of the poster of every video no longer staged, here or parked with
/// another chat's draft. egui keeps temporary data for good, so its texture
/// would outlive the attachment.
pub(super) fn forget_pending_posters(ctx: &egui::Context, app: &App) {
    let kept_id = egui::Id::new("pending-posters-kept");
    let kept = ctx
        .data_mut(|data| data.get_temp::<Vec<std::path::PathBuf>>(kept_id))
        .unwrap_or_default();
    let parked = app
        .accounts
        .iter()
        .flat_map(|account| account.attachment_drafts.values())
        .flat_map(|draft| draft.pending.iter());
    let current: Vec<std::path::PathBuf> = app
        .pending
        .iter()
        .chain(parked)
        .filter_map(|staged| match &staged.item {
            crate::app::Pending::File(path) if crate::image_preview::is_video(path) => {
                Some(path.clone())
            }
            _ => None,
        })
        .collect();
    if kept == current {
        return;
    }
    ctx.data_mut(|data| {
        for path in kept.iter().filter(|path| !current.contains(path)) {
            data.remove::<std::sync::Arc<std::sync::Mutex<PendingPoster>>>(egui::Id::new((
                "pending-poster",
                path.as_path(),
            )));
        }
        data.insert_temp(kept_id, current);
    });
}

/// Pixel size of a staged picture file, read once from its header.
pub(super) fn pending_picture_size(ctx: &egui::Context, path: &std::path::Path) -> Option<Vec2> {
    let id = egui::Id::new(("pending-size", path));
    let known = ctx.data_mut(|data| data.get_temp::<Option<(u32, u32)>>(id));
    let size = known.unwrap_or_else(|| {
        let size = image::image_dimensions(path).ok();
        ctx.data_mut(|data| data.insert_temp(id, size));
        size
    });
    size.map(|(width, height)| vec2(width as f32, height as f32))
}

/// Largest size for a staged picture or clip: the native aspect ratio fitted
/// into `bounds`, enlarged at most twice, and never thinner than a tile.
pub(super) fn pending_media_size(native: Vec2, bounds: Vec2) -> Vec2 {
    let native = vec2(native.x.max(1.0), native.y.max(1.0));
    let scale = (bounds.x / native.x).min(bounds.y / native.y).min(2.0);
    let fitted = native * scale;
    let floor = (PENDING_TILE / fitted.min_elem()).max(1.0);
    // A strip of a panorama keeps its ratio and may run past a tile's floor.
    let lifted = (fitted * floor).min(bounds.max(Vec2::splat(PENDING_TILE)));
    if lifted.x / lifted.y > 0.0 {
        lifted
    } else {
        fitted
    }
}

/// Side of the square a document or an undecoded attachment shows as.
pub(super) const PENDING_TILE: f32 = 72.0;

/// The spring a tile added to a strip already open pops in on, from
/// [`TILE_POP_FROM`] of its size: a touch of bounce, as a control answers.
pub(super) const TILE_POP: crate::motion::Spring = crate::motion::Spring::SNAPPY;
/// The size a popping tile starts from: never from nothing.
pub(super) const TILE_POP_FROM: f32 = 0.92;
/// How long a popping tile takes to become opaque.
pub(super) const TILE_FADE_IN: f64 = 0.15;
/// How long after the one before it each tile added together pops in. The
/// fourth and later go with the fourth, so a batch is in within 0.3 s.
pub(super) const TILE_STAGGER: f64 = 0.04;
pub(super) const TILE_STAGGER_MAX: usize = 3;

/// A staged attachment's tile as it was drawn, kept to paint it as it
/// leaves the strip or while the strip closes: its picture and name
/// (without the hover shade or the remove button) relative to the strip's
/// top left, and the textures they show, held so the pictures outlive the
/// attachment for as long as its tile is seen.
#[derive(Clone)]
pub(super) struct TileGhost {
    pub(super) id: crate::app::AttachmentId,
    pub(super) place: Rect,
    pub(super) shapes: Vec<egui::Shape>,
    pub(super) _textures: Vec<egui::TextureHandle>,
}

/// How one staged attachment's tile moves.
#[derive(Clone, Copy)]
pub(super) struct TileMotion {
    pub(super) id: crate::app::AttachmentId,
    /// Where it lay last pass, relative to the strip's top left.
    pub(super) place: Rect,
    /// How far from its place it is drawn as it slides into a new one,
    /// and how fast that shrinks, per second.
    pub(super) offset: Vec2,
    pub(super) velocity: Vec2,
    /// When it pops in, for one added to a strip already open, until it
    /// has.
    pub(super) pops: Option<f64>,
}

/// The attachment strip's tiles as they move, kept in the context's memory
/// per chat and brought up to date once a pass by [`pending_strip`].
#[derive(Clone, Default)]
pub(super) struct TileMotions {
    pub(super) chat: String,
    pub(super) tiles: Vec<TileMotion>,
    /// Every tile drawn last pass, and those leaving with when they went.
    pub(super) ghosts: std::sync::Arc<Vec<TileGhost>>,
    pub(super) leaving: Vec<(TileGhost, f64)>,
    /// How tall the strip was laid out, the space under it included.
    pub(super) height: f32,
    pub(super) time: f64,
    pub(super) pass: u64,
}

pub(super) fn tile_motions_id(chat: &str) -> egui::Id {
    egui::Id::new(("pending-tiles", chat))
}

/// The staged tiles of `chat`'s strip as they move, for tests: each one's
/// id, whether it is still popping in, and how far from its place it is
/// drawn; and how many tiles are leaving.
#[cfg(test)]
pub(crate) fn tile_motion(
    ctx: &egui::Context,
    chat: &str,
) -> (Vec<(crate::app::AttachmentId, bool, Vec2)>, usize) {
    ctx.data(|data| data.get_temp::<TileMotions>(tile_motions_id(chat)))
        .map_or_else(
            || (Vec::new(), 0),
            |motions| {
                (
                    motions
                        .tiles
                        .iter()
                        .map(|tile| (tile.id, tile.pops.is_some(), tile.offset))
                        .collect(),
                    motions.leaving.len(),
                )
            },
        )
}

/// What the attachment strip shows as it closes: the tiles drawn last.
#[derive(Clone, Default)]
pub(super) struct PendingCard {
    pub(super) height: f32,
    pub(super) ghosts: std::sync::Arc<Vec<TileGhost>>,
}

impl PartialEq for PendingCard {
    fn eq(&self, other: &Self) -> bool {
        self.height == other.height && std::sync::Arc::ptr_eq(&self.ghosts, &other.ghosts)
    }
}

/// The strip of attachments staged in `chat`'s composer, opening out of the
/// field as the first is added and sinking back once they are sent or
/// removed ([`reveal_strip`]). While it is open, each tile added pops in,
/// one removed fades and shrinks where it was without holding its room,
/// and the rest slide to their new places.
pub(super) fn attachment_strip(app: &mut App, ui: &mut egui::Ui, chat: &str, calm: bool) {
    let id = tile_motions_id(chat);
    let card = (!app.pending.is_empty()).then(|| {
        ui.ctx()
            .data(|data| data.get_temp::<TileMotions>(id))
            .filter(|motions| motions.chat == chat)
            .map(|motions| PendingCard {
                height: motions.height,
                ghosts: motions.ghosts,
            })
            .unwrap_or_default()
    });
    let live = card.is_some();
    let drawn = reveal_strip(
        ui,
        calm,
        egui::Id::new("pending-reveal"),
        chat,
        card.as_ref(),
        false,
        |strip, card| {
            if live {
                pending_strip(app, strip, chat, calm);
            } else {
                paint_tile_ghosts(strip, card);
            }
        },
    );
    if !drawn {
        // Lets go of the kept pictures once nothing shows them.
        ui.ctx().data_mut(|data| data.remove::<TileMotions>(id));
    }
}

/// Paints the tiles the strip last showed, as it closes.
pub(super) fn paint_tile_ghosts(ui: &mut egui::Ui, card: &PendingCard) {
    let origin = egui::emath::TSTransform::from_translation(ui.cursor().min.to_vec2());
    let shapes = card
        .ghosts
        .iter()
        .flat_map(|ghost| ghost.shapes.iter().cloned())
        .map(|mut shape| {
            shape.transform(origin);
            shape
        })
        .collect();
    ui.painter().add(egui::Shape::Vec(shapes));
    ui.add_space(card.height);
}
