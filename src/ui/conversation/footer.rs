//! Zeus's own parts of quotes, footers and menu shading.

use super::*;

/// Room between a quote's text and the bar on its left, and its right edge.
pub(super) const QUOTE_SIDE: i8 = 10;
/// The even ring a Nothing bubble keeps around a card at its edge.
pub(super) const CARD_RING: i8 = 6;

/// How far a card at a bubble's edge (a quote, a link preview) reaches past
/// the bubble's text padding, and how round it is. In the Nothing style it
/// sits in a thin even ring, its corners following the bubble's 18, while
/// the text keeps the wider padding; elsewhere it keeps to the padding.
pub(super) fn edge_card(ui: &egui::Ui, palette: &Palette, radius: u8) -> (Margin, u8) {
    if palette.bubbles != theme::BubbleStyle::Nothing {
        return (Margin::ZERO, radius);
    }
    let pad = NOTHING_TEXT_MARGIN;
    // Nothing drawn above it yet: it meets the bubble's top too.
    let top = if ui.min_rect().height() < 1.0 {
        CARD_RING - pad.top
    } else {
        0
    };
    let side = CARD_RING - pad.left;
    (
        Margin {
            left: side,
            right: side,
            top,
            bottom: 0,
        },
        widgets::NOTHING_BUBBLE_RADIUS - CARD_RING as u8,
    )
}

/// Room inside the chip, beside the glyph and the words.
pub(super) const CALL_CHIP_PAD_X: f32 = 8.0;
/// Room inside the chip, above and below the glyph and the words.
pub(super) const CALL_CHIP_PAD_Y: f32 = 4.0;
/// Corner of the chip. The bubble's own radius is 10, and the chip sits
/// four points inside it.
pub(super) const CALL_CHIP_RADIUS: u8 = 6;
/// `#000` at 5%. 13 is `0.05 * 255`, rounded.
pub(super) const CALL_CHIP_FILL: Color32 = Color32::from_rgba_unmultiplied_const(0, 0, 0, 13);

/// [`footer_width`] for a message whose status reads `status` in place of its
/// clock and ticks (see [`status_text`]).
pub(super) fn footer_width_with(
    ui: &egui::Ui,
    message: &Message,
    starred: bool,
    pinned: bool,
    status: Option<&str>,
) -> f32 {
    let font = theme::regular(theme::TEXT_MICRO);
    let time = match status {
        Some(text) => {
            ui.painter()
                .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                .size()
                .x
        }
        None => clock_width(ui, message),
    };
    let edited = if message.edited {
        ui.painter()
            .layout_no_wrap("edited".to_owned(), font, Color32::WHITE)
            .size()
            .x
            + 4.0
    } else {
        0.0
    };
    let not_sent = if not_sent(message) {
        ui.painter()
            .layout_no_wrap(
                NOT_SENT.to_owned(),
                theme::medium(theme::TEXT_MICRO),
                Color32::WHITE,
            )
            .size()
            .x
            + 6.0
    } else {
        0.0
    };
    // Only a message that carries a mark reserves its slot, as WhatsApp does:
    // a plain footer is the clock and the ticks alone. Starring or pinning
    // grows that bubble's footer by one slot each.
    let marks =
        if starred { FOOTER_MARK_SLOT } else { 0.0 } + if pinned { FOOTER_MARK_SLOT } else { 0.0 };
    // A word takes the place of the ticks, and of the clock beside them.
    let ticks = if message.from_me && status.is_none() {
        19.0
    } else {
        0.0
    };
    time + edited + not_sent + ticks + marks
}

/// The reference style leaves the clock and delivery state outside the bubble;
/// only edited and pinned/starred marks still reserve room inside it.
pub(super) fn message_footer_width(
    ui: &egui::Ui,
    message: &Message,
    starred: bool,
    pinned: bool,
    palette: &Palette,
    time_inside: bool,
    status: Option<&str>,
) -> f32 {
    if time_inside {
        return footer_width_with(ui, message, starred, pinned, status);
    }
    let edited = if message.edited {
        ui.painter()
            .layout_no_wrap(
                "edited".to_owned(),
                theme::regular(theme::TEXT_MICRO),
                palette.dim,
            )
            .size()
            .x
            + 4.0
    } else {
        0.0
    };
    edited
        + if starred { FOOTER_MARK_SLOT } else { 0.0 }
        + if pinned { FOOTER_MARK_SLOT } else { 0.0 }
}

/// How far toward black a bubble goes while its menu is open, in a light
/// palette: enough to mark the message the menu acts on, and no more.
pub(super) const MENU_SHADE_LIGHT: f32 = 0.07;
/// The same in a dark palette, where a bubble needs more to show it.
pub(super) const MENU_SHADE_DARK: f32 = 0.12;

/// How far toward black the bubble `bubble_id` is drawn now: its palette's
/// full shade while its menu is open, nothing once it has closed, easing
/// between the two over [`theme::MOTION_QUICK`] as the menu opens and
/// closes, at once with a screen reader. Only a bubble whose menu is open or
/// still fading keeps its progress, so the bubbles scrolled past leave
/// nothing behind.
pub(super) fn menu_shade(ctx: &egui::Context, palette: &Palette, bubble_id: egui::Id) -> f32 {
    let open = egui::Popup::is_id_open(ctx, bubble_id.with("popup"));
    let id = bubble_id.with("menu-shade");
    let pass = ctx.cumulative_pass_nr();
    // The progress, and the pass it last moved in.
    let previous = ctx.data(|data| data.get_temp::<(f32, u64)>(id));
    if !open && previous.is_none() {
        return 0.0;
    }
    let progress = if let Some((progress, moved)) = previous
        && moved == pass
    {
        // Drawn again in the same pass: it has moved this frame already.
        progress
    } else if !crate::motion::reduced(ctx) {
        let step = ctx.input(|input| input.stable_dt).min(0.1) / theme::MOTION_QUICK;
        let towards = if open { step } else { -step };
        let from = previous.map_or(0.0, |(progress, _)| progress);
        (from + towards).clamp(0.0, 1.0)
    } else if open {
        1.0
    } else {
        0.0
    };
    if progress > 0.0 {
        ctx.data_mut(|data| data.insert_temp(id, (progress, pass)));
    } else {
        ctx.data_mut(|data| data.remove::<(f32, u64)>(id));
    }
    if progress > 0.0 && progress < 1.0 {
        ctx.request_repaint();
    }
    menu_shade_depth(palette) * theme::ease_out(progress)
}

/// A bubble's full shade under its menu in `palette`.
pub(super) fn menu_shade_depth(palette: &Palette) -> f32 {
    if palette.dark {
        MENU_SHADE_DARK
    } else {
        MENU_SHADE_LIGHT
    }
}

/// `colour` `amount` of the way to black, as opaque as it was.
pub(super) fn shaded(colour: Color32, amount: f32) -> Color32 {
    let darker = colour.lerp_to_gamma(Color32::BLACK, amount);
    Color32::from_rgba_premultiplied(darker.r(), darker.g(), darker.b(), colour.a())
}

/// A bubble of `fill` as it is drawn while its menu is open.
#[cfg(test)]
pub(crate) fn menu_shaded(palette: &Palette, fill: Color32) -> Color32 {
    shaded(fill, menu_shade_depth(palette))
}

/// Darkens every colour in `shape` `amount` of the way to black, keeping
/// each one's opacity, so a gradient, a tail and anti-aliased edges darken
/// alike.
pub(super) fn shade_shape(shape: &mut egui::Shape, amount: f32) {
    egui::epaint::shape_transform::adjust_colors(shape, move |colour| {
        if *colour != Color32::PLACEHOLDER {
            *colour = shaded(*colour, amount);
        }
    });
}

/// The opacity of black that, laid over a picture, darkens it as much as
/// `amount` darkens a fill.
pub(super) fn shade_alpha(amount: f32) -> u8 {
    (amount.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The corners of a message's bubble, as drawn, for the ring that marks it
/// focused.
pub(super) fn bubble_outline_corners(
    palette: &Palette,
    message: &Message,
    fill: Color32,
    tail: Option<widgets::Side>,
    own: bool,
    first_in_run: bool,
    last_in_run: bool,
) -> CornerRadius {
    if fill == Color32::TRANSPARENT {
        return CornerRadius::from(picture_radius(palette, message));
    }
    match palette.bubbles {
        theme::BubbleStyle::Nothing => {
            widgets::grouped_bubble_corners(own, first_in_run, last_in_run)
        }
        theme::BubbleStyle::Messages => CornerRadius::same(widgets::MESSAGES_BUBBLE_RADIUS),
        _ => widgets::bubble_corners(tail),
    }
}

/// A photo without accompanying text can use a tighter frame than a text
/// balloon; quotes and forwarded labels keep the room they need.
pub(super) fn compact_picture_message(message: &Message) -> bool {
    (matches!(message.content, Content::Image { caption: None, .. })
        || (matches!(
            message.content,
            Content::Video {
                caption: None,
                note: false,
                ..
            }
        ) && !compact_attachment_message(message)))
        && message.quoted.is_none()
        && !message.forwarded
}

/// How far a picture or video in a Nothing bubble that also holds a quote,
/// a forwarded label or a caption reaches past the text padding, so it sits
/// in the same even ring as a picture alone. Without a caption it meets the
/// bubble's bottom too.
pub(super) fn picture_bleed(
    ui: &egui::Ui,
    palette: &Palette,
    message: &Message,
    captioned: bool,
) -> Margin {
    if compact_picture_message(message) {
        return Margin::ZERO;
    }
    if palette.bubbles != theme::BubbleStyle::Nothing {
        // Messages and WhatsApp: the picture keeps the room the bubble gives
        // above it on every side, so its rim is even.
        let margin = bubble_margin(palette, message, false);
        let side = margin.top - margin.left;
        return Margin {
            left: side,
            right: side,
            top: 0,
            bottom: if captioned {
                0
            } else {
                margin.top - margin.bottom
            },
        };
    }
    let mut bleed = edge_card(ui, palette, 0).0;
    if !captioned {
        bleed.bottom = CARD_RING - NOTHING_TEXT_MARGIN.bottom;
    }
    bleed
}

/// File cards have their own inset surface inside the message balloon.
pub(super) fn compact_attachment_message(message: &Message) -> bool {
    matches!(message.content, Content::Document { .. })
        || matches!(
            &message.content,
            Content::Video { media, .. } if message.thumbnail.is_none() && media.path.is_none()
        )
}

pub(super) fn reference_footer_marks(
    ui: &mut egui::Ui,
    message: &Message,
    slot: Option<Rect>,
    starred: bool,
    pinned: bool,
    palette: &Palette,
) {
    let width = message_footer_width(ui, message, starred, pinned, palette, false, None);
    if width <= 0.0 {
        return;
    }
    let rect = match slot {
        Some(slot) => slot,
        None => {
            let row_width = ui.min_rect().width().max(width);
            let (rect, _) = ui.allocate_exact_size(vec2(row_width, 15.0), Sense::hover());
            rect
        }
    };
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            footer_id(&message.chat, &message.id),
            Rect::from_min_max(pos2(rect.right() - width, rect.top()), rect.max),
        );
    });
    let mut x = rect.right();
    if pinned {
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::Pin,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, rect.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            palette.secondary,
        );
    }
    if message.edited {
        let galley = ui.painter().layout_no_wrap(
            "edited".to_owned(),
            theme::regular(theme::TEXT_MICRO),
            palette.dim,
        );
        x -= galley.size().x + 4.0;
        ui.painter().galley(
            pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            palette.dim,
        );
    }
    if starred {
        x -= FOOTER_MARK_SLOT;
        theme::paint_icon(
            ui,
            Icon::StarFill,
            Rect::from_center_size(
                pos2(x + FOOTER_MARK / 2.0, rect.center().y),
                Vec2::splat(FOOTER_MARK),
            ),
            FOOTER_MARK,
            theme::STAR,
        );
    }
}
