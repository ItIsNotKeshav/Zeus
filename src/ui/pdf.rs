//! The PDF viewer, a peek: a large card that flies out of the document's
//! message over the frosted, dimmed window, and back into it when closed.
//! Its pages run one under another under a frosted toolbar with the page
//! being read, zoom, search, Save as and Open in another app. Search floats
//! in a frosted card under the toolbar and lights every match on the pages,
//! the current one most.
//!
//! Only the pages on screen are drawn and asked for (see [`crate::pdf`]);
//! a page still rendering is a white sheet that its picture fades into.

use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, UiBuilder, Vec2, pos2, vec2};

use crate::app::App;
use crate::model::Action;
use crate::pdf::{Failure, Layout as Pages, Load, Viewer};
use crate::theme::{self, Icon, Palette};

/// Height of the toolbar across the card's top.
const HEADER: f32 = 52.0;
/// The search card's size and its distance from the header and the edge.
const SEARCH_WIDTH: f32 = 380.0;
const SEARCH_HEIGHT: f32 = 48.0;
const SEARCH_INSET: f32 = 12.0;
/// A page's corners.
const PAGE_RADIUS: u8 = 3;
/// A highlighter's yellow for the matches, and orange for the current one.
/// Pages are white in the Dark and Light themes alike, and a theme's accent
/// can be white or grey, so the marks keep their own colour.
const HIT: Color32 = Color32::from_rgba_premultiplied(110, 92, 0, 110);
const CURRENT_HIT: Color32 = Color32::from_rgb(255, 140, 0);
/// The card's flight, as the media viewer's picture flies: a shade of
/// bounce, so it settles instead of stopping.
const FLIGHT_SECONDS: f64 = 0.4;
const FLIGHT_BOUNCE: f64 = 0.12;
/// Points one arrow key scrolls.
const LINE: f32 = 64.0;

/// What the view keeps between frames: where it is scrolled, the zoom it
/// was laid out at, and a scroll easing toward a place.
#[derive(Clone, Copy)]
struct View {
    offset: Vec2,
    zoom: f32,
    /// Where a zoom keeps still on screen, when the pointer chose.
    anchor: Option<egui::Pos2>,
    glide: Option<Glide>,
    /// Whether a hit has been brought into view for the current query.
    revealed: bool,
}

#[derive(Clone, Copy)]
struct Glide {
    from: Vec2,
    to: Vec2,
    start: f64,
}

fn view_id(viewer: &Viewer) -> egui::Id {
    egui::Id::new(("pdf-view", viewer.path.as_path(), viewer.message.as_str()))
}

fn layer() -> egui::LayerId {
    egui::LayerId::new(egui::Order::Foreground, egui::Id::new("pdf-viewer"))
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let now = ctx.input(|input| input.time);
    let palette = app.palette;
    let locale = app.locale;
    // A closed viewer flies back into its message, and is let go (its
    // pages with it) once it has landed.
    if let Some((closed, retired)) = app.pdf_retired.as_mut() {
        let mut ignored = Vec::new();
        let landed = peek(ctx, retired, &palette, locale, false, now, &mut ignored);
        if landed || now - *closed > 1.0 {
            forget(ctx, retired);
            app.pdf_retired = None;
        }
    }
    let Some(mut viewer) = app.pdf_viewer.take() else {
        return;
    };
    let mut actions = Vec::new();
    peek(ctx, &mut viewer, &palette, locale, true, now, &mut actions);
    app.pdf_viewer = Some(viewer);
    app.actions.extend(actions);
}

/// Where the card settles: most of the window, the margin around it growing
/// with the window, and never wider than a comfortable reading desk.
fn card_rect(window: Rect) -> Rect {
    let x = (window.width() * 0.06)
        .clamp(16.0, 96.0)
        .max((window.width() - 1280.0) / 2.0);
    let y = (window.height() * 0.05).clamp(12.0, 48.0);
    window.shrink2(vec2(x, y))
}

/// Where the card flies from: the document's card in its message, when it
/// is on screen.
fn source_rect(ctx: &egui::Context, viewer: &Viewer) -> Option<Rect> {
    let id = super::conversation::bubble_id(&viewer.chat, &viewer.message).with("card");
    ctx.data(|data| data.get_temp::<Rect>(id))
        .filter(|rect| rect.is_positive() && ctx.content_rect().intersects(*rect))
}

/// Draws `viewer` as a card over the dimmed, frosted window, flying out of
/// its message as it opens (`open`) and back into it as it closes. Returns
/// whether a closing card has landed.
fn peek(
    ctx: &egui::Context,
    viewer: &mut Viewer,
    palette: &Palette,
    locale: crate::i18n::Locale,
    open: bool,
    now: f64,
    actions: &mut Vec<Action>,
) -> bool {
    // From nothing the first time, then turning round mid-flight with the
    // speed it has.
    let t = crate::motion::spring_from(
        ctx,
        egui::Id::new("pdf-peek"),
        if open { 1.0 } else { 0.0 },
        0.0,
        crate::motion::Spring::new(FLIGHT_SECONDS, FLIGHT_BOUNCE),
    );
    if !open && t <= 0.001 {
        return true;
    }
    // The bounce may carry the flight past its ends; what fades does not.
    let shown = t.clamp(0.0, 1.0);
    let window = ctx.content_rect();
    let card = card_rect(window);
    // The card is laid out at its full size and drawn scaled into where the
    // flight has it: from the message's card to the settled one.
    let flying = match source_rect(ctx, viewer) {
        Some(from) => Rect::from_min_max(from.min.lerp(card.min, t), from.max.lerp(card.max, t)),
        None => Rect::from_center_size(
            card.center(),
            card.size() * (theme::CLOSE_SCALE + (1.0 - theme::CLOSE_SCALE) * t),
        ),
    };
    let scale = (flying.width() / card.width()).max(0.01);
    let clip = Rect::from_center_size(card.center(), vec2(card.width(), flying.height() / scale));
    let radius = egui::lerp(10.0..=f32::from(theme::RADIUS_MENU), shown) / scale;
    let id = view_id(viewer);
    let mut view = ctx.data(|data| data.get_temp::<View>(id)).unwrap_or(View {
        offset: Vec2::ZERO,
        zoom: viewer.zoom,
        anchor: None,
        glide: None,
        revealed: false,
    });
    let mut backdrop_shapes = 0;
    let response = egui::Modal::new(layer().id)
        .area(
            egui::Area::new(layer().id)
                .order(egui::Order::Foreground)
                .fixed_pos(window.min),
        )
        .frame(egui::Frame::new())
        .backdrop_color(Color32::TRANSPARENT)
        .show(ctx, |ui| {
            ui.set_min_size(window.size());
            // The window behind frosts and dims as the card arrives.
            ui.painter().add(super::glass::veil(window, 0.0, shown));
            ui.painter()
                .rect_filled(window, 0.0, palette.shadow.gamma_multiply(shown));
            let (_, backdrop) = ui.allocate_exact_size(window.size(), egui::Sense::click());
            backdrop_shapes = ui
                .ctx()
                .graphics(|graphics| graphics.get(layer()).map_or(0, |list| list.next_idx().0));
            let mut card_ui = ui.new_child(UiBuilder::new().max_rect(card));
            card_ui.set_clip_rect(clip);
            if !open {
                card_ui.disable();
            }
            // Clicks on the card stay on it.
            card_ui.interact(card, id.with("card"), egui::Sense::click());
            let corners = CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8);
            card_ui
                .painter()
                .add(palette.float_shadow().as_shape(clip, corners));
            card_ui
                .painter()
                .rect_filled(clip, corners, palette.overlay);
            // In the Light theme the field is a shade darker than the white
            // pages, so their edges show.
            if !palette.dark {
                card_ui
                    .painter()
                    .rect_filled(clip, corners, palette.tint(0.07));
            }
            let failure = match &viewer.load {
                Load::Failed(failure) => Some(*failure),
                Load::Opening | Load::Ready(_) => None,
            };
            // Until the pages are known, only the worker's answer is awaited.
            if viewer.pages().is_empty() {
                viewer.exchange(ui.ctx(), &[], 0..0, 0, now);
            }
            let ui = &mut card_ui;
            let current = if !viewer.pages().is_empty() {
                stage(ui, viewer, &mut view, palette, locale, card, now)
            } else if let Some(failure) = failure {
                failed(ui, palette, locale, viewer, failure, card, actions);
                0
            } else {
                let middle = Rect::from_center_size(card.center(), Vec2::splat(32.0));
                theme::paint_spinner(ui, middle, 28.0, palette.secondary);
                0
            };
            header(ui, viewer, palette, locale, card, radius, current, actions);
            search_card(ui, viewer, &mut view, palette, locale, card, current);
            ui.painter().rect_stroke(
                clip,
                corners,
                palette.glass_edge(),
                egui::StrokeKind::Inside,
            );
            if open && backdrop.clicked() {
                actions.push(Action::ClosePdf);
            }
        });
    // Mid-flight, everything on the card is scaled into place; the
    // backdrop under it stays where it is.
    if (scale - 1.0).abs() > 0.001 || flying.center() != card.center() {
        let transform = egui::emath::TSTransform::new(
            flying.center().to_vec2() - card.center().to_vec2() * scale,
            scale,
        );
        ctx.graphics_mut(|graphics| {
            if let Some(list) = graphics.get_mut(layer()) {
                for index in backdrop_shapes..list.next_idx().0 {
                    list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                        clipped.transform(transform);
                    });
                }
            }
        });
    }
    // Until the flight rests.
    if (t - if open { 1.0 } else { 0.0 }).abs() > 0.0005 {
        ctx.request_repaint();
    }
    if open && response.should_close() {
        actions.push(if viewer.search.open {
            Action::ClosePdfSearch
        } else {
            Action::ClosePdf
        });
    }
    ctx.data_mut(|data| data.insert_temp(id, view));
    false
}

/// Forgets where the view of `viewer` was, for the next time it opens.
pub fn forget(ctx: &egui::Context, viewer: &Viewer) {
    let id = view_id(viewer);
    ctx.data_mut(|data| data.remove::<View>(id));
}

/// The pages, under the header: scrolled, zoomed and searched. Returns the
/// page being read.
fn stage(
    ui: &mut egui::Ui,
    viewer: &mut Viewer,
    view: &mut View,
    palette: &Palette,
    locale: crate::i18n::Locale,
    window: Rect,
    now: f64,
) -> usize {
    let sizes = viewer.pages().to_vec();
    let width = window.width();
    let viewport_height = window.height();
    let enabled = ui.is_enabled();
    // The wheel with Ctrl or Cmd, and a pinch, zoom about the pointer.
    if enabled && let Some((factor, anchor)) = zoom_input(ui, window) {
        viewer.set_zoom(viewer.zoom * factor, now);
        view.anchor = Some(anchor);
    }
    let layout = Pages::new(&sizes, width, viewer.zoom, HEADER);
    let mut offset = None;
    if (view.zoom - viewer.zoom).abs() > f32::EPSILON {
        let before = Pages::new(&sizes, width, view.zoom, HEADER);
        let anchor = view.anchor.take().unwrap_or(window.center()) - window.min;
        offset = Some(rezoom(&sizes, &before, &layout, view.offset, anchor));
        view.zoom = viewer.zoom;
        view.glide = None;
    }
    let max_offset = (layout.size - window.size()).max(Vec2::ZERO);
    // A search hit, or a key, eases the pages to a place.
    if let Some((page, fraction)) = viewer.scroll_to.take()
        && page < sizes.len()
    {
        let rect = layout.page_rect(&sizes, page);
        let y = rect.top() + rect.height() * fraction - (viewport_height - HEADER) * 0.35 - HEADER;
        let mut to = view.offset;
        to.y = y;
        glide_to(view, to.clamp(Vec2::ZERO, max_offset), now);
    }
    if enabled && let Some(to) = key_scroll(ui, view, viewport_height - HEADER, layout.size.y) {
        glide_to(view, to.clamp(Vec2::ZERO, max_offset), now);
    }
    if let Some(glide) = view.glide {
        let seconds = theme::motion_time(ui.ctx(), theme::MOTION_PANEL);
        let t = if seconds > 0.0 {
            ((now - glide.start) as f32 / seconds).clamp(0.0, 1.0)
        } else {
            1.0
        };
        offset = Some(glide.from + (glide.to - glide.from) * theme::ease_out(t));
        if t >= 1.0 {
            view.glide = None;
        } else {
            ui.ctx().request_repaint();
        }
    }
    let mut scroll = egui::ScrollArea::both()
        .id_salt("pdf-pages")
        .auto_shrink(false)
        .scroll_bar_rect(Rect::from_min_max(
            pos2(window.left(), window.top() + HEADER),
            window.max,
        ));
    if let Some(offset) = offset {
        scroll = scroll.scroll_offset(offset.clamp(Vec2::ZERO, max_offset));
    }
    let mut stage_ui = ui.new_child(UiBuilder::new().max_rect(window));
    let ppp = ui.ctx().pixels_per_point();
    let max_side = ui.ctx().input(|input| input.max_texture_side);
    let output = scroll.show_viewport(&mut stage_ui, |ui, viewport| {
        ui.set_min_size(layout.size);
        let origin = ui.max_rect().min.to_vec2();
        let visible = layout.visible(&sizes, viewport.top(), viewport.bottom());
        let middle = layout.page_at(viewport.top() + HEADER + (viewport.height() - HEADER) / 2.0);
        for page in visible.clone() {
            let rect = layout.page_rect(&sizes, page).translate(origin);
            paint_page(ui, viewer, palette, locale, page, rect);
        }
        if viewer.search.open {
            paint_hits(ui, viewer, &layout, &sizes, visible.clone(), origin);
        }
        let wanted = crate::pdf::wanted_pages(visible.clone(), sizes.len(), middle, |page| {
            let points = sizes[page].x * layout.scale;
            crate::pdf::render_width(points, ppp, sizes[page].y / sizes[page].x, max_side)
        });
        (visible, middle, wanted)
    });
    view.offset = output.state.offset;
    let (visible, middle, wanted) = output.inner;
    viewer.exchange(ui.ctx(), &wanted, visible, middle, now);
    middle
}

/// Where to scroll after a zoom so the point under `anchor` (from the
/// stage's corner) stays under it.
fn rezoom(sizes: &[Vec2], before: &Pages, after: &Pages, offset: Vec2, anchor: Vec2) -> Vec2 {
    let point = offset + anchor;
    let page = before.page_at(point.y).min(sizes.len().saturating_sub(1));
    let ratio = after.scale / before.scale;
    let y = if sizes.is_empty() {
        point.y
    } else {
        let down = (point.y - before.tops[page]) / (sizes[page].y * before.scale);
        after.tops[page] + down * sizes[page].y * after.scale
    };
    let x = after.size.x / 2.0 + (point.x - before.size.x / 2.0) * ratio;
    (pos2(x, y) - anchor).to_vec2().max(Vec2::ZERO)
}

fn glide_to(view: &mut View, to: Vec2, now: f64) {
    view.glide = Some(Glide {
        from: view.offset,
        to,
        start: now,
    });
}

/// The keys that move through the pages, while nothing typed takes them:
/// the arrows a line, Page Up and Down and Space a screen, Home and End to
/// either end.
fn key_scroll(ui: &egui::Ui, view: &View, screen: f32, height: f32) -> Option<Vec2> {
    if ui.ctx().text_edit_focused() {
        return None;
    }
    let base = view.glide.map_or(view.offset, |glide| glide.to);
    ui.input_mut(|input| {
        use egui::{Key, Modifiers};
        let page = screen * 0.9;
        let mut to = None;
        if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
            to = Some(base.y + LINE);
        }
        if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
            to = Some(base.y - LINE);
        }
        if input.consume_key(Modifiers::NONE, Key::PageDown)
            || input.consume_key(Modifiers::NONE, Key::Space)
        {
            to = Some(base.y + page);
        }
        if input.consume_key(Modifiers::NONE, Key::PageUp)
            || input.consume_key(Modifiers::SHIFT, Key::Space)
        {
            to = Some(base.y - page);
        }
        if input.consume_key(Modifiers::NONE, Key::Home) {
            to = Some(0.0);
        }
        if input.consume_key(Modifiers::NONE, Key::End) {
            to = Some(height);
        }
        to.map(|y| vec2(base.x, y))
    })
}

/// The zoom a Ctrl or Cmd wheel or a pinch asks for over the stage, and the
/// point it is about. Each wheel notch is one zoom step.
fn zoom_input(ui: &egui::Ui, area: Rect) -> Option<(f32, egui::Pos2)> {
    let (touch, hover) = ui.input(|input| {
        (
            input.multi_touch().map(|touch| touch.center_pos),
            input.pointer.hover_pos(),
        )
    });
    let anchor = touch.or(hover)?;
    if !area.contains(anchor) {
        return None;
    }
    let (zoom_speed, line_speed) = ui.ctx().options(|options| {
        let input = &options.input_options;
        (input.scroll_zoom_speed, input.line_scroll_speed)
    });
    let factor = ui.input(|input| {
        let pinch = input.multi_touch().is_some()
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Zoom(_)));
        let factor = input.zoom_delta();
        if pinch || factor == 1.0 {
            factor
        } else {
            // egui turns a Ctrl wheel into exp(speed * points); this makes
            // a notch one step.
            factor.powf(crate::pdf::ZOOM_STEP.ln() / (zoom_speed * line_speed))
        }
    });
    (factor != 1.0).then_some((factor, anchor))
}

/// One page: a white sheet lifted off the field, its picture fading in once
/// rendered, a spinner until then, or a note if it could not be.
fn paint_page(
    ui: &egui::Ui,
    viewer: &Viewer,
    palette: &Palette,
    locale: crate::i18n::Locale,
    page: usize,
    rect: Rect,
) {
    let painter = ui.painter();
    let radius = CornerRadius::same(PAGE_RADIUS);
    painter.add(
        egui::epaint::Shadow {
            offset: [0, 2],
            blur: 14,
            spread: 0,
            color: palette.shadow.gamma_multiply(0.45),
        }
        .as_shape(rect, radius),
    );
    painter.rect_filled(rect, radius, Color32::WHITE);
    let id = egui::Id::new(("pdf-page", page));
    match viewer.texture(page) {
        Some(held) => {
            let shown = theme::animate(ui.ctx(), id, true, theme::MOTION_QUICK);
            egui::Image::from_texture((held.texture.id(), rect.size()))
                .corner_radius(radius)
                .tint(Color32::WHITE.gamma_multiply(shown))
                .paint_at(ui, rect);
        }
        None if viewer.page_failed(page) => {
            theme::paint_icon(
                ui,
                Icon::CircleAlert,
                Rect::from_center_size(rect.center() - vec2(0.0, 12.0), Vec2::splat(24.0)),
                24.0,
                Color32::from_gray(150),
            );
            painter.text(
                rect.center() + vec2(0.0, 18.0),
                Align2::CENTER_CENTER,
                crate::i18n::gettext(locale, "This page could not be shown"),
                theme::regular(theme::TEXT_CAPTION),
                Color32::from_gray(110),
            );
        }
        None => {
            theme::animate(ui.ctx(), id, false, 0.0);
            let spinner = Rect::from_center_size(rect.center(), Vec2::splat(24.0));
            if ui.is_rect_visible(spinner) {
                theme::paint_spinner(ui, spinner, 20.0, Color32::from_gray(170));
            }
        }
    }
}

/// Lights the search's hits on the pages on screen; the current one more,
/// with an edge.
fn paint_hits(
    ui: &egui::Ui,
    viewer: &Viewer,
    layout: &Pages,
    sizes: &[Vec2],
    visible: std::ops::Range<usize>,
    origin: Vec2,
) {
    let hits = &viewer.search.hits;
    let first = hits.partition_point(|hit| hit.page < visible.start);
    let lit = theme::animate(
        ui.ctx(),
        egui::Id::new(("pdf-hit", viewer.search.current)),
        true,
        theme::MOTION_QUICK,
    );
    for (index, hit) in hits.iter().enumerate().skip(first) {
        if hit.page >= visible.end {
            break;
        }
        let page = layout.page_rect(sizes, hit.page).translate(origin);
        let current = index == viewer.search.current;
        for rect in &hit.rects {
            let rect = Rect::from_min_max(
                page.min + rect.min.to_vec2() * page.size(),
                page.min + rect.max.to_vec2() * page.size(),
            )
            .expand(1.5);
            let fill = if current {
                CURRENT_HIT.gamma_multiply(0.3 + 0.12 * lit)
            } else {
                HIT
            };
            ui.painter().rect_filled(rect, 3.0, fill);
            if current {
                ui.painter().rect_stroke(
                    rect,
                    3.0,
                    egui::Stroke::new(1.5, CURRENT_HIT.gamma_multiply(lit)),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
}

/// Why the document is not shown, and the way to read it anyway.
fn failed(
    ui: &mut egui::Ui,
    palette: &Palette,
    locale: crate::i18n::Locale,
    viewer: &Viewer,
    failure: Failure,
    window: Rect,
    actions: &mut Vec<Action>,
) {
    let (icon, title) = match failure {
        Failure::Locked => (
            Icon::Lock,
            crate::i18n::gettext(locale, "This PDF is protected with a password"),
        ),
        Failure::Unreadable => (
            Icon::CircleAlert,
            crate::i18n::gettext(locale, "This PDF can't be shown here"),
        ),
    };
    let body = Rect::from_center_size(window.center(), vec2(window.width().min(420.0), 200.0));
    let mut ui = ui.new_child(
        UiBuilder::new()
            .max_rect(body)
            .layout(Layout::top_down(Align::Center)),
    );
    theme::icon(&mut ui, icon, 40.0, palette.dim);
    ui.add_space(theme::SPACE_S);
    theme::text(
        &mut ui,
        title.as_ref(),
        theme::semibold(theme::TEXT_BODY + 2.0),
        palette.text,
    );
    ui.add_space(4.0);
    theme::text(
        &mut ui,
        crate::i18n::gettext(locale, "Open it in another app to read it.").as_ref(),
        theme::regular(theme::TEXT_BODY),
        palette.secondary,
    );
    ui.add_space(theme::SPACE_M);
    if theme::soft_button(
        &mut ui,
        palette,
        Some(Icon::ExternalLink),
        &crate::i18n::gettext(locale, "Open in another app"),
        false,
    )
    .clicked()
    {
        actions.push(Action::ClosePdf);
        actions.push(Action::OpenFile(viewer.path.clone()));
    }
}

/// The frosted bar across the top: the document's name and the page being
/// read, then search, zoom, Save as, Open in another app and Close.
fn header(
    ui: &mut egui::Ui,
    viewer: &mut Viewer,
    palette: &Palette,
    locale: crate::i18n::Locale,
    window: Rect,
    radius: f32,
    current: usize,
    actions: &mut Vec<Action>,
) {
    let rect = Rect::from_min_max(window.min, pos2(window.right(), window.top() + HEADER));
    let top = radius.round().clamp(0.0, 255.0) as u8;
    let corners = CornerRadius {
        nw: top,
        ne: top,
        sw: 0,
        se: 0,
    };
    ui.painter().add(super::glass::rounded(rect, radius));
    ui.painter()
        .rect_filled(rect, corners, palette.menu_glass());
    ui.painter()
        .hline(rect.x_range(), rect.bottom() - 0.5, palette.glass_edge());
    let mut bar = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(14.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    let gettext = |text| crate::i18n::gettext(locale, text);
    if theme::icon_button(
        &mut bar,
        Icon::X,
        18.0,
        palette.secondary,
        palette.text,
        &gettext("Close (Esc)"),
    )
    .clicked()
    {
        actions.push(Action::ClosePdf);
    }
    if theme::icon_button(
        &mut bar,
        Icon::ExternalLink,
        18.0,
        palette.secondary,
        palette.text,
        &gettext("Open in another app"),
    )
    .clicked()
    {
        actions.push(Action::ClosePdf);
        actions.push(Action::OpenFile(viewer.path.clone()));
    }
    if theme::icon_button(
        &mut bar,
        Icon::Download,
        18.0,
        palette.secondary,
        palette.text,
        &gettext("Save as…"),
    )
    .clicked()
    {
        actions.push(Action::SaveAttachmentAs {
            path: viewer.path.clone(),
            name: viewer.title.clone(),
        });
    }
    let ready = !viewer.pages().is_empty();
    if ready {
        bar.add_space(theme::SPACE_S);
        if theme::icon_button(
            &mut bar,
            Icon::Plus,
            18.0,
            palette.secondary,
            palette.text,
            &gettext("Zoom in"),
        )
        .clicked()
        {
            actions.push(Action::ZoomPdf(crate::pdf::ZOOM_STEP));
        }
        let fit = (viewer.zoom - 1.0).abs() < 0.005;
        let label = if fit {
            gettext("Fit").into_owned()
        } else {
            format!("{:.0}%", viewer.zoom * 100.0)
        };
        if theme::soft_button(&mut bar, palette, None, &label, false)
            .on_hover_text(gettext("Fit to the window (Ctrl+0)"))
            .clicked()
        {
            actions.push(Action::ZoomPdf(0.0));
        }
        if theme::icon_button(
            &mut bar,
            Icon::Minus,
            18.0,
            palette.secondary,
            palette.text,
            &gettext("Zoom out"),
        )
        .clicked()
        {
            actions.push(Action::ZoomPdf(1.0 / crate::pdf::ZOOM_STEP));
        }
        bar.add_space(theme::SPACE_S);
        let searching = viewer.search.open;
        if theme::icon_button(
            &mut bar,
            Icon::Search,
            18.0,
            if searching {
                palette.accent
            } else {
                palette.secondary
            },
            palette.text,
            &gettext("Find in document (Ctrl+F)"),
        )
        .clicked()
        {
            actions.push(if searching {
                Action::ClosePdfSearch
            } else {
                Action::FindInPdf
            });
        }
    }
    // The name and the page take what the controls leave, on the left.
    bar.with_layout(Layout::left_to_right(Align::Center), |ui| {
        let text = ui.available_width();
        ui.allocate_ui_with_layout(vec2(text, 34.0), Layout::top_down(Align::Min), |ui| {
            ui.set_max_width(text);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&viewer.title)
                        .font(theme::semibold(theme::TEXT_BODY))
                        .color(palette.text),
                )
                .truncate()
                .selectable(false),
            );
            let detail = match viewer.pages().len() {
                0 => String::new(),
                count => crate::i18n::gettext(locale, "Page {page} of {count}")
                    .replace("{page}", &(current + 1).min(count).to_string())
                    .replace("{count}", &count.to_string()),
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(detail)
                        .font(theme::regular(theme::TEXT_CAPTION))
                        .color(palette.secondary),
                )
                .truncate()
                .selectable(false),
            );
        });
    });
}

/// The search card under the header, sliding and fading in as it opens and
/// out as it closes: the field, how many matches, and the way between them.
fn search_card(
    ui: &mut egui::Ui,
    viewer: &mut Viewer,
    view: &mut View,
    palette: &Palette,
    locale: crate::i18n::Locale,
    window: Rect,
    current: usize,
) {
    let open = theme::animate(
        ui.ctx(),
        egui::Id::new("pdf-search-open"),
        viewer.search.open,
        theme::MOTION_PANEL,
    );
    if open <= 0.0 {
        view.revealed = false;
        return;
    }
    // New text or a new query: count again, and bring the first match at
    // or after the page being read into view.
    let changed = viewer.recount();
    if changed {
        view.revealed = false;
    }
    if !view.revealed && !viewer.search.hits.is_empty() && viewer.search.open {
        let hits = &viewer.search.hits;
        viewer.search.current = hits.partition_point(|hit| hit.page < current) % hits.len();
        viewer.reveal_current();
        view.revealed = true;
    }
    let width = SEARCH_WIDTH.min(window.width() - 2.0 * SEARCH_INSET);
    let rect = Rect::from_min_size(
        pos2(
            window.right() - SEARCH_INSET - width,
            window.top() + HEADER + SEARCH_INSET - 8.0 * (1.0 - open),
        ),
        vec2(width, SEARCH_HEIGHT),
    );
    let radius = f32::from(theme::RADIUS);
    let painter = ui.painter();
    painter.add(
        palette
            .float_shadow()
            .as_shape(rect, CornerRadius::same(theme::RADIUS)),
    );
    painter.add(super::glass::veil(rect, radius, open));
    painter.rect_filled(rect, radius, palette.menu_glass());
    painter.rect_stroke(rect, radius, palette.glass_edge(), egui::StrokeKind::Inside);
    let mut card = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(8.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    card.set_opacity(open);
    if !viewer.search.open {
        card.disable();
    }
    let gettext = |text| crate::i18n::gettext(locale, text);
    if theme::icon_button(
        &mut card,
        Icon::X,
        16.0,
        palette.secondary,
        palette.text,
        &gettext("Close search (Esc)"),
    )
    .clicked()
    {
        viewer.search.open = false;
    }
    let count = viewer.search.hits.len();
    if theme::icon_button(
        &mut card,
        Icon::ChevronDown,
        16.0,
        palette.secondary,
        palette.text,
        &gettext("Next match (Enter)"),
    )
    .clicked()
    {
        viewer.step_hit(false);
    }
    if theme::icon_button(
        &mut card,
        Icon::ChevronUp,
        16.0,
        palette.secondary,
        palette.text,
        &gettext("Previous match (Shift+Enter)"),
    )
    .clicked()
    {
        viewer.step_hit(true);
    }
    let (read, pages) = viewer.text_progress();
    let status = if crate::pdf::fold_query(&viewer.search.query).is_empty() {
        String::new()
    } else if count > 0 {
        gettext("{current} of {count}")
            .replace("{current}", &(viewer.search.current + 1).to_string())
            .replace("{count}", &count.to_string())
    } else if read < pages {
        gettext("Searching…").into_owned()
    } else {
        gettext("No matches").into_owned()
    };
    card.add_sized(
        vec2(76.0, SEARCH_HEIGHT),
        egui::Label::new(
            egui::RichText::new(status)
                .font(theme::regular(theme::TEXT_CAPTION))
                .color(palette.secondary),
        )
        .selectable(false),
    );
    let field_id = egui::Id::new("pdf-search-field");
    if std::mem::take(&mut viewer.search.focus) {
        card.memory_mut(|memory| memory.request_focus(field_id));
    }
    let field_width = card.available_width();
    let mut field = card.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(rect.left() + 8.0, rect.center().y - 17.0),
                vec2(field_width, 34.0),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = super::widgets::search_field(
        &mut field,
        palette,
        field_id,
        &mut viewer.search.query,
        &gettext("Find in document"),
        field_width,
    );
    if response.lost_focus() && field.input(|input| input.key_pressed(egui::Key::Enter)) {
        let back = field.input(|input| input.modifiers.shift);
        viewer.step_hit(back);
        field.memory_mut(|memory| memory.request_focus(field_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zoom_keeps_the_point_under_the_pointer() {
        let sizes = [vec2(600.0, 800.0), vec2(600.0, 800.0)];
        let before = Pages::new(&sizes, 648.0, 1.0, HEADER);
        let after = Pages::new(&sizes, 648.0, 2.0, HEADER);
        // The pointer over the middle of the second page, scrolled to it.
        let offset = vec2(0.0, before.tops[1] - 100.0);
        let anchor = vec2(324.0, 500.0);
        let point = offset + anchor;
        let down = (point.y - before.tops[1]) / 800.0;
        let moved = rezoom(&sizes, &before, &after, offset, anchor);
        let under = moved + anchor;
        assert!((under.y - (after.tops[1] + down * 1600.0)).abs() < 0.01);
        assert!(
            (under.x - after.size.x / 2.0).abs() < 0.01,
            "the centre stays the centre"
        );
    }
}
