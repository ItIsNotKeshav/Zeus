//! Zeus's uploads in flight and the pending strip.

use super::*;

/// How long an upload takes to rise into place when it is sent.
pub(super) const UPLOAD_ENTRANCE: f32 = theme::MOTION_PANEL;
/// How far below its place an upload starts its entrance.
pub(super) const UPLOAD_RISE: f32 = 14.0;
/// The progress ring over a picture: its radius and stroke.
pub(super) const UPLOAD_RING: f32 = 22.0;
pub(super) const UPLOAD_RING_STROKE: f32 = 3.0;
/// The space between an upload's picture or card and its caption.
pub(super) const UPLOAD_CAPTION_GAP: f32 = 4.0;

/// Attachments on their way, as your own messages at the end of the chat,
/// each at the size its message will have: a picture or clip with a
/// progress ring over it, pictures sent together as their album's grid, or
/// a file card whose icon is ringed by its progress, the caption under the
/// one that carries it. One sent from the composer flies there from its
/// tile in the strip when `fly`; else it rises into place. Its message
/// takes its place once it is in the archive.
pub(super) fn upload_rows(
    ui: &mut egui::Ui,
    palette: &Palette,
    uploads: &[crate::model::Upload],
    fly: bool,
) {
    let Some(first) = uploads.first() else {
        return;
    };
    // The ring turns while anything is on its way.
    ui.ctx().request_repaint();
    // Its message, once it arrives, takes its place without popping in.
    let now = ui.input(|input| input.time);
    ui.ctx()
        .data_mut(|data| data.insert_temp(upload_seen_id(&first.chat), now));
    let bubble = own_bubble_width(palette, ui.available_width());
    let mut rest = uploads;
    while let Some(upload) = rest.first() {
        let members = upload.batch.map_or(1, |batch| {
            let together = |next: &&crate::model::Upload| next.batch == Some(batch);
            rest.iter().take_while(together).count()
        });
        let (run, after) = rest.split_at(members.max(1));
        rest = after;
        ui.add_space(RUN_GAP);
        if run.len() > 1 {
            upload_album(ui, palette, run, bubble, fly);
        } else {
            upload_row(ui, palette, upload, bubble, fly);
        }
    }
}

/// One attachment on its way, on its own.
pub(super) fn upload_row(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: &crate::model::Upload,
    bubble: f32,
    fly: bool,
) {
    let id = ui.id().with(("upload", upload.id));
    let shown = ui.ctx().animate_bool_with_time(
        id,
        true,
        theme::motion_time(ui.ctx(), UPLOAD_ENTRANCE),
    );
    let fraction = upload_fraction(ui, upload);
    if upload.kind == crate::model::UploadKind::Sticker
        && let Some(crate::model::UploadPreview::File(path)) = &upload.preview
    {
        upload_sticker(ui, palette, upload, path);
        return;
    }
    let now = ui.input(|input| input.time);
    let flight = fly.then(|| take_flight(ui.ctx(), upload, now)).flatten();
    // Flying, it needs no rise of its own. Eased out, so it settles softly.
    let eased = if flight.is_some() {
        1.0
    } else {
        theme::ease_out(shown)
    };
    let start = crate::ui::swipe::mark(ui);
    let face = if pictured(upload) {
        upload_picture(ui, palette, upload, fraction, bubble, eased)
    } else {
        upload_card(ui, palette, upload, fraction, bubble.min(CARD_WIDTH), eased)
    };
    if let Some((from, started)) = flight {
        fly_from_composer(ui, start, &upload.chat, from, face, now - started);
    }
}

/// Pictures and clips on their way as one album, laid out as its message
/// will be: square tiles two to a row, the caption under them. The first
/// tile carries the bubble and the caption on its flight; the others fly
/// on their own.
pub(super) fn upload_album(
    ui: &mut egui::Ui,
    palette: &Palette,
    members: &[crate::model::Upload],
    bubble: f32,
    fly: bool,
) {
    let now = ui.input(|input| input.time);
    let flights: Vec<Option<(Rect, f64)>> = members
        .iter()
        .map(|member| fly.then(|| take_flight(ui.ctx(), member, now)).flatten())
        .collect();
    let id = ui.id().with(("upload", members[0].id));
    let shown = ui.ctx().animate_bool_with_time(
        id,
        true,
        theme::motion_time(ui.ctx(), UPLOAD_ENTRANCE),
    );
    let eased = if flights.iter().any(Option::is_some) {
        1.0
    } else {
        theme::ease_out(shown)
    };
    let caption = members.iter().find_map(|member| member.caption.as_deref());
    let captioned = caption.is_some();
    let side = album_tile_side(palette, bubble, captioned);
    let rows = members.len().div_ceil(2);
    let grid = vec2(
        side * 2.0 + ALBUM_GAP,
        rows as f32 * (side + ALBUM_GAP) - ALBUM_GAP,
    );
    let rim = own_picture_rim(palette, captioned, false);
    let colours = palette.on_bubble(true);
    let text = text_bubble_margin(palette);
    let caption = caption.map(|caption| {
        let wrap = grid.x + 2.0 * rim - f32::from(text.left) - f32::from(text.right);
        let font = theme::regular(BODY_SIZE);
        widgets::line(ui, caption, font, colours.text, wrap.max(0.0), usize::MAX)
    });
    let below = match &caption {
        Some(caption) => UPLOAD_CAPTION_GAP + caption.size().y + f32::from(text.bottom),
        None => rim,
    };
    let mut start = crate::ui::swipe::mark(ui);
    let size = vec2(grid.x + 2.0 * rim, rim + grid.y + below);
    let (frame, eased) = upload_bubble(ui, palette, size, eased, rim > 0.0);
    let origin = frame.min + Vec2::splat(rim);
    if let Some(caption) = caption {
        let at = pos2(
            frame.left() + f32::from(text.left),
            origin.y + grid.y + UPLOAD_CAPTION_GAP,
        );
        caption.paint(ui, at, colours.text.gamma_multiply(eased));
    }
    // The grid's outer corners follow the picture's own radius; corners
    // where two tiles meet share one small radius, as the album's do.
    let outer = own_picture_corner(palette, captioned, false);
    let inner = ALBUM_SEAM_RADIUS.min(outer);
    let count = members.len();
    for (index, (member, flight)) in members.iter().zip(flights).enumerate() {
        let (row, column) = (index / 2, index % 2);
        let corner = |first: bool, second: bool| {
            if first || second { inner } else { outer }.round() as u8
        };
        let (left, above) = (column > 0, row > 0);
        let right = column == 0 && index + 1 < count;
        let below = index + 2 < count;
        let corners = CornerRadius {
            nw: corner(left, above),
            ne: corner(right, above),
            sw: corner(left, below),
            se: corner(right, below),
        };
        let place = column as f32 * (side + ALBUM_GAP);
        let tile = Rect::from_min_size(
            origin + vec2(place, row as f32 * (side + ALBUM_GAP)),
            Vec2::splat(side),
        );
        let fraction = upload_fraction(ui, member);
        paint_upload_media(ui, palette, member, tile, corners, fraction, eased);
        if let Some((from, started)) = flight {
            fly_from_composer(ui, start, &member.chat, from, tile, now - started);
        }
        start = crate::ui::swipe::mark(ui);
    }
}

/// How much of an upload has gone, eased as it climbs.
pub(super) fn upload_fraction(ui: &egui::Ui, upload: &crate::model::Upload) -> Option<f32> {
    let id = ui.id().with(("upload", upload.id)).with("progress");
    upload
        .fraction()
        .map(|fraction| theme::animate_value(ui.ctx(), id, fraction, theme::MOTION_PANEL))
}

/// Whether an upload is drawn as its picture: a picture or a clip that has
/// one to show.
pub(super) fn pictured(upload: &crate::model::Upload) -> bool {
    upload.preview.is_some()
        && matches!(
            upload.kind,
            crate::model::UploadKind::Image | crate::model::UploadKind::Video
        )
}

/// The size in pixels of what an upload shows: a pasted picture's, a
/// picture file's from its header, or a clip's poster's.
pub(super) fn upload_natural(ctx: &egui::Context, upload: &crate::model::Upload) -> Option<Vec2> {
    match upload.preview.as_ref()? {
        crate::model::UploadPreview::Pixels { width, height, .. } => {
            Some(vec2(*width as f32, *height as f32))
        }
        crate::model::UploadPreview::File(path)
            if upload.kind == crate::model::UploadKind::Video =>
        {
            // Until its frame arrives the clip holds a landscape slot, as
            // it did staged.
            let poster = pending_poster(ctx, path);
            Some(poster.map_or(vec2(16.0, 9.0), |poster| poster.size_vec2()))
        }
        crate::model::UploadPreview::File(path) => pending_picture_size(ctx, path),
    }
}

/// What an upload's picture is drawn from at `size`: a pasted picture's
/// thumbnail, a clip's poster, or the picture file.
pub(super) fn upload_texture(
    ui: &egui::Ui,
    upload: &crate::model::Upload,
    size: Vec2,
) -> Option<egui::TextureId> {
    match upload.preview.as_ref()? {
        crate::model::UploadPreview::Pixels { texture, .. } => {
            texture.as_ref().map(egui::TextureHandle::id)
        }
        crate::model::UploadPreview::File(path)
            if upload.kind == crate::model::UploadKind::Video =>
        {
            pending_poster(ui.ctx(), path).map(|poster| poster.id())
        }
        crate::model::UploadPreview::File(path) => {
            let image = widgets::fitted_image(ui, path, size);
            image.load_for_size(ui.ctx(), size).ok()?.texture_id()
        }
    }
}

/// The side of the "sending" chip under a sticker on its way.
pub(super) const STICKER_CHIP: f32 = 22.0;
/// How much a sticker on its way is faded, until its message arrives.
pub(super) const STICKER_WAITING_FADE: f32 = 0.25;

/// When a chat's sticker on its way first showed, and when it was last
/// drawn, so its message, once it arrives, carries on the same pop rather
/// than popping again.
#[derive(Clone, Copy)]
pub(super) struct StickerSeen {
    pub(super) since: f64,
    pub(super) drawn: f64,
}

pub(super) fn sticker_seen_id(chat: &str) -> egui::Id {
    egui::Id::new(("sticker-upload-seen", chat))
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
/// without a bubble, faded a little with a turning ring beside it. It pops
/// out of its bottom right corner as a sent bubble does, once; its message
/// carries that pop on when it takes the sticker's place.
pub(super) fn upload_sticker(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: &crate::model::Upload,
    path: &Path,
) {
    let square = Vec2::splat(STICKER_SIDE);
    let texture = sticker_texture(ui, path, Rect::from_min_size(ui.cursor().min, square));
    let size = texture.map_or(square, |(_, natural)| fit_sticker(natural.x, natural.y));
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let rest = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size);
    let now = ui.input(|input| input.time);
    let id = ui.id().with(("upload-sticker", upload.id));
    let since = ui.ctx().data_mut(|data| *data.get_temp_mut_or(id, now));
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            sticker_seen_id(&upload.chat),
            StickerSeen { since, drawn: now },
        );
    });
    let elapsed = now - since;
    let (scale, opacity) = if crate::motion::reduced(ui.ctx()) || POP.settled(elapsed) {
        (1.0, 1.0)
    } else {
        crate::motion::request_frame(ui.ctx());
        (pop_scale(elapsed), pop_opacity(elapsed))
    };
    let pop = scale_about(rest.right_bottom(), scale);
    let rect = pop.mul_rect(rest);
    let painter = ui.painter_at(row.expand(4.0));
    match texture {
        Some((texture, _)) => {
            painter.image(
                texture,
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE.gamma_multiply((1.0 - STICKER_WAITING_FADE) * opacity),
            );
        }
        None => {
            painter.rect_filled(rect, theme::RADIUS, palette.surface.gamma_multiply(opacity));
            theme::paint_icon(
                ui,
                Icon::Sticker,
                Rect::from_center_size(rect.center(), Vec2::splat(28.0 * scale)),
                28.0 * scale,
                palette.secondary.gamma_multiply(opacity),
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
        Color32::from_black_alpha((120.0 * opacity) as u8),
    );
    progress_ring(
        ui,
        chip.center(),
        STICKER_CHIP / 2.0 - 5.0,
        None,
        Color32::WHITE.gamma_multiply(opacity),
    );
}

/// The bubble of an upload, at the right edge, `eased` of the way into place.
/// A picture that stands without a bubble leaves it unfilled.
pub(super) fn upload_bubble(
    ui: &mut egui::Ui,
    palette: &Palette,
    size: Vec2,
    eased: f32,
    filled: bool,
) -> (Rect, f32) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
    let rect = Rect::from_min_size(pos2(row.right() - size.x, row.top()), size)
        .translate(vec2(0.0, UPLOAD_RISE * (1.0 - eased)));
    if !filled {
        return (rect, eased);
    }
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

/// A picture or clip on its way, at the size its message will have: the
/// picture itself, dimmed under its progress, in the rim its bubble keeps,
/// and the caption under it as its message shows it. Answers where the
/// picture is drawn.
pub(super) fn upload_picture(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: &crate::model::Upload,
    fraction: Option<f32>,
    bubble: f32,
    eased: f32,
) -> Rect {
    let captioned = upload.caption.is_some();
    let video = upload.kind == crate::model::UploadKind::Video;
    let bounds = own_picture_bounds(palette, bubble, captioned, video);
    let natural = upload_natural(ui.ctx(), upload).unwrap_or(vec2(4.0, 3.0));
    let size = fit_picture(natural.x, natural.y, bounds.x, bounds.y);
    let rim = own_picture_rim(palette, captioned, video);
    let colours = palette.on_bubble(true);
    // The caption keeps the bubble's text padding inside the rim.
    let text = text_bubble_margin(palette);
    let caption = upload.caption.as_deref().map(|caption| {
        let wrap = size.x + 2.0 * rim - f32::from(text.left) - f32::from(text.right);
        let font = theme::regular(BODY_SIZE);
        widgets::line(ui, caption, font, colours.text, wrap.max(0.0), usize::MAX)
    });
    let below = match &caption {
        Some(caption) => UPLOAD_CAPTION_GAP + caption.size().y + f32::from(text.bottom),
        None => rim,
    };
    let outer = vec2(size.x + 2.0 * rim, rim + size.y + below);
    let (frame, eased) = upload_bubble(ui, palette, outer, eased, rim > 0.0);
    let picture = Rect::from_min_size(frame.min + Vec2::splat(rim), size);
    let corner = own_picture_corner(palette, captioned, video).round() as u8;
    let corners = CornerRadius::same(corner);
    paint_upload_media(ui, palette, upload, picture, corners, fraction, eased);
    if let Some(caption) = caption {
        let at = pos2(
            frame.left() + f32::from(text.left),
            picture.bottom() + UPLOAD_CAPTION_GAP,
        );
        caption.paint(ui, at, colours.text.gamma_multiply(eased));
    }
    picture
}

/// An upload's picture in `rect`, dimmed, under its progress ring and an
/// arrow, with how much has gone in its corner.
pub(super) fn paint_upload_media(
    ui: &egui::Ui,
    palette: &Palette,
    upload: &crate::model::Upload,
    rect: Rect,
    corners: CornerRadius,
    fraction: Option<f32>,
    eased: f32,
) {
    let painter = ui.painter_at(rect);
    match upload_texture(ui, upload, rect.size()) {
        Some(texture) => {
            // Cover the frame, cropping the longer side.
            let uv = match upload_natural(ui.ctx(), upload) {
                Some(size) if size.min_elem() > 0.0 => cover_uv(size, rect.size()),
                _ => Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            };
            let tint = Color32::WHITE.gamma_multiply(eased);
            let picture = egui::epaint::RectShape::filled(rect, corners, tint);
            painter.add(picture.with_texture(texture, uv));
        }
        None => {
            let icon = if upload.kind == crate::model::UploadKind::Video {
                Icon::Video
            } else {
                Icon::Image
            };
            painter.rect_filled(rect, corners, palette.surface);
            let place = Rect::from_center_size(rect.center(), Vec2::splat(28.0));
            theme::paint_icon(ui, icon, place, 28.0, palette.secondary);
        }
    }
    painter.rect_filled(rect, corners, Color32::from_black_alpha((90.0 * eased) as u8));
    let ring = Rect::from_center_size(rect.center(), Vec2::splat(UPLOAD_RING * 2.0 + 12.0));
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
            rect.left_bottom() + vec2(8.0, -8.0 - galley.size().y - 6.0),
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
/// its icon ringed by the progress, its name, and how much has gone, and
/// the caption under it. Answers where the card is drawn.
pub(super) fn upload_card(
    ui: &mut egui::Ui,
    palette: &Palette,
    upload: &crate::model::Upload,
    fraction: Option<f32>,
    cap: f32,
    eased: f32,
) -> Rect {
    let colours = palette.on_bubble(true);
    let (margin, inset) = attachment_insets(palette);
    let card = vec2(cap - margin.x * 2.0, ATTACHMENT_ROW);
    let outer = card + inset * 2.0 + margin * 2.0;
    let text = text_bubble_margin(palette);
    let caption = upload.caption.as_deref().map(|caption| {
        let wrap = outer.x - f32::from(text.left) - f32::from(text.right);
        let font = theme::regular(BODY_SIZE);
        widgets::line(ui, caption, font, colours.text, wrap.max(0.0), usize::MAX)
    });
    let below = match &caption {
        Some(caption) => caption.size().y + f32::from(text.bottom),
        None => 0.0,
    };
    let (bubble, eased) = upload_bubble(ui, palette, outer + vec2(0.0, below), eased, true);
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
    if let Some(caption) = caption {
        let at = pos2(
            bubble.left() + f32::from(text.left),
            card.bottom() + margin.y,
        );
        caption.paint(ui, at, colours.text.gamma_multiply(eased));
    }
    card
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
pub(crate) fn pending_poster(
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
/// another chat's draft, nor on its way. egui keeps temporary data for good,
/// so its texture would outlive the attachment.
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
    // A clip on its way shows the poster it had staged.
    let sending = app
        .accounts
        .iter()
        .flat_map(|account| account.uploads.iter())
        .filter_map(|upload| match &upload.preview {
            Some(crate::model::UploadPreview::File(path))
                if upload.kind == crate::model::UploadKind::Video =>
            {
                Some(path.clone())
            }
            _ => None,
        });
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
        .chain(sending)
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
pub(crate) fn pending_picture_size(ctx: &egui::Context, path: &std::path::Path) -> Option<Vec2> {
    let id = egui::Id::new(("pending-size", path));
    let known = ctx.data_mut(|data| data.get_temp::<Option<(u32, u32)>>(id));
    let size = known.unwrap_or_else(|| {
        let size = image::image_dimensions(path).ok();
        ctx.data_mut(|data| data.insert_temp(id, size));
        size
    });
    size.map(|(width, height)| vec2(width as f32, height as f32))
}

/// How much of a transcript's width our own bubbles take in the Nothing
/// style; [`WHATSAPP_WIDTH_SHARE`] and [`MESSAGES_WIDTH_SHARE`] are the
/// others'.
pub(super) const NOTHING_WIDTH_SHARE: f32 = 0.70;
/// The widest our own bubble grows, however wide the transcript.
pub(super) const OWN_BUBBLE_WIDEST: f32 = 560.0;

/// How wide our own bubble may grow in a transcript `width` wide.
pub(super) fn own_bubble_width(palette: &Palette, width: f32) -> f32 {
    let share = match palette.bubbles {
        theme::BubbleStyle::Messages => MESSAGES_WIDTH_SHARE,
        theme::BubbleStyle::Nothing => NOTHING_WIDTH_SHARE,
        theme::BubbleStyle::WhatsApp => WHATSAPP_WIDTH_SHARE,
    };
    (width * share).min(OWN_BUBBLE_WIDEST)
}

pub(super) fn own_bubble_width_id(chat: &str) -> egui::Id {
    egui::Id::new(("own-bubble-width", chat))
}

/// Keeps how wide our bubble may grow in `chat`'s transcript, for the
/// composer's strip to show what it stages at the size it will be sent.
pub(super) fn note_own_bubble_width(ui: &egui::Ui, palette: &Palette, chat: &str) {
    let width = own_bubble_width(palette, ui.available_width());
    ui.ctx()
        .data_mut(|data| data.insert_temp(own_bubble_width_id(chat), width));
}

/// How much of its width our bubble keeps beside a picture or clip we send,
/// as the bubble lays it out. A picture alone in the Messages style stands
/// without a bubble.
pub(super) fn own_media_inset(palette: &Palette, captioned: bool, video: bool) -> f32 {
    match palette.bubbles {
        theme::BubbleStyle::Messages if captioned || video => MESSAGES_BUBBLE_MARGIN.sum().x,
        theme::BubbleStyle::Nothing => 28.0,
        _ => 20.0,
    }
}

/// The bounds a picture or clip we send is fitted into in a bubble `bubble`
/// wide, without growing it: the bubble's room, with the ring a captioned
/// picture reaches into, and the picture's own limits.
pub(super) fn own_picture_bounds(
    palette: &Palette,
    bubble: f32,
    captioned: bool,
    video: bool,
) -> Vec2 {
    let reach = if captioned {
        match palette.bubbles {
            theme::BubbleStyle::WhatsApp => WHATSAPP_TEXT_MARGIN.left - WHATSAPP_TEXT_MARGIN.top,
            theme::BubbleStyle::Messages => {
                MESSAGES_BUBBLE_MARGIN.left - MESSAGES_BUBBLE_MARGIN.top
            }
            theme::BubbleStyle::Nothing => NOTHING_TEXT_MARGIN.left - CARD_RING,
        }
    } else {
        0
    };
    let reach = f32::from(2 * reach);
    let limit = if captioned && palette.bubbles == theme::BubbleStyle::Nothing {
        PICTURE_WIDTH + reach
    } else {
        PICTURE_WIDTH
    };
    let inset = own_media_inset(palette, captioned, video);
    let width = (bubble - inset + reach).min(limit).max(0.0);
    let height = if video {
        PICTURE_HEIGHT.min(width * 1.3)
    } else {
        PICTURE_HEIGHT
    };
    vec2(width, height)
}

/// The rim our bubble keeps round a picture or clip we send.
pub(super) fn own_picture_rim(palette: &Palette, captioned: bool, video: bool) -> f32 {
    match palette.bubbles {
        theme::BubbleStyle::Nothing => f32::from(CARD_RING),
        theme::BubbleStyle::Messages if captioned || video => f32::from(MESSAGES_BUBBLE_MARGIN.top),
        theme::BubbleStyle::Messages => 0.0,
        theme::BubbleStyle::WhatsApp if captioned => f32::from(WHATSAPP_TEXT_MARGIN.top),
        theme::BubbleStyle::WhatsApp => 4.0,
    }
}

/// How round the corners of a picture or clip we send are: its bubble's,
/// less the rim round it.
pub(super) fn own_picture_corner(palette: &Palette, captioned: bool, video: bool) -> f32 {
    let radius = match palette.bubbles {
        theme::BubbleStyle::Nothing => widgets::NOTHING_BUBBLE_RADIUS,
        theme::BubbleStyle::Messages => widgets::MESSAGES_BUBBLE_RADIUS,
        // The bubble's own corners less the few points round the picture.
        theme::BubbleStyle::WhatsApp => return f32::from(widgets::BUBBLE_RADIUS) - 4.0,
    };
    f32::from(radius) - own_picture_rim(palette, captioned, video)
}

/// The side of each square tile of an album we send, in a bubble `bubble`
/// wide.
pub(super) fn album_tile_side(palette: &Palette, bubble: f32, captioned: bool) -> f32 {
    let width = CARD_WIDTH.min(bubble - own_media_inset(palette, captioned, false));
    ((width.max(0.0) - ALBUM_GAP) / 2.0).max(80.0)
}

/// The size a staged picture of `native` pixels is shown at: the size it
/// will have once sent, fitted into `bounds` as its message fits it, and
/// only made smaller to share the strip (`shared`) or fit its `room`.
pub(super) fn staged_media_size(
    native: Vec2,
    bounds: Vec2,
    shared: Option<Vec2>,
    room: Vec2,
) -> Vec2 {
    let sent = fit_picture(native.x, native.y, bounds.x, bounds.y);
    let limit = shared.map_or(room, |shared| shared.min(room));
    let shrink = (limit.x / sent.x.max(1.0)).min(limit.y / sent.y.max(1.0));
    sent * shrink.min(1.0)
}

/// Where a staged picture of `native` pixels is drawn inside its tile's
/// `media` area: as large as fits, centred, in its own aspect ratio. A tile
/// is held at [`PENDING_TILE`] at its least, so a panorama or a wide
/// screenshot in a narrow window is shown whole, on the tile, rather than
/// stretched to the tile's shape.
pub(crate) fn contained(native: Vec2, media: Rect) -> Rect {
    let native = vec2(native.x.max(1.0), native.y.max(1.0));
    let scale = (media.width() / native.x).min(media.height() / native.y);
    Rect::from_center_size(media.center(), native * scale.max(0.0))
}

/// The widest a staged file's card is drawn: room for a name and its type
/// and size beside the icon, as the file's message shows them.
pub(crate) const FILE_CARD_WIDTH: f32 = 220.0;
/// How tall a staged file's card is: its icon and two lines, with room.
pub(crate) const FILE_CARD_HEIGHT: f32 = 64.0;

/// What a staged file says under its name: its type and how large it is,
/// such as "PDF · 1.2 MB", as a sent file's card says it. Read once.
pub(crate) fn file_detail(ctx: &egui::Context, path: &std::path::Path) -> String {
    let id = egui::Id::new(("pending-file-detail", path));
    if let Some(known) = ctx.data(|data| data.get_temp::<String>(id)) {
        return known;
    }
    let kind = path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty() && extension.len() <= 6)
        .map(str::to_uppercase);
    let size = std::fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| crate::util::bytes(metadata.len()));
    let detail = kind.into_iter().chain(size).collect::<Vec<_>>().join(" · ");
    ctx.data_mut(|data| data.insert_temp(id, detail.clone()));
    detail
}

/// The icon a staged file's card shows, by what kind of file it is.
pub(crate) fn file_icon(path: &std::path::Path) -> Icon {
    match mime_guess2::from_path(path).first() {
        Some(mime) if mime.type_() == "audio" => Icon::Music,
        Some(mime) if mime.type_() == "video" => Icon::Video,
        Some(mime) if mime.type_() == "image" => Icon::Image,
        _ => Icon::FileText,
    }
}

/// A staged file as a card: its icon in a tinted disc, then its name and
/// under it its type and size, each cut short to the card rather than
/// running past it. The card itself is drawn by the caller.
pub(crate) fn paint_file_card(
    ui: &egui::Ui,
    palette: &Palette,
    rect: Rect,
    path: &std::path::Path,
) {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let detail = file_detail(ui.ctx(), path);
    let disc_size = 36.0_f32.min(rect.height() - 16.0).max(0.0);
    let inset = ((rect.height() - disc_size) / 2.0).min(14.0);
    let disc = pos2(rect.left() + inset + disc_size / 2.0, rect.center().y);
    ui.painter()
        .circle_filled(disc, disc_size / 2.0, palette.accent.gamma_multiply(0.22));
    theme::paint_icon(
        ui,
        file_icon(path),
        Rect::from_center_size(disc, Vec2::splat(18.0)),
        18.0,
        palette.accent,
    );
    let text_left = disc.x + disc_size / 2.0 + 10.0;
    // Clear of the remove button in the card's top right corner.
    let width = (rect.right() - 26.0 - text_left).max(0.0);
    let name = widgets::line(
        ui,
        &name,
        theme::medium(theme::TEXT_CAPTION),
        palette.text,
        width,
        1,
    );
    let detail = widgets::line(
        ui,
        &detail,
        theme::regular(theme::TEXT_MICRO),
        palette.secondary,
        width,
        1,
    );
    let gap = 2.0;
    let top = rect.center().y - (name.size().y + gap + detail.size().y) / 2.0;
    name.paint(ui, pos2(text_left, top), palette.text);
    detail.paint(
        ui,
        pos2(text_left, top + name.size().y + gap),
        palette.secondary,
    );
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
    /// Where its picture, or its card, was drawn inside the tile.
    pub(super) face: Rect,
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
    /// Where the strip's top left was, on screen.
    pub(super) origin: egui::Pos2,
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
    // Tiles just sent fly to their rows; the strip lets them go.
    let flying = flying_attachments(ui.ctx(), chat);
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
                paint_tile_ghosts(strip, card, &flying);
            }
        },
    );
    if !drawn {
        // Lets go of the kept pictures once nothing shows them.
        ui.ctx().data_mut(|data| data.remove::<TileMotions>(id));
    }
}

/// Paints the tiles the strip last showed, as it closes, but for those
/// `flying` to their rows.
pub(super) fn paint_tile_ghosts(
    ui: &mut egui::Ui,
    card: &PendingCard,
    flying: &[crate::app::AttachmentId],
) {
    let origin = egui::emath::TSTransform::from_translation(ui.cursor().min.to_vec2());
    let shapes = card
        .ghosts
        .iter()
        .filter(|ghost| !flying.contains(&ghost.id))
        .flat_map(|ghost| ghost.shapes.iter().cloned())
        .map(|mut shape| {
            shape.transform(origin);
            shape
        })
        .collect();
    ui.painter().add(egui::Shape::Vec(shapes));
    ui.add_space(card.height);
}

/// An attachment just sent from a chat's composer, on its way from its tile
/// in the strip to its row in the transcript.
#[derive(Clone)]
pub(super) struct AttachmentFlight {
    pub(super) chat: String,
    pub(super) staged: crate::app::AttachmentId,
    /// Where its picture or card was drawn in the strip, on screen.
    pub(super) from: Rect,
    pub(super) sent: f64,
    /// Unix seconds at the send, to tell its message from older fresh ones.
    pub(super) wall: i64,
    /// When its row first flew, and when its row or message was last drawn.
    pub(super) started: Option<f64>,
    pub(super) drawn: f64,
    /// Whether its upload is still on its way.
    pub(super) live: bool,
    /// The message that carries the flight on once the upload is done.
    pub(super) message: Option<String>,
}

/// The flights under way, oldest first.
pub(super) type Flights = Vec<AttachmentFlight>;

pub(super) fn attachment_flights_id() -> egui::Id {
    egui::Id::new("attachment-flights")
}

/// How long a flight waits for its row to be drawn before it is let go.
pub(super) const FLIGHT_WAIT: f64 = 3.0;

/// Whether a flight is still to be flown at `now`: its row has not come
/// yet, or it is on its way. One whose upload ended with no message taking
/// it over (it failed, or the chat was left) ends there.
pub(super) fn flight_kept(flight: &AttachmentFlight, now: f64) -> bool {
    match flight.started {
        None => now - flight.sent < FLIGHT_WAIT,
        Some(_) if flight.message.is_none() && now - flight.drawn > TYPING_HANDOFF => false,
        Some(started) => !send_settled(now - started),
    }
}

/// Starts a flight for each attachment staged in `chat`'s composer as it is
/// sent, from where its tile was drawn.
pub(super) fn record_attachment_flights(app: &App, ctx: &egui::Context, chat: &str) {
    let Some(motions) = ctx
        .data(|data| data.get_temp::<TileMotions>(tile_motions_id(chat)))
        .filter(|motions| motions.chat == chat)
    else {
        return;
    };
    let now = ctx.input(|input| input.time);
    let wall = crate::util::now();
    let flights: Vec<AttachmentFlight> = app
        .pending
        .iter()
        .filter_map(|staged| motions.ghosts.iter().find(|ghost| ghost.id == staged.id))
        .map(|ghost| AttachmentFlight {
            chat: chat.to_owned(),
            staged: ghost.id,
            from: ghost.face.translate(motions.origin.to_vec2()),
            sent: now,
            wall,
            started: None,
            drawn: now,
            live: true,
            message: None,
        })
        .collect();
    ctx.data_mut(|data| {
        let known = data.get_temp_mut_or_default::<Flights>(attachment_flights_id());
        known.retain(|flight| flight_kept(flight, now));
        known.extend(flights);
    });
}

/// Notes which of `chat`'s flights still have their upload on its way, and
/// lets go of those done with.
pub(super) fn note_live_flights(ctx: &egui::Context, chat: &str, uploads: &[crate::model::Upload]) {
    let now = ctx.input(|input| input.time);
    ctx.data_mut(|data| {
        let flights = data.get_temp_mut_or_default::<Flights>(attachment_flights_id());
        for flight in flights.iter_mut().filter(|flight| flight.chat == chat) {
            flight.live = uploads
                .iter()
                .any(|upload| upload.staged == Some(flight.staged));
        }
        flights.retain(|flight| flight_kept(flight, now));
    });
}

/// The attachments of `chat` flying from the strip.
pub(super) fn flying_attachments(
    ctx: &egui::Context,
    chat: &str,
) -> Vec<crate::app::AttachmentId> {
    let now = ctx.input(|input| input.time);
    ctx.data(|data| data.get_temp::<Flights>(attachment_flights_id()))
        .unwrap_or_default()
        .into_iter()
        .filter(|flight| flight.chat == chat && flight_kept(flight, now))
        .map(|flight| flight.staged)
        .collect()
}

/// Where an upload's row flies from and when it set off, while it is on
/// its way: the first time it is drawn starts it.
pub(super) fn take_flight(
    ctx: &egui::Context,
    upload: &crate::model::Upload,
    now: f64,
) -> Option<(Rect, f64)> {
    let staged = upload.staged?;
    ctx.data_mut(|data| {
        let flights = data.get_temp_mut_or_default::<Flights>(attachment_flights_id());
        let ours = |flight: &&mut AttachmentFlight| {
            flight.chat == upload.chat && flight.staged == staged && flight.message.is_none()
        };
        let flight = flights.iter_mut().rev().find(ours)?;
        let started = *flight.started.get_or_insert(now);
        flight.drawn = now;
        let flying = !send_settled(now - started);
        flying.then_some((flight.from, started))
    })
}

/// When an upload of `chat` was last drawn, for its message to take its
/// place without popping in.
pub(super) fn upload_seen_id(chat: &str) -> egui::Id {
    egui::Id::new(("upload-seen", chat))
}

/// Whether `message` is one of ours that went as an upload drawn as a row:
/// a picture, a clip, a file or a sound that is not a voice message.
pub(super) fn sent_attachment(message: &Message) -> bool {
    matches!(
        message.content,
        Content::Image { .. }
            | Content::Video { note: false, .. }
            | Content::Document { .. }
            | Content::Audio {
                voice_note: false,
                ..
            }
    )
}

/// Where a message we sent flies from and when its flight set off, while
/// it carries on the flight of the upload it took the place of: the upload
/// ended a moment ago, mid-way, and the message came since it was sent.
pub(super) fn carried_flight(
    ctx: &egui::Context,
    chat: &str,
    message: &Message,
    now: f64,
) -> Option<(Rect, f64)> {
    if !message.from_me || !sent_attachment(message) {
        return None;
    }
    ctx.data_mut(|data| {
        let flights = data.get_temp_mut_or_default::<Flights>(attachment_flights_id());
        let mine = |flight: &AttachmentFlight| {
            flight.chat == chat && flight.message.as_deref() == Some(message.id.as_str())
        };
        let free = |flight: &AttachmentFlight| {
            flight.chat == chat
                && flight.message.is_none()
                && !flight.live
                && flight.started.is_some()
                && now - flight.drawn <= TYPING_HANDOFF
                && message.timestamp >= flight.wall - 1
        };
        let index = flights
            .iter()
            .position(mine)
            .or_else(|| flights.iter().position(free))?;
        let flight = &mut flights[index];
        flight.message = Some(message.id.clone());
        flight.drawn = now;
        let started = flight.started?;
        let flying = !send_settled(now - started);
        flying.then_some((flight.from, started))
    })
}

/// Where a row whose place is `to` is drawn `elapsed` seconds into its
/// flight from `from`: it rises on the sent bubble's spring and slides
/// across on its other, growing or shrinking from the tile's size to its
/// own as it goes.
pub(super) fn flight_transform(from: Rect, to: Rect, elapsed: f64) -> egui::emath::TSTransform {
    let across = SEND_ACROSS.value(elapsed);
    let up = SEND_RISE.value(elapsed);
    let start = if to.width() > 0.0 && to.height() > 0.0 {
        (from.width() / to.width())
            .min(from.height() / to.height())
            .clamp(0.25, 4.0)
    } else {
        1.0
    };
    let scale = start + (1.0 - start) * across;
    let lead = from.center() - to.center();
    let center = to.center() + vec2(lead.x * (1.0 - across), lead.y * (1.0 - up));
    egui::emath::TSTransform::new(center.to_vec2() - to.center().to_vec2() * scale, scale)
}

/// Lifts what was drawn since `start` onto a layer above the chat, so it
/// can cross the composer on its way, and draws it `elapsed` seconds into
/// its flight from `from` to `to`, where it is laid out.
pub(super) fn fly_from_composer(
    ui: &egui::Ui,
    start: egui::layers::ShapeIdx,
    chat: &str,
    from: Rect,
    to: Rect,
    elapsed: f64,
) {
    let now = ui.input(|input| input.time);
    // The list rises to meet a new last row: aim where the row is drawn
    // now, not where it ends, and carry it along with the list.
    let ctx = ui.ctx();
    let glide = ctx.data(|data| data.get_temp::<Glide>(Glide::id(chat)));
    let shift = vec2(0.0, glide.map_or(0.0, |glide| glide.shift(now, 0)));
    let transform = flight_transform(from, to.translate(shift), elapsed)
        * egui::emath::TSTransform::from_translation(shift);
    let layer = ui.layer_id();
    let flying = egui::LayerId::new(egui::Order::Foreground, send_flight_id());
    let clip = ctx.content_rect();
    ctx.graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        let end = list.next_idx();
        let shapes: Vec<egui::Shape> = list
            .all_entries()
            .skip(start.0)
            .take(end.0 - start.0)
            .map(|clipped| clipped.shape.clone())
            .collect();
        for index in start.0..end.0 {
            list.reset_shape(egui::layers::ShapeIdx(index));
        }
        let above = graphics.entry(flying);
        for mut shape in shapes {
            shape.transform(transform);
            above.add(clip, shape);
        }
    });
    crate::motion::request_frame(ctx);
}
